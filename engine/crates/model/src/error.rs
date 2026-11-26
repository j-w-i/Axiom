//! What goes wrong, in the few classes the loop decides by. Nothing is retried here.
//!
//! Every text in an error is the provider's or the system's: untrusted. Before one leaves the
//! transport it is sealed: cleared of the credential in case the server repeats it,
//! then cut to a length (not before, or half a secret could be left at the cut) and freed of control
//! characters.

use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;

use crate::secret::Secret;

/// Longest text taken from outside.
const TEXT: usize = 2048;
/// Longest identifier taken from outside.
const IDENT: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// The credential is missing, wrong, or not allowed this.
    Auth,
    /// Money: no credit, or a spending limit. Waiting does not help.
    Billing,
    RateLimit,
    /// The provider has too much to do.
    Overloaded,
    /// The provider will not take this request, however often it is sent.
    BadRequest,
    /// The request is too long for the model or the endpoint.
    ContextTooLong,
    /// The provider failed.
    Server,
    /// It was not reached, or did not answer in time.
    Network,
    /// The answer began and did not end.
    CutShort,
    /// What came is not what the protocol says, or larger than is taken.
    Protocol,
}

/// Whether the same request may be sent again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    Never,
    /// The provider did nothing with it.
    Safe,
    /// It may have worked on it, and charged. What it sent so far has to be thrown away first.
    Restart,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}", self.words())]
pub struct Error {
    pub class: Class,
    pub retry: Retry,
    pub message: String,
    /// Of the answer, if there was one.
    pub status: Option<u16>,
    /// What the provider calls it.
    pub code: Option<String>,
    /// For the provider's support.
    pub request_id: Option<String>,
    /// How long the provider asks to be left alone.
    pub retry_after: Option<Duration>,
}

impl Error {
    pub fn new(class: Class, retry: Retry, message: impl AsRef<str>) -> Self {
        Self {
            class,
            retry,
            message: message.as_ref().to_owned(),
            status: None,
            code: None,
            request_id: None,
            retry_after: None,
        }
    }
    pub(crate) fn protocol(message: impl AsRef<str>) -> Self {
        Self::new(Class::Protocol, Retry::Never, message)
    }
    pub(crate) fn cut_short(message: impl AsRef<str>) -> Self {
        Self::new(Class::CutShort, Retry::Restart, message)
    }

    /// The provider's own account: an answer's body, or an error in the stream, where `status` is
    /// none. The generic names decide only where no status does.
    pub(crate) fn provider(status: Option<u16>, body: &str) -> Self {
        Self::named(status, None, body)
    }

    /// The same, where the error's name is beside its body.
    pub(crate) fn named(status: Option<u16>, name: Option<&str>, body: &str) -> Self {
        let mut told = Told::read(body);
        told.code = name.filter(|n| !n.is_empty()).map(str::to_owned).or(told.code);
        let code = told.code.as_deref().unwrap_or("");
        // Kept as it came: of a part of it, it could not be said whether it is part of a secret.
        let shape = shape(code);
        let class = specific(code, &told)
            .or_else(|| specific(&shape, &told))
            .or_else(|| status.map(by_status))
            .or_else(|| generic(code).or_else(|| generic(&shape)))
            .or_else(|| told.status.map(by_status))
            .unwrap_or(Class::Server);
        let retry = match (class, status) {
            (Class::RateLimit | Class::Overloaded, Some(_)) => Retry::Safe,
            (Class::Server, Some(501 | 505)) => Retry::Never,
            (Class::RateLimit | Class::Overloaded | Class::Server, _) => Retry::Restart,
            _ => Retry::Never,
        };
        let message = match (told.message, status) {
            (Some(m), _) if !m.trim().is_empty() => m,
            (_, Some(s)) => format!("the provider answered {s} and said nothing more"),
            (_, None) => "the provider reported an error and said nothing more".into(),
        };
        Self {
            status,
            code: told.code,
            request_id: told.request_id,
            ..Self::new(class, retry, message)
        }
    }

    /// Fit to be shown and kept.
    pub(crate) fn sealed(mut self, secret: Option<&Secret>) -> Self {
        let seal = |text: &str, longest| match secret {
            Some(secret) => clean(&secret.strip(text), longest),
            None => clean(text, longest),
        };
        self.message = seal(&self.message, TEXT);
        self.code = self.code.map(|c| seal(&c, IDENT));
        self.request_id = self.request_id.map(|r| seal(&r, IDENT));
        self
    }

    fn words(&self) -> String {
        let class = match self.class {
            Class::Auth => "not authorised",
            Class::Billing => "billing",
            Class::RateLimit => "rate limited",
            Class::Overloaded => "overloaded",
            Class::BadRequest => "bad request",
            Class::ContextTooLong => "too long",
            Class::Server => "provider error",
            Class::Network => "network",
            Class::CutShort => "cut short",
            Class::Protocol => "protocol",
        };
        let status = self.status.map(|s| format!(" ({s})")).unwrap_or_default();
        format!("{class}{status}: {}", self.message)
    }
}

fn clean(text: &str, longest: usize) -> String {
    let mut end = text.len().min(longest);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out: String =
        text[..end].chars().map(|c| if c.is_control() && c != '\n' { ' ' } else { c }).collect();
    if end < text.len() {
        out.push('…');
    }
    out
}

/// What an error body says, in the shapes seen: Anthropic's, OpenAI's, Google's, OpenRouter's,
/// Amazon's, LM Studio's (a string), and those of common Python servers (`detail`, `message`).
#[derive(Default)]
struct Told {
    code: Option<String>,
    /// The status as the body has it, which is all there is of one in a stream.
    status: Option<u16>,
    message: Option<String>,
    request_id: Option<String>,
}
impl Told {
    fn read(body: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(body) else {
            return Self { message: Some(body.trim().to_owned()), ..Self::default() };
        };
        let string = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_owned);
        let error = match value.get("error") {
            Some(e) => e,
            None => &value,
        };
        let text = |e: &Value| match e {
            Value::String(s) => Some(s.clone()),
            e => string(e.get("message"))
                .or_else(|| string(e.get("Message")))
                .or_else(|| string(e.get("detail"))),
        };
        // A code says more than a type; Anthropic's is one level down and has only some. Google's
        // and OpenRouter's codes are numbers, and what they mean is elsewhere.
        let reason = || error.get("details")?.as_array()?.iter().find_map(|d| d.get("reason"));
        let code = string(error.pointer("/details/error_code"))
            .or_else(|| string(reason()))
            .or_else(|| string(error.pointer("/metadata/error_type")))
            .or_else(|| string(error.get("code")))
            .or_else(|| string(error.get("status")))
            .or_else(|| string(error.get("__type")))
            .or_else(|| string(error.get("type")).filter(|t| t != "error"));
        // The second is what a model answered Bedrock (API_runtime_ConverseStream#Errors).
        let number = |key| error.get(key).and_then(Value::as_u64);
        let number = number("code").or_else(|| number("originalStatusCode"));
        Self {
            code,
            status: number.and_then(|n| u16::try_from(n).ok()).filter(|n| (400..600).contains(n)),
            message: text(error).or_else(|| Some(value.to_string())),
            request_id: string(value.get("request_id")),
        }
    }
}

/// The name of an error's shape, as Amazon has servers and clients write it.
fn shape(name: &str) -> String {
    let name = name.split(':').next().unwrap_or_default();
    let name = name.rsplit('#').next().unwrap_or_default().trim();
    // In a stream it begins small.
    let mut chars = name.chars();
    chars.next().map(|c| c.to_ascii_uppercase()).into_iter().chain(chars).collect()
}

fn specific(code: &str, told: &Told) -> Option<Class> {
    let message = told.message.as_deref().unwrap_or("").to_ascii_lowercase();
    match code {
        "billing_error"
        | "insufficient_quota"
        | "enforced_spend_limit_reached"
        | "credit_balance_exhausted"
        | "organization_spend_limit_exceeded"
        | "project_spend_limit_exceeded"
        | "organization_usage_limit_exceeded"
        | "payment_required"
        // A plan's own limit: waiting is its person's to decide, and nothing is tried again for it.
        | "subscription_sharing_usage_limit_exceeded" => return Some(Class::Billing),
        "subscription_sharing_user_not_eligible"
        | "subscription_sharing_invalid_user"
        | "chatpass_v2_scope_not_authorized"
        | "chatpass_v2_invalid_authorization_context" => return Some(Class::Auth),
        "subscription_sharing_usage_unavailable" | "subscription_sharing_user_unavailable" => {
            return Some(Class::Overloaded);
        }
        "subscription_sharing_unsupported_capability"
        | "subscription_sharing_route_not_supported" => return Some(Class::BadRequest),
        "context_length_exceeded" | "request_too_large" => return Some(Class::ContextTooLong),
        "overloaded_error" | "server_is_overloaded" | "provider_overloaded" => {
            return Some(Class::Overloaded);
        }
        // Google answers 400 to this and OpenRouter 403 to these.
        "API_KEY_INVALID" => return Some(Class::Auth),
        "content_policy_violation" | "refusal" => return Some(Class::BadRequest),
        // Bedrock's (APIReference/CommonErrors and userguide/troubleshooting-api-error-codes), where
        // the status alone says less, or differs from page to page.
        "ThrottlingException" => return Some(Class::RateLimit),
        "ModelNotReadyException" => return Some(Class::Overloaded),
        "NotAuthorized" | "RequestExpired" | "IncompleteSignature" => return Some(Class::Auth),
        "ModelErrorException" | "ModelStreamErrorException" if !is_too_long(&message) => {
            return Some(told.status.map_or(Class::Server, by_status));
        }
        _ => {}
    }
    // Only words tell these from other bad requests.
    if message.starts_with("you have reached your specified") {
        return Some(Class::Billing);
    }
    is_too_long(&message).then_some(Class::ContextTooLong)
}

fn is_too_long(message: &str) -> bool {
    // The last two are Gemini's and Bedrock's, from memory: their documentation has no words for it.
    let too_long = [
        "prompt is too long",
        "maximum context length",
        "context length",
        "context window",
        "input token count",
        "input is too long",
    ];
    let vague = ["context length", "context window", "input token count"];
    let exceeded = ["exceed", "greater than", "too long", "larger than", "overflow", "not enough"];
    too_long.iter().any(|w| {
        message.contains(w) && (!vague.contains(w) || exceeded.iter().any(|e| message.contains(e)))
    })
}

fn by_status(status: u16) -> Class {
    match status {
        401 | 403 | 407 => Class::Auth,
        402 => Class::Billing,
        413 => Class::ContextTooLong,
        429 => Class::RateLimit,
        503 | 529 => Class::Overloaded,
        408 | 500..=599 => Class::Server,
        400..=499 => Class::BadRequest,
        _ => Class::Protocol,
    }
}

fn generic(code: &str) -> Option<Class> {
    Some(match code {
        "authentication_error"
        | "permission_error"
        | "invalid_api_key"
        | "authentication"
        | "permission_denied"
        | "UNAUTHENTICATED"
        | "AccessDeniedException"
        | "ExpiredTokenException"
        | "UnrecognizedClientException"
        | "PERMISSION_DENIED" => Class::Auth,
        "rate_limit_error" | "rate_limit_exceeded" | "slow_down" | "RESOURCE_EXHAUSTED" => {
            Class::RateLimit
        }
        "UNAVAILABLE" | "ServiceUnavailableException" => Class::Overloaded,
        "invalid_request_error"
        | "not_found_error"
        | "conflict_error"
        | "model_not_found"
        | "invalid_request"
        | "invalid_prompt"
        | "data_residency_mismatch"
        | "bio_policy"
        | "misalignment_policy_violation"
        | "INVALID_ARGUMENT"
        | "FAILED_PRECONDITION"
        | "NOT_FOUND"
        | "ValidationException"
        | "ResourceNotFoundException" => Class::BadRequest,
        _ => return None,
    })
}

/// Seconds, as a number or until a date. `now` is seconds since 1970.
pub(crate) fn retry_after(value: &str, now: u64) -> Option<Duration> {
    let value = value.trim();
    if let Ok(seconds) = value.parse::<f64>() {
        return Duration::try_from_secs_f64(seconds).ok();
    }
    http_date(value).map(|at| Duration::from_secs(at.saturating_sub(now)))
}

/// `Sun, 06 Nov 1994 08:49:37 GMT`, the only form that may be sent.
fn http_date(text: &str) -> Option<u64> {
    let mut parts = text.split_once(", ")?.1.split(' ');
    let day: u64 = parts.next()?.parse().ok().filter(|d| (1..=31).contains(d))?;
    let months =
        ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let name = parts.next()?;
    let month = months.iter().position(|m| *m == name)? as u64 + 1;
    let year: u64 = parts.next()?.parse().ok().filter(|y| (1970..=9999).contains(y))?;
    let mut clock = parts.next()?.splitn(3, ':').map(|p| p.parse::<u64>().ok());
    let (h, m, s) = (clock.next()??, clock.next()??, clock.next()??);
    if parts.next() != Some("GMT") || parts.next().is_some() || h > 23 || m > 59 || s > 60 {
        return None;
    }
    // Howard Hinnant's days_from_civil, for years of this era; March is month 0.
    let y = if month <= 2 { year - 1 } else { year };
    let (era, yoe) = (y / 400, y % 400);
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe).checked_sub(719_468)?;
    Some(days * 86_400 + h * 3600 + m * 60 + s)
}

/// Whether a JSON body is an account of an error, whatever its status.
pub(crate) fn is_error(body: &str) -> bool {
    #[derive(Deserialize)]
    struct Probe {
        error: Option<serde::de::IgnoredAny>,
    }
    serde_json::from_str::<Probe>(body).is_ok_and(|p| p.error.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOV_6_1994: u64 = 784_111_777;

    #[test]
    fn dates_and_seconds() {
        assert_eq!(retry_after("7", 0), Some(Duration::from_secs(7)));
        assert_eq!(retry_after(" 0.25 ", 0), Some(Duration::from_millis(250)));
        let date = "Sun, 06 Nov 1994 08:49:37 GMT";
        assert_eq!(retry_after(date, NOV_6_1994 - 90), Some(Duration::from_secs(90)));
        assert_eq!(retry_after(date, NOV_6_1994 + 90), Some(Duration::ZERO));
        for (text, at) in [
            ("Thu, 01 Jan 1970 00:00:00 GMT", 0),
            ("Tue, 29 Feb 2000 23:59:59 GMT", 951_868_799),
            ("Wed, 01 Mar 2000 00:00:00 GMT", 951_868_800),
            ("Wed, 31 Dec 2025 00:00:00 GMT", 1_767_139_200),
            ("Mon, 01 Jan 2029 00:00:00 GMT", 1_861_920_000),
        ] {
            assert_eq!(http_date(text), Some(at), "{text}");
        }
        for bad in [
            "",
            "soon",
            "-1",
            "NaN",
            "inf",
            "1e300",
            "Sun, 06 Nov 1994 08:49:37",
            "Sun, 06 Nov 1994 08:49:37 EST",
            "Sun, 06 Nov 1994 08:49:37 GMT x",
            "Sun, 06 Nov 1994 24:00:00 GMT",
            "Sun, 06 Nov 1994 08:60:00 GMT",
            "Sun, 06 Nov 1994 08:49 GMT",
            "Sun, 32 Nov 1994 08:49:37 GMT",
            "Sun, 00 Nov 1994 08:49:37 GMT",
            "Sun, 06 Non 1994 08:49:37 GMT",
            "Sun, 06 Nov 1969 08:49:37 GMT",
            "Sun, 06 Nov 99999999999999999 08:49:37 GMT",
        ] {
            assert_eq!(retry_after(bad, 0), None, "{bad}");
        }
    }

    #[test]
    fn text_from_outside_is_bounded_and_plain() {
        let seal = |text: &str| Error::protocol(text).sealed(None).message;
        assert_eq!(seal("a\u{1b}[31mb\r\nc\0"), "a [31mb \nc ");
        let long = seal(&"é".repeat(TEXT));
        assert!(long.len() <= TEXT + '…'.len_utf8() && long.ends_with("é…"));
        assert_eq!(seal(&"é".repeat(TEXT / 2)), "é".repeat(TEXT / 2));
        assert_eq!(seal(&format!("x{}", "é".repeat(TEXT / 2))).chars().last(), Some('…'));
        let e = Error::provider(
            Some(400),
            &format!(r#"{{"error":{{"code":"{0}"}},"request_id":"{0}"}}"#, "y".repeat(500)),
        )
        .sealed(None);
        assert!(e.code.unwrap().len() < 140 && e.request_id.unwrap().len() < 140);
    }

    #[test]
    fn a_secret_is_gone_before_the_cut() {
        let secret = Secret::new("canary-0123456789".into());
        for before in TEXT - 20..=TEXT + 1 {
            let body = format!("{}canary-0123456789 and more", "x".repeat(before));
            let e = Error::provider(Some(500), &body).sealed(Some(&secret));
            assert!(!format!("{e:?}").contains("cana"), "{before}");
        }
        let body = r#"{"error":{"message":"\u0063anary-0123456789","code":"canary-0123456789"},"request_id":"canary-0123456789"}"#;
        let e = Error::provider(Some(500), body).sealed(Some(&secret));
        assert!(!format!("{e:?} {e}").contains("anary"));
        assert_eq!(e.message, "[secret]");
    }

    #[test]
    fn classes() {
        use {Class::*, Retry::*};
        let anthropic = |kind: &str, message: &str| {
            format!(
                r#"{{"type":"error","error":{{"type":"{kind}","message":"{message}"}},"request_id":"req_1"}}"#
            )
        };
        let openai = |kind: &str, code: &str| {
            format!(r#"{{"error":{{"message":"m","type":"{kind}","param":null,"code":"{code}"}}}}"#)
        };
        for (status, body, class, retry) in [
            (Some(400), anthropic("invalid_request_error", "bad"), BadRequest, Never),
            (Some(400), anthropic("invalid_request_error", "prompt is too long: 2 tokens > 1 maximum"), ContextTooLong, Never),
            (Some(400), anthropic("invalid_request_error", "You have reached your specified API usage limits"), Billing, Never),
            (Some(401), anthropic("authentication_error", "invalid x-api-key"), Auth, Never),
            (Some(402), anthropic("billing_error", "pay"), Billing, Never),
            (Some(402), String::new(), Billing, Never),
            (Some(403), anthropic("permission_error", "no"), Auth, Never),
            (Some(404), anthropic("not_found_error", "model: x"), BadRequest, Never),
            (Some(409), anthropic("conflict_error", "x"), BadRequest, Never),
            (Some(413), anthropic("request_too_large", "x"), ContextTooLong, Never),
            (Some(413), String::new(), ContextTooLong, Never),
            (Some(429), anthropic("rate_limit_error", "slow"), RateLimit, Safe),
            (Some(429), r#"{"type":"error","error":{"type":"rate_limit_error","message":"m","details":{"error_code":"enforced_spend_limit_reached"}}}"#.into(), Billing, Never),
            (Some(429), r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota","code":"insufficient_quota"}}"#.into(), Billing, Never),
            (Some(400), r#"{"error":{"message":"This model's maximum context length is 8 tokens","type":"invalid_request_error","code":"context_length_exceeded"}}"#.into(), ContextTooLong, Never),
            (Some(400), r#"{"error":"maximum context length is 8"}"#.into(), ContextTooLong, Never),
            (Some(400), r#"{"error":"the context window is a fine thing"}"#.into(), BadRequest, Never),
            (Some(401), r#"{"error":{"message":"k","type":"invalid_request_error","code":"invalid_api_key"}}"#.into(), Auth, Never),
            // The name is the same for many; the status says which.
            (Some(401), anthropic("invalid_request_error", "x"), Auth, Never),
            (Some(500), anthropic("invalid_request_error", "x"), Server, Restart),
            (Some(408), String::new(), Server, Restart),
            (Some(500), anthropic("api_error", "x"), Server, Restart),
            (Some(501), String::new(), Server, Never),
            (Some(504), anthropic("timeout_error", "x"), Server, Restart),
            (Some(503), "<html>busy</html>".into(), Overloaded, Safe),
            (Some(529), anthropic("overloaded_error", "Overloaded"), Overloaded, Safe),
            (Some(500), anthropic("overloaded_error", "Overloaded"), Overloaded, Safe),
            (Some(418), String::new(), BadRequest, Never),
            (Some(304), String::new(), Protocol, Never),
            // developers.openai.com/api/docs/guides/error-codes: the type may be the same for all.
            (Some(429), openai("insufficient_quota", "credit_balance_exhausted"), Billing, Never),
            (Some(429), openai("insufficient_quota", "organization_spend_limit_exceeded"), Billing, Never),
            (Some(429), openai("x", "project_spend_limit_exceeded"), Billing, Never),
            (Some(429), openai("x", "organization_usage_limit_exceeded"), Billing, Never),
            (Some(429), openai("rate_limit_error", "slow_down"), RateLimit, Safe),
            (Some(503), openai("service_unavailable_error", "server_is_overloaded"), Overloaded, Safe),
            // A ChatGPT plan's (siwc/token-sharing-open-source): a limit is never tried again.
            (Some(429), openai("x", "subscription_sharing_usage_limit_exceeded"), Billing, Never),
            (Some(403), openai("x", "subscription_sharing_user_not_eligible"), Auth, Never),
            (Some(400), openai("x", "subscription_sharing_invalid_user"), Auth, Never),
            (Some(400), openai("x", "chatpass_v2_scope_not_authorized"), Auth, Never),
            (Some(400), openai("x", "chatpass_v2_invalid_authorization_context"), Auth, Never),
            (Some(503), openai("x", "subscription_sharing_usage_unavailable"), Overloaded, Safe),
            (Some(503), openai("x", "subscription_sharing_user_unavailable"), Overloaded, Safe),
            (Some(400), openai("x", "subscription_sharing_unsupported_capability"), BadRequest, Never),
            (Some(404), openai("x", "subscription_sharing_route_not_supported"), BadRequest, Never),
            (Some(400), openai("invalid_request_error", "invalid_json_schema"), BadRequest, Never),
            (Some(400), r#"{"error":{"code":400,"message":"The input token count (9) exceeds the maximum number of tokens allowed (8).","status":"INVALID_ARGUMENT"}}"#.into(), ContextTooLong, Never),
            (Some(400), r#"{"error":{"code":400,"message":"the input token count is a fine thing","status":"INVALID_ARGUMENT"}}"#.into(), BadRequest, Never),
            (Some(400), r#"{"error":{"code":400,"message":"m","status":"INVALID_ARGUMENT","details":[{"@type":"x"},{"reason":"API_KEY_INVALID"}]}}"#.into(), Auth, Never),
            // The answer's status, not the one in the body; and in a stream the body's, if it is one.
            (Some(500), r#"{"error":{"code":429,"message":"m"}}"#.into(), Server, Restart),
            (None, r#"{"error":{"code":429,"message":"m"}}"#.into(), RateLimit, Restart),
            (None, r#"{"error":{"code":502,"message":"m"}}"#.into(), Server, Restart),
            (None, r#"{"error":{"code":401,"message":"m"}}"#.into(), Auth, Never),
            (None, r#"{"error":{"code":70936,"message":"m"}}"#.into(), Server, Restart),
            (None, r#"{"error":{"code":600,"message":"m"}}"#.into(), Server, Restart),
            (None, r#"{"error":{"code":399,"message":"m"}}"#.into(), Server, Restart),
            (None, anthropic("overloaded_error", "Overloaded"), Overloaded, Restart),
            (None, anthropic("rate_limit_error", "x"), RateLimit, Restart),
            (None, anthropic("invalid_request_error", "x"), BadRequest, Never),
            (None, anthropic("authentication_error", "x"), Auth, Never),
            (None, anthropic("something_new", "x"), Server, Restart),
            (None, "{}".into(), Server, Restart),
        ] {
            let e = Error::provider(status, &body);
            assert_eq!((e.class, e.retry, e.status), (class, retry, status), "{status:?} {body}");
        }
    }

    #[test]
    fn what_the_provider_said_is_kept() {
        let e = Error::provider(
            Some(404),
            r#"{"type":"error","error":{"type":"not_found_error","message":"model: x"},"request_id":"req_1"}"#,
        );
        assert_eq!(e.message, "model: x");
        assert_eq!(e.code.as_deref(), Some("not_found_error"));
        assert_eq!(e.request_id.as_deref(), Some("req_1"));
        assert_eq!(e.to_string(), "bad request (404): model: x");
        assert_eq!(Error::provider(Some(500), r#"{"detail":"d"}"#).message, "d");
        assert_eq!(Error::provider(Some(500), r#"{"message":"m"}"#).message, "m");
        assert_eq!(Error::provider(Some(500), r#"{"error":"e"}"#).message, "e");
        assert_eq!(Error::provider(Some(500), " plain ").message, "plain");
        assert_eq!(Error::provider(Some(500), r#"{"odd":1}"#).message, r#"{"odd":1}"#);
        assert_eq!(
            Error::provider(Some(500), "").message,
            "the provider answered 500 and said nothing more"
        );
        assert!(is_error(r#"{"error":{}}"#) && is_error(r#"{"error":"x"}"#));
        assert!(!is_error(r#"{"error":null}"#) && !is_error("[]") && !is_error("x"));
    }
}
