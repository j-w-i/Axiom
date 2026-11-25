//! Agents that work apart, through the real socket: their copy of the folder is made, worked in, and
//! brought in as a person says. The model is a server that answers from a script.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use axiom_host::{Config, client::Client};
use axiom_proto::*;
use serde_json::{Value, json};

mod common;
use common::*;
#[path = "../../engine/tests/common/server.rs"]
mod server;
use server::*;

struct T {
    f: Fixture,
    server: Server,
    c: Client,
    s: SessionId,
    connection: String,
    seq: u64,
}

fn config(f: &Fixture) -> Config {
    let mut c = Config::new(f.paths.clone());
    c.home = Some(f.dir.path().join("home"));
    c
}

fn git(dir: &Path, args: &[&str]) {
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
    async fn new(script: Vec<Reply>) -> Self {
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
    async fn restart(&mut self) {
        self.c.call(Call::Shutdown { force: true }).await.unwrap();
        while self.f.paths.token().exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.f.serve_with(config(&self.f));
        self.c = self.f.client().await;
        self.c.call(Call::Follow { session: self.s.clone(), after: self.seq }).await.unwrap();
    }
    fn path(&self, n: &str) -> PathBuf {
        self.f.dir.path().join("w").join(n)
    }
    fn write(&self, n: &str, text: &str) {
        std::fs::create_dir_all(self.path(n).parent().unwrap()).unwrap();
        std::fs::write(self.path(n), text).unwrap();
    }
    fn file(&self, n: &str) -> Option<String> {
        std::fs::read_to_string(self.path(n)).ok()
    }
    fn copy_of(&self, a: &AgentId) -> PathBuf {
        self.f.dir.path().join("home/.axiom-work").join(&self.s.0).join(&a.0)
    }
    fn copied(&self, a: &AgentId, n: &str) -> Option<String> {
        std::fs::read_to_string(self.copy_of(a).join(n)).ok()
    }
    async fn agent(&mut self, name: &str, access: Access, apart: bool) -> AgentId {
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
        };
        AgentId(self.c.call(call).await.unwrap()["agent"].as_str().unwrap().into())
    }
    /// It is told something and has the model do this.
    async fn does(&mut self, a: &AgentId, command: &str) -> (Outcome, Option<String>) {
        let args = json!({ "command": command }).to_string();
        self.server.add(calls(&[("c1", "shell", &args)]));
        self.server.add(text(&["done"]));
        self.tell(a).await.unwrap();
        self.ended().await
    }
    async fn tell(&mut self, a: &AgentId) -> Result<Value, axiom_host::client::ClientError> {
        let (session, agent) = (self.s.clone(), a.clone());
        self.c.call(Call::Prompt { session, agent, text: "go".into(), key: None }).await
    }
    async fn ended(&mut self) -> (Outcome, Option<String>) {
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
                        let call = Call::Answer { session, request, answer, digest };
                        self.c.call(call).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
    async fn changes(&mut self, a: &AgentId) -> CopyChanges {
        let call = Call::CopyChanges { session: self.s.clone(), agent: a.clone() };
        serde_json::from_value(self.c.call(call).await.unwrap()).unwrap()
    }
    async fn bring(&mut self, a: &AgentId, all: &[&str]) -> Value {
        let c = self.changes(a).await;
        let files = c
            .files
            .into_iter()
            .filter(|f| all.contains(&f.path.as_str()))
            .map(|f| CopySeen { path: f.path, digest: f.digest })
            .collect();
        let call = Call::CopyBring { session: self.s.clone(), agent: a.clone(), files };
        self.c.call(call).await.unwrap()
    }
    async fn remove(&mut self, a: &AgentId, lose: bool) -> Result<Value, WireError> {
        let call = Call::CopyRemove { session: self.s.clone(), agent: a.clone(), lose };
        match self.c.call(call).await {
            Err(axiom_host::client::ClientError::Refused(e)) => Err(e),
            r => Ok(r.unwrap()),
        }
    }
    /// What was put on record of copies.
    async fn record(&mut self) -> Vec<String> {
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
fn paths(c: &CopyChanges) -> Vec<(&str, &str)> {
    c.files.iter().map(|f| (f.path.as_str(), f.what.as_str())).collect()
}

#[tokio::test]
async fn an_agent_works_apart_and_a_person_brings_it_in() {
    let mut t = T::new(vec![]).await;
    let w = t.f.dir.path().join("w");
    git(&w, &["init", "-q", "-b", "trunk"]);
    t.write(".gitignore", "/target\n");
    t.write("a.txt", "one\n");
    t.write("b.txt", "two\n");
    t.write("target/tracked", "t");
    git(&w, &["add", "-A"]);
    git(&w, &["add", "-f", "target/tracked"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    t.write("a.txt", "one and not committed\n");
    t.write("target/big", "x");
    let a = t.agent("Builder", Access::Work, true).await;
    // Nothing before it is spoken to.
    assert_eq!(t.changes(&a).await, CopyChanges::default());
    assert!(!t.copy_of(&a).exists());
    let (o, e) = t
        .does(
            &a,
            "pwd > where; echo agent > a.txt; mkdir -p new && echo n > new/n.txt; rm b.txt; \
             git log --oneline > log; ls target > seen",
        )
        .await;
    assert_eq!((o, e), (Outcome::Completed, None));
    let copy = std::fs::canonicalize(t.copy_of(&a)).unwrap();
    assert_eq!(t.copied(&a, "where").unwrap().trim(), copy.to_str().unwrap());
    assert!(t.copied(&a, "log").unwrap().contains("first"), "git says what there was");
    // What git ignores is not there, but for what it tracks all the same.
    assert_eq!(t.copied(&a, "seen").as_deref(), Some("tracked\n"));
    assert!(t.server.requests()[0].to_string().contains("a copy of the project's folder"));
    // None of it is in the folder.
    assert_eq!(t.file("a.txt").as_deref(), Some("one and not committed\n"));
    assert!(t.file("b.txt").is_some() && t.file("where").is_none());
    let c = t.changes(&a).await;
    assert!(c.made && c.whole && c.back == 0);
    assert_eq!(
        paths(&c),
        [
            ("a.txt", "changed"),
            ("b.txt", "removed"),
            ("log", "new"),
            ("new/n.txt", "new"),
            ("seen", "new"),
            ("where", "new"),
        ]
    );
    assert!(c.diff.contains("-one and not committed\n+agent\n"), "{}", c.diff);
    assert!(c.files.iter().all(|f| !f.clash && f.never.is_none() && f.others.is_empty()));
    // It cannot reach the folder by itself.
    let there = t.path("evil");
    t.does(
        &a,
        &format!("echo x > {0}; cat {1} > stolen", there.display(), t.path("a.txt").display()),
    )
    .await;
    assert!(!there.exists());
    assert_eq!(t.copied(&a, "stolen").unwrap_or_default(), "");

    let r = t.bring(&a, &["a.txt", "b.txt", "new/n.txt"]).await;
    assert_eq!(r, json!({"brought": ["a.txt", "b.txt", "new/n.txt"], "left": []}));
    assert_eq!(t.file("a.txt").as_deref(), Some("agent\n"));
    assert_eq!(t.file("new/n.txt").as_deref(), Some("n\n"));
    assert_eq!((t.file("b.txt"), t.file("log")), (None, None));
    let c = t.changes(&a).await;
    assert_eq!(c.back, 3);
    assert!(c.files.iter().all(|f| f.path != "a.txt" && f.path != "b.txt"));
    let record = t.record().await;
    assert!(record[0].starts_with("brought in: a.txt (") && record[0].contains("b.txt (removed)"));

    // Put back, but not over what has been changed since.
    t.write("a.txt", "mine\n");
    let call = Call::CopyBack { session: t.s.clone(), agent: a.clone() };
    let r = t.c.call(call.clone()).await.unwrap();
    assert_eq!(r["back"], json!(["b.txt", "new/n.txt"]));
    assert_eq!(r["left"], json!([{"path": "a.txt", "why": "it has changed since"}]));
    assert_eq!((t.file("b.txt").as_deref(), t.file("new/n.txt")), (Some("two\n"), None));
    t.write("a.txt", "agent\n");
    assert_eq!(t.c.call(call).await.unwrap()["back"], json!(["a.txt"]));
    assert_eq!(t.file("a.txt").as_deref(), Some("one and not committed\n"));
    assert!(paths(&t.changes(&a).await).contains(&("a.txt", "changed")));

    // It is there after the host has been away.
    t.restart().await;
    assert!(paths(&t.changes(&a).await).contains(&("a.txt", "changed")));
    t.does(&a, "cat a.txt >> again").await;
    assert_eq!(t.copied(&a, "again").as_deref(), Some("agent\n"));

    // Thrown away only with a word to it, and then there is another.
    let e = t.remove(&a, false).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::Busy);
    assert!(e.message.contains("are not in the folder"), "{}", e.message);
    assert!(t.copy_of(&a).exists());
    t.remove(&a, true).await.unwrap();
    assert!(!t.copy_of(&a).exists());
    assert!(!t.changes(&a).await.made);
    // It can be set up as another, as any agent.
    let (session, access) = (t.s.clone(), Some(Access::Ask));
    let set = Call::AgentSet { session, agent: a.clone(), execution: None, access };
    t.c.call(set).await.unwrap();
    t.does(&a, "true").await;
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("one and not committed\n"));
    assert_eq!(t.changes(&a).await.files, []);
    t.remove(&a, false).await.unwrap();
    assert_eq!(t.record().await.len(), 5);
}

#[tokio::test]
async fn what_is_shown_is_what_is_brought_in() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one\n");
    t.write("b.txt", "two\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.does(&a, "echo agent > a.txt; echo agent > b.txt; echo x > .envrc").await;
    let bring =
        |files: Vec<CopySeen>| Call::CopyBring { session: t.s.clone(), agent: a.clone(), files };
    let seen = |path: &str, digest: Option<&str>| CopySeen {
        path: path.into(),
        digest: digest.map(Into::into),
    };
    let c: CopyChanges = {
        let call = Call::CopyChanges { session: t.s.clone(), agent: a.clone() };
        serde_json::from_value(t.c.call(call).await.unwrap()).unwrap()
    };
    let digest = c.files.iter().find(|f| f.path == "a.txt").unwrap().digest.clone().unwrap();
    // Changed after it was shown; another digest; none; a name that is not below the folder.
    let call = bring(vec![
        seen("a.txt", Some("other")),
        seen("b.txt", None),
        seen("../w/a.txt", None),
        seen("../evil", Some(&digest)),
        seen("/etc/x", Some(&digest)),
    ]);
    let r = t.c.call(call).await.unwrap();
    assert_eq!(r["brought"].as_array().map(Vec::len), Some(0), "{r}");
    assert_eq!(r["left"].as_array().map(Vec::len), Some(5), "{r}");
    // The folder's own has changed.
    std::fs::write(t.path("a.txt"), "mine\n").unwrap();
    let call = bring(vec![seen("a.txt", Some(&digest)), seen(".envrc", Some(&digest))]);
    let r = t.c.call(call).await.unwrap();
    assert_eq!(r["brought"], json!([]), "{r}");
    assert_eq!(t.file("a.txt").as_deref(), Some("mine\n"));
    assert_eq!(t.file("b.txt").as_deref(), Some("two\n"));
    assert!(t.c.call(bring(vec![])).await.is_err());
    let c = t.changes(&a).await;
    assert!(c.files.iter().find(|f| f.path == "a.txt").unwrap().clash);
    // Nothing was brought in, so nothing is put back.
    assert_eq!(c.back, 0);
}

#[tokio::test]
async fn two_who_change_the_same_are_told_of() {
    let mut t = T::new(vec![]).await;
    t.write("same.txt", "one\n");
    t.write("x.txt", "x\n");
    let a = t.agent("Ada", Access::Work, true).await;
    let b = t.agent("Ben", Access::Work, true).await;
    t.does(&a, "echo ada > same.txt; echo ada > x.txt").await;
    t.does(&b, "cat same.txt > was; echo ben > same.txt").await;
    assert_eq!(t.copied(&b, "was").as_deref(), Some("one\n"), "they do not see each other's");
    let c = t.changes(&a).await;
    let others =
        |c: &CopyChanges, p: &str| c.files.iter().find(|f| f.path == p).unwrap().others.clone();
    assert_eq!((others(&c, "same.txt"), others(&c, "x.txt")), (vec!["Ben".to_owned()], vec![]));
    t.bring(&a, &["same.txt"]).await;
    // Now it is a clash for the other, who is brought up to date and settles it.
    let c = t.changes(&b).await;
    assert!(c.files.iter().find(|f| f.path == "same.txt").unwrap().clash);
    let call = Call::CopyRenew { session: t.s.clone(), agent: b.clone() };
    let r = t.c.call(call).await.unwrap();
    assert_eq!((&r["clashes"], &r["taken"]), (&json!(["same.txt"]), &json!(0)));
    let say = r["say"].as_str().unwrap();
    assert!(say.contains("same.txt"), "{say}");
    let place = say.split("What the project has now is in ").nth(1).unwrap();
    let place = place.split(", under").next().unwrap().to_owned();
    t.does(&b, &format!("cat {place}/same.txt same.txt > both && mv both same.txt")).await;
    assert_eq!(t.copied(&b, "same.txt").as_deref(), Some("ada\nben\n"));
    let c = t.changes(&b).await;
    assert!(!c.files.iter().find(|f| f.path == "same.txt").unwrap().clash);
    assert!(c.diff.contains(" ada\n+ben\n"), "{}", c.diff);
    t.bring(&b, &["same.txt"]).await;
    assert_eq!(t.file("same.txt").as_deref(), Some("ada\nben\n"));
    // What the folder has and only it changed is taken over.
    let r = t.c.call(Call::CopyRenew { session: t.s.clone(), agent: a.clone() }).await.unwrap();
    assert_eq!((&r["taken"], &r["say"]), (&json!(1), &Value::Null));
    assert_eq!(t.copied(&a, "same.txt").as_deref(), Some("ada\nben\n"));
}

#[tokio::test]
async fn the_folders_rules_hold_in_a_copy() {
    let mut t = T::new(vec![]).await;
    t.write("secret.txt", "SECRET");
    t.write("a.txt", "one");
    t.write(
        ".axiom/policy.toml",
        "version = 1\n[[rule]]\nid = 's'\ntool = 'read'\npaths = ['secret.txt']\ndecision = 'deny'\n\
         [[rule]]\nid = 'w'\ntool = 'write'\npaths = ['a.txt']\ndecision = 'deny'\n",
    );
    t.write(".gitignore", ".axiom\n");
    let a = t.agent("Builder", Access::Work, true).await;
    t.write("b.txt", "two");
    let write = |p: &str| json!({"path": p, "content": "agent"}).to_string();
    t.server.add(calls(&[
        ("c1", "read", r#"{"path":"a.txt"}"#),
        ("c2", "read", r#"{"path":"b.txt"}"#),
    ]));
    t.server.add(calls(&[
        ("c3", "write", &write("a.txt")),
        ("c4", "write", &write("b.txt")),
        ("c5", "shell", r#"{"command":"cat secret.txt > got; true"}"#),
    ]));
    t.server.add(text(&["done"]));
    t.tell(&a).await.unwrap();
    assert_eq!(t.ended().await.0, Outcome::Completed);
    // Ignored, so it is not in the copy; it is the folder's that counts.
    assert_eq!(t.copied(&a, ".axiom/policy.toml"), None);
    assert_eq!(t.copied(&a, "secret.txt"), None, "what may not be read is not copied");
    assert_eq!(t.copied(&a, "got").as_deref(), Some(""));
    assert_eq!(t.copied(&a, "a.txt").as_deref(), Some("one"));
    assert_eq!(t.copied(&a, "b.txt").as_deref(), Some("agent"));
}

#[tokio::test]
async fn what_stands_in_the_way_of_a_copy() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one");
    let a = t.agent("Builder", Access::Work, true).await;
    // One that was begun and not ended.
    let half = t.copy_of(&a);
    std::fs::create_dir_all(&half).unwrap();
    std::fs::write(half.join("half"), "x").unwrap();
    t.does(&a, "true").await;
    assert!(t.copied(&a, "half").is_none() && t.copied(&a, "a.txt").is_some());
    // Its place is a link to elsewhere.
    let b = t.agent("Other", Access::Work, true).await;
    let elsewhere = t.f.dir.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("precious"), "x").unwrap();
    std::os::unix::fs::symlink(&elsewhere, t.copy_of(&b)).unwrap();
    let e = refused(t.tell(&b).await);
    assert!(!e.message.is_empty());
    assert!(elsewhere.join("precious").exists() && !elsewhere.join("a.txt").exists());
    std::fs::remove_file(t.copy_of(&b)).unwrap();
    t.does(&b, "true").await;
    assert!(t.copied(&b, "a.txt").is_some());
    // It became a link after it was made.
    std::fs::rename(t.copy_of(&b), t.f.dir.path().join("moved")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, t.copy_of(&b)).unwrap();
    let call = Call::CopyChanges { session: t.s.clone(), agent: b.clone() };
    assert!(refused(t.c.call(call).await).message.contains("leads elsewhere"));
    assert!(t.tell(&b).await.is_err());
    assert!(t.remove(&b, true).await.is_err());
    assert!(elsewhere.join("precious").exists());
    // One that works in the folder has none.
    let c = t.agent("In it", Access::Work, false).await;
    let call = Call::CopyChanges { session: t.s.clone(), agent: c.clone() };
    assert!(refused(t.c.call(call).await).message.contains("in the folder itself"));
    // Thrown away by hand: the next time there is another.
    std::fs::remove_dir_all(t.copy_of(&a)).unwrap();
    assert!(!t.changes(&a).await.made);
    t.does(&a, "true").await;
    assert!(t.copied(&a, "a.txt").is_some());
}

/// While it works nothing is taken from under it.
#[tokio::test]
async fn a_copy_that_is_worked_in_stays() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one");
    let a = t.agent("Builder", Access::Work, true).await;
    t.server.add(text(&["thinking"]).hangs());
    let run = RunId(t.tell(&a).await.unwrap()["run"].as_str().unwrap().into());
    assert_eq!(t.remove(&a, true).await.unwrap_err().code, ErrorCode::Busy);
    let call = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    assert_eq!(refused(t.c.call(call).await).code, ErrorCode::Busy);
    assert!(t.copy_of(&a).exists());
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    t.ended().await;
}

/// Who works in a copy does not get the folder changed by telling one who does not.
#[tokio::test]
async fn telling_one_that_works_in_the_folder_is_asked_about() {
    let mut t = T::new(vec![]).await;
    t.write("a.txt", "one");
    let a = t.agent("Apart", Access::Work, true).await;
    t.agent("Inside", Access::Work, false).await;
    t.agent("Looks", Access::Read, false).await;
    t.agent("Also", Access::Work, true).await;
    for (name, asks) in [("Inside", true), ("Looks", false), ("Also", false)] {
        let args = json!({"do": "tell", "name": name, "text": "do it"}).to_string();
        t.server.add(calls(&[("c1", "agent", &args)]));
        for _ in 0..4 {
            t.server.add(text(&["done"]));
        }
        t.tell(&a).await.unwrap();
        let mut asked = false;
        let (mut runs, mut begun) = (0, false);
        loop {
            let m = t.c.next_within(LONG).await.unwrap().expect("more");
            let Message::Events { events, .. } = m else { continue };
            for e in events {
                match e.body {
                    Body::RequestOpened { request, digest, scope, .. } => {
                        asked = true;
                        assert!(scope.contains(name), "{scope}");
                        let session = t.s.clone();
                        let answer = Answer::Deny;
                        t.c.call(Call::Answer { session, request, answer, digest }).await.unwrap();
                    }
                    Body::RunStarted { .. } => (runs, begun) = (runs + 1, true),
                    Body::RunEnded { .. } => runs -= 1,
                    _ => {}
                }
            }
            if begun && runs == 0 {
                break;
            }
        }
        assert_eq!(asked, asks, "{name}");
        quiet(&mut t.c).await;
        t.server.clear();
    }
}
