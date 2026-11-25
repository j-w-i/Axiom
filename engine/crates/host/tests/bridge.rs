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
        for name in ["goose", "agy", "claude"] {
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
            apart: false,
        };
        AgentId(self.c.call(call).await.unwrap()["agent"].as_str().unwrap().into())
    }
    async fn until<R>(&mut self, mut f: impl FnMut(&Body) -> Option<R>) -> R {
        let mut all = Vec::new();
        loop {
            let m = self.c.next_within(Duration::from_secs(20)).await.unwrap();
            let Some(m) = m else { panic!("nothing more after {all:#?}") };
            if let Message::Events { events, .. } = m {
                if let Some(r) = events.iter().find_map(|e| f(&e.body)) {
                    return r;
                }
                all.extend(events.into_iter().map(|e| e.body));
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
        let call = Call::Answer { session: self.s.clone(), request, answer, digest, words: None };
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
    let late = Call::Answer {
        session: t.s.clone(),
        request: id,
        answer: Answer::Once,
        digest,
        words: None,
    };
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
    let v = c.call(Call::Peer { tool: "agent".into(), args: json!({"do": "list"}) }).await.unwrap();
    assert_eq!(v["ok"], true);
    // Another tool is not among what it opens, whatever its arguments would be for that one.
    let e = c.call(Call::Peer { tool: "shell".into(), args: json!({"do": "list"}) }).await;
    assert!(e.unwrap_err().to_string().contains("opens nothing else"));
    // A client has no such tool, and a wrong key opens nothing.
    assert!(
        t.c.call(Call::Peer { tool: "agent".into(), args: json!({"do": "list"}) }).await.is_err()
    );
    for wrong in ["", "x", &grant[1..]] {
        let mut c = Client::raw(&paths).await.unwrap();
        assert!(c.call(Call::PeerHello { grant: wrong.into() }).await.is_err(), "{wrong}");
    }
    let mut c = Client::raw(&paths).await.unwrap();
    assert!(
        c.call(Call::Peer { tool: "agent".into(), args: json!({"do": "list"}) }).await.is_err(),
        "not before hello"
    );
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
    let e = old
        .call(Call::Peer { tool: "agent".into(), args: json!({"do": "list"}) })
        .await
        .unwrap_err();
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

// Claude Code with Axiom's tools in place of its own.

fn claude_script(more: &[Value]) -> Vec<Value> {
    let mut all = vec![
        json!({"hear": {"type": "control_request", "request": {"subtype": "initialize"}}, "as": "i"}),
        json!({"say": {"type": "control_response", "response": {"subtype": "success", "request_id": "$i/request_id", "response": {"commands": [], "models": []}}}}),
        json!({"hear": {"type": "user"}, "as": "p"}),
        json!({"say": {"type": "system", "subtype": "init", "uuid": "u1", "session_id": "$p/session_id", "claude_code_version": "2.1.330", "cwd": "/w", "tools": [], "model": "claude-opus-5-5", "permissionMode": "dontAsk"}}),
        // What it says of a call of Axiom's is not shown beside what Axiom says of it.
        json!({"say": {"type": "assistant", "uuid": "u2", "session_id": "$p/session_id", "parent_tool_use_id": null, "message": {"id": "m1", "type": "message", "role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "mcp__axiom__list", "input": {"path": "."}}]}}}),
    ];
    all.extend_from_slice(more);
    all
}

async fn with_hands(access: Access) -> (T, AgentId, RunId) {
    let mut t = T::new(&claude_script(&[
        json!({"hear": {"type": "control_request", "request": {"subtype": "interrupt"}}, "as": "x"}),
        json!({"say": {"type": "control_response", "response": {"subtype": "success", "request_id": "$x/request_id", "response": {}}}}),
        json!({"say": {"type": "result", "subtype": "error_during_execution", "uuid": "u3", "session_id": "$p/session_id", "is_error": true, "num_turns": 1, "stop_reason": null, "usage": {}, "user_message_uuids": ["$p/uuid"]}}),
    ]))
    .await;
    let execution = Execution {
        harness: "claude".into(),
        provider: None,
        model: None,
        connection: None,
        effort: None,
        environment: Some("axiom".into()),
    };
    let add = |e: Execution, access| Call::AgentAdd {
        session: t.s.clone(),
        name: "Lead".into(),
        execution: e,
        access,
        trust: false,
        instructions: vec![],
        apart: false,
    };
    // Its own way of asking is in no documentation; Axiom's needs none.
    let own = add(Execution { environment: None, ..execution.clone() }, Access::Ask);
    let odd = add(Execution { environment: Some("x".into()), ..execution.clone() }, access);
    let goose = add(Execution { harness: "goose".into(), ..execution.clone() }, Access::Ask);
    let good = add(execution, access);
    for (call, why) in
        [(own, "no mode"), (odd, "no such environment"), (goose, "no such environment")]
    {
        let e = t.c.call(call).await.unwrap_err().to_string();
        assert!(e.contains(why), "{e}");
    }
    let v = t.c.call(good).await.unwrap();
    let a = AgentId(v["agent"].as_str().unwrap().into());
    let call =
        Call::Prompt { session: t.s.clone(), agent: a.clone(), text: "go".into(), key: None };
    let run = RunId(t.c.call(call).await.unwrap()["run"].as_str().unwrap().into());
    let (id, digest, subject) = t.question().await;
    for part in [
        "--tools ''",
        "--permission-mode dontAsk",
        "--strict-mcp-config",
        "--setting-sources user",
        "'mcp__axiom__*'",
    ] {
        assert!(subject.contains(part), "{part}: {subject}");
    }
    t.answer(id, Answer::Once, digest).await;
    (t, a, run)
}

impl T {
    async fn mcp(&self) -> (B, Vec<String>) {
        let seen = self.f.dir.path().join("bin/seen.json");
        for _ in 0..1000 {
            if let Ok(s) = std::fs::read_to_string(&seen)
                && let Ok(v) = serde_json::from_str::<Value>(&s)
            {
                let args: Vec<String> = v["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a.as_str().unwrap().into())
                    .collect();
                assert_eq!(v["env"]["ENABLE_TOOL_SEARCH"], "false");
                let at = args.iter().position(|a| a == "--mcp-config").unwrap();
                let c: Value = serde_json::from_str(&args[at + 1]).unwrap();
                let s = &c["mcpServers"]["axiom"];
                let to =
                    s["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_owned());
                return (B::start(s["command"].as_str().unwrap(), &to.collect::<Vec<_>>()), args);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("it was never started");
    }
    fn file(&self, n: &str) -> Option<String> {
        std::fs::read_to_string(self.f.dir.path().join("w").join(n)).ok()
    }
}
impl B {
    async fn tool(&mut self, id: u64, name: &str, args: Value) {
        self.say(json!({"jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": {"name": name, "arguments": args}}))
            .await;
    }
    async fn told(&mut self, id: u64) -> (String, bool) {
        let v = self.hear().await;
        assert_eq!(v["id"], id, "{v}");
        let r = &v["result"];
        (r["content"][0]["text"].as_str().unwrap().to_owned(), r["isError"] == false)
    }
}

#[tokio::test]
async fn its_tools_are_axioms_and_so_is_what_holds_them_back() {
    let (mut t, a, _) = with_hands(Access::Ask).await;
    let (mut b, args) = t.mcp().await;
    let words = args.iter().position(|a| a == "--append-system-prompt-file").unwrap();
    let words = std::fs::read_to_string(&args[words + 1]).unwrap();
    assert!(words.contains("a person is asked") && words.contains("Your only tools"), "{words}");
    b.say(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})).await;
    let v = b.hear().await;
    let tools = v["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(
        names,
        [
            "read", "write", "edit", "patch", "list", "glob", "search", "shell", "job", "fetch",
            "agent"
        ]
    );
    assert!(tools.iter().all(|t| t["_meta"]["anthropic/maxResultSizeChars"] == 200_000));
    // What may be looked at is not asked about, and is shown once.
    std::fs::write(t.f.dir.path().join("w/a.txt"), "one\n").unwrap();
    b.tool(2, "read", json!({"path": "a.txt"})).await;
    assert!(b.told(2).await.0.contains("one"));
    // A change is, and a no is a no.
    b.tool(3, "write", json!({"path": "n.txt", "content": "x\n"})).await;
    let (id, digest, subject) = t.question().await;
    assert!(subject.contains("+x"), "{subject}");
    let call = Call::Answer {
        session: t.s.clone(),
        request: id,
        answer: Answer::Deny,
        digest,
        words: Some("not there".into()),
    };
    t.c.call(call).await.unwrap();
    let (said, ok) = b.told(3).await;
    assert!(!ok && said.contains("a person said no. They said: not there"), "{said}");
    assert!(t.file("n.txt").is_none());
    // It is told with its no, and not once more as if its person had spoken.
    assert!(!history(&t.f, &t.s).iter().any(|b| format!("{b:?}").contains("I said no")));
    b.tool(4, "write", json!({"path": "n.txt", "content": "x\n"})).await;
    let (id, digest, _) = t.question().await;
    t.answer(id, Answer::Once, digest).await;
    assert!(b.told(4).await.1);
    assert_eq!(t.file("n.txt").as_deref(), Some("x\n"));
    // A command runs in Axiom's sandbox: not outside the folder, not on the network, no secret.
    b.tool(5, "shell", json!({"command": "echo in > in.txt; echo out > ../out.txt; echo done"}))
        .await;
    let (id, digest, _) = t.question().await;
    t.answer(id, Answer::Once, digest).await;
    let (said, _) = b.told(5).await;
    assert!(said.contains("done") && t.file("in.txt").is_some(), "{said}");
    assert!(!t.f.dir.path().join("out.txt").exists());
    // What it is not allowed at all, nobody is asked about.
    b.tool(6, "write", json!({"path": "../../x", "content": "x"})).await;
    assert!(!b.told(6).await.1);
    b.tool(7, "write", json!({"path": ".git/config", "content": "x"})).await;
    assert!(b.told(7).await.0.contains("protected"));
    for (id, tool, args) in [(8, "fly", json!({})), (9, "read", json!("x"))] {
        b.tool(id, tool, args).await;
        assert!(!b.told(id).await.1, "{tool}");
    }
    // Whoever asked waits no longer: it is taken back, and the call ends.
    b.tool(10, "shell", json!({"command": "touch late"})).await;
    let (id, digest, _) = t.question().await;
    b.say(
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 10}}),
    )
    .await;
    assert_ne!(t.closed().await, "allowed");
    let late = Call::Answer {
        session: t.s.clone(),
        request: id,
        answer: Answer::Once,
        digest,
        words: None,
    };
    assert!(t.c.call(late).await.is_err());
    assert!(t.file("late").is_none());
    let v = t.c.call(Call::Audit { after: 0, limit: 20 }).await.unwrap();
    let log: Vec<axiom_policy::audit::Entry> = serde_json::from_value(v).unwrap();
    use axiom_policy::audit::Containment as C;
    let of: Vec<_> = log.iter().map(|e| (e.tool.as_str(), e.containment)).collect();
    assert_eq!(of, [("harness/claude", C::None), ("write", C::Axiom), ("shell", C::Axiom)]);
    assert!(log.iter().all(|e| e.chain == [a.clone()] && e.answered.is_some()));
    // Each on the transcript once, by Axiom's own word for it.
    let mut c = t.f.client().await;
    c.call(Call::Follow { session: t.s.clone(), after: 0 }).await.unwrap();
    let mut names = Vec::new();
    while names.len() < 9 {
        let m = c.next_within(Duration::from_secs(10)).await.unwrap().unwrap();
        if let Message::Events { events, .. } = m {
            names.extend(events.into_iter().filter_map(|e| match e.body {
                Body::ItemStarted { kind: ItemKind::Tool, name, .. } => Some(name),
                _ => None,
            }));
        }
    }
    assert_eq!(
        names,
        ["read", "write", "write", "shell", "write", "write", "fly", "read", "shell"]
    );
}

/// SI-24: stopped by a person, what it does stops; and outside a run nothing is done for it.
#[tokio::test]
async fn it_is_stopped_with_what_it_does() {
    let (mut t, _, run) = with_hands(Access::Work).await;
    let (mut b, args) = t.mcp().await;
    let mark = format!("8{:05}", std::process::id() % 100_000);
    let alive = || {
        let o = std::process::Command::new("/bin/ps").args(["-axo", "args="]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with(&format!("sleep {mark}")))
    };
    b.tool(1, "job", json!({"do": "start", "command": format!("sleep {mark}")})).await;
    assert!(b.told(1).await.1);
    while !alive() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    b.tool(2, "shell", json!({"command": "echo begun; sleep 60; touch late"})).await;
    t.until(|b| matches!(b, Body::ItemDelta { text, .. } if text.contains("begun")).then_some(()))
        .await;
    let now = std::time::Instant::now();
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    let outcome = t
        .until(|b| match b {
            Body::RunEnded { outcome, .. } => Some(*outcome),
            _ => None,
        })
        .await;
    assert_eq!(outcome, Outcome::Cancelled);
    let (said, ok) = b.told(2).await;
    assert!(!ok && said.contains("Not done") || said.contains("Stopped"), "{said}");
    assert!(now.elapsed() < Duration::from_secs(20));
    assert!(t.file("late").is_none());
    // Also what ran in the background.
    let now = std::time::Instant::now();
    while alive() {
        assert!(now.elapsed() < Duration::from_secs(10), "a job runs on");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut refused = false;
    for _ in 0..200 {
        b.tool(3, "write", json!({"path": "after.txt", "content": "x"})).await;
        let (said, ok) = b.told(3).await;
        if !ok {
            assert!(said.contains("in no run"), "{said}");
            refused = true;
            break;
        }
        std::fs::remove_file(t.f.dir.path().join("w/after.txt")).unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(refused && t.file("after.txt").is_none());
    let words = args.iter().position(|a| a == "--append-system-prompt-file").unwrap();
    assert!(std::path::Path::new(&args[words + 1]).exists());
    // With the program, its words go.
    t.c.call(Call::Shutdown { force: true }).await.unwrap();
    for _ in 0..1500 {
        if !std::path::Path::new(&args[words + 1]).exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("they are there still");
}
