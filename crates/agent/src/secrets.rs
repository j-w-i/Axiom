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
        || n == SSH_AGENT_VAR
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

/// A short line for an approval card: how many variables are withheld, and which. A withheld
/// `SSH_AUTH_SOCK` is always named, first, with what it breaks and how to pass it: a `git push`
/// over SSH that fails for want of it is explained on the card (review rev3).
pub fn describe(withheld: &[String]) -> Option<String> {
    if withheld.is_empty() {
        return None;
    }
    let is_ssh = |n: &str| n.eq_ignore_ascii_case(SSH_AGENT_VAR);
    let ssh = withheld.iter().find(|n| is_ssh(n));
    let mut names: Vec<&str> = ssh
        .into_iter()
        .chain(withheld.iter().filter(|n| !is_ssh(n)))
        .map(String::as_str)
        .take(8)
        .collect();
    if withheld.len() > 8 {
        names.push("…");
    }
    let mut line = format!(
        "environment: {} secret variable{} withheld ({})",
        withheld.len(),
        if withheld.len() == 1 { "" } else { "s" },
        names.join(", ")
    );
    if let Some(name) = ssh {
        line.push_str(&format!(
            "; without {name}, git over SSH (push, fetch, signing) cannot use your SSH agent: \
             add it to settings agent.passEnv to pass it"
        ));
    }
    Some(line)
}

/// The SSH agent's socket: withheld by default ([`looks_secret`]).
const SSH_AGENT_VAR: &str = "SSH_AUTH_SOCK";

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

    /// Review rev3: a withheld SSH agent socket is named on the card, however many other names
    /// are withheld, with the setting that passes it.
    #[test]
    fn a_withheld_ssh_agent_is_named_with_how_to_pass_it() {
        let mut env: Vec<String> = (0..10).map(|i| format!("A{i}_TOKEN")).collect();
        env.push("SSH_AUTH_SOCK".into());
        let w = withheld(env.iter().map(String::as_str), &[], &[]);
        let line = describe(&w).unwrap();
        assert!(
            line.starts_with("environment: 11 secret variables withheld (SSH_AUTH_SOCK, A0_TOKEN"),
            "{line}"
        );
        assert!(line.contains("git over SSH"), "{line}");
        assert!(line.contains("agent.passEnv"), "{line}");
        // Passed: neither withheld nor mentioned.
        let w = withheld(
            env.iter().map(String::as_str),
            &[],
            &["ssh_auth_sock".into()],
        );
        assert!(!describe(&w).unwrap().contains("SSH"));
    }
}
