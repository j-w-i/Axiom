//! Policy evaluation (`docs/protocols/agents.md` §Permission policy). Pure.
//!
//! - A rule matches when every field it sets matches: `effect`; `tool` (exact, or a prefix
//!   ending in `.*`); `path` (a workspace-relative glob that every target path must match; a
//!   call without paths never matches a path rule); `argvPrefix` (the argv starts with it).
//! - Among matching rules the most specific wins (the number of `tool`, `path`, `argvPrefix`
//!   it sets, then the longer `argvPrefix`); among equals, deny > ask > allow. No match: the
//!   policy's `default`.
//! - Workspace rules can only make the result stricter: the final decision is the stricter of
//!   the user's and the workspace's.

use axiom_protocol::agent::{Decision, Effect, Policy, PolicyRule};
use axiom_protocol::WorkspacePath;

/// The built-in user policy: ask, except reads.
pub fn default_policy() -> Policy {
    Policy {
        default: Decision::Ask,
        rules: vec![PolicyRule {
            effect: Some(Effect::Read),
            tool: None,
            path: None,
            argv_prefix: None,
            decision: Decision::Allow,
        }],
    }
}

/// What the gate decides about.
#[derive(Debug, Clone)]
pub struct Call<'a> {
    pub tool: &'a str,
    pub effect: Effect,
    pub paths: &'a [WorkspacePath],
    pub argv: Option<&'a [String]>,
}

fn rank(d: Decision) -> u8 {
    match d {
        Decision::Allow => 0,
        Decision::Ask => 1,
        Decision::Deny => 2,
    }
}

pub fn stricter(a: Decision, b: Decision) -> Decision {
    if rank(a) >= rank(b) {
        a
    } else {
        b
    }
}

fn tool_matches(pattern: &str, tool: &str) -> bool {
    match pattern.strip_suffix(".*") {
        Some(prefix) => {
            tool.len() > prefix.len() + 1
                && tool.starts_with(prefix)
                && tool.as_bytes()[prefix.len()] == b'.'
        }
        None => pattern == tool,
    }
}

pub fn matches(rule: &PolicyRule, call: &Call<'_>) -> bool {
    if rule.effect.is_some_and(|e| e != call.effect) {
        return false;
    }
    if let Some(t) = &rule.tool {
        if !tool_matches(t, call.tool) {
            return false;
        }
    }
    if let Some(g) = &rule.path {
        if call.paths.is_empty() || !call.paths.iter().all(|p| glob(g, &p.0)) {
            return false;
        }
    }
    if let Some(prefix) = &rule.argv_prefix {
        match call.argv {
            Some(argv) if argv.len() >= prefix.len() && argv[..prefix.len()] == prefix[..] => {}
            _ => return false,
        }
    }
    true
}

/// More fields set is more specific; with equal fields, a longer `argvPrefix` is.
fn specificity(r: &PolicyRule) -> (usize, usize) {
    (
        r.tool.is_some() as usize + r.path.is_some() as usize + r.argv_prefix.is_some() as usize,
        r.argv_prefix.as_ref().map_or(0, Vec::len),
    )
}

/// A short description of a rule, for approval cards and the audit log.
pub fn describe(r: &PolicyRule) -> String {
    let mut parts = Vec::new();
    if let Some(e) = r.effect {
        parts.push(
            serde_json::to_value(e)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
        );
    }
    if let Some(t) = &r.tool {
        parts.push(t.clone());
    }
    if let Some(p) = &r.path {
        parts.push(p.clone());
    }
    if let Some(a) = &r.argv_prefix {
        parts.push(format!("`{}`", a.join(" ")));
    }
    let d = match r.decision {
        Decision::Allow => "allow",
        Decision::Ask => "ask",
        Decision::Deny => "deny",
    };
    format!("{} → {d}", parts.join(" "))
}

/// One policy's decision and the rule that made it (`None` = its default).
pub fn evaluate(policy: &Policy, call: &Call<'_>) -> (Decision, Option<PolicyRule>) {
    let best = policy
        .rules
        .iter()
        .filter(|r| matches(r, call))
        .max_by(|a, b| {
            specificity(a)
                .cmp(&specificity(b))
                .then(rank(a.decision).cmp(&rank(b.decision)))
        });
    match best {
        Some(r) => (r.decision, Some(r.clone())),
        None => (policy.default, None),
    }
}

/// Every source together: the user policy with the session's grants, then the workspace's rules
/// (stricter only).
pub fn decide(
    user: &Policy,
    session_grants: &[PolicyRule],
    workspace: Option<&Policy>,
    call: &Call<'_>,
) -> (Decision, Option<String>) {
    let mut combined = user.clone();
    combined.rules.extend(session_grants.iter().cloned());
    let (mut d, mut why) = evaluate(&combined, call);
    let mut reason = why.take().map(|r| describe(&r));
    if let Some(ws) = workspace {
        let (wd, wr) = evaluate(ws, call);
        if rank(wd) > rank(d) {
            d = wd;
            reason = Some(match wr {
                Some(r) => format!("workspace: {}", describe(&r)),
                None => "workspace default".into(),
            });
        }
    }
    (d, reason)
}

/// `*` matches within a segment, `**` any number of segments, `?` one character.
pub fn glob(pattern: &str, path: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let s: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segs(&p, &s)
}

fn segs(p: &[&str], s: &[&str]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(&"**") => (0..=s.len()).any(|i| segs(&p[1..], &s[i..])),
        Some(first) => {
            !s.is_empty() && seg(first.as_bytes(), s[0].as_bytes()) && segs(&p[1..], &s[1..])
        }
    }
}

fn seg(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') => (0..=s.len()).any(|i| seg(&p[1..], &s[i..])),
        Some(b'?') => !s.is_empty() && seg(&p[1..], &s[1..]),
        Some(c) => s.first() == Some(c) && seg(&p[1..], &s[1..]),
    }
}

#[derive(serde::Deserialize)]
struct ProjectToml {
    agent: Option<AgentToml>,
}
#[derive(serde::Deserialize)]
struct AgentToml {
    policy: Option<PolicyToml>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyToml {
    default: Option<Decision>,
    #[serde(default)]
    rule: Vec<PolicyRule>,
}

/// The `[agent.policy]` table of a `.axiom/project.toml` (other tables are ignored here), or
/// `None` without one. A workspace without `default` adds no default of its own (allow is the
/// identity of "the stricter of"). `Err` is the parse error, for the user.
pub fn from_project_toml(text: &str) -> Result<Option<Policy>, String> {
    let doc: ProjectToml = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(doc.agent.and_then(|a| a.policy).map(|p| Policy {
        default: p.default.unwrap_or(Decision::Allow),
        rules: p.rule,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use Decision::*;
    use Effect::*;

    fn rule(
        effect: Option<Effect>,
        tool: Option<&str>,
        path: Option<&str>,
        argv: Option<&[&str]>,
        d: Decision,
    ) -> PolicyRule {
        PolicyRule {
            effect,
            tool: tool.map(String::from),
            path: path.map(String::from),
            argv_prefix: argv.map(|a| a.iter().map(|s| s.to_string()).collect()),
            decision: d,
        }
    }

    fn wp(v: &[&str]) -> Vec<WorkspacePath> {
        v.iter().map(|s| WorkspacePath(s.to_string())).collect()
    }

    #[test]
    fn glob_cases() {
        for (p, s, want) in [
            ("src/**", "src/a.rs", true),
            ("src/**", "src/a/b/c.rs", true),
            ("src/**", "src", true),
            ("src/**", "tests/a.rs", false),
            ("**/*.rs", "a.rs", true),
            ("**/*.rs", "x/y/a.rs", true),
            ("**/*.rs", "x/y/a.toml", false),
            ("*.md", "README.md", true),
            ("*.md", "docs/a.md", false),
            ("src/?.rs", "src/a.rs", true),
            ("src/?.rs", "src/ab.rs", false),
            ("Cargo.toml", "Cargo.toml", true),
        ] {
            assert_eq!(glob(p, s), want, "{p} vs {s}");
        }
    }

    type Case = (
        &'static str,
        Effect,
        Vec<WorkspacePath>,
        Option<Vec<String>>,
        Decision,
    );

    /// The table of `agents.md` §Permission policy, as cases.
    #[test]
    fn gate_policy_table() {
        let user = Policy {
            default: Ask,
            rules: vec![
                rule(Some(Read), None, None, None, Allow),
                rule(Some(WriteWorkspace), None, None, None, Ask),
                rule(Some(Execute), None, None, Some(&["cargo", "build"]), Allow),
                rule(Some(Execute), None, None, Some(&["cargo"]), Ask),
                rule(None, Some("mcp.github.*"), None, None, Deny),
                rule(Some(WriteWorkspace), None, Some("docs/**"), None, Allow),
                rule(
                    Some(WriteWorkspace),
                    None,
                    Some("docs/secret/**"),
                    None,
                    Deny,
                ),
                rule(
                    Some(WriteWorkspace),
                    Some("fs.applyPatch"),
                    Some("docs/**"),
                    None,
                    Ask,
                ),
                rule(Some(Credentials), None, None, None, Allow),
            ],
        };
        let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let cases: Vec<Case> = vec![
            ("fs.read", Read, wp(&["a"]), None, Allow),
            ("workspace.search", Read, vec![], None, Allow),
            (
                "fs.applyPatch",
                WriteWorkspace,
                wp(&["src/a.rs"]),
                None,
                Ask,
            ),
            (
                "task.run",
                Execute,
                vec![],
                Some(argv(&["cargo", "build", "--release"])),
                Allow,
            ),
            (
                "task.run",
                Execute,
                vec![],
                Some(argv(&["cargo", "test"])),
                Ask,
            ),
            (
                "task.run",
                Execute,
                vec![],
                Some(argv(&["rm", "-rf", "/"])),
                Ask,
            ),
            ("task.run", Execute, vec![], Some(argv(&["cargo"])), Ask),
            ("mcp.github.create_issue", Execute, vec![], None, Deny),
            ("mcp.githubx.y", Execute, vec![], None, Ask),
            ("mcp.github", Execute, vec![], None, Ask),
            // Specificity: a path rule beats an effect-only rule.
            (
                "other.write",
                WriteWorkspace,
                wp(&["docs/a.md"]),
                None,
                Allow,
            ),
            // Equal specificity: deny beats allow.
            (
                "other.write",
                WriteWorkspace,
                wp(&["docs/secret/k.md"]),
                None,
                Deny,
            ),
            // tool + path is more specific than path alone.
            (
                "fs.applyPatch",
                WriteWorkspace,
                wp(&["docs/a.md"]),
                None,
                Ask,
            ),
            // A path rule needs every path to match.
            (
                "other.write",
                WriteWorkspace,
                wp(&["docs/a.md", "src/b.rs"]),
                None,
                Ask,
            ),
            // A path rule never matches a call without paths.
            ("other.write", WriteWorkspace, vec![], None, Ask),
            ("env.install", Install, vec![], None, Ask),
            ("x", Deploy, vec![], None, Ask),
            ("x", Credentials, vec![], None, Allow),
        ];
        for (tool, effect, paths, argv, want) in cases {
            let call = Call {
                tool,
                effect,
                paths: &paths,
                argv: argv.as_deref(),
            };
            assert_eq!(
                evaluate(&user, &call).0,
                want,
                "{tool} {effect:?} {paths:?} {argv:?}"
            );
        }
    }

    #[test]
    fn gate_workspace_rules_are_stricter_only() {
        let user = Policy {
            default: Allow,
            rules: vec![rule(Some(Execute), None, None, None, Allow)],
        };
        // A workspace that tries to allow more than the user cannot.
        let loose = Policy {
            default: Allow,
            rules: vec![rule(Some(Delete), None, None, None, Allow)],
        };
        let strict = Policy {
            default: Allow,
            rules: vec![rule(
                Some(Execute),
                None,
                None,
                Some(&["cargo", "publish"]),
                Deny,
            )],
        };
        let user_ask = Policy {
            default: Ask,
            rules: vec![],
        };
        let argv: Vec<String> = vec!["cargo".into(), "publish".into()];
        let exec = Call {
            tool: "task.run",
            effect: Execute,
            paths: &[],
            argv: Some(&argv),
        };
        let del = Call {
            tool: "fs.applyPatch",
            effect: Delete,
            paths: &[],
            argv: None,
        };
        assert_eq!(decide(&user, &[], Some(&strict), &exec).0, Deny);
        assert!(decide(&user, &[], Some(&strict), &exec)
            .1
            .unwrap()
            .starts_with("workspace:"));
        assert_eq!(decide(&user, &[], None, &exec).0, Allow);
        assert_eq!(decide(&user_ask, &[], Some(&loose), &del).0, Ask);
        // A workspace default of ask makes every unmatched call at least ask.
        let ws_ask = Policy {
            default: Ask,
            rules: vec![],
        };
        assert_eq!(decide(&user, &[], Some(&ws_ask), &exec).0, Ask);
    }

    #[test]
    fn gate_session_grants_join_the_user_policy() {
        let user = default_policy();
        let paths = wp(&["src/a.rs"]);
        let call = Call {
            tool: "fs.applyPatch",
            effect: WriteWorkspace,
            paths: &paths,
            argv: None,
        };
        assert_eq!(decide(&user, &[], None, &call).0, Ask);
        let grant = rule(
            Some(WriteWorkspace),
            Some("fs.applyPatch"),
            Some("src/a.rs"),
            None,
            Allow,
        );
        assert_eq!(
            decide(&user, std::slice::from_ref(&grant), None, &call).0,
            Allow
        );
        let other = wp(&["src/b.rs"]);
        let call2 = Call {
            paths: &other,
            ..call
        };
        assert_eq!(decide(&user, &[grant], None, &call2).0, Ask);
    }

    #[test]
    fn gate_project_toml_policy() {
        let p = from_project_toml(
            "[environment]\nrust = \"1.84.0\"\n\n[agent.policy]\ndefault = \"ask\"\n[[agent.policy.rule]]\neffect = \"execute\"\nargvPrefix = [\"cargo\", \"publish\"]\ndecision = \"deny\"\n",
        )
        .unwrap()
        .unwrap();
        assert_eq!(p.default, Ask);
        assert_eq!(
            p.rules[0].argv_prefix.as_deref().unwrap(),
            ["cargo", "publish"]
        );
        assert_eq!(from_project_toml("[environment]\n").unwrap(), None);
        assert!(from_project_toml("[agent.policy]\nbogus = 1\n").is_err());
        let no_default = from_project_toml("[agent.policy]\n").unwrap().unwrap();
        assert_eq!(no_default.default, Allow);
    }

    #[test]
    fn describe_rules() {
        assert_eq!(
            describe(&rule(
                Some(Execute),
                None,
                None,
                Some(&["cargo", "build"]),
                Allow
            )),
            "execute `cargo build` → allow"
        );
    }
}
