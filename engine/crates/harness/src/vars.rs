//! What a person may say of how their program is started (decision 0016). By what a name contains,
//! so more is refused than need be: what is refused for nothing is named.

use axiom_spawn::Env;

use crate::talk::{loose, plain};

/// A variable a program is given.
#[derive(Clone, PartialEq, Eq)]
pub struct Var {
    pub name: String,
    pub value: String,
    pub secret: bool,
}
impl std::fmt::Debug for Var {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = if self.secret { "(secret)" } else { &self.value };
        write!(f, "{}={value}", self.name)
    }
}

/// Shorter secrets would be found in everything.
pub const SHORTEST_SECRET: usize = 8;

/// Set by Axiom, or where a program keeps what is its own.
const OURS: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "TMPDIR",
    "TMP",
    "TEMP",
    "TERM",
    "PWD",
    "OLDPWD",
    "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "CODEX_SQLITE_HOME",
    "IS_SANDBOX",
];
const PARTS: &[(&str, &[&str])] = &[
    (
        "sends what it says, or what signs it in, elsewhere",
        &[
            "BASE_URL",
            "BASEURL",
            "API_BASE",
            "ENDPOINT",
            "PROXY",
            "HEADERS",
            "EXTRA_BODY",
            "CA_CERT",
            "SSL_CERT",
            "CA_BUNDLE",
            "TLS_",
            "_URL",
            "_HOST",
            "GATEWAY",
        ],
    ),
    ("is a plan's login", &["OAUTH", "ANTHROPIC_AUTH_TOKEN", "ACCESS_TOKEN", "REFRESH_TOKEN"]),
    (
        "says that it is another program",
        &["ENTRYPOINT", "USER_AGENT", "ORIGINATOR", "AGENT_SDK", "CLIENT_APP", "MANAGED_BY"],
    ),
    (
        "names a program to be run",
        &["HELPER", "COMMAND", "SHELL", "ENV_FILE", "EXECUTABLE", "ASKPASS", "HOOK", "PLUGIN"],
    ),
    ("holds it back", &["SANDBOX", "PERMISSION", "APPROVAL", "TRUST", "SCRUB"]),
];

/// Why a variable may not be set, if it may not.
pub fn refused(name: &str) -> Option<&'static str> {
    let upper = name.to_ascii_uppercase();
    if !Env::may(name) {
        return Some("makes programs run or read something");
    }
    if OURS.contains(&upper.as_str()) {
        return Some("is set by Axiom");
    }
    if loose(name) {
        return Some("would let go of it");
    }
    let of = |parts: &[&str]| parts.iter().any(|p| upper.contains(p));
    PARTS.iter().find(|(_, parts)| of(parts)).map(|(why, _)| *why)
}

/// Whether a value under this name is taken for a secret whatever is said of it.
pub fn secret(name: &str) -> bool {
    let words: Vec<String> = name.split('_').map(str::to_ascii_uppercase).collect();
    let has = |w: &str| words.iter().any(|x| x == w);
    has("KEY")
        || has("APIKEY")
        || has("SECRET")
        || has("TOKEN")
        || has("PAT")
        || words.iter().any(|w| w.starts_with("PASS"))
}

/// Why an argument is never passed on, if it is not: it lets go, or looks like a secret.
pub fn argument(a: &str) -> Option<&'static str> {
    if a.is_empty() || a.len() > 1024 || a.chars().any(char::is_control) {
        return Some("is not an argument");
    }
    if loose(a) {
        return Some("would let go of it");
    }
    let flag = a.split('=').next().unwrap_or(a);
    let words: Vec<String> =
        flag.split(['-', '_']).map(str::to_ascii_lowercase).filter(|w| !w.is_empty()).collect();
    let named = |w: &str| words.iter().any(|x| x == w);
    let key = named("apikey") || (named("api") && named("key"));
    if flag.starts_with('-')
        && (key || named("token") || named("password") || named("secret") || named("passwd"))
    {
        return Some("is for a secret, and everybody on this computer can see arguments");
    }
    None
}

/// Of a program whose boundary Axiom relies on: those that cannot move it, each with one value.
pub(crate) fn only(
    extra: &[String],
    flags: &[&str],
    value: impl Fn(&str, &str) -> bool,
) -> Result<(), String> {
    let mut all = extra.iter();
    while let Some(a) = all.next() {
        let (flag, v) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_owned())),
            _ => (a.as_str(), None),
        };
        if !flags.contains(&flag) {
            return Err(a.clone());
        }
        let Some(v) = v.or_else(|| all.next().cloned()) else { return Err(a.clone()) };
        if !plain(&v) || !value(flag, &v) {
            return Err(format!("{flag} {v}"));
        }
    }
    Ok(())
}

/// Takes secrets out of what a program says.
#[derive(Clone, Default)]
pub(crate) struct Hide(Vec<Vec<u8>>);

impl Hide {
    pub fn of(vars: &[Var]) -> Self {
        let mut all: Vec<Vec<u8>> = Vec::new();
        for v in vars.iter().filter(|v| v.secret && !v.value.is_empty()) {
            all.push(v.value.clone().into_bytes());
            // As it is inside a string.
            if let Ok(s) = serde_json::to_string(&v.value) {
                all.push(s.as_bytes()[1..s.len() - 1].to_vec());
            }
        }
        // What is part of another goes last.
        all.sort_by_key(|s| std::cmp::Reverse(s.len()));
        all.dedup();
        Self(all)
    }

    pub fn out_of<'a>(&self, line: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
        let mut line = std::borrow::Cow::Borrowed(line);
        for s in &self.0 {
            if !line.windows(s.len()).any(|w| w == s.as_slice()) {
                continue;
            }
            let (mut out, mut at) = (Vec::with_capacity(line.len()), 0);
            while at < line.len() {
                if line[at..].starts_with(s) {
                    out.extend_from_slice(b"(secret)");
                    at += s.len();
                } else {
                    out.push(line[at]);
                    at += 1;
                }
            }
            line = std::borrow::Cow::Owned(out);
        }
        line
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn what_may_be_set() {
        for n in [
            "AWS_REGION",
            "AWS_PROFILE",
            "AWS_BEARER_TOKEN_BEDROCK",
            "AWS_SESSION_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_ACCESS_KEY_ID",
            "AWS_CONFIG_FILE",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLOUD_ML_REGION",
            "ANTHROPIC_VERTEX_PROJECT_ID",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "ANTHROPIC_API_KEY",
            "MAX_THINKING_TOKENS",
            "ANTHROPIC_MODEL",
            "OPENAI_API_KEY",
            "RUST_LOG",
        ] {
            assert_eq!(refused(n), None, "{n}");
        }
        for n in [
            "",
            "1A",
            "A B",
            "A=B",
            "é",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "NODE_OPTIONS",
            "node_options",
            "npm_config_prefix",
            "PYTHONPATH",
            "PYTHONSTARTUP",
            "PERL5OPT",
            "RUBYOPT",
            "JAVA_TOOL_OPTIONS",
            "GIT_SSH_COMMAND",
            "SSH_AUTH_SOCK",
            "BASH_ENV",
            "ENV",
            "SHELL",
            "PATH",
            "HOME",
            "Path",
            "CLAUDE_CONFIG_DIR",
            "CODEX_HOME",
            "ANTHROPIC_BASE_URL",
            "ANTHROPIC_BEDROCK_BASE_URL",
            "AWS_ENDPOINT_URL",
            "AWS_ENDPOINT_URL_S3",
            "HTTPS_PROXY",
            "https_proxy",
            "NO_PROXY",
            "ANTHROPIC_CUSTOM_HEADERS",
            "NODE_EXTRA_CA_CERTS",
            "SSL_CERT_FILE",
            "CODEX_CA_CERTIFICATE",
            "OLLAMA_HOST",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "ANTHROPIC_AUTH_TOKEN",
            "CODEX_ACCESS_TOKEN",
            "CLAUDE_CODE_ENTRYPOINT",
            "CODEX_INTERNAL_ORIGINATOR_OVERRIDE",
            "CLAUDE_AGENT_SDK_VERSION",
            "CLAUDE_CODE_SHELL_PREFIX",
            "CLAUDE_ENV_FILE",
            "GOOGLE_EXTERNAL_ACCOUNT_ALLOW_EXECUTABLES",
            "IS_SANDBOX",
            "GEMINI_SANDBOX",
            "X_AUTO_APPROVE",
            "CLAUDE_CODE_SUBPROCESS_ENV_SCRUB",
            "SOME_PERMISSION_MODE",
        ] {
            assert!(refused(n).is_some(), "{n}");
        }
    }

    #[test]
    fn what_is_a_secret_whatever_is_said() {
        for n in [
            "ANTHROPIC_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_ACCESS_KEY_ID",
            "AWS_SESSION_TOKEN",
            "AWS_BEARER_TOKEN_BEDROCK",
            "X_PASSWORD",
            "X_PASS",
            "GITHUB_PAT",
            "db_passwd",
            "APIKEY",
        ] {
            assert!(secret(n), "{n}");
        }
        for n in ["AWS_REGION", "MAX_THINKING_TOKENS", "GOOGLE_APPLICATION_CREDENTIALS", "KEYS"] {
            assert!(!secret(n), "{n}");
        }
    }

    #[test]
    fn what_is_never_an_argument() {
        for a in
            ["acp", "--acp", "--model=x", "--max-output-tokens", "-v", "a b", "--allow-read=/x"]
        {
            assert_eq!(argument(a), None, "{a}");
        }
        for a in [
            "",
            "--yolo",
            "--trust-all-tools",
            "a\nb",
            "a\0",
            "--api-key",
            "--api_key=x",
            "--apikey",
            "--token=abc",
            "--auth-token",
            "--password",
            "--client-secret=x",
        ] {
            assert!(argument(a).is_some(), "{a:?}");
        }
        assert!(argument(&"x".repeat(1025)).is_some());
    }

    #[test]
    fn a_secret_is_taken_out() {
        let var = |v: &str, secret| Var { name: "N".into(), value: v.into(), secret };
        let h = Hide::of(&[
            var("sk-abc12345", true),
            var("sk-abc12345-long", true),
            var("a\"quoted\" one", true),
            var("shown-value", false),
        ]);
        let out = |s: &str| String::from_utf8(h.out_of(s.as_bytes()).into_owned()).unwrap();
        assert_eq!(out("x sk-abc12345 y sk-abc12345"), "x (secret) y (secret)");
        assert_eq!(out("sk-abc12345-long"), "(secret)");
        assert_eq!(out(r#"{"t":"a \"a\\\"quoted\\\" one"}"#), r#"{"t":"a \"a\\\"quoted\\\" one"}"#);
        let said = serde_json::json!({"t": "has a\"quoted\" one in it"}).to_string();
        assert_eq!(out(&said), r#"{"t":"has (secret) in it"}"#);
        assert_eq!(out("shown-value sk-abc1234"), "shown-value sk-abc1234");
        assert!(!format!("{:?}", var("sk-abc12345", true)).contains("sk-"));
        assert!(format!("{:?}", var("shown", false)).contains("shown"));
    }
}
