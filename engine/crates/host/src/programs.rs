//! Programs a person has: those that are known, those they added themselves, and how each is to be
//! started (decision 0016). Kept in Axiom's own folder; secret values are in the [`Vault`] only.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use axiom_harness::{Acp, Harness, Var, find, vars};
use axiom_policy::{Place, Reach};
use axiom_proto::{ErrorCode, ProgramInfo, SecretText, VarInfo, VarSet, WireError, limits};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    connections::{Vault, bad, internal},
    delegated::{self, Known},
    paths::private_file,
};

/// Most entries, arguments and variables of one, and bytes of what is shown of them together: they
/// are part of the question before each start.
pub const MOST: usize = 100;
pub const ARGS: usize = 32;
pub const VARS: usize = 32;
const SHOWN: usize = 4 * 1024;
const VALUE: usize = 1024;
const SECRET: usize = 16 * 1024;
const NAME: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    id: String,
    harness: String,
    /// Of one the person added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    /// Of this account only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    vars: Vec<KeptVar>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeptVar {
    name: String,
    /// Absent: it is a secret, in the vault.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<String>,
}

/// What a person says of one.
#[derive(Debug)]
pub struct Said {
    pub harness: Option<String>,
    pub account: Option<String>,
    pub name: Option<String>,
    pub path: Option<String>,
    pub args: Vec<String>,
    pub vars: Vec<VarSet>,
}

/// How one is started, beside how every one is.
#[derive(Debug, Clone, Default)]
pub struct How {
    pub path: Option<Place>,
    pub args: Vec<String>,
    /// By name; of a secret, where in the vault it is.
    vars: Vec<(String, Result<String, String>)>,
}

impl How {
    /// As a shell would have them, a secret by its name.
    pub fn shown(&self) -> Vec<(String, Option<String>)> {
        self.vars.iter().map(|(n, v)| (n.clone(), v.clone().ok())).collect()
    }
}

pub struct Programs {
    file: PathBuf,
    vault: Arc<dyn Vault>,
    all: Mutex<Vec<Kept>>,
    /// Where those that nobody said a path of are looked for; `None`: where programs usually are.
    pub looked_in: Option<Vec<PathBuf>>,
}

fn no_such() -> WireError {
    WireError::new(ErrorCode::NotFound, "no such program")
}

/// Where a value of an entry is in the vault.
fn place(entry: &str, var: &str) -> String {
    format!("{entry}.{var}")
}

impl Programs {
    /// As they were kept. A file that cannot be read is an error: nothing is written over it.
    pub fn open(
        file: &Path,
        vault: Arc<dyn Vault>,
        looked_in: Option<Vec<PathBuf>>,
    ) -> io::Result<Self> {
        let all = match std::fs::read_to_string(file) {
            Ok(t) => serde_json::from_str(&t)
                .map_err(|e| io::Error::other(format!("{}: {e}", file.display())))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Self { file: file.into(), vault, all: Mutex::new(all), looked_in })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Kept>> {
        self.all.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, all: &[Kept]) -> Result<(), WireError> {
        let text = serde_json::to_string_pretty(all).map_err(internal)?;
        private_file(&self.file, &text).map_err(internal)
    }

    pub fn list(&self) -> Vec<ProgramInfo> {
        let one = |k: &Kept| ProgramInfo {
            harness: k.harness.clone(),
            account: k.account.clone(),
            name: k.name.clone(),
            path: k.path.clone(),
            args: k.args.clone(),
            vars: k
                .vars
                .iter()
                .map(|v| VarInfo {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    lasts: v.value.is_some() || self.vault.lasts(),
                })
                .collect(),
        };
        self.lock().iter().map(one).collect()
    }

    /// Those that are known, and the person's own.
    pub fn known(&self) -> Vec<Known> {
        let mut all = delegated::known();
        for k in self.lock().iter() {
            if let Some(name) = &k.name {
                all.push(own(&k.harness, name));
            }
        }
        all
    }

    pub fn known_as(&self, id: &str) -> Option<Known> {
        self.known().into_iter().find(|k| k.id == id)
    }

    /// How one is started, as it is said of it and then of the account.
    pub fn how(&self, harness: &str, account: Option<&str>) -> Result<How, WireError> {
        let all = self.lock();
        let of = |a: Option<&str>| {
            all.iter().find(move |k| k.harness == harness && k.account.as_deref() == a)
        };
        let mut how = How::default();
        for k in of(None).into_iter().chain(account.and_then(|a| of(Some(a)))) {
            if let Some(p) = &k.path {
                how.path = Some(Place::new(p).map_err(|e| bad(format!("{p}: {e}")))?);
            }
            how.args.extend(k.args.iter().cloned());
            for v in &k.vars {
                how.vars.retain(|x| x.0 != v.name);
                let value = v.value.clone().ok_or_else(|| place(&k.id, &v.name));
                how.vars.push((v.name.clone(), value));
            }
        }
        Ok(how)
    }

    /// What it is given, with what is secret.
    pub fn vars(&self, how: &How) -> Result<Vec<Var>, WireError> {
        let one = |(name, v): &(String, Result<String, String>)| {
            let (value, secret) = match v {
                Ok(v) => (v.clone(), false),
                Err(at) => {
                    let found = self.vault.get(at).map_err(internal)?;
                    let gone = || {
                        bad(format!("{name} is not in the system's store any more; set it again"))
                    };
                    (found.ok_or_else(gone)?.to_string(), true)
                }
            };
            Ok(Var { name: name.clone(), value, secret })
        };
        how.vars.iter().map(one).collect()
    }

    /// Says how one is started, or adds one. Answers which. `written`: what agents may write to.
    pub fn set(&self, s: Said, written: &Reach) -> Result<String, WireError> {
        let name = s.name.map(|n| n.trim().to_owned());
        let known = match &s.harness {
            Some(h) => Some(self.known_as(h).ok_or_else(no_such)?),
            None => None,
        };
        let own = known.as_ref().is_none_or(|k| k.own);
        if own && s.account.is_some() {
            return Err(bad("this program has one account: its own sign-in"));
        }
        if own {
            self.named(name.as_deref(), s.harness.as_deref())?;
            if s.path.is_none() {
                return Err(bad("where the program is has to be said: its whole path"));
            }
        } else if name.is_some() {
            return Err(bad("only a program of your own is given a name"));
        }
        if let Some(p) = &s.path {
            if limits::text_len(p) > limits::WORKSPACE {
                return Err(bad("the path is too long"));
            }
            let at = Place::new(p).map_err(|e| bad(format!("{p}: {e}")))?;
            find::take(&at, written).map_err(|e| bad(e.to_string()))?;
            find::beside(&at, written).map_err(|e| bad(e.to_string()))?;
        }
        if s.args.len() > ARGS {
            return Err(bad(format!("a program has at most {ARGS} arguments of its person's")));
        }
        if let Some((a, why)) = s.args.iter().find_map(|a| Some((a, vars::argument(a)?))) {
            return Err(bad(format!("{a:?} {why}")));
        }
        let harness = known.as_ref().map(|k| k.harness.clone()).unwrap_or_else(acp);
        harness.passes(&s.args).map_err(|e| bad(e.to_string()))?;

        let mut all = self.lock();
        let at = s
            .harness
            .as_ref()
            .and_then(|h| all.iter().position(|k| k.harness == *h && k.account == s.account));
        let before = at.map(|i| all[i].clone());
        if at.is_none() && all.len() >= MOST {
            return Err(bad("there are as many programs as there may be"));
        }
        let fresh = || axiom_proto::RequestId::new().0;
        let id = match &before {
            Some(k) => k.id.clone(),
            None => fresh().replacen("q_", "v_", 1),
        };
        let harness = s.harness.unwrap_or_else(|| fresh().replacen("q_", "p_", 1));
        let had = before.as_ref().map(|k| k.vars.as_slice()).unwrap_or_default();
        let (kept, secrets) = understood(s.vars, had)?;
        let shown: usize = s.args.iter().map(String::len).sum::<usize>()
            + s.path.as_ref().map_or(0, String::len)
            + kept
                .iter()
                .map(|v| v.name.len() + v.value.as_ref().map_or(8, String::len))
                .sum::<usize>();
        if shown > SHOWN {
            return Err(bad("its arguments and variables are too long to be shown when it starts"));
        }
        let next = Kept {
            id: id.clone(),
            harness: harness.clone(),
            name,
            account: s.account,
            path: s.path,
            args: s.args,
            vars: kept,
        };
        // Those that were there before and are another now can be put back.
        let mut done: Vec<(String, Option<Zeroizing<String>>)> = Vec::new();
        let undo = |done: &[(String, Option<Zeroizing<String>>)]| {
            for (at, was) in done {
                let _ = match was {
                    Some(v) => self.vault.put(at, v),
                    None => self.vault.remove(at),
                };
            }
        };
        for (n, v) in secrets {
            let at = place(&id, &n);
            let was = self.vault.get(&at).ok().flatten();
            if let Err(e) = self.vault.put(&at, &v) {
                undo(&done);
                return Err(internal(e));
            }
            done.push((at, was));
        }
        let mut whole = all.clone();
        match at {
            Some(i) => whole[i] = next.clone(),
            None => whole.push(next.clone()),
        }
        if let Err(e) = self.write(&whole) {
            undo(&done);
            return Err(e);
        }
        *all = whole;
        // What is no secret of it any more.
        for v in had.iter().filter(|v| v.value.is_none()) {
            if !next.vars.iter().any(|n| n.name == v.name && n.value.is_none()) {
                let _ = self.vault.remove(&place(&id, &v.name));
            }
        }
        Ok(harness)
    }

    /// No other is called so: who runs is never in doubt.
    fn named(&self, name: Option<&str>, but: Option<&str>) -> Result<(), WireError> {
        let Some(n) = name.filter(|n| {
            !n.is_empty() && limits::text_len(n) <= NAME && !n.chars().any(char::is_control)
        }) else {
            return Err(bad("a program's name has between 1 and 100 bytes"));
        };
        let same = |a: &str| {
            let plain = |s: &str| -> String {
                s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
            };
            plain(a) == plain(n)
        };
        if same("Axiom") || self.known().iter().any(|k| Some(k.id.as_str()) != but && same(&k.name))
        {
            return Err(bad("another program has this name"));
        }
        Ok(())
    }

    /// One of the person's own is there no more; of a known one, what was said of it. What runs goes
    /// on until it ends.
    pub fn remove(&self, harness: &str, account: Option<&str>) -> Result<(), WireError> {
        let here = |k: &Kept| k.harness == harness && k.account.as_deref() == account;
        if !self.lock().iter().any(here) {
            return Err(no_such());
        }
        self.drop_where(here)
    }

    /// An account has been removed.
    pub fn forget(&self, account: &str) -> Result<(), WireError> {
        let here = |k: &Kept| k.account.as_deref() == Some(account);
        if !self.lock().iter().any(here) {
            return Ok(());
        }
        self.drop_where(here)
    }

    /// The secrets go first: one without its entry would stay for good.
    fn drop_where(&self, here: impl Fn(&Kept) -> bool) -> Result<(), WireError> {
        let mut all = self.lock();
        for k in all.iter().filter(|k| here(k)) {
            for v in k.vars.iter().filter(|v| v.value.is_none()) {
                self.vault.remove(&place(&k.id, &v.name)).map_err(internal)?;
            }
        }
        let next: Vec<Kept> = all.iter().filter(|k| !here(k)).cloned().collect();
        self.write(&next)?;
        *all = next;
        Ok(())
    }
}

type Secrets = Vec<(String, Zeroizing<String>)>;

/// What is kept of variables, and what goes to the vault. One of those a secret that was said before
/// may be left as it is.
fn understood(said: Vec<VarSet>, had: &[KeptVar]) -> Result<(Vec<KeptVar>, Secrets), WireError> {
    if said.len() > VARS {
        return Err(bad(format!("a program has at most {VARS} variables of its person's")));
    }
    let (mut kept, mut secrets): (Vec<KeptVar>, Secrets) = (Vec::new(), Vec::new());
    for VarSet { name, value, secret } in said {
        // Cleared when it is dropped.
        let value = value.map(|SecretText(v)| Zeroizing::new(v));
        if let Some(why) = vars::refused(&name) {
            return Err(bad(format!("{name:?} cannot be set: it {why}")));
        }
        if kept.iter().any(|k| k.name == name) {
            return Err(bad(format!("{name} is there twice")));
        }
        let secret = secret || vars::secret(&name);
        let Some(value) = value else {
            if !had.iter().any(|h| h.name == name && h.value.is_none()) {
                return Err(bad(format!("{name} has no value")));
            }
            kept.push(KeptVar { name, value: None });
            continue;
        };
        if secret {
            if value.len() < vars::SHORTEST_SECRET || value.len() > SECRET || value.contains('\0') {
                return Err(bad(format!("{name}: a secret has between 8 and 16384 bytes")));
            }
            kept.push(KeptVar { name: name.clone(), value: None });
            secrets.push((name, value));
        } else {
            if value.is_empty() || value.len() > VALUE || value.chars().any(char::is_control) {
                return Err(bad(format!(
                    "{name}: a value has between 1 and 1024 bytes on one line"
                )));
            }
            kept.push(KeptVar { name, value: Some(value.to_string()) });
        }
    }
    Ok((kept, secrets))
}

fn acp() -> Harness {
    Harness::Acp(Acp::default())
}

fn own(id: &str, name: &str) -> Known {
    Known { id: id.into(), name: name.into(), program: String::new(), harness: acp(), own: true }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::connections::{MemoryVault, Text};

    const CANARY: &str = "canary-81723-secret";

    struct T {
        d: tempfile::TempDir,
        vault: Arc<MemoryVault>,
    }
    impl T {
        fn new() -> Self {
            let t = Self { d: tempfile::tempdir().unwrap(), vault: Arc::default() };
            for x in ["bin", "open", "w"] {
                std::fs::create_dir(t.at(x)).unwrap();
            }
            for x in ["bin/mine", "open/mine", "w/mine"] {
                std::fs::write(t.at(x), "#!/bin/sh\n").unwrap();
                std::fs::set_permissions(t.at(x), PermissionsExt::from_mode(0o755)).unwrap();
            }
            std::fs::set_permissions(t.at("open"), PermissionsExt::from_mode(0o1777)).unwrap();
            t
        }
        fn at(&self, s: &str) -> String {
            let d = std::fs::canonicalize(self.d.path()).unwrap();
            d.join(s).to_str().unwrap().into()
        }
        fn open(&self) -> Programs {
            Programs::open(&self.d.path().join("programs.json"), self.vault.clone(), None).unwrap()
        }
        fn file(&self) -> String {
            std::fs::read_to_string(self.d.path().join("programs.json")).unwrap_or_default()
        }
        fn written(&self) -> Reach {
            Reach::of([Place::new(self.at("w")).unwrap()])
        }
        fn mine(&self) -> Said {
            Said {
                harness: None,
                account: None,
                name: Some("Mine".into()),
                path: Some(self.at("bin/mine")),
                args: vec!["acp".into()],
                vars: vec![],
            }
        }
        fn of(&self, harness: &str) -> Said {
            let (harness, name, path) = (Some(harness.into()), None, None);
            Said { harness, name, path, args: vec![], ..self.mine() }
        }
        fn held(&self) -> usize {
            self.vault.count()
        }
    }
    fn shown(name: &str, value: &str) -> VarSet {
        VarSet { name: name.into(), value: Some(SecretText(value.into())), secret: false }
    }
    fn kept(name: &str) -> VarSet {
        VarSet { name: name.into(), value: None, secret: true }
    }

    #[test]
    fn one_of_the_persons_own_is_added_changed_and_removed() {
        let t = T::new();
        let p = t.open();
        let id = p.set(t.mine(), &t.written()).unwrap();
        assert!(id.starts_with("p_"));
        let k = t.open().known_as(&id).unwrap();
        assert_eq!((k.name.as_str(), k.own), ("Mine", true));
        assert_eq!(p.how(&id, None).unwrap().args, ["acp"]);
        // The same, by another name; not one that another has.
        let again = Said { harness: Some(id.clone()), name: Some("My own".into()), ..t.mine() };
        assert_eq!(p.set(again, &t.written()).unwrap(), id);
        let again = Said { harness: Some(id.clone()), name: Some("My own".into()), ..t.mine() };
        assert_eq!(p.set(again, &t.written()).unwrap(), id);
        assert_eq!(p.known_as(&id).unwrap().name, "My own");
        assert_eq!(p.list().len(), 1);
        for n in ["", "Claude Code", "claude  code", "axiom", "my-own", "a\nb"] {
            let e = p.set(Said { name: Some(n.into()), ..t.mine() }, &t.written());
            assert!(e.is_err(), "{n:?}");
        }
        p.remove(&id, None).unwrap();
        assert!(p.known_as(&id).is_none() && p.remove(&id, None).is_err());
        assert!(t.open().list().is_empty());
        assert_eq!(p.known().len(), delegated::known().len());
    }

    #[test]
    fn what_could_have_been_put_there_is_not_taken() {
        let t = T::new();
        let p = t.open();
        let at = |x: &str| Said { path: Some(t.at(x)), ..t.mine() };
        for (s, why) in [
            (at("w/mine"), "agent may write"),
            (at("open/mine"), "anyone"),
            (at("bin"), "not a file"),
            (at("bin/none"), "cannot be used"),
            (Said { path: Some("bin/mine".into()), ..t.mine() }, "bin/mine"),
            (Said { path: None, ..t.mine() }, "whole path"),
        ] {
            let e = p.set(s, &t.written()).unwrap_err();
            assert!(e.message.contains(why), "{e}");
        }
        assert!(p.list().is_empty() && t.file().is_empty());
        // Of a known one the same.
        let s = Said { path: Some(t.at("w/mine")), ..t.of("codex") };
        assert!(p.set(s, &t.written()).is_err());
        let s = Said { path: Some(t.at("bin/mine")), ..t.of("codex") };
        assert_eq!(p.set(s, &t.written()).unwrap(), "codex");
        assert_eq!(p.how("codex", None).unwrap().path.unwrap().as_str(), t.at("bin/mine"));
    }

    #[test]
    fn what_cannot_be_said_of_one() {
        let t = T::new();
        let p = t.open();
        let w = t.written();
        let vars = |v: Vec<VarSet>| Said { vars: v, ..t.of("claude") };
        let args = |h: &str, a: &[&str]| Said {
            args: a.iter().map(|a| (*a).to_owned()).collect(),
            ..t.of(h)
        };
        for (s, why) in [
            (vars(vec![shown("NODE_OPTIONS", "x")]), "NODE_OPTIONS"),
            (vars(vec![shown("ANTHROPIC_BASE_URL", "https://x")]), "elsewhere"),
            (vars(vec![shown("CLAUDE_CONFIG_DIR", "/x")]), "set by Axiom"),
            (vars(vec![shown("A", "1"), shown("A", "2")]), "twice"),
            (vars(vec![shown("A", "")]), "on one line"),
            (vars(vec![shown("A", "a\nb")]), "on one line"),
            (vars(vec![shown("X_TOKEN", "short")]), "between 8"),
            (vars(vec![kept("X_TOKEN")]), "has no value"),
            (vars(vec![VarSet { secret: false, ..kept("A") }]), "has no value"),
            (vars((0..33).map(|n| shown(&format!("A{n}"), "x")).collect()), "at most 32"),
            (
                vars((0..5).map(|n| shown(&format!("A{n}"), &"x".repeat(1000))).collect()),
                "too long",
            ),
            (args("claude", &["--dangerously-skip-permissions"]), "let go"),
            (args("claude", &["--permission-mode", "acceptEdits"]), "not passed on"),
            (args("claude", &["--settings={}"]), "not passed on"),
            (args("claude", &["--fallback-model"]), "not passed on"),
            (args("claude", &["--fallback-model", "--x"]), "not passed on"),
            (args("codex", &["-c", "sandbox_mode=x"]), "not passed on"),
            (args("codex", &["--config=approval_policy=never"]), "not passed on"),
            (args("goose", &["--api-key=x"]), "secret"),
            (args("goose", &["x"; 33]), "at most 32"),
            (Said { name: Some("Other".into()), ..t.of("goose") }, "of your own"),
            (t.of("nobody"), "no such program"),
            (Said { account: Some("c_1".into()), ..t.mine() }, "one account"),
        ] {
            let e = p.set(s, &w).unwrap_err();
            assert!(e.message.contains(why), "{e} / {why}");
        }
        assert!(p.list().is_empty() && t.held() == 0);
        for s in [
            args("claude", &["--fallback-model", "sonnet", "--max-turns=9"]),
            args("codex", &["-c", "model_verbosity=low"]),
            args("goose", &["--profile", "mine"]),
        ] {
            p.set(s, &w).unwrap();
        }
    }

    #[test]
    fn secrets_are_in_the_vault_and_nowhere_else() {
        let t = T::new();
        let p = t.open();
        let w = t.written();
        let vars = vec![
            shown("AWS_REGION", "eu-west-1"),
            // Whatever is said of it.
            shown("AWS_BEARER_TOKEN_BEDROCK", CANARY),
            VarSet { secret: true, ..shown("OTHER", "other-secret") },
        ];
        p.set(Said { vars, ..t.of("claude") }, &w).unwrap();
        assert!(!t.file().contains(CANARY) && !t.file().contains("other-secret"), "{}", t.file());
        assert!(!format!("{:?}", p.list()).contains(CANARY) && t.held() == 2);
        let of = |p: &Programs| p.vars(&p.how("claude", None).unwrap()).unwrap();
        let all = of(&t.open());
        let value =
            |n: &str| all.iter().find(|v| v.name == n).map(|v| (v.value.as_str(), v.secret));
        assert_eq!(value("AWS_BEARER_TOKEN_BEDROCK"), Some((CANARY, true)));
        assert_eq!(value("AWS_REGION"), Some(("eu-west-1", false)));
        assert!(!format!("{all:?}").contains(CANARY));
        // As they are named before a start.
        let how = p.how("claude", None).unwrap();
        assert!(how.shown().contains(&("OTHER".into(), None)));
        assert!(how.shown().contains(&("AWS_REGION".into(), Some("eu-west-1".into()))));
        // One stays, one is another, one is gone.
        let vars = vec![kept("AWS_BEARER_TOKEN_BEDROCK"), shown("AWS_REGION", "us-east-1")];
        p.set(Said { vars, ..t.of("claude") }, &w).unwrap();
        assert_eq!(t.held(), 1);
        assert!(of(&p).iter().any(|v| v.value == CANARY));
        // Shown from now on: no more in the vault.
        let vars = vec![VarSet { secret: true, ..shown("X", "some-secret-x") }];
        p.set(Said { vars, ..t.of("claude") }, &w).unwrap();
        p.set(Said { vars: vec![shown("X", "now shown")], ..t.of("claude") }, &w).unwrap();
        assert_eq!((t.held(), of(&p)[0].secret), (0, false));
        // There is none to be kept.
        assert!(p.set(Said { vars: vec![kept("X")], ..t.of("claude") }, &w).is_err());
        // What the vault has lost is said.
        let vars = vec![shown("X_TOKEN", CANARY)];
        p.set(Said { vars, ..t.of("claude") }, &w).unwrap();
        t.vault.clear();
        let e = p.vars(&p.how("claude", None).unwrap()).unwrap_err();
        assert!(e.message.contains("X_TOKEN") && e.message.contains("set it again"), "{e}");
        p.remove("claude", None).unwrap();
        assert!(p.list().is_empty());
    }

    #[test]
    fn an_account_has_its_own_over_its_programs() {
        let t = T::new();
        let p = t.open();
        let w = t.written();
        let vars = vec![shown("A", "1"), shown("B", "1")];
        let args = vec!["--max-turns=9".to_owned()];
        p.set(Said { vars, args, ..t.of("claude") }, &w).unwrap();
        let account = Some("c_1".to_owned());
        let vars = vec![shown("B", "2"), shown("C_TOKEN", CANARY)];
        let args = vec!["--betas=x".to_owned()];
        p.set(Said { account, vars, args, ..t.of("claude") }, &w).unwrap();
        let of = |a| {
            let how = p.how("claude", a).unwrap();
            (how.args.clone(), how.shown())
        };
        let s = |x: &str| x.to_owned();
        let v = |n: &str, x: &str| (s(n), Some(s(x)));
        assert_eq!(of(None), (vec![s("--max-turns=9")], vec![v("A", "1"), v("B", "1")]));
        assert_eq!(of(Some("c_2")), of(None));
        assert_eq!(
            of(Some("c_1")),
            (
                vec![s("--max-turns=9"), s("--betas=x")],
                vec![v("A", "1"), v("B", "2"), (s("C_TOKEN"), None)]
            )
        );
        // With the account goes what was said of it.
        p.forget("c_1").unwrap();
        assert_eq!((of(Some("c_1")), t.held()), (of(None), 0));
        p.forget("c_1").unwrap();
    }

    /// Nothing is half said: the store takes it all, or it is as it was.
    #[test]
    fn a_store_that_fails_changes_nothing() {
        struct Some2(MemoryVault, Mutex<u32>);
        impl Vault for Some2 {
            fn put(&self, id: &str, secret: &Text) -> Result<(), String> {
                let mut n = self.1.lock().unwrap();
                if *n == 0 {
                    return Err("locked".into());
                }
                *n -= 1;
                self.0.put(id, secret)
            }
            fn get(&self, id: &str) -> Result<Option<Text>, String> {
                self.0.get(id)
            }
            fn remove(&self, id: &str) -> Result<(), String> {
                self.0.remove(id)
            }
        }
        let t = T::new();
        let v = Arc::new(Some2(MemoryVault::default(), Mutex::new(1)));
        let file = t.d.path().join("programs.json");
        let p = Programs::open(&file, v.clone(), None).unwrap();
        let two = |a: &str| vec![shown("A_TOKEN", a), shown("B_TOKEN", "bbbbbbbb")];
        let e = p.set(Said { vars: two(CANARY), ..t.of("claude") }, &t.written()).unwrap_err();
        assert!(e.message.contains("locked") && p.list().is_empty() && v.0.count() == 0);
        *v.1.lock().unwrap() = 2;
        p.set(Said { vars: two(CANARY), ..t.of("claude") }, &t.written()).unwrap();
        // One and its undoing.
        *v.1.lock().unwrap() = 1;
        assert!(p.set(Said { vars: two("another-one"), ..t.of("claude") }, &t.written()).is_err());
        std::fs::write(&file, "{").unwrap();
        assert!(Programs::open(&file, v, None).is_err());
    }
}
