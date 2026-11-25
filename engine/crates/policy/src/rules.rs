//! Rules choose between what capabilities already allow. They are data: no loops, no code, no I/O.
//! The strictest of all that match decides, wherever it is written: deny, then ask, then allow
//! (SI-16). Without a rule, the agent's access decides.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    caps::{Caps, Host},
    paths::{self, Place},
    profile::Keep,
    shell,
};

/// In order of strictness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

/// What happens where no rule speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Looks; changes nothing. Commands run contained and cannot write.
    Read,
    /// Asks before each change and each command.
    Ask,
    /// Changes what its capabilities cover and runs what can be read, contained, without asking.
    Work,
}

/// Which sandbox a command runs in. There is no "none" to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunIn {
    ReadOnly,
    Workspace,
}

#[derive(Debug, Clone, Copy)]
pub enum Action<'a> {
    Read(&'a Place),
    Write(&'a Place),
    /// A shell command, and where it starts.
    Run {
        command: &'a str,
        cwd: &'a Place,
    },
    Fetch(&'a Host),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub decision: Decision,
    /// For a command: the sandbox it runs in, if it is allowed.
    pub run_in: RunIn,
    /// The rules that decided; empty: the access, or the capabilities.
    pub rules: Vec<String>,
    /// In words, for the person and for the model.
    pub why: String,
}

impl Verdict {
    /// For what needs no judging.
    pub fn allowed(why: &str) -> Self {
        Self::new(Decision::Allow, why)
    }
    fn new(decision: Decision, why: impl Into<String>) -> Self {
        Self { decision, run_in: RunIn::ReadOnly, rules: Vec::new(), why: why.into() }
    }
}

/// Who wrote a set of rules. A workspace is written by whoever wrote the repository (SI-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Person,
    Workspace,
}

#[derive(Debug, thiserror::Error)]
pub enum BadPolicy {
    #[error("the policy is too long")]
    Long,
    #[error("{0}")]
    Toml(#[from] toml::de::Error),
    #[error("this version of the policy is not known")]
    Version,
    #[error("rule {0}: {1}")]
    Rule(String, &'static str),
    #[error("rule {0}: {1:?} should have matched and does not")]
    Match(String, String),
    #[error("rule {0}: {1:?} matches and should not")]
    NotMatch(String, String),
}

const LONGEST: usize = 256 * 1024;
const MOST_RULES: usize = 1000;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    #[serde(default)]
    rule: Vec<RuleText>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleText {
    id: String,
    decision: Decision,
    tool: Tool,
    /// For `shell`: words in order; the first is the program's name.
    #[serde(default)]
    argv: Vec<Word>,
    /// For `shell`: options that change what runs. `x` is also `x=…`; `x*` is whatever starts with x.
    #[serde(default)]
    deny_args: Vec<String>,
    /// For `shell` that is allowed: in which sandbox.
    run_in: Option<RunIn>,
    /// For `read` and `write`: below the workspace, `*` within a name, `**` any names.
    #[serde(default)]
    paths: Vec<String>,
    /// For `fetch`.
    #[serde(default)]
    hosts: Vec<Host>,
    #[serde(default, rename = "match")]
    yes: Vec<String>,
    #[serde(default, rename = "not_match")]
    no: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Tool {
    Shell,
    Read,
    Write,
    Fetch,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Word {
    One(String),
    Any(Vec<String>),
    Rest { rest: Rest },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Rest {
    Any,
}

#[derive(Debug, Clone)]
struct Rule(RuleText);

/// Finds a program by name in the folders commands are found in.
pub type Programs<'a> = &'a dyn Fn(&str) -> Option<Place>;

#[derive(Debug, Clone, Default)]
pub struct Policy {
    rules: Vec<Rule>,
}

impl Policy {
    /// All or nothing: a rule that cannot be understood might have been the one that denies.
    pub fn parse(text: &str, from: Source) -> Result<Self, BadPolicy> {
        if text.len() > LONGEST {
            return Err(BadPolicy::Long);
        }
        let f: File = toml::from_str(text)?;
        if f.version != 1 {
            return Err(BadPolicy::Version);
        }
        let bad = |r: &RuleText, why| Err(BadPolicy::Rule(r.id.clone(), why));
        if f.rule.len() > MOST_RULES {
            return Err(BadPolicy::Long);
        }
        let mut ids = BTreeSet::new();
        let mut rules = Vec::new();
        for r in f.rule {
            if r.id.is_empty() || !ids.insert(r.id.clone()) {
                return bad(&r, "needs a name of its own");
            }
            if from == Source::Workspace && r.decision == Decision::Allow {
                return bad(&r, "a workspace can ask or deny, not allow");
            }
            let shell = r.tool == Tool::Shell;
            let files = matches!(r.tool, Tool::Read | Tool::Write);
            if shell != !r.argv.is_empty() {
                return bad(&r, "argv is for shell, and shell needs it");
            }
            if files == r.paths.is_empty() {
                return bad(&r, "paths are for read and write, and those need them");
            }
            if (r.tool == Tool::Fetch) == r.hosts.is_empty() {
                return bad(&r, "hosts are for fetch, and fetch needs them");
            }
            if (shell && r.decision == Decision::Allow) != r.run_in.is_some() {
                return bad(&r, "run_in is for a shell command that is allowed, and that needs it");
            }
            if !r.deny_args.is_empty() && !(shell && r.decision == Decision::Allow) {
                return bad(&r, "deny_args narrows what is allowed");
            }
            if r.deny_args.iter().any(|a| a.trim_end_matches('*').is_empty()) {
                return bad(&r, "an option to deny has a name");
            }
            if shell {
                match r.argv.first() {
                    Some(Word::One(p)) if !p.is_empty() && !p.contains('/') => {}
                    _ => return bad(&r, "argv starts with the name of one program"),
                }
                let rests = r.argv.iter().filter(|w| matches!(w, Word::Rest { .. })).count();
                let last = matches!(r.argv.last(), Some(Word::Rest { .. }));
                if rests > 1 || (rests == 1 && !last) {
                    return bad(&r, "the rest comes last");
                }
            }
            if r.paths.iter().any(|p| !pattern_is_plain(p)) {
                return bad(&r, "a path is below the workspace, without . and ..");
            }
            let rule = Rule(r);
            rule.prove()?;
            rules.push(rule);
        }
        Ok(Self { rules })
    }

    /// All rules of both.
    pub fn and(mut self, other: Self) -> Self {
        self.rules.extend(other.rules);
        self
    }

    /// `workspace` is where relative patterns start.
    pub fn decide(
        &self,
        action: Action<'_>,
        access: Access,
        caps: &Caps,
        keep: &Keep,
        workspace: &Place,
        programs: Programs<'_>,
    ) -> Verdict {
        use Decision::{Allow, Ask, Deny};
        match action {
            Action::Read(p) => {
                if keep.hides(p) {
                    return Verdict::new(Deny, "it is kept from agents");
                }
                if !caps.read.covers(p) {
                    return Verdict::new(Deny, "it is outside what this agent may read");
                }
                self.files(Tool::Read, p, workspace, Allow, "reading is allowed here")
            }
            Action::Write(p) => {
                if keep.hides(p) {
                    return Verdict::new(Deny, "it is kept from agents");
                }
                if let Some(why) = paths::protected(p) {
                    return Verdict::new(Deny, format!("it is protected: {why}"));
                }
                if access == Access::Read {
                    return Verdict::new(Deny, "this agent may only read");
                }
                if !caps.write.covers(p) {
                    return Verdict::new(Deny, "it is outside what this agent may write");
                }
                let (d, why) = match access {
                    Access::Work if paths::review_name(p.name()) => {
                        (Ask, "it runs when someone builds, tests or installs")
                    }
                    Access::Work => (Allow, "writing is allowed here"),
                    _ => (Ask, "this agent asks before each change"),
                };
                self.files(Tool::Write, p, workspace, d, why)
            }
            Action::Fetch(h) => {
                if !caps.net.covers(h) {
                    return Verdict::new(Deny, "the host is not one this agent may reach");
                }
                let base = if access == Access::Ask { Ask } else { Allow };
                let mut v = Verdict::new(base, "the host is one this agent may reach");
                let hit = self.rules.iter().filter(|r| r.0.tool == Tool::Fetch);
                strictest(&mut v, hit.filter(|r| r.0.hosts.iter().any(|x| x.covers(h))));
                v
            }
            Action::Run { command, cwd } => self.run(command, cwd, access, caps, programs),
        }
    }

    fn files(&self, tool: Tool, p: &Place, ws: &Place, base: Decision, why: &str) -> Verdict {
        let mut v = Verdict::new(base, why);
        let Ok(rel) = p.as_path().strip_prefix(ws.as_path()) else { return v };
        let rel: Vec<&str> = rel.components().filter_map(|c| c.as_os_str().to_str()).collect();
        let hit = self.rules.iter().filter(|r| r.0.tool == tool);
        strictest(
            &mut v,
            hit.filter(|r| r.0.paths.iter().any(|x| path_matches(x, &rel, r.0.decision))),
        );
        v
    }

    fn run(
        &self,
        command: &str,
        cwd: &Place,
        access: Access,
        caps: &Caps,
        programs: Programs<'_>,
    ) -> Verdict {
        use Decision::{Allow, Ask, Deny};
        if !caps.run {
            return Verdict::new(Deny, "this agent may not run anything");
        }
        let most = if access == Access::Read { RunIn::ReadOnly } else { RunIn::Workspace };
        let parsed = match shell::parse(command) {
            Ok(p) => p,
            Err(e) => {
                // Not readable, so nothing can allow it; a denied program named in it still counts.
                let mut v = Verdict { run_in: most, ..Verdict::new(Ask, e.to_string()) };
                let loose = command.split(|c: char| !(c.is_alphanumeric() || "_-.+".contains(c)));
                let words: BTreeSet<&str> = loose.filter(|w| !w.is_empty()).collect();
                let denied = self.rules.iter().filter(|r| {
                    r.0.tool == Tool::Shell
                        && r.0.decision == Deny
                        && matches!(r.0.argv.as_slice(), [Word::One(p)] | [Word::One(p), Word::Rest { .. }] if words.contains(p.as_str()))
                });
                strictest(&mut v, denied);
                return v;
            }
        };
        let mut all: Option<Verdict> = None;
        for w in &parsed {
            let harmless = matches!(w[0].as_str(), "echo" | "pwd" | "true" | "false" | "cd");
            let (d, why) = match access {
                _ if harmless => (Allow, "it does nothing but say something"),
                Access::Ask => (Ask, "this agent asks before each command"),
                Access::Read => (Allow, "it can be read, and runs contained without writing"),
                Access::Work => (Allow, "it can be read, and runs contained"),
            };
            let mut v = Verdict { run_in: most, ..Verdict::new(d, why) };
            let hit: Vec<&Rule> =
                self.rules.iter().filter(|r| r.applies(w, Some((cwd, programs)))).collect();
            // A person who wrote "allow" for this answers the asking that access would do; another
            // rule that asks or denies still counts.
            if let Some(d) = hit.iter().map(|r| r.0.decision).max() {
                v.decision = if access == Access::Ask { d } else { d.max(v.decision) };
                let deciding: Vec<&&Rule> = hit.iter().filter(|r| r.0.decision == d).collect();
                v.rules = deciding.iter().map(|r| r.0.id.clone()).collect();
                v.why = format!("rule {}", v.rules.join(", "));
                for r in hit {
                    v.run_in = r.0.run_in.map_or(v.run_in, |x| x.min(v.run_in));
                }
            }
            all = Some(match all {
                None => v,
                Some(a) => worse(a, v),
            });
        }
        all.unwrap_or_else(|| Verdict::new(Ask, "nothing could be read"))
    }
}

/// Of two parts of one command: the stricter decides, in the narrower sandbox.
fn worse(a: Verdict, b: Verdict) -> Verdict {
    let run_in = a.run_in.min(b.run_in);
    let mut out = match a.decision.cmp(&b.decision) {
        std::cmp::Ordering::Greater => a,
        std::cmp::Ordering::Less => b,
        std::cmp::Ordering::Equal => {
            let mut rules = a.rules.clone();
            rules.extend(b.rules.iter().filter(|r| !a.rules.contains(r)).cloned());
            let why = if rules.is_empty() { a.why } else { format!("rule {}", rules.join(", ")) };
            Verdict { rules, why, ..a }
        }
    };
    out.run_in = run_in;
    out
}

/// For files and hosts: an `allow` answers an asking base; nothing lowers a denial.
fn strictest<'a>(v: &mut Verdict, hit: impl Iterator<Item = &'a Rule>) {
    let hit: Vec<&Rule> = hit.collect();
    let Some(d) = hit.iter().map(|r| r.0.decision).max() else { return };
    if v.decision == Decision::Deny {
        return;
    }
    v.decision = d;
    v.rules = hit.iter().filter(|r| r.0.decision == d).map(|r| r.0.id.clone()).collect();
    v.why = format!("rule {}", v.rules.join(", "));
}

impl Rule {
    /// `at`: where the command starts and how programs are found; `None` only when proving a rule's
    /// own examples.
    fn applies(&self, w: &[String], at: Option<(&Place, Programs<'_>)>) -> bool {
        let r = &self.0;
        let (Some(first), Some(Word::One(program))) = (w.first(), r.argv.first()) else {
            return false;
        };
        if r.tool != Tool::Shell {
            return false;
        }
        let named = if r.decision == Decision::Allow {
            // Allowed is the one file this name leads to, not whatever is called the same.
            match at {
                None => first == program,
                Some((cwd, programs)) => {
                    let want = programs(program);
                    let got =
                        if first.contains('/') { cwd.join(first).ok() } else { programs(first) };
                    want.is_some() && want == got
                }
            }
        } else {
            paths::fold(first.rsplit('/').next().unwrap_or(first)) == paths::fold(program)
        };
        if !named || (r.decision == Decision::Allow && w[1..].iter().any(|a| self.denies_arg(a))) {
            return false;
        }
        let mut args = w[1..].iter();
        for p in &r.argv[1..] {
            let ok = match p {
                Word::Rest { rest: Rest::Any } => return true,
                Word::One(s) => args.next().is_some_and(|a| a == s),
                Word::Any(v) => args.next().is_some_and(|a| v.contains(a)),
            };
            if !ok {
                return false;
            }
        }
        args.next().is_none()
    }

    fn denies_arg(&self, a: &str) -> bool {
        self.0.deny_args.iter().any(|d| match d.strip_suffix('*') {
            Some(p) => a.starts_with(p),
            None => a.strip_prefix(d.as_str()).is_some_and(|r| r.is_empty() || r.starts_with('=')),
        })
    }

    /// The rule's own examples.
    fn prove(&self) -> Result<(), BadPolicy> {
        let hits = |s: &str| match self.0.tool {
            Tool::Shell => shell::parse(s).is_ok_and(|p| p.iter().all(|w| self.applies(w, None))),
            Tool::Fetch => Host::parse(s).is_ok_and(|h| self.0.hosts.iter().any(|x| x.covers(&h))),
            Tool::Read | Tool::Write => {
                let rel: Vec<&str> = s.split('/').collect();
                self.0.paths.iter().any(|p| path_matches(p, &rel, self.0.decision))
            }
        };
        for s in &self.0.yes {
            if !hits(s) {
                return Err(BadPolicy::Match(self.0.id.clone(), s.clone()));
            }
        }
        for s in &self.0.no {
            if hits(s) {
                return Err(BadPolicy::NotMatch(self.0.id.clone(), s.clone()));
            }
        }
        Ok(())
    }
}

fn pattern_is_plain(p: &str) -> bool {
    !p.is_empty()
        && p.len() <= 1024
        && p.split('/').count() <= 64
        && p.split('/')
            .all(|n| !n.is_empty() && n != "." && n != ".." && (n == "**" || !n.contains("**")))
}

/// What is asked about or denied is so however it is spelled, and with all that is below it.
fn path_matches(pattern: &str, rel: &[&str], d: Decision) -> bool {
    let loose = d != Decision::Allow;
    let pat: Vec<&str> = pattern.split('/').collect();
    if names_match(&pat, rel, loose) {
        return true;
    }
    loose && (1..rel.len()).any(|n| names_match(&pat, &rel[..n], loose))
}

/// Linear in names × pattern: the last `**` is the only place to come back to.
fn names_match(pat: &[&str], rel: &[&str], loose: bool) -> bool {
    let (mut p, mut r, mut back) = (0, 0, None);
    while r < rel.len() {
        if p < pat.len() && pat[p] == "**" {
            back = Some((p + 1, r));
            p += 1;
        } else if p < pat.len() && name_matches(pat[p], rel[r], loose) {
            p += 1;
            r += 1;
        } else if let Some((bp, br)) = back {
            p = bp;
            r = br + 1;
            back = Some((bp, br + 1));
        } else {
            return false;
        }
    }
    pat[p..].iter().all(|x| *x == "**")
}

fn name_matches(pat: &str, name: &str, loose: bool) -> bool {
    let (pat, name) = if loose {
        (paths::fold(pat), paths::fold(name))
    } else {
        (pat.to_owned(), name.to_owned())
    };
    let (pat, name): (Vec<char>, Vec<char>) = (pat.chars().collect(), name.chars().collect());
    let (mut p, mut n, mut back) = (0, 0, None);
    while n < name.len() {
        if pat.get(p) == Some(&'*') {
            back = Some((p + 1, n));
            p += 1;
        } else if pat.get(p) == Some(&name[n]) {
            p += 1;
            n += 1;
        } else if let Some((bp, bn)) = back {
            p = bp;
            n = bn + 1;
            back = Some((bp, bn + 1));
        } else {
            return false;
        }
    }
    pat[p..].iter().all(|c| *c == '*')
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use crate::{Net, Reach, Tools};
    use Decision::{Allow, Ask, Deny};
    use proptest::prelude::*;

    fn p(s: &str) -> Place {
        Place::new(s).unwrap()
    }
    fn caps() -> Caps {
        Caps {
            read: Reach::of([p("/w"), p("/h/me")]),
            write: Reach::of([p("/w")]),
            net: Net::of([Host::parse("a.io").unwrap()]),
            run: true,
            tools: Tools::All,
            ..Caps::default()
        }
    }
    fn programs(n: &str) -> Option<Place> {
        matches!(n, "cargo" | "git" | "rm" | "ls" | "make").then(|| p(&format!("/usr/bin/{n}")))
    }
    fn decide(pol: &Policy, a: Action<'_>, access: Access) -> Verdict {
        pol.decide(
            a,
            access,
            &caps(),
            &Keep::new(p("/h/me"), vec![p("/opt/axiom")]),
            &p("/w"),
            &programs,
        )
    }
    fn run(pol: &Policy, command: &str, access: Access) -> Verdict {
        decide(pol, Action::Run { command, cwd: &p("/w") }, access)
    }
    fn policy(s: &str) -> Policy {
        Policy::parse(&format!("version = 1\n{s}"), Source::Person).unwrap()
    }
    const TEST: &str = r#"
        [[rule]]
        id = "cargo"
        tool = "shell"
        argv = ["cargo", ["test", "check"], { rest = "any" }]
        deny_args = ["--config", "-Z*"]
        run_in = "workspace"
        decision = "allow"
        match = ["cargo test", "cargo check -p x --all"]
        not_match = ["cargo run", "cargo test --config x=y", "cargo test --config=x", "cargo test -Zfoo", "cargo test $X"]
    "#;

    #[test]
    fn without_rules_access_decides() {
        let none = Policy::default();
        let (f, o) = (p("/w/a.rs"), p("/x/a"));
        for (a, want) in [
            (Action::Read(&f), [Allow, Allow, Allow]),
            (Action::Write(&f), [Deny, Ask, Allow]),
            (Action::Read(&o), [Deny; 3]),
            (Action::Write(&o), [Deny; 3]),
            (Action::Run { command: "ls -l", cwd: &f }, [Allow, Ask, Allow]),
            (Action::Run { command: "ls $X", cwd: &f }, [Ask; 3]),
            (Action::Run { command: "echo hi", cwd: &f }, [Allow; 3]),
            (Action::Fetch(&Host::parse("a.io").unwrap()), [Allow, Ask, Allow]),
            (Action::Fetch(&Host::parse("b.io").unwrap()), [Deny; 3]),
        ] {
            for (acc, w) in [Access::Read, Access::Ask, Access::Work].into_iter().zip(want) {
                assert_eq!(decide(&none, a, acc).decision, w, "{a:?} {acc:?}");
            }
        }
        assert_eq!(run(&none, "ls", Access::Read).run_in, RunIn::ReadOnly);
        assert_eq!(run(&none, "ls $X", Access::Read).run_in, RunIn::ReadOnly);
        assert_eq!(run(&none, "ls", Access::Work).run_in, RunIn::Workspace);
        let mut c = caps();
        c.run = false;
        let k = Keep::new(p("/h/me"), vec![]);
        let v = none.decide(
            Action::Run { command: "echo a", cwd: &f },
            Access::Work,
            &c,
            &k,
            &f,
            &programs,
        );
        assert_eq!(v.decision, Deny);
    }

    /// SI-11, SI-12: nothing a rule says changes these.
    #[test]
    fn no_rule_opens_what_is_kept_or_protected() {
        let all = policy(
            "[[rule]]\nid='w'\ntool='write'\npaths=['**']\ndecision='allow'\n[[rule]]\nid='r'\ntool='read'\npaths=['**']\ndecision='allow'",
        );
        let k = Keep::new(p("/w/home"), vec![p("/w/own")]);
        for f in [
            "/w/.git/config",
            "/w/a/.GIT/x",
            "/w/.vscode/tasks.json",
            "/w/.mcp.json",
            "/w/own/x",
            "/w/OWN/x",
            "/w/home/.ssh/id",
        ] {
            let v =
                all.decide(Action::Write(&p(f)), Access::Work, &caps(), &k, &p("/w"), &programs);
            assert_eq!(v.decision, Deny, "{f}");
        }
        for f in ["/w/own/x", "/w/home/.aws/credentials", "/w/home/.AWS/x"] {
            let v = all.decide(Action::Read(&p(f)), Access::Work, &caps(), &k, &p("/w"), &programs);
            assert_eq!(v.decision, Deny, "{f}");
        }
        let v = decide(&all, Action::Write(&p("/x")), Access::Work);
        assert_eq!(v.decision, Deny, "outside the capabilities");
        assert_eq!(
            decide(&all, Action::Write(&p("/w/a")), Access::Read).decision,
            Deny,
            "only reads"
        );
        assert_eq!(
            decide(&Policy::default(), Action::Write(&p("/w/Makefile")), Access::Work).decision,
            Ask
        );
    }

    #[test]
    fn a_rule_allows_what_it_names_and_nothing_else() {
        let pol = policy(TEST);
        for (s, want) in [
            ("cargo test -p x", Allow),
            ("/usr/bin/cargo test", Allow),
            ("cargo check", Allow),
            ("cargo run", Ask),
            ("cargo", Ask),
            ("./cargo test", Ask),
            ("/w/cargo test", Ask),
            ("cargo test --config x", Ask),
            ("cargo test --config=x", Ask),
            ("cargo test -Zx", Ask),
            ("CARGO test", Ask),
            ("cargo test && rm -r x", Ask),
            ("cargo test; cargo check", Allow),
            ("cargo test | ls", Ask),
            ("cargo test > x", Ask),
            ("cargo test $(x)", Ask),
            ("echo a && cargo test", Allow),
        ] {
            assert_eq!(run(&pol, s, Access::Ask).decision, want, "{s}");
        }
        let v = run(&pol, "cargo test", Access::Ask);
        assert_eq!((v.rules, v.run_in), (vec!["cargo".to_string()], RunIn::Workspace));
        assert_eq!(
            run(&pol, "cargo test", Access::Read).run_in,
            RunIn::ReadOnly,
            "access is the most there is"
        );
        // Planted: the name leads to nothing that is trusted.
        let none = |_: &str| None;
        let k = Keep::new(p("/h/me"), vec![]);
        let v = pol.decide(
            Action::Run { command: "cargo test", cwd: &p("/w") },
            Access::Ask,
            &caps(),
            &k,
            &p("/w"),
            &none,
        );
        assert_eq!(v.decision, Ask);
    }

    #[test]
    fn the_strictest_decides() {
        let pol = policy(&format!(
            "{TEST}\n[[rule]]\nid='no-rm'\ntool='shell'\nargv=['rm',{{rest='any'}}]\ndecision='deny'\n[[rule]]\nid='ask-x'\ntool='shell'\nargv=['cargo','test','-p','x']\ndecision='ask'\n[[rule]]\nid='ro'\ntool='shell'\nargv=['make']\ndecision='allow'\nrun_in='read-only'"
        ));
        for acc in [Access::Read, Access::Ask, Access::Work] {
            for s in [
                "rm x",
                "/bin/rm x",
                "./RM x",
                "cargo test && rm x",
                "rm x; cargo test",
                "echo $(rm x)",
                "x=1 rm y",
            ] {
                let v = run(&pol, s, acc);
                assert_eq!((v.decision, v.rules), (Deny, vec!["no-rm".to_string()]), "{s}");
            }
            assert_eq!(run(&pol, "cargo test -p x", acc).decision, Ask);
        }
        assert_eq!(
            run(&pol, "echo $(farm)", Access::Work).decision,
            Ask,
            "a name within another is not it"
        );
        let v = run(&pol, "make && cargo test", Access::Ask);
        assert_eq!((v.decision, v.run_in, v.rules.len()), (Allow, RunIn::ReadOnly, 2));
        let both = policy(TEST).and(Policy::parse("version=1\n[[rule]]\nid='c'\ntool='shell'\nargv=['cargo',{rest='any'}]\ndecision='deny'", Source::Workspace).unwrap());
        assert_eq!(run(&both, "cargo test", Access::Work).decision, Deny);
    }

    #[test]
    fn rules_about_files_and_hosts() {
        let pol = policy(
            "[[rule]]\nid='gen'\ntool='write'\npaths=['src/gen/**']\ndecision='deny'\n[[rule]]\nid='doc'\ntool='write'\npaths=['docs/*.md']\ndecision='allow'\nmatch=['docs/a.md']\nnot_match=['docs/x/a.md','docs/a.rs']\n[[rule]]\nid='env'\ntool='read'\npaths=['**/.env*']\ndecision='ask'\n[[rule]]\nid='h'\ntool='fetch'\nhosts=['*.a.io']\ndecision='deny'",
        );
        let w = |s: &str, a| decide(&pol, Action::Write(&p(s)), a).decision;
        assert_eq!(w("/w/src/gen/a.rs", Access::Work), Deny);
        assert_eq!(w("/w/src/GEN/x/a.rs", Access::Work), Deny);
        assert_eq!(w("/w/src/gen", Access::Work), Deny);
        assert_eq!(w("/w/docs/a.md", Access::Ask), Allow);
        assert_eq!(w("/w/docs/A.MD", Access::Ask), Ask, "allowed is only what is named");
        assert_eq!(w("/w/docs/x/a.md", Access::Ask), Ask);
        let r = |s: &str| decide(&pol, Action::Read(&p(s)), Access::Work).decision;
        assert_eq!(
            [r("/w/.env"), r("/w/a/b/.ENV.local"), r("/w/.env.d/x"), r("/w/env")],
            [Ask, Ask, Ask, Allow]
        );
        assert_eq!(r("/h/me/.env"), Allow, "patterns are about the workspace");
        assert_eq!(
            decide(&pol, Action::Fetch(&Host::parse("a.io").unwrap()), Access::Work).decision,
            Deny
        );
    }

    /// SI-5.
    #[test]
    fn a_workspace_only_narrows() {
        let e = Policy::parse(&format!("version = 1\n{TEST}"), Source::Workspace).unwrap_err();
        assert!(matches!(e, BadPolicy::Rule(..)), "{e}");
    }

    #[test]
    fn what_is_not_understood_is_refused_whole() {
        let ok = "id='a'\ntool='shell'\nargv=['x']\ndecision='ask'";
        assert!(Policy::parse(&format!("version=1\n[[rule]]\n{ok}"), Source::Person).is_ok());
        for bad in [
            "".to_string(),
            "version=2".into(),
            "version=1\nunknown=1".into(),
            format!("version=1\n[[rule]]\n{ok}\nextra=1"),
            format!("version=1\n[[rule]]\n{ok}\n[[rule]]\n{ok}"),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid=''\ntool='shell'\nargv=['x']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='allow'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='allow'\nrun_in='none'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='ask'\nrun_in='workspace'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='ask'\ndeny_args=['-x']".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='allow'\nrun_in='workspace'\ndeny_args=['*']".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=[['x','y']]\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['/bin/x']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x',{rest='any'},'y']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x',{rest='some'}]\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='write'\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='write'\npaths=['../x']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='write'\npaths=['/x']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='write'\npaths=['a**b']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='write'\npaths=['a']\nargv=['x']\ndecision='ask'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='fetch'\nhosts=['*']\ndecision='deny'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='fetch'\ndecision='deny'".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='ask'\nmatch=['y']".into(),
            "version=1\n[[rule]]\nid='a'\ntool='shell'\nargv=['x']\ndecision='ask'\nnot_match=['x']".into(),
            "version=1\n[[rule]]\nid='a'\ntool='write'\npaths=['a']\ndecision='ask'\nmatch=['b']".into(),
            "version=1\n[[rule]]\nid='a'\ntool='fetch'\nhosts=['a.io']\ndecision='ask'\nnot_match=['a.io']".into(),
            format!("version=1\n#{}", "x".repeat(LONGEST)),
        ] {
            assert!(Policy::parse(&bad, Source::Person).is_err(), "{bad}");
        }
        let many: String = (0..=MOST_RULES)
            .map(|i| format!("[[rule]]\nid='{i}'\ntool='shell'\nargv=['x']\ndecision='ask'\n"))
            .collect();
        assert!(Policy::parse(&format!("version=1\n{many}"), Source::Person).is_err());
    }

    /// As the plain definition has it, and fast where that would not be.
    #[test]
    fn patterns_are_not_slow() {
        let pat = ["**", "a"].repeat(30);
        let rel = vec!["a"; 2000];
        let t = std::time::Instant::now();
        assert!(!names_match(&[pat, vec!["b"]].concat(), &rel, false));
        assert!(!name_matches(&"*a".repeat(200), &format!("{}b", "a".repeat(4000)), false));
        assert!(t.elapsed().as_secs() < 20);
    }

    proptest! {
        #[test]
        fn patterns_mean_what_they_say(pat in prop::collection::vec("(\\*\\*|[ab*]{1,2})", 1..5), rel in prop::collection::vec("[ab]{1,2}", 0..5)) {
            fn name(p: &[u8], n: &[u8]) -> bool {
                match p.split_first() {
                    None => n.is_empty(),
                    Some((b'*', r)) => (0..=n.len()).any(|i| name(r, &n[i..])),
                    Some((c, r)) => n.split_first().is_some_and(|(x, m)| x == c && name(r, m)),
                }
            }
            fn names(p: &[String], r: &[String]) -> bool {
                match p.split_first() {
                    None => r.is_empty(),
                    Some((x, q)) if x == "**" => (0..=r.len()).any(|i| names(q, &r[i..])),
                    Some((x, q)) => r.split_first().is_some_and(|(y, s)| name(x.as_bytes(), y.as_bytes()) && names(q, s)),
                }
            }
            let a: Vec<&str> = pat.iter().map(String::as_str).collect();
            let b: Vec<&str> = rel.iter().map(String::as_str).collect();
            prop_assert_eq!(names_match(&a, &b, false), names(&pat, &rel));
        }

        /// SI-16: a deny rule wins beside any other rules, in any order.
        #[test]
        fn deny_beats_everything(ds in prop::collection::vec(0u8..3, 1..6), acc in 0u8..3) {
            let text: String = ds.iter().enumerate().map(|(i, d)| {
                let (d, r) = [("allow", "run_in='workspace'"), ("ask", ""), ("deny", "")][*d as usize];
                format!("[[rule]]\nid='{i}'\ntool='shell'\nargv=['ls',{{rest='any'}}]\ndecision='{d}'\n{r}\n")
            }).collect();
            let acc = [Access::Read, Access::Ask, Access::Work][acc as usize];
            let v = run(&policy(&text), "ls -l", acc);
            let worst = ds.iter().max().unwrap();
            prop_assert_eq!(v.decision, [Allow, Ask, Deny][*worst as usize]);
        }
    }
}
