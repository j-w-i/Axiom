//! What a sandbox is to allow, as data. `axiom-spawn` turns it into what the operating system
//! understands, or fails (SI-8). Made only here, so that certain things are never allowed whoever
//! asks: Axiom's own state (SI-12), what is protected (SI-11), writing to a whole home folder.

use crate::{
    caps::{Caps, Net, Reach},
    paths::{self, Place},
};

/// What is kept from every agent, whatever its capabilities say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keep {
    home: Place,
    own: Vec<Place>,
    secret: Vec<Place>,
}

/// Below the home folder: what signs a person in, and what others write down about them.
const SECRET: &[&str] = &[
    ".ssh",
    ".gnupg",
    ".aws",
    ".azure",
    ".config/gcloud",
    ".config/gh",
    ".config/hub",
    ".config/op",
    ".config/rclone",
    ".config/sops",
    ".kube",
    ".docker/config.json",
    ".netrc",
    "_netrc",
    ".pgpass",
    ".my.cnf",
    ".git-credentials",
    ".config/git/credentials",
    ".npmrc",
    ".yarnrc.yml",
    ".pypirc",
    ".gem/credentials",
    ".cargo/credentials",
    ".cargo/credentials.toml",
    ".gradle/gradle.properties",
    ".m2/settings.xml",
    ".terraform.d/credentials.tfrc.json",
    ".vault-token",
    ".password-store",
    ".local/share/keyrings",
    ".local/share/kwalletd",
    ".bash_history",
    ".zsh_history",
    ".zsh_sessions",
    ".python_history",
    ".node_repl_history",
    ".psql_history",
    ".claude",
    ".claude.json",
    ".codex",
    ".copilot",
    ".gemini",
    ".antigravity",
    ".cursor",
    ".mozilla",
    ".config/google-chrome",
    ".config/chromium",
    ".config/BraveSoftware",
    ".thunderbird",
    "Library/Keychains",
    "Library/Cookies",
    "Library/Mail",
    "Library/Messages",
    "Library/Safari",
    "Library/Calendars",
    "Library/Containers",
    "Library/Group Containers",
    "Library/Application Support",
    "Library/LaunchAgents",
    "Library/Mobile Documents",
    "AppData/Roaming",
    "AppData/Local/Google",
    "AppData/Local/Microsoft",
];

impl Keep {
    /// `own` is where Axiom keeps its state, sockets and binaries.
    pub fn new(home: Place, own: Vec<Place>) -> Self {
        let secret = SECRET.iter().filter_map(|s| home.join(s).ok()).collect();
        Self { home, own, secret }
    }

    pub fn with_secret(mut self, more: impl IntoIterator<Item = Place>) -> Self {
        self.secret.extend(more);
        self
    }

    /// What nobody reads.
    pub fn hidden(&self) -> impl Iterator<Item = &Place> {
        self.own.iter().chain(&self.secret)
    }

    pub fn hides(&self, p: &Place) -> bool {
        self.hidden().any(|h| h.covers_folded(p))
    }

    pub fn home(&self) -> &Place {
        &self.home
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadProfile {
    #[error("{0} is too much to write to")]
    TooWide(Place),
    #[error("{0} holds Axiom's own files")]
    Own(Place),
    #[error("{0} holds what signs you in, or is inside it")]
    Secret(Place),
    #[error("{0} is protected: {1}")]
    Protected(Place, &'static str),
    #[error("the agent may not start anything")]
    NoRun,
}

/// What one contained process may do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    read: Reach,
    write: Reach,
    net: Net,
    hidden: Vec<Place>,
}

impl Profile {
    /// `system` is read by every program: libraries, tools, certificates. `scratch` is this agent's
    /// own temporary folder.
    pub fn of(
        caps: &Caps,
        keep: &Keep,
        system: &Reach,
        scratch: &Place,
    ) -> Result<Self, BadProfile> {
        if !caps.run {
            return Err(BadProfile::NoRun);
        }
        for r in caps.write.roots() {
            if r.parent().is_none() || r.covers_folded(&keep.home) {
                return Err(BadProfile::TooWide(r.clone()));
            }
            if let Some(why) = paths::protected(r) {
                return Err(BadProfile::Protected(r.clone(), why));
            }
            for o in &keep.own {
                if r.covers_folded(o) || o.covers_folded(r) {
                    return Err(BadProfile::Own(r.clone()));
                }
            }
            for s in &keep.secret {
                if r.covers_folded(s) || s.covers_folded(r) {
                    return Err(BadProfile::Secret(r.clone()));
                }
            }
        }
        // A folder inside what is hidden cannot be shown: hiding wins, so say so at once.
        for r in caps.read.roots() {
            if let Some(h) = keep.own.iter().find(|h| h.covers_folded(r)) {
                return Err(BadProfile::Own(h.clone()));
            }
            if let Some(h) = keep.secret.iter().find(|h| h.covers_folded(r)) {
                return Err(BadProfile::Secret(h.clone()));
            }
        }
        let scratch = std::iter::once(scratch.clone());
        let write: Vec<Place> = caps.write.roots().iter().cloned().chain(scratch).collect();
        let read = caps.read.roots().iter().chain(system.roots()).chain(&write).cloned();
        Ok(Self {
            read: Reach::of(read),
            net: caps.net.clone(),
            hidden: keep.hidden().cloned().collect(),
            write: Reach::of(write),
        })
    }

    /// The same without writing (except the scratch folder) and without a network.
    pub fn read_only(
        caps: &Caps,
        keep: &Keep,
        system: &Reach,
        scratch: &Place,
    ) -> Result<Self, BadProfile> {
        let caps = Caps { write: Reach::none(), net: Net::none(), ..caps.clone() };
        Self::of(&caps, keep, system, scratch)
    }

    pub fn read(&self) -> &Reach {
        &self.read
    }
    pub fn write(&self) -> &Reach {
        &self.write
    }
    pub fn net(&self) -> &Net {
        &self.net
    }
    pub fn hidden(&self) -> &[Place] {
        &self.hidden
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    use crate::Tools;

    fn p(s: &str) -> Place {
        Place::new(s).unwrap()
    }
    fn keep() -> Keep {
        Keep::new(p("/h/me"), vec![p("/h/me/Library/Application Support/Axiom"), p("/opt/axiom")])
    }
    fn caps(read: &[&str], write: &[&str]) -> Caps {
        Caps {
            read: Reach::of(read.iter().map(|s| p(s))),
            write: Reach::of(write.iter().map(|s| p(s))),
            run: true,
            tools: Tools::All,
            ..Caps::default()
        }
    }
    fn of(c: &Caps) -> Result<Profile, BadProfile> {
        Profile::of(c, &keep(), &Reach::of([p("/usr")]), &p("/tmp/x"))
    }

    #[test]
    fn what_is_never_written() {
        for (w, why) in [
            ("/", "TooWide"),
            ("/h", "TooWide"),
            ("/h/me", "TooWide"),
            ("/H/ME", "TooWide"),
            ("/opt", "Own"),
            ("/opt/axiom/bin", "Own"),
            ("/OPT/Axiom/x", "Own"),
            ("/h/me/.ssh", "Protected"),
            ("/h/me/w/.git/hooks", "Protected"),
            ("/h/me/Library", "Own"),
            ("/h/me/.config", "Secret"),
            ("/h/me/.kube/x", "Secret"),
        ] {
            let e = of(&caps(&[], &[w])).unwrap_err();
            assert!(format!("{e:?}").starts_with(why), "{w}: {e:?}");
        }
        assert!(of(&caps(&["/h/me/w"], &["/h/me/w"])).is_ok());
        assert!(matches!(of(&Caps { run: false, ..caps(&[], &[]) }), Err(BadProfile::NoRun)));
    }

    #[test]
    fn what_is_never_read() {
        for r in [
            "/h/me/.ssh",
            "/h/me/.ssh/x",
            "/h/me/Library/Application Support/Axiom/d",
            "/h/me/.SSH/x",
        ] {
            assert!(of(&caps(&[r], &[])).is_err(), "{r}");
        }
        // A wider folder may be read; what is hidden in it stays hidden.
        let f = of(&caps(&["/h/me"], &[])).unwrap();
        assert!(f.hidden().contains(&p("/h/me/.ssh")));
        assert!(keep().hides(&p("/h/me/.ssh/id_ed25519")) && keep().hides(&p("/h/me/.Aws")));
        assert!(!keep().hides(&p("/h/me/w")));
    }

    #[test]
    fn nothing_asked_for_is_nothing_but_the_scratch_folder() {
        let f = of(&caps(&[], &[])).unwrap();
        assert_eq!(f.write().roots(), [p("/tmp/x")]);
        assert_eq!(f.read().roots(), [p("/tmp/x"), p("/usr")]);
        assert!(f.net().is_empty());
        let all =
            Caps { net: Net::of([crate::Host::parse("a.io").unwrap()]), ..caps(&["/w"], &["/w"]) };
        let r = Profile::read_only(&all, &keep(), &Reach::of([p("/usr")]), &p("/tmp/x")).unwrap();
        assert_eq!(r.write().roots(), [p("/tmp/x")]);
        assert!(r.net().is_empty() && r.read().covers(&p("/w/a")));
    }
}
