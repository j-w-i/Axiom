//! What an installed program is given of Axiom: one tool, by a program of the host's own that it
//! starts itself. Here the test stands for it once it runs.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{process::Stdio, sync::Arc, time::Duration};

use axiom_host::{Config, client::Client, connections::MemoryVault, paths::Paths};
use axiom_proto::*;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines as Said};

mod common;
use common::*;
#[path = "../../engine/tests/common/server.rs"]
mod server;
use server::*;

struct T {
    f: Fixture,
    c: Client,
    s: SessionId,
}

impl T {
    async fn new(script: &[Value]) -> Self {
        let f = fixture();
        for d in ["w", "home", "bin"] {
            std::fs::create_dir(f.dir.path().join(d)).unwrap();
        }
        let fake = env!("CARGO_BIN_EXE_host-fake-agent");
        for name in ["goose", "agy"] {
            std::fs::copy(fake, f.dir.path().join("bin").join(name)).unwrap();
        }
        let lines: String = script.iter().map(|v| format!("{v}\n")).collect();
        std::fs::write(f.dir.path().join("bin/script.jsonl"), lines).unwrap();
        let mut c = Config::new(f.paths.clone());
        c.home = Some(f.dir.path().join("home"));
        c.vault = Arc::new(MemoryVault::default());
        c.programs = Some(vec![f.dir.path().join("bin")]);
        c.bridge = Some(env!("CARGO_BIN_EXE_axiom-host").into());
        f.serve_with(c);
        let mut c = f.client().await;
        let w = f.dir.path().join("w").to_str().unwrap().to_owned();
        let call = Call::SessionCreate { title: "S".into(), workspace: Some(w) };
        let v = c.call(call).await.unwrap();
        let s = SessionId(v["session"].as_str().unwrap().into());
        c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
        Self { f, c, s }
    }
    async fn add(
        &mut self,
        name: &str,
        harness: &str,
        access: Access,
        server: Option<&Server>,
    ) -> AgentId {
        let connection = match server {
            Some(s) => {
                let call = Call::ConnectionAdd {
                    name: name.into(),
                    provider: "openai-compatible".into(),
                    url: Some(s.url.clone()),
                    secret: None,
                };
                let v = self.c.call(call).await.unwrap();
                Some(v["connection"].as_str().unwrap().to_owned())
            }
            None => None,
        };
        let execution = Execution {
            harness: harness.into(),
            provider: None,
            model: server.map(|_| "m".into()),
            connection,
            effort: None,
            environment: None,
        };
        let call = Call::AgentAdd {
            session: self.s.clone(),
            name: name.into(),
            execution,
            access,
            trust: true,
            instructions: vec![],
        };
        AgentId(self.c.call(call).await.unwrap()["agent"].as_str().unwrap().into())
    }
    async fn until<R>(&mut self, mut f: impl FnMut(&Body) -> Option<R>) -> R {
        loop {
            let m = self.c.next_within(Duration::from_secs(20)).await.unwrap().expect("more");
            if let Message::Events { events, .. } = m
                && let Some(r) = events.iter().find_map(|e| f(&e.body))
            {
                return r;
            }
        }
    }
    async fn question(&mut self) -> (RequestId, String, String) {
        self.until(|b| match b {
            Body::RequestOpened { request, digest, subject, .. } => {
                Some((request.clone(), digest.clone(), subject.clone()))
            }
            _ => None,
        })
        .await
    }
    async fn closed(&mut self) -> String {
        self.until(|b| match b {
            Body::RequestClosed { answer, .. } => Some(answer.clone()),
            _ => None,
        })
        .await
    }
    async fn answer(&mut self, request: RequestId, answer: Answer, digest: String) {
        let call = Call::Answer { session: self.s.clone(), request, answer, digest };
        self.c.call(call).await.unwrap();
    }
    /// What the program was told to start.
    async fn bridge(&self) -> (String, Vec<String>) {
        for _ in 0..1000 {
            let heard = std::fs::read_to_string(self.f.dir.path().join("bin/heard.jsonl"));
            for l in heard.unwrap_or_default().lines() {
                let v: Value = serde_json::from_str(l).unwrap();
                if v["method"] == "session/new" {
                    let s = &v["params"]["mcpServers"][0];
                    assert_eq!((s["name"].as_str(), &s["env"]), (Some("axiom"), &json!([])));
                    let args = s["args"].as_array().unwrap().iter();
                    let args = args.map(|a| a.as_str().unwrap().to_owned()).collect();
                    return (s["command"].as_str().unwrap().to_owned(), args);
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("it was never told of a session");
    }
}

/// The program's end of the bridge.
struct B {
    _p: tokio::process::Child,
    to: tokio::process::ChildStdin,
    from: Said<BufReader<tokio::process::ChildStdout>>,
}
impl B {
    fn start(command: &str, args: &[String]) -> Self {
        let mut p = tokio::process::Command::new(command)
            .args(args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let (to, from) = (p.stdin.take().unwrap(), p.stdout.take().unwrap());
        Self { _p: p, to, from: BufReader::new(from).lines() }
    }
    async fn say(&mut self, v: Value) {
        self.to.write_all(format!("{v}\n").as_bytes()).await.unwrap();
    }
    async fn hear(&mut self) -> Value {
        let l = tokio::time::timeout(Duration::from_secs(20), self.from.next_line()).await;
        serde_json::from_str(&l.unwrap().unwrap().unwrap()).unwrap()
    }
    async fn ask(&mut self, id: u64, args: Value) {
        self.say(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": "agent", "arguments": args}}))
            .await;
    }
    async fn call(&mut self, id: u64, args: Value) -> (String, bool) {
        self.ask(id, args).await;
        let v = self.hear().await;
        assert_eq!(v["id"], id);
        let r = &v["result"];
        (r["content"][0]["text"].as_str().unwrap().to_owned(), r["isError"] == false)
    }
}

fn script() -> Vec<Value> {
    vec![
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 1, "agentCapabilities": {}}}}),
        json!({"hear": {"method": "session/new"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "s1"}}}),
        json!({"hear": {"method": "session/prompt"}}),
        json!({"hear": {"method": "session/cancel"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 2, "result": {"stopReason": "cancelled"}}}),
    ]
}

async fn started() -> (T, AgentId, RunId) {
    let mut t = T::new(&script()).await;
    let lead = t.add("Lead", "goose", Access::Ask, None).await;
    let call =
        Call::Prompt { session: t.s.clone(), agent: lead.clone(), text: "go".into(), key: None };
    let run = RunId(t.c.call(call).await.unwrap()["run"].as_str().unwrap().into());
    let (id, digest, _) = t.question().await;
    t.answer(id, Answer::Once, digest).await;
    (t, lead, run)
}

#[tokio::test]
async fn a_program_gives_others_work_and_is_told_what_came_of_it() {
    let looks = Server::start(vec![text(&["It says hello."])]).await;
    let builds = Server::start(vec![text(&["Built."])]).await;
    let (mut t, lead, _) = started().await;
    t.add("Looker", "axiom", Access::Read, Some(&looks)).await;
    t.add("Builder", "axiom", Access::Work, Some(&builds)).await;
    let (command, args) = t.bridge().await;
    // No key is among what others can see; the file is its owner's alone (SI-3).
    assert_eq!((args.len(), args[0].as_str()), (2, "--mcp"));
    let file = std::path::PathBuf::from(&args[1]);
    let key = serde_json::from_str::<Value>(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let key = key["grant"].as_str().unwrap().to_owned();
    assert!(key.len() == 64 && !args[1].contains(&key));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    let mut b = B::start(&command, &args);
    b.say(json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}))
        .await;
    assert_eq!(b.hear().await["result"]["serverInfo"]["name"], "axiom");
    let (list, ok) = b.call(1, json!({"do": "list"})).await;
    assert!(
        ok && list.contains("Looker (") && list.contains("Builder (") && !list.contains("Lead")
    );
    let (said, ok) = b.call(2, json!({"do": "tell", "name": "Looker", "text": "Look."})).await;
    assert!(ok && said.contains("Looker has it"), "{said}");
    let (said, ok) = b.call(3, json!({"do": "wait"})).await;
    assert!(ok && said.contains("It says hello."), "{said}");
    let told = looks.requests()[0]["messages"].to_string();
    assert!(told.contains("another agent of this session, Lead"));
    // SI-1: one that may do more is told only with a person's yes.
    b.ask(4, json!({"do": "tell", "name": "Builder", "text": "Build."})).await;
    let (id, digest, subject) = t.question().await;
    assert_eq!(subject, "Builder: Build.");
    t.answer(id, Answer::Deny, digest).await;
    assert!(
        b.hear().await["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("a person said no")
    );
    assert!(builds.requests().is_empty());
    b.ask(5, json!({"do": "tell", "name": "Builder", "text": "Build."})).await;
    let (id, digest, _) = t.question().await;
    t.answer(id, Answer::Once, digest).await;
    assert_eq!(b.hear().await["result"]["isError"], false);
    let (said, _) = b.call(6, json!({"do": "wait"})).await;
    assert!(said.contains("Built."), "{said}");
    // What may not be done is not, and nobody is asked: its terms want a person's own words.
    t.add("Google", "agy", Access::Ask, None).await;
    let (said, ok) = b.call(8, json!({"do": "tell", "name": "Google", "text": "x"})).await;
    assert!(!ok && said.contains("a person's own words"), "{said}");
    // Not asked any more, it is not asked about any more.
    b.ask(7, json!({"do": "tell", "name": "Builder", "text": "More."})).await;
    let (id, digest, _) = t.question().await;
    b.say(
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}}),
    )
    .await;
    assert_eq!(t.closed().await, "expired");
    let late = Call::Answer { session: t.s.clone(), request: id, answer: Answer::Once, digest };
    assert!(t.c.call(late).await.is_err());
    assert_eq!(builds.requests().len(), 1);
    // On record, in order.
    let v = t.c.call(Call::Audit { after: 0, limit: 20 }).await.unwrap();
    let log: Vec<axiom_policy::audit::Entry> = serde_json::from_value(v).unwrap();
    let of: Vec<_> = log.iter().map(|e| (e.tool.as_str(), e.answered.is_some())).collect();
    assert_eq!(of, [("harness/goose", true), ("agent", false), ("agent", true)]);
    assert!(log.iter().all(|e| e.chain == [lead.clone()]));
    assert_eq!(axiom_policy::audit::broken(&log), None);
}

#[tokio::test]
async fn its_key_opens_one_tool_and_dies_with_it() {
    let (mut t, _, run) = started().await;
    let (_, args) = t.bridge().await;
    let way: Value = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let paths = Paths::new(way["data"].as_str().unwrap().into());
    let grant = way["grant"].as_str().unwrap().to_owned();
    let mut c = Client::raw(&paths).await.unwrap();
    c.call(Call::PeerHello { grant: grant.clone() }).await.unwrap();
    let refused = [
        Call::Status {},
        Call::SessionList { limit: 1, offset: 0 },
        Call::Shutdown { force: true },
        Call::Connections {},
        Call::Cancel { session: t.s.clone(), run: run.clone() },
        Call::Follow { session: t.s.clone(), after: 0 },
        Call::Audit { after: 0, limit: 1 },
        Call::PeerHello { grant: grant.clone() },
    ];
    for call in refused {
        let e = c.call(call.clone()).await.unwrap_err();
        assert!(e.to_string().contains("opens nothing else"), "{call:?}: {e}");
    }
    let v = c.call(Call::Peer { args: json!({"do": "list"}) }).await.unwrap();
    assert_eq!(v["ok"], true);
    // A client has no such tool, and a wrong key opens nothing.
    assert!(t.c.call(Call::Peer { args: json!({"do": "list"}) }).await.is_err());
    for wrong in ["", "x", &grant[1..]] {
        let mut c = Client::raw(&paths).await.unwrap();
        assert!(c.call(Call::PeerHello { grant: wrong.into() }).await.is_err(), "{wrong}");
    }
    let mut c = Client::raw(&paths).await.unwrap();
    assert!(c.call(Call::Peer { args: json!({"do": "list"}) }).await.is_err(), "not before hello");
    let mut old = Client::raw(&paths).await.unwrap();
    old.call(Call::PeerHello { grant: grant.clone() }).await.unwrap();
    // The program ends: so does its key, and the file that held it.
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    t.until(|b| matches!(b, Body::RunEnded { .. }).then_some(())).await;
    let mut gone = false;
    for _ in 0..1000 {
        let mut c = Client::raw(&paths).await.unwrap();
        gone = c.call(Call::PeerHello { grant: grant.clone() }).await.is_err();
        if gone && !std::path::Path::new(&args[1]).exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        // It stays between runs; changed, it ends.
        let set = Call::AgentSet {
            session: t.s.clone(),
            agent: t.agent().await,
            execution: None,
            access: Some(Access::Ask),
        };
        let _ = t.c.call(set).await;
    }
    assert!(gone && !std::path::Path::new(&args[1]).exists());
    // Also for who had shown it before.
    let e = old.call(Call::Peer { args: json!({"do": "list"}) }).await.unwrap_err();
    assert!(e.to_string().contains("not good any more"), "{e}");
}

impl T {
    /// The only one there is.
    async fn agent(&mut self) -> AgentId {
        let mut c = self.f.client().await;
        c.call(Call::Follow { session: self.s.clone(), after: 0 }).await.unwrap();
        loop {
            let m = c.next_within(Duration::from_secs(10)).await.unwrap().unwrap();
            if let Message::Events { events, .. } = m {
                for e in events {
                    if let Body::AgentAdded { agent, .. } = e.body {
                        return agent;
                    }
                }
            }
        }
    }
}
