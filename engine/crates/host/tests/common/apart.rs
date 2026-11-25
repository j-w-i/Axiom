#![allow(dead_code)]
//! A session on a folder, with agents that may work in copies of it, through the real socket. The model
//! is a server that answers from a script.

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use axiom_host::{Config, client::Client};
use axiom_proto::*;
use serde_json::{Value, json};

use super::common::*;
use super::server::*;

pub struct T {
    pub f: Fixture,
    pub server: Server,
    pub c: Client,
    pub s: SessionId,
    pub connection: String,
    pub seq: u64,
}

pub fn config(f: &Fixture) -> Config {
    let mut c = Config::new(f.paths.clone());
    c.home = Some(f.dir.path().join("home"));
    c
}

pub fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

impl T {
    pub async fn new(script: Vec<Reply>) -> Self {
        let f = fixture();
        for d in ["w", "home"] {
            std::fs::create_dir(f.dir.path().join(d)).unwrap();
        }
        let server = Server::start(script).await;
        f.serve_with(config(&f));
        let mut c = f.client().await;
        let call = Call::ConnectionAdd {
            name: "Fake".into(),
            provider: "openai-compatible".into(),
            url: Some(server.url.clone()),
            secret: None,
        };
        let connection = c.call(call).await.unwrap()["connection"].as_str().unwrap().to_owned();
        let w = Some(f.dir.path().join("w").to_str().unwrap().to_owned());
        let v = c.call(Call::SessionCreate { title: "S".into(), workspace: w }).await.unwrap();
        let s = SessionId(v["session"].as_str().unwrap().into());
        c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
        Self { f, server, c, s, connection, seq: 0 }
    }
    pub async fn restart(&mut self) {
        self.c.call(Call::Shutdown { force: true }).await.unwrap();
        while self.f.paths.token().exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.f.serve_with(config(&self.f));
        self.c = self.f.client().await;
        self.c.call(Call::Follow { session: self.s.clone(), after: self.seq }).await.unwrap();
    }
    pub fn path(&self, n: &str) -> PathBuf {
        self.f.dir.path().join("w").join(n)
    }
    pub fn write(&self, n: &str, text: &str) {
        std::fs::create_dir_all(self.path(n).parent().unwrap()).unwrap();
        std::fs::write(self.path(n), text).unwrap();
    }
    pub fn file(&self, n: &str) -> Option<String> {
        std::fs::read_to_string(self.path(n)).ok()
    }
    pub fn copy_of(&self, a: &AgentId) -> PathBuf {
        self.f.dir.path().join("home/.axiom-work").join(&self.s.0).join(&a.0)
    }
    pub fn copied(&self, a: &AgentId, n: &str) -> Option<String> {
        std::fs::read_to_string(self.copy_of(a).join(n)).ok()
    }
    pub async fn agent(&mut self, name: &str, access: Access, apart: bool) -> AgentId {
        let call = Call::AgentAdd {
            session: self.s.clone(),
            name: name.into(),
            execution: Execution {
                harness: "axiom".into(),
                provider: None,
                model: Some("m".into()),
                connection: Some(self.connection.clone()),
                effort: None,
                environment: None,
            },
            access,
            trust: false,
            instructions: vec![],
            apart,
            plan: false,
        };
        AgentId(self.c.call(call).await.unwrap()["agent"].as_str().unwrap().into())
    }
    /// It is told something and has the model do this.
    pub async fn does(&mut self, a: &AgentId, command: &str) -> (Outcome, Option<String>) {
        let args = json!({ "command": command }).to_string();
        self.server.add(calls(&[("c1", "shell", &args)]));
        self.server.add(text(&["done"]));
        self.tell(a).await.unwrap();
        self.ended().await
    }
    pub async fn tell(&mut self, a: &AgentId) -> Result<Value, axiom_host::client::ClientError> {
        self.say(a, "go").await
    }
    pub async fn say(
        &mut self,
        a: &AgentId,
        text: &str,
    ) -> Result<Value, axiom_host::client::ClientError> {
        let (session, agent) = (self.s.clone(), a.clone());
        self.c.call(Call::Prompt { session, agent, text: text.into(), key: None }).await
    }
    /// All of the session's history.
    pub fn history(&self) -> Vec<Body> {
        let j = axiom_journal::Reader::open(&self.f.paths.journal()).unwrap();
        let (mut all, mut after) = (Vec::new(), 0);
        loop {
            let page = j.read(&self.s, after, 1 << 20).unwrap();
            let Some(last) = page.last() else { return all };
            after = last.seq;
            all.extend(page.into_iter().map(|e| e.body));
        }
    }
    /// An agent's items of a kind.
    pub fn items(&self, a: &AgentId, kind: ItemKind) -> Vec<ItemId> {
        let of = |b: Body| match b {
            Body::ItemStarted { item, agent, kind: k, .. } if agent == *a && k == kind => {
                Some(item)
            }
            _ => None,
        };
        self.history().into_iter().filter_map(of).collect()
    }
    pub async fn ended(&mut self) -> (Outcome, Option<String>) {
        loop {
            let m = self.c.next_within(LONG).await.unwrap().expect("the run ends");
            let Message::Events { events, .. } = m else { continue };
            for e in events {
                self.seq = e.seq;
                match e.body {
                    Body::RunEnded { outcome, error, .. } => return (outcome, error),
                    // Commands that write are asked about, however they are contained.
                    Body::RequestOpened { request, digest, .. } => {
                        let (session, answer) = (self.s.clone(), Answer::Once);
                        let call = Call::Answer { session, request, answer, digest, words: None };
                        self.c.call(call).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
    pub async fn changes(&mut self, a: &AgentId) -> CopyChanges {
        let call = Call::CopyChanges { session: self.s.clone(), agent: a.clone() };
        serde_json::from_value(self.c.call(call).await.unwrap()).unwrap()
    }
    pub async fn bring(&mut self, a: &AgentId, all: &[&str]) -> Value {
        let c = self.changes(a).await;
        let files =
            c.files.into_iter().filter(|f| all.contains(&f.path.as_str())).map(seen).collect();
        let call = Call::CopyBring { session: self.s.clone(), agent: a.clone(), files };
        self.c.call(call).await.unwrap()
    }
    pub async fn remove(&mut self, a: &AgentId, lose: bool) -> Result<Value, WireError> {
        let call = Call::CopyRemove { session: self.s.clone(), agent: a.clone(), lose };
        match self.c.call(call).await {
            Err(axiom_host::client::ClientError::Refused(e)) => Err(e),
            r => Ok(r.unwrap()),
        }
    }
    /// What was put on record of copies.
    pub async fn record(&mut self) -> Vec<String> {
        let v = self.c.call(Call::Audit { after: 0, limit: 200 }).await.unwrap().to_string();
        let v: Value = serde_json::from_str(&v).unwrap();
        let entries = v.as_array().unwrap().iter().map(|e| match e {
            Value::String(s) => serde_json::from_str::<Value>(s).unwrap(),
            e => e.clone(),
        });
        entries
            .filter(|e| e["tool"] == "copy")
            .map(|e| e["subject"].as_str().unwrap().to_owned())
            .collect()
    }
}
pub fn seen(f: CopyFile) -> CopySeen {
    CopySeen { path: f.path, digest: f.digest, run: f.run, there: f.there }
}
pub fn file<'a>(c: &'a CopyChanges, p: &str) -> &'a CopyFile {
    c.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("{p}: {c:?}"))
}
pub fn paths(c: &CopyChanges) -> Vec<(&str, &str)> {
    c.files.iter().map(|f| (f.path.as_str(), f.what.as_str())).collect()
}
