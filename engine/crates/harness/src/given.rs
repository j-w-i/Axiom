//! An account of Axiom's, given to a harness for one start (decision 0023): a key at a provider, or a
//! server on this computer, and one of its models. What the harness is told of it is written by the
//! host into a folder of Axiom's own for that harness, or given in its environment; the key only ever
//! in its environment, under a name the harness is told to read, never in an argument or a file.
//!
//! How each is told is as its own documentation and code say it (`research/harnesses-for-accounts.md`,
//! read 2026-11-27). None of them has been started for real here.

use axiom_policy::{Access, Place};
use serde_json::{Value, json};

use crate::Var;

/// Where an account's models are served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Anthropic,
    OpenAi,
    OpenRouter,
    Gemini,
    DeepSeek,
    /// A server on this computer that speaks OpenAI's chat completions, at this address.
    Local(String),
}

impl Route {
    /// From what Axiom calls the provider. Those no harness here can run are none (Bedrock, a plan).
    pub fn of(provider: &str, url: Option<&str>) -> Option<Self> {
        Some(match provider {
            "anthropic" => Self::Anthropic,
            "openai" => Self::OpenAi,
            "openrouter" => Self::OpenRouter,
            "gemini" => Self::Gemini,
            "deepseek" => Self::DeepSeek,
            "lm-studio" => Self::Local(url.unwrap_or("http://127.0.0.1:1234/v1").to_owned()),
            "openai-compatible" => Self::Local(url?.to_owned()),
            _ => return None,
        })
    }

    /// The variable its key is read from: the one each of these harnesses reads by itself for the
    /// provider, so that nothing else has to say it.
    fn key_var(&self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Local(_) => LOCAL_KEY,
        }
    }

    /// Its name in pi-ai's catalogue (dsh and Pi) and in OpenCode's.
    fn id(&self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::OpenRouter => "openrouter",
            Self::Gemini => "google",
            Self::DeepSeek => "deepseek",
            Self::Local(_) => LOCAL,
        }
    }
}

/// What a server on this computer is called in a harness's configuration.
const LOCAL: &str = "axiom-local";
/// A server on this computer is given this, also without a key: dsh and Pi want a variable named.
const LOCAL_KEY: &str = "AXIOM_LOCAL_KEY";

/// One account and one of its models.
#[derive(Clone, PartialEq, Eq)]
pub struct Account {
    pub route: Route,
    pub model: String,
    pub key: Option<String>,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = if self.key.is_some() { "(secret)" } else { "none" };
        write!(f, "{:?} {} key {key}", self.route, self.model)
    }
}

/// Which of the harnesses that run accounts' models this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runs {
    /// DeepSeek Harness, `dsh --profile acp`.
    Dsh,
    /// `opencode acp`.
    OpenCode,
    /// `pi --mode rpc`.
    Pi,
}

/// What it is started with for one account: arguments after the program's own, variables Axiom sets,
/// files it writes first (by their place), and the key.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Given {
    pub args: Vec<String>,
    /// Axiom's own, not a person's: set as they are.
    pub vars: Vec<(String, String)>,
    pub files: Vec<(Place, String)>,
    pub key: Option<Var>,
}

fn place(home: &Place, name: &str) -> Result<Place, String> {
    home.join(name).map_err(|e| e.to_string())
}

fn plain(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 500
        && model.chars().all(|c| c.is_ascii_graphic() && !matches!(c, '"' | '\\' | '`' | '$'))
}

impl Runs {
    /// The providers it can run, as Axiom calls them.
    pub fn runs(&self, r: &Route) -> bool {
        // pi-ai's catalogue (dsh, Pi) has each, and OpenCode's; a server here only by an address.
        match (self, r) {
            (_, Route::Local(u)) => u.starts_with("http://") || u.starts_with("https://"),
            (Self::Dsh | Self::OpenCode | Self::Pi, _) => true,
        }
    }

    /// How it is started for `a`, its own state in `home`, which is Axiom's and nobody else's. `agent`
    /// names what is this attempt's alone.
    pub fn give(
        &self,
        a: &Account,
        access: Access,
        home: &Place,
        agent: &str,
    ) -> Result<Given, String> {
        if !self.runs(&a.route) {
            return Err("it cannot run this provider's models".into());
        }
        if !plain(&a.model) {
            return Err(format!("{:?} is not a model's name it could be told", a.model));
        }
        let key = match (&a.key, &a.route) {
            (Some(k), r) => Some(Var { name: r.key_var().into(), value: k.clone(), secret: true }),
            // What a server on this computer is sent; it asks for nothing.
            (None, Route::Local(_)) => {
                Some(Var { name: LOCAL_KEY.into(), value: "none".into(), secret: false })
            }
            (None, _) => return Err("this account has no key".into()),
        };
        let mut g = match self {
            Self::Dsh => dsh(a, access, home, agent)?,
            Self::OpenCode => opencode(a, access, home)?,
            Self::Pi => pi(a, home, agent)?,
        };
        g.key = key;
        Ok(g)
    }
}

/// A YAML scalar: JSON's string is one.
fn y(s: &str) -> String {
    Value::String(s.to_owned()).to_string()
}

/// A patch of its own after the profile's (`--patch`): the account's route, and the ACP profile on it.
fn dsh(a: &Account, access: Access, home: &Place, agent: &str) -> Result<Given, String> {
    let route = a.route.id();
    let mut p = format!(
        "- id: llm-pi-ai\n  config:\n    providers:\n      {route}:\n        apiKeyEnv: {}\n",
        y(a.route.key_var())
    );
    if let Route::Local(base) = &a.route {
        p += &format!(
            "        api: openai-completions\n        baseURL: {}\n        models:\n          - id: {}\n",
            y(base),
            y(&a.model)
        );
    }
    p += &format!("- id: acp\n  config:\n    provider: {}\n    model: {}\n", y(route), y(&a.model));
    let file = place(&place(home, "agents")?, &format!("{agent}.patch.yml"))?;
    // `read-only` and `workspace-write` ask before what their sandbox does not allow; the third
    // neither holds back nor asks, and is never given.
    let mode = match access {
        Access::Read => "read-only",
        Access::Ask | Access::Work => "workspace-write",
    };
    Ok(Given {
        args: vec!["--patch".into(), file.to_string()],
        vars: vec![
            ("DSH_HOME".into(), home.to_string()),
            // The person's own instructions and skills are not this agent's.
            ("DSH_AGENTS_HOME".into(), place(home, "agents-home")?.to_string()),
            ("DSH_PERMISSION_MODE".into(), mode.into()),
            ("DSH_TELEMETRY_DISABLED".into(), "1".into()),
        ],
        files: vec![(file, p)],
        key: None,
    })
}

/// Its whole configuration in its environment, applied after any other; its state in folders of
/// Axiom's own.
fn opencode(a: &Account, access: Access, home: &Place) -> Result<Given, String> {
    let id = a.route.id();
    let mut provider = json!({ "options": { "apiKey": format!("{{env:{}}}", a.route.key_var()) } });
    if let Route::Local(base) = &a.route {
        provider = json!({
            "npm": "@ai-sdk/openai-compatible",
            "name": "Axiom",
            "options": { "baseURL": base, "apiKey": format!("{{env:{LOCAL_KEY}}}") },
            "models": { a.model.as_str(): {} },
        });
    }
    // Most of what it may it does without asking unless it is told to ask.
    let permission = match access {
        Access::Read => {
            json!({"*": "deny", "read": "allow", "glob": "allow", "grep": "allow", "list": "allow"})
        }
        Access::Ask => json!({"*": "ask"}),
        Access::Work => json!({"*": "allow", "external_directory": "ask", "doom_loop": "ask"}),
    };
    let config = json!({
        "$schema": "https://opencode.ai/config.json",
        "model": format!("{id}/{}", a.model),
        "enabled_providers": [id],
        "provider": { id: provider },
        "permission": permission,
        "autoupdate": false,
        "share": "disabled",
    });
    let dir = |n: &str| place(home, n).map(|p| p.to_string());
    Ok(Given {
        args: vec![],
        vars: vec![
            ("OPENCODE_CONFIG_CONTENT".into(), config.to_string()),
            ("XDG_CONFIG_HOME".into(), dir("config")?),
            ("XDG_DATA_HOME".into(), dir("data")?),
            ("XDG_CACHE_HOME".into(), dir("cache")?),
            ("XDG_STATE_HOME".into(), dir("state")?),
            // A repository's own configuration names servers and programs (SI-5).
            ("OPENCODE_DISABLE_PROJECT_CONFIG".into(), "1".into()),
            ("OPENCODE_DISABLE_AUTOUPDATE".into(), "1".into()),
            ("OPENCODE_DISABLE_MODELS_FETCH".into(), "1".into()),
            ("OPENCODE_DISABLE_LSP_DOWNLOAD".into(), "1".into()),
            ("OPENCODE_DISABLE_DEFAULT_PLUGINS".into(), "1".into()),
            ("OPENCODE_DISABLE_CLAUDE_CODE".into(), "1".into()),
        ],
        files: vec![],
        key: None,
    })
}

/// Its folder of Axiom's own for this agent; a server on this computer in its `models.json`.
fn pi(a: &Account, home: &Place, agent: &str) -> Result<Given, String> {
    let dir = place(&place(home, "agents")?, agent)?;
    let mut files = Vec::new();
    if let Route::Local(base) = &a.route {
        let models = json!({"providers": {LOCAL: {
            "baseUrl": base,
            "api": "openai-completions",
            "apiKey": format!("${LOCAL_KEY}"),
            "models": [{"id": a.model}],
        }}});
        files.push((place(&dir, "models.json")?, format!("{models:#}\n")));
    }
    Ok(Given {
        args: [
            "--no-session",
            "--offline",
            // A repository's own `.pi` is not read.
            "--no-approve",
            "--provider",
            a.route.id(),
            "--model",
            &a.model,
        ]
        .map(str::to_owned)
        .into(),
        vars: vec![
            ("PI_CODING_AGENT_DIR".into(), dir.to_string()),
            ("PI_OFFLINE".into(), "1".into()),
        ],
        files,
        key: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn home() -> Place {
        Place::new("/data/harnesses/x").unwrap()
    }
    fn account(route: Route, key: Option<&str>) -> Account {
        Account { route, model: "m-1".into(), key: key.map(str::to_owned) }
    }
    fn var<'a>(g: &'a Given, n: &str) -> Option<&'a str> {
        g.vars.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str())
    }

    #[test]
    fn the_key_is_in_its_environment_and_nowhere_else() {
        let a = account(Route::Anthropic, Some("sk-secret-1234"));
        for r in [Runs::Dsh, Runs::OpenCode, Runs::Pi] {
            for access in [Access::Read, Access::Ask, Access::Work] {
                let g = r.give(&a, access, &home(), "a1").unwrap();
                let k = g.key.clone().unwrap();
                assert_eq!((k.name.as_str(), k.secret), ("ANTHROPIC_API_KEY", true), "{r:?}");
                let all = format!("{:?} {:?} {:?}", g.args, g.vars, g.files);
                assert!(!all.contains("sk-secret"), "{r:?}: {all}");
            }
        }
        let e =
            Runs::Dsh.give(&account(Route::OpenAi, None), Access::Ask, &home(), "a").unwrap_err();
        assert!(e.contains("no key"));
    }

    #[test]
    fn dsh_is_told_the_route_and_the_model_in_a_patch_of_its_own() {
        let g = Runs::Dsh
            .give(&account(Route::OpenRouter, Some("k-12345678")), Access::Work, &home(), "a1")
            .unwrap();
        assert_eq!(g.args, ["--patch", "/data/harnesses/x/agents/a1.patch.yml"]);
        let (at, text) = &g.files[0];
        assert_eq!(at.as_str(), "/data/harnesses/x/agents/a1.patch.yml");
        assert_eq!(
            text,
            "- id: llm-pi-ai\n  config:\n    providers:\n      openrouter:\n        apiKeyEnv: \"OPENROUTER_API_KEY\"\n- id: acp\n  config:\n    provider: \"openrouter\"\n    model: \"m-1\"\n"
        );
        assert_eq!(var(&g, "DSH_HOME"), Some("/data/harnesses/x"));
        assert_eq!(var(&g, "DSH_AGENTS_HOME"), Some("/data/harnesses/x/agents-home"));
        assert_eq!(var(&g, "DSH_TELEMETRY_DISABLED"), Some("1"));
        let mode = |a| {
            var(
                &Runs::Dsh
                    .give(&account(Route::OpenAi, Some("k-12345678")), a, &home(), "a")
                    .unwrap(),
                "DSH_PERMISSION_MODE",
            )
            .map(str::to_owned)
        };
        assert_eq!(mode(Access::Read).as_deref(), Some("read-only"));
        assert_eq!(mode(Access::Work).as_deref(), Some("workspace-write"));
        // A server on this computer is a route of its own, with its address and model.
        let g = Runs::Dsh
            .give(
                &account(Route::Local("http://127.0.0.1:1234/v1".into()), None),
                Access::Ask,
                &home(),
                "a1",
            )
            .unwrap();
        assert!(g.files[0].1.contains("      axiom-local:\n        apiKeyEnv: \"AXIOM_LOCAL_KEY\"\n        api: openai-completions\n        baseURL: \"http://127.0.0.1:1234/v1\"\n        models:\n          - id: \"m-1\"\n"), "{}", g.files[0].1);
        assert_eq!(
            g.key.unwrap(),
            Var { name: "AXIOM_LOCAL_KEY".into(), value: "none".into(), secret: false }
        );
    }

    #[test]
    fn opencode_is_told_everything_in_its_environment_and_reads_no_project_of_its_own() {
        let g = Runs::OpenCode
            .give(&account(Route::OpenAi, Some("k-12345678")), Access::Ask, &home(), "a1")
            .unwrap();
        assert!(g.args.is_empty() && g.files.is_empty());
        let c: Value = serde_json::from_str(var(&g, "OPENCODE_CONFIG_CONTENT").unwrap()).unwrap();
        assert_eq!(c["model"], "openai/m-1");
        assert_eq!(c["enabled_providers"], json!(["openai"]));
        assert_eq!(c["provider"]["openai"]["options"]["apiKey"], "{env:OPENAI_API_KEY}");
        assert_eq!(c["permission"], json!({"*": "ask"}));
        assert_eq!(c["autoupdate"], false);
        for v in [
            "OPENCODE_DISABLE_PROJECT_CONFIG",
            "OPENCODE_DISABLE_AUTOUPDATE",
            "OPENCODE_DISABLE_MODELS_FETCH",
        ] {
            assert_eq!(var(&g, v), Some("1"), "{v}");
        }
        assert_eq!(var(&g, "XDG_DATA_HOME"), Some("/data/harnesses/x/data"));
        let read = Runs::OpenCode
            .give(&account(Route::OpenAi, Some("k-12345678")), Access::Read, &home(), "a")
            .unwrap();
        let c: Value =
            serde_json::from_str(var(&read, "OPENCODE_CONFIG_CONTENT").unwrap()).unwrap();
        assert_eq!(
            (c["permission"]["*"].as_str(), c["permission"]["read"].as_str()),
            (Some("deny"), Some("allow"))
        );
    }

    #[test]
    fn pi_is_told_by_its_arguments_and_a_folder_of_its_own() {
        let g = Runs::Pi
            .give(&account(Route::DeepSeek, Some("k-12345678")), Access::Work, &home(), "a1")
            .unwrap();
        assert_eq!(
            g.args,
            [
                "--no-session",
                "--offline",
                "--no-approve",
                "--provider",
                "deepseek",
                "--model",
                "m-1"
            ]
        );
        assert_eq!(var(&g, "PI_CODING_AGENT_DIR"), Some("/data/harnesses/x/agents/a1"));
        let g = Runs::Pi
            .give(
                &account(Route::Local("http://127.0.0.1:1234/v1".into()), None),
                Access::Work,
                &home(),
                "a1",
            )
            .unwrap();
        let (at, text) = &g.files[0];
        assert_eq!(at.as_str(), "/data/harnesses/x/agents/a1/models.json");
        let v: Value = serde_json::from_str(text).unwrap();
        assert_eq!(v["providers"]["axiom-local"]["apiKey"], "$AXIOM_LOCAL_KEY");
    }

    #[test]
    fn what_could_not_be_said_is_refused() {
        for m in ["", "a\"b", "a b", "a\nb", "$(x)", "a\\b"] {
            let a = Account { model: m.into(), ..account(Route::OpenAi, Some("k-12345678")) };
            for r in [Runs::Dsh, Runs::OpenCode, Runs::Pi] {
                assert!(r.give(&a, Access::Ask, &home(), "a").is_err(), "{m:?} {r:?}");
            }
        }
        assert_eq!(
            Route::of("bedrock", Some("https://bedrock-runtime.us-east-1.amazonaws.com")),
            None
        );
        assert_eq!(Route::of("chatgpt", None), None);
        assert_eq!(Route::of("openai-compatible", None), None);
        assert_eq!(
            Route::of("lm-studio", None),
            Some(Route::Local("http://127.0.0.1:1234/v1".into()))
        );
    }
}
