//! What the HTTP adapters share: the base-URL rule, the client, and HTTP status → error.

use super::ProviderError;
use std::time::Duration;

/// HTTPS, or plain HTTP to a loopback host (local model servers). Returns the URL without a
/// trailing `/`.
pub(crate) fn check_base_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim_end_matches('/');
    let parsed = reqwest::Url::parse(trimmed).map_err(|e| format!("`{url}` is not a URL: {e}"))?;
    match parsed.scheme() {
        "https" => Ok(trimmed.to_string()),
        "http" if is_loopback(&parsed) => Ok(trimmed.to_string()),
        "http" => Err(format!(
            "`{url}` uses plain HTTP to a host that is not this machine; use HTTPS"
        )),
        other => Err(format!("`{url}` has unsupported scheme `{other}`")),
    }
}

fn is_loopback(u: &reqwest::Url) -> bool {
    match u.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

pub(crate) fn client() -> Result<reqwest::Client, ProviderError> {
    reqwest::Client::builder()
        .user_agent(concat!("axiom/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        // Between bytes of a streamed response: a model may think for a while before its first
        // token, so this is generous.
        .read_timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| ProviderError::Transport(e.to_string()))
}

/// Maps a non-success response. `body` is the response text (used for a message, never logged
/// with request headers).
pub(crate) fn status_error(
    status: reqwest::StatusCode,
    retry_after: Option<&str>,
    body: &str,
) -> ProviderError {
    let message = error_message(body).unwrap_or_else(|| format!("HTTP {}", status.as_u16()));
    match status.as_u16() {
        401 | 403 => ProviderError::Auth,
        429 => ProviderError::RateLimited {
            retry_after: retry_after
                .and_then(|s| s.trim().parse::<f64>().ok())
                .filter(|s| s.is_finite() && *s >= 0.0)
                .map(Duration::from_secs_f64),
        },
        413 => ProviderError::ContextTooLong,
        400 if looks_like_context_overflow(&message) => ProviderError::ContextTooLong,
        400..=499 => ProviderError::InvalidRequest(message),
        _ => ProviderError::Unavailable,
    }
}

/// `{"error":{"message":…}}` (Anthropic, OpenAI) or `{"error":"…"}` (Ollama, llama.cpp).
pub(crate) fn error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let e = v.get("error")?;
    e.get("message")
        .and_then(|m| m.as_str())
        .or_else(|| e.as_str())
        .map(String::from)
}

pub(crate) fn looks_like_context_overflow(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    m.contains("prompt is too long")
        || m.contains("context length")
        || m.contains("context window")
        || m.contains("context size")
        || m.contains("maximum context")
        || m.contains("too many tokens")
}

pub(crate) fn transport(e: reqwest::Error) -> ProviderError {
    // reqwest's Display includes the URL but never headers.
    ProviderError::Transport(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_rule() {
        assert_eq!(
            check_base_url("https://api.anthropic.com/").unwrap(),
            "https://api.anthropic.com"
        );
        assert!(check_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(check_base_url("http://localhost:8080").is_ok());
        assert!(check_base_url("http://[::1]:1234/v1").is_ok());
        assert!(check_base_url("http://example.com").is_err());
        assert!(check_base_url("ftp://example.com").is_err());
        assert!(check_base_url("not a url").is_err());
    }

    #[test]
    fn status_mapping() {
        use reqwest::StatusCode as S;
        assert_eq!(status_error(S::UNAUTHORIZED, None, ""), ProviderError::Auth);
        assert_eq!(
            status_error(S::TOO_MANY_REQUESTS, Some("2"), ""),
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(2))
            }
        );
        assert_eq!(
            status_error(
                S::BAD_REQUEST,
                None,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"prompt is too long: 210000 tokens > 200000 maximum"}}"#
            ),
            ProviderError::ContextTooLong
        );
        assert_eq!(
            status_error(S::BAD_REQUEST, None, r#"{"error":"model 'x' not found"}"#),
            ProviderError::InvalidRequest("model 'x' not found".into())
        );
        assert_eq!(
            status_error(S::from_u16(529).unwrap(), None, ""),
            ProviderError::Unavailable
        );
    }
}
