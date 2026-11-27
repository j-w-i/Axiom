//! What the browser asks for, decided and made here (decision 0021). Only a server of the agent's own
//! is let through: plain HTTP on this computer's loopback address, at a port a process of the agent's
//! tree listens on now, looked at again for each request. Everything else is refused, and its person
//! is shown what was. The host makes the request itself; the browser's own network leads nowhere.

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    time::Duration,
};

use base64::Engine;
use reqwest::{Method, Url, redirect};
use serde_json::{Value, json};

use crate::Inner;

/// Who decides whose a port is.
pub trait Gate: Send + Sync + 'static {
    /// Whether a process of the agent's own tree listens on `port` now.
    fn serves(&self, port: u16) -> bool;
}

/// Of a response's body.
const BODY: usize = 32 * 1024 * 1024;
const LONGEST: Duration = Duration::from_secs(30);

/// Not passed on: they are of one connection, not of what is asked.
const HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "proxy-connection",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];

/// Where `url` may be fetched from, or why not.
pub fn decide(url: &str, gate: &dyn Gate) -> Result<SocketAddr, String> {
    let no = |why: &str| Err(format!("{url}: {why}"));
    let Ok(u) = Url::parse(url) else { return no("not an address") };
    if u.scheme() != "http" {
        return no("only an agent's own server, over http, is opened");
    }
    if !u.username().is_empty() || u.password().is_some() {
        return no("an address with a name and password is not opened");
    }
    // As the address is read: 127.1 and 0x7f.0.0.1 are 127.0.0.1 by then.
    let ip = match u.host_str() {
        Some("localhost" | "127.0.0.1") => IpAddr::V4(Ipv4Addr::LOCALHOST),
        Some("[::1]") => IpAddr::V6(Ipv6Addr::LOCALHOST),
        _ => return no("only what an agent serves on this computer is opened"),
    };
    let Some(port) = u.port_or_known_default() else { return no("no port") };
    if !gate.serves(port) {
        return no("nothing of this agent's serves there");
    }
    Ok(SocketAddr::new(ip, port))
}

pub(crate) fn client() -> Result<reqwest::Client, String> {
    // Plain HTTP only. It has to be told of TLS all the same: a TLS that trusts nobody.
    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .https_only(false)
        .redirect(redirect::Policy::none())
        .retry(reqwest::retry::never())
        .referer(false)
        .http1_only()
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(LONGEST)
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| e.to_string())
}

/// What came back, as the browser is given it.
struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

async fn fetch(
    client: &reqwest::Client,
    to: SocketAddr,
    url: &str,
    request: &Value,
) -> Result<Answer, String> {
    let mut u = Url::parse(url).map_err(|e| e.to_string())?;
    // To the address that was decided, whatever the name said.
    u.set_ip_host(to.ip()).map_err(|()| "no such address".to_owned())?;
    let method = Method::from_bytes(request["method"].as_str().unwrap_or("GET").as_bytes())
        .map_err(|e| e.to_string())?;
    let mut r = client.request(method, u);
    for (k, v) in request["headers"].as_object().into_iter().flatten() {
        if HOP.contains(&k.to_ascii_lowercase().as_str()) {
            continue;
        }
        if let Some(v) = v.as_str() {
            r = r.header(k.as_str(), v);
        }
    }
    let mut body = Vec::new();
    for e in request["postDataEntries"].as_array().into_iter().flatten() {
        if let Some(b) = e["bytes"].as_str() {
            let b =
                base64::engine::general_purpose::STANDARD.decode(b).map_err(|e| e.to_string())?;
            body.extend(b);
        }
    }
    if body.is_empty()
        && let Some(p) = request["postData"].as_str()
    {
        body.extend_from_slice(p.as_bytes());
    }
    if !body.is_empty() {
        r = r.body(body);
    }
    let mut res = r.send().await.map_err(|e| e.to_string())?;
    let status = res.status().as_u16();
    let headers = res
        .headers()
        .iter()
        .filter(|(k, _)| !HOP.contains(&k.as_str()))
        .filter_map(|(k, v)| Some((k.as_str().to_owned(), v.to_str().ok()?.to_owned())))
        .collect();
    let mut body = Vec::new();
    while let Some(c) = res.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + c.len() > BODY {
            return Err("the answer is too large".into());
        }
        body.extend_from_slice(&c);
    }
    Ok(Answer { status, headers, body })
}

/// Answers one paused request: with what the agent's server said, or not at all.
pub(crate) async fn answer(inner: &Inner, p: &Value) {
    let id = p["requestId"].clone();
    let url = p["request"]["url"].as_str().unwrap_or_default();
    let fail = |reason: &str| json!({"requestId": id, "errorReason": reason});
    let to = match decide(url, &*inner.gate) {
        Ok(to) => to,
        Err(_) => {
            // What a page itself made is not looked at.
            if !url.starts_with("data:") && !url.starts_with("blob:") {
                inner.refused(url);
            }
            let _ = inner.on_page("Fetch.failRequest", fail("BlockedByClient")).await;
            return;
        }
    };
    match fetch(&inner.client, to, url, &p["request"]).await {
        Ok(a) => {
            let headers: Vec<Value> =
                a.headers.iter().map(|(k, v)| json!({"name": k, "value": v})).collect();
            let body = base64::engine::general_purpose::STANDARD.encode(&a.body);
            let m = json!({"requestId": id, "responseCode": a.status, "responseHeaders": headers, "body": body});
            if inner.on_page("Fetch.fulfillRequest", m).await.is_err() {
                let _ = inner.on_page("Fetch.failRequest", fail("Failed")).await;
            }
        }
        Err(_) => {
            let _ = inner.on_page("Fetch.failRequest", fail("ConnectionRefused")).await;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    struct Serves(Vec<u16>);
    impl Gate for Serves {
        fn serves(&self, port: u16) -> bool {
            self.0.contains(&port)
        }
    }

    #[test]
    fn only_an_agents_own_server_is_let_through() {
        let g = Serves(vec![5173, 80]);
        let v4 = SocketAddr::from(([127, 0, 0, 1], 5173));
        assert_eq!(decide("http://127.0.0.1:5173/a?b", &g), Ok(v4));
        assert_eq!(decide("http://localhost:5173/", &g), Ok(v4));
        assert_eq!(
            decide("http://[::1]:5173/", &g),
            Ok(SocketAddr::from((Ipv6Addr::LOCALHOST, 5173)))
        );
        assert_eq!(decide("http://127.0.0.1/", &g).map(|a| a.port()), Ok(80));
        for no in [
            "http://127.0.0.1:5432/",
            "https://127.0.0.1:5173/",
            "http://127.0.0.2:5173/",
            "http://0.0.0.0:5173/",
            "http://example.com:5173/",
            "http://localhost.example.com:5173/",
            "http://192.168.1.1:5173/",
            "http://user:pw@127.0.0.1:5173/",
            "file:///etc/passwd",
            "chrome://settings",
            "ws://127.0.0.1:5173/",
            "data:text/html,hi",
            "not an address",
        ] {
            assert!(decide(no, &g).is_err(), "{no}");
        }
    }
}
