// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::{path::Path, time::Duration};

use axiom_policy::{
    Answer, Ask, Budget, Caps, Keep, Net, Place, Profile, Reach, Requests, Tools, Unsandboxed,
};
use axiom_spawn::{Contain, Ended, Env, Job, SpawnError, spawn};
use tokio::io::AsyncReadExt;

pub struct Fixture {
    pub dir: tempfile::TempDir,
}

pub fn p(s: impl AsRef<Path>) -> Place {
    Place::new(s).unwrap()
}

/// A home with a secret, a workspace with a repository, Axiom's own folder, and a folder outside.
pub fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    for x in [
        "home/.ssh",
        "home/w/.git/hooks",
        "home/w/src",
        "home/w/sub",
        "own",
        "out",
        "tmp",
        "home/lib",
    ] {
        std::fs::create_dir_all(d.join(x)).unwrap();
    }
    for (f, t) in [
        ("home/.ssh/id", "SECRET"),
        ("own/token", "SECRET"),
        ("out/f", "outside"),
        ("home/w/.git/config", "orig"),
        ("home/w/src/a.txt", "text"),
        ("home/lib/l", "lib"),
    ] {
        std::fs::write(d.join(f), t).unwrap();
    }
    Fixture { dir }
}

pub fn system() -> Reach {
    let all = [
        "/bin",
        "/usr",
        "/sbin",
        "/lib",
        "/lib64",
        "/etc/alternatives",
        "/etc/ld.so.cache",
        "/System",
        "/Library/Apple",
        "/private/var/select",
        "/private/etc/localtime",
        "/private/var/db/timezone",
        "/etc/passwd",
        "/private/etc/passwd",
    ];
    Reach::of(all.iter().filter(|s| Path::new(s).exists()).map(p))
}

impl Fixture {
    pub fn at(&self, s: &str) -> Place {
        p(std::fs::canonicalize(self.dir.path()).unwrap().join(s))
    }
    pub fn read(&self, s: &str) -> Option<String> {
        std::fs::read_to_string(self.dir.path().join(s)).ok()
    }
    pub fn keep(&self) -> Keep {
        Keep::new(self.at("home"), vec![self.at("own")])
    }
    pub fn caps(&self) -> Caps {
        Caps {
            read: Reach::of([self.at("home/w"), self.at("home/lib")]),
            write: Reach::of([self.at("home/w")]),
            net: Net::none(),
            run: true,
            tools: Tools::All,
            budget: Budget::default(),
        }
    }
    pub fn profile(&self) -> Profile {
        Profile::of(&self.caps(), &self.keep(), &system(), &self.at("tmp")).unwrap()
    }
    pub fn env(&self) -> Env {
        let path = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"].map(p);
        Env::new(&path, &self.at("home"), &self.at("tmp"))
    }
    pub fn job(&self, script: &str, profile: Profile) -> Job {
        Job {
            program: p("/bin/sh"),
            args: vec!["-c".into(), script.into()],
            cwd: self.at("home/w"),
            env: self.env(),
            stdin: false,
            contain: Contain::Sandbox(profile),
        }
    }
    pub async fn sh(&self, script: &str) -> Out {
        run(self.job(script, self.profile())).await.unwrap()
    }
}

#[derive(Debug)]
pub struct Out {
    pub ended: Ended,
    pub out: String,
    pub err: String,
}
impl Out {
    pub fn ok(&self) -> bool {
        self.ended.code == Some(0)
    }
}

pub async fn run(job: Job) -> Result<Out, SpawnError> {
    let mut r = spawn(job)?;
    let (mut o, mut e) = (r.stdout.take().unwrap(), r.stderr.take().unwrap());
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let all = async {
        let (ended, _, _) =
            tokio::join!(r.wait(), o.read_to_end(&mut out), e.read_to_end(&mut err));
        ended.unwrap()
    };
    let ended = tokio::time::timeout(Duration::from_secs(60), all).await.expect("it ends");
    Ok(Out {
        ended,
        out: String::from_utf8_lossy(&out).into_owned(),
        err: String::from_utf8_lossy(&err).into_owned(),
    })
}

/// As a person would give it.
pub fn unsandboxed() -> Unsandboxed {
    let ask = Ask {
        chain: vec![],
        workspace: "w".into(),
        tool: "shell".into(),
        schema: String::new(),
        subject: "x".into(),
        scope: "everything".into(),
        unsandboxed: true,
    };
    let mut r = Requests::default();
    let id = r.open(&ask).unwrap();
    r.answer(&id, Answer::Once, &ask.digest()).unwrap().unwrap().unsandboxed(&ask).unwrap()
}

pub fn alive(pid: i32) -> bool {
    // Not a zombie either.
    let o = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&o.stdout).trim().to_owned();
    !s.is_empty() && !s.starts_with('Z')
}
