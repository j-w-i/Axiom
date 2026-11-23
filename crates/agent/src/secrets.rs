//! Which of the daemon's environment variables a process started for an agent does not get
//! (`docs/architecture/security.md` §Processes; AUD-010).
//!
//! - Every process an agent starts through a tool (`task.run`, `build.run`, `acp.terminal`,
//!   `preview.start`) loses the variables whose names look like secrets ([`looks_secret`]) and
//!   the API-key variables model providers name (`apiKeyEnv`), unless the person lists the name
//!   in settings `agent.passEnv`.
//! - External agents and MCP servers keep their environment (they sign in with it), except the
//!   `apiKeyEnv` variables: Axiom's own model keys never reach a child it did not start for
//!   that provider. A provider or server that needs one gets it through its own `env`.

/// `*_TOKEN`, `*_KEY`, `*_SECRET`, `*_CREDENTIALS`, `*_PAT` (a personal access token),
/// `*_AUTH`, `*PASSWORD*` (and `PASSWD`), `*DATABASE_URL` (it carries the password) and
/// `SSH_AUTH_SOCK` (the SSH agent signs for whoever can reach it), compared without regard to
/// case. Names only: this is hygiene, not a boundary (`security.md` §Processes; review N7).
pub fn looks_secret(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    n.ends_with("_TOKEN")
        || n.ends_with("_KEY")
        || n.ends_with("_SECRET")
        || n.ends_with("_CREDENTIALS")
        || n.ends_with("_PAT")
        || n.ends_with("_AUTH")
        || n.ends_with("DATABASE_URL")
        || n == "SSH_AUTH_SOCK"
        || n.contains("PASSWORD")
        || n.contains("PASSWD")
}

/// The names in `env` (the daemon's variables) an agent-started process must not get: secret-
/// looking ones and the providers' key variables, minus those the person allowed. Sorted.
pub fn withheld<'a>(
    env: impl IntoIterator<Item = &'a str>,
    key_vars: &[String],
    allowed: &[String],
) -> Vec<String> {
    let allowed = |n: &str| allowed.iter().any(|a| a.eq_ignore_ascii_case(n));
    let mut v: Vec<String> = env
        .into_iter()
        .filter(|n| (looks_secret(n) || key_vars.iter().any(|k| k == n)) && !allowed(n))
        .map(String::from)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// The names of the daemon's own environment (valid Unicode only: others cannot be secrets a
/// provider names, and are passed through).
pub fn daemon_names() -> Vec<String> {
    std::env::vars_os()
        .filter_map(|(k, _)| k.into_string().ok())
        .collect()
}

/// A short line for an approval card: how many variables are withheld, and which.
pub fn describe(withheld: &[String]) -> Option<String> {
    if withheld.is_empty() {
        return None;
    }
    let mut names: Vec<&str> = withheld.iter().map(String::as_str).take(8).collect();
    if withheld.len() > 8 {
        names.push("…");
    }
    Some(format!(
        "environment: {} secret variable{} withheld ({})",
        withheld.len(),
        if withheld.len() == 1 { "" } else { "s" },
        names.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_names() {
        for n in [
            "GITHUB_TOKEN",
            "gh_token",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "OPENAI_API_KEY",
            "DB_PASSWORD",
            "MYSQL_PWD_PASSWD",
            "CLIENT_SECRET",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "GITHUB_PAT",
            "gitlab_pat",
            "NPM_CONFIG__AUTH",
            "DOCKER_AUTH",
            "DATABASE_URL",
            "database_url",
            "TEST_DATABASE_URL",
            "SSH_AUTH_SOCK",
        ] {
            assert!(looks_secret(n), "{n}");
        }
        for n in [
            "PATH",
            "HOME",
            "LANG",
            "TOKENIZERS_PARALLELISM",
            "KEYMAP",
            "RUSTUP_HOME",
            "XAUTHORITY",
            "PATH_PATTERN",
            "AUTHOR",
            "DATABASE_URL_TIMEOUT",
            "SSH_AGENT_PID",
        ] {
            assert!(!looks_secret(n), "{n}");
        }
    }

    #[test]
    fn withheld_names_respect_the_allow_list() {
        let env = ["PATH", "GITHUB_TOKEN", "MY_LLM", "NPM_TOKEN"];
        let w = withheld(env, &["MY_LLM".into()], &["npm_token".into()]);
        assert_eq!(w, ["GITHUB_TOKEN", "MY_LLM"]);
        assert_eq!(
            describe(&w).unwrap(),
            "environment: 2 secret variables withheld (GITHUB_TOKEN, MY_LLM)"
        );
        assert_eq!(describe(&[]), None);
    }
}
