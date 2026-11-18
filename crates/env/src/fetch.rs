//! HTTPS downloads and version listings. Network access only to URLs taken from a validated
//! manifest; HTTPS only, including every redirect (plain HTTP only with the test-only flag).

use crate::detect::compare_versions;
use crate::error::EnvError;
use axiom_protocol::provider::VersionSource;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

/// Largest text document (version list, checksum file) read into memory.
const MAX_TEXT: usize = 16 * 1024 * 1024;

#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    allow_http: bool,
    github_api: String,
}

/// Network configuration. `Default` is the production setting.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// TEST ONLY: also accept plain `http://` URLs to loopback hosts (a local fixture server).
    pub allow_http: bool,
    /// Base URL of the GitHub REST API.
    pub github_api: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        HttpConfig {
            allow_http: false,
            github_api: "https://api.github.com".into(),
        }
    }
}

impl Http {
    pub fn new(cfg: HttpConfig) -> Result<Http, EnvError> {
        let allow_http = cfg.allow_http;
        let policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                attempt.error("too many redirects")
            } else if attempt.url().scheme() != "https"
                && !(allow_http && is_loopback_http(attempt.url()))
            {
                attempt.error("redirect to a non-HTTPS URL")
            } else {
                attempt.follow()
            }
        });
        let client = reqwest::Client::builder()
            .user_agent(concat!("axiom/", env!("CARGO_PKG_VERSION")))
            .redirect(policy)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .https_only(!allow_http)
            .build()
            .map_err(|e| EnvError::Network(e.to_string()))?;
        Ok(Http {
            client,
            allow_http,
            github_api: cfg.github_api,
        })
    }

    fn check(&self, url: &str) -> Result<reqwest::Url, EnvError> {
        let u = reqwest::Url::parse(url)
            .map_err(|e| EnvError::InvalidParams(format!("Invalid URL {url}: {e}")))?;
        match u.scheme() {
            "https" => Ok(u),
            "http" if self.allow_http && is_loopback_http(&u) => Ok(u),
            s => Err(EnvError::InvalidParams(format!(
                "Only HTTPS downloads are allowed, not {s}: {url}"
            ))),
        }
    }

    /// GET a small text document.
    pub async fn get_text(&self, url: &str) -> Result<String, EnvError> {
        let u = self.check(url)?;
        let resp = self
            .client
            .get(u)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| EnvError::Network(net(&e)))?;
        let mut body = Vec::new();
        let mut s = resp.bytes_stream();
        while let Some(chunk) = s.next().await {
            let chunk = chunk.map_err(|e| EnvError::Network(net(&e)))?;
            if body.len() + chunk.len() > MAX_TEXT {
                return Err(EnvError::Network(format!("{url} is too large")));
            }
            body.extend_from_slice(&chunk);
        }
        String::from_utf8(body).map_err(|_| EnvError::Network(format!("{url} is not text")))
    }

    /// The `Content-Length` of a URL, if the server states it (HEAD, 10 s).
    pub async fn size(&self, url: &str) -> Option<u64> {
        let u = self.check(url).ok()?;
        let r = tokio::time::timeout(Duration::from_secs(10), self.client.head(u).send())
            .await
            .ok()?
            .ok()?;
        // Not `content_length()`: on a HEAD response that is the (empty) body's length.
        r.error_for_status()
            .ok()?
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)?
            .to_str()
            .ok()?
            .parse()
            .ok()
    }

    /// Streams `url` into `dest` (created or truncated), hashing as it goes. Returns the
    /// lowercase SHA-256 and the byte count. `progress(done, total)` is called per chunk.
    pub async fn download(
        &self,
        url: &str,
        dest: &Path,
        cancel: &CancellationToken,
        progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<(String, u64), EnvError> {
        let u = self.check(url)?;
        let resp = tokio::select! {
            r = self.client.get(u).send() => r,
            _ = cancel.cancelled() => return Err(EnvError::Cancelled),
        }
        .and_then(|r| r.error_for_status())
        .map_err(|e| EnvError::Network(net(&e)))?;
        let total = resp.content_length();
        let mut file = tokio::fs::File::create(dest)
            .await
            .map_err(EnvError::io("Cannot write to the download cache"))?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut stream = resp.bytes_stream();
        loop {
            let next = tokio::select! {
                n = stream.next() => n,
                _ = cancel.cancelled() => return Err(EnvError::Cancelled),
            };
            let Some(chunk) = next else { break };
            let chunk = chunk.map_err(|e| EnvError::Network(net(&e)))?;
            hasher.update(&chunk);
            file.write_all(&chunk)
                .await
                .map_err(EnvError::io("Cannot write to the download cache"))?;
            done += chunk.len() as u64;
            progress(done, total);
        }
        file.flush()
            .await
            .map_err(EnvError::io("Cannot write to the download cache"))?;
        Ok((hex::encode(hasher.finalize()), done))
    }

    /// Versions from a manifest's source, newest first, deduplicated.
    pub async fn versions(&self, source: &VersionSource) -> Result<Vec<String>, EnvError> {
        let mut out = match source {
            VersionSource::Static { versions } => versions.clone(),
            VersionSource::Json { url, pointer } => {
                let text = self.get_text(url).await?;
                let doc: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| EnvError::Network(format!("{url} is not JSON: {e}")))?;
                select(&doc, pointer)
            }
            VersionSource::GithubReleases { repo, tag_prefix } => {
                let url = format!(
                    "{}/repos/{repo}/releases?per_page=100",
                    self.github_api.trim_end_matches('/')
                );
                let text = self.get_text(&url).await?;
                let doc: serde_json::Value = serde_json::from_str(&text)
                    .map_err(|e| EnvError::Network(format!("{url} is not JSON: {e}")))?;
                doc.as_array()
                    .into_iter()
                    .flatten()
                    .filter(|r| !r["draft"].as_bool().unwrap_or(false))
                    .filter(|r| !r["prerelease"].as_bool().unwrap_or(false))
                    .filter_map(|r| r["tag_name"].as_str())
                    .filter_map(|t| t.strip_prefix(tag_prefix.as_str()))
                    .map(String::from)
                    .collect()
            }
        };
        out.retain(|v| crate::install::valid_version(v));
        out.sort_by(|a, b| compare_versions(b, a));
        out.dedup();
        Ok(out)
    }
}

fn is_loopback_http(u: &reqwest::Url) -> bool {
    let host = u.host_str().unwrap_or_default();
    u.scheme() == "http"
        && (host == "localhost"
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .is_ok_and(|a| a.is_loopback()))
}

fn net(e: &reqwest::Error) -> String {
    // Without the URL: the message is user-presentable and the URL is in the plan already.
    let mut s = e.without_url_ref().to_string();
    let mut src = std::error::Error::source(e);
    while let Some(inner) = src {
        s.push_str(": ");
        s.push_str(&inner.to_string());
        src = inner.source();
    }
    s
}

trait WithoutUrl {
    fn without_url_ref(&self) -> String;
}

impl WithoutUrl for reqwest::Error {
    fn without_url_ref(&self) -> String {
        if let Some(s) = self.status() {
            format!("the server answered {s}")
        } else if self.is_timeout() {
            "the connection timed out".into()
        } else if self.is_connect() {
            "cannot connect".into()
        } else if self.is_redirect() {
            "refused redirect".into()
        } else {
            "transfer failed".into()
        }
    }
}

/// RFC 6901 pointer where a `*` segment fans out over array elements and object values.
pub fn select(doc: &serde_json::Value, pointer: &str) -> Vec<String> {
    let mut cur = vec![doc];
    for seg in pointer.split('/').skip(1) {
        let seg = seg.replace("~1", "/").replace("~0", "~");
        let mut next = Vec::new();
        for v in cur {
            if seg == "*" {
                match v {
                    serde_json::Value::Array(a) => next.extend(a.iter()),
                    serde_json::Value::Object(o) => next.extend(o.values()),
                    _ => {}
                }
            } else if let Some(x) = match v {
                serde_json::Value::Array(a) => seg.parse::<usize>().ok().and_then(|i| a.get(i)),
                serde_json::Value::Object(o) => o.get(&seg),
                _ => None,
            } {
                next.push(x);
            }
        }
        cur = next;
    }
    cur.into_iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect()
}

/// The digest for `file_name` in a checksum file: `sha256sum` lines (`<hex>  [*]<name>`), or
/// one bare digest.
pub fn digest_from_checksum_file(text: &str, file_name: &str) -> Option<String> {
    let is_hex = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let lines: Vec<Vec<&str>> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>())
        .filter(|t| !t.is_empty())
        .collect();
    if let [only] = lines.as_slice() {
        if only.len() == 1 && is_hex(only[0]) {
            return Some(only[0].to_ascii_lowercase());
        }
    }
    lines.iter().find_map(|t| {
        // `*` marks binary mode; the name may carry a directory (`*./rustup-init`).
        let name = t.get(1).map(|n| n.trim_start_matches('*'));
        let base = name.map(|n| n.rsplit(['/', '\\']).next().unwrap_or(n));
        (t.len() >= 2 && is_hex(t[0]) && base == Some(file_name))
            .then(|| t[0].to_ascii_lowercase())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pointer_with_wildcards() {
        let doc = json!([{"version": "v22.1.0", "lts": false}, {"version": "v20.0.0"}]);
        assert_eq!(select(&doc, "/*/version"), ["v22.1.0", "v20.0.0"]);
        assert_eq!(select(&json!({"a": {"b": ["x"]}}), "/a/b/0"), ["x"]);
        assert!(select(&doc, "/*/missing").is_empty());
    }

    #[test]
    fn checksum_files() {
        let d = "a".repeat(64);
        let e = "B".repeat(64);
        let sums = format!("{d}  node-a.tar.gz\n{e} *node-b.tar.xz\n");
        assert_eq!(
            digest_from_checksum_file(&sums, "node-a.tar.gz"),
            Some(d.clone())
        );
        assert_eq!(
            digest_from_checksum_file(&sums, "node-b.tar.xz"),
            Some("b".repeat(64))
        );
        assert_eq!(digest_from_checksum_file(&sums, "other"), None);
        // static.rust-lang.org's format.
        let rustup = format!("{d} *./rustup-init\n");
        assert_eq!(digest_from_checksum_file(&rustup, "rustup-init"), Some(d.clone()));
        assert_eq!(
            digest_from_checksum_file(&format!("{d}\n"), "anything"),
            Some(d)
        );
        assert_eq!(digest_from_checksum_file("not a digest", "x"), None);
    }

    #[test]
    fn only_https_unless_testing() {
        let h = Http::new(HttpConfig::default()).unwrap();
        assert!(h.check("https://example.org/x").is_ok());
        assert!(matches!(
            h.check("http://example.org/x"),
            Err(EnvError::InvalidParams(_))
        ));
        assert!(h.check("file:///etc/passwd").is_err());
        let t = Http::new(HttpConfig {
            allow_http: true,
            ..HttpConfig::default()
        })
        .unwrap();
        assert!(t.check("http://127.0.0.1:1/x").is_ok());
        assert!(
            t.check("http://example.org/x").is_err(),
            "plain HTTP only to loopback"
        );
        assert!(t.check("file:///etc/passwd").is_err());
    }
}
