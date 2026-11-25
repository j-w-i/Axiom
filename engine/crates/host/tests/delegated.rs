//! Agents that are programs of the person's own, through the real socket. The program plays a script
//! (`harness/tests/fake`): no real harness is run.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, Instant};

use axiom_host::{Config, client::Client};
use axiom_proto::*;
use serde_json::{Value, json};

mod common;
use common::*;

struct T {
    f: Fixture,
    c: Client,
    s: SessionId,
    /// All that was said, so far.
    log: Vec<Body>,
}

const FAKE: &str = env!("CARGO_BIN_EXE_host-fake-agent");

impl T {
    async fn new(script: &[Value]) -> Self {
        let f = fixture();
        for d in ["w", "home", "bin"] {
            std::fs::create_dir(f.dir.path().join(d)).unwrap();
        }
        for name in ["goose", "claude"] {
            std::fs::copy(FAKE, f.dir.path().join("bin").join(name)).unwrap();
        }
        let t = Self { c: Self::host(&f).await, s: SessionId::new(), f, log: vec![] };
        t.script(script);
        let mut t = t;
        let w = t.at("w");
        let call = Call::SessionCreate { title: "S".into(), workspace: Some(w) };
        let v = t.c.call(call).await.unwrap();
        t.s = SessionId(v["session"].as_str().unwrap().into());
        t.c.call(Call::Follow { session: t.s.clone(), after: 0 }).await.unwrap();
        t
    }
    async fn host(f: &Fixture) -> Client {
        let mut c = Config::new(f.paths.clone());
        c.home = Some(f.dir.path().join("home"));
        c.programs = Some(vec![f.dir.path().join("bin")]);
        f.serve_with(c);
        f.client().await
    }
    async fn restart(&mut self) {
        self.c.call(Call::Shutdown { force: true }).await.unwrap();
        for _ in 0..1000 {
            if !self.f.paths.token().exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.c = Self::host(&self.f).await;
        let after =
            self.c.call(Call::SessionList { limit: 1, offset: 0 }).await.unwrap()[0]["last_seq"]
                .as_u64()
                .unwrap();
        self.c.call(Call::Follow { session: self.s.clone(), after }).await.unwrap();
    }
    fn script(&self, all: &[Value]) {
        let text: String = all.iter().map(|v| format!("{v}\n")).collect();
        std::fs::write(self.f.dir.path().join("bin/script.jsonl"), text).unwrap();
        let _ = std::fs::remove_file(self.f.dir.path().join("bin/heard.jsonl"));
    }
    fn at(&self, s: &str) -> String {
        std::fs::canonicalize(self.f.dir.path()).unwrap().join(s).to_str().unwrap().into()
    }
    fn heard(&self) -> String {
        std::fs::read_to_string(self.f.dir.path().join("bin/heard.jsonl")).unwrap_or_default()
    }
    fn add(&self, harness: &str, access: Access, trust: bool) -> Call {
        Call::AgentAdd {
            session: self.s.clone(),
            name: "Lead".into(),
            execution: Execution {
                harness: harness.into(),
                provider: None,
                model: None,
                connection: None,
                effort: None,
                environment: None,
            },
            access,
            trust,
            instructions: vec![],
            apart: false,
        }
    }
    async fn agent(&mut self, harness: &str, access: Access) -> AgentId {
        let call = self.add(harness, access, true);
        AgentId(self.c.call(call).await.unwrap()["agent"].as_str().unwrap().into())
    }
    async fn prompt(&mut self, a: &AgentId, text: &str) -> Value {
        let (session, agent) = (self.s.clone(), a.clone());
        let call = Call::Prompt { session, agent, text: text.into(), key: None };
        self.c.call(call).await.unwrap()
    }
    async fn until<R>(&mut self, mut f: impl FnMut(&Body) -> Option<R>) -> (R, Vec<Body>) {
        let mut all = Vec::new();
        loop {
            let m = self.c.next_within(Duration::from_secs(30)).await.unwrap();
            let Some(m) = m else { panic!("nothing more after {all:#?}; heard {}", self.heard()) };
            if let Message::Events { events, .. } = m {
                for e in events {
                    let r = f(&e.body);
                    self.log.push(e.body.clone());
                    all.push(e.body);
                    if let Some(r) = r {
                        return (r, all);
                    }
                }
            }
        }
    }
    async fn ended(&mut self) -> (Outcome, Option<String>, Vec<Body>) {
        let ((o, e), all) = self
            .until(|b| match b {
                Body::RunEnded { outcome, error, .. } => Some((*outcome, error.clone())),
                _ => None,
            })
            .await;
        (o, e, all)
    }
    async fn question(&mut self) -> (RequestId, String, String, String) {
        self.until(|b| match b {
            Body::RequestOpened {
                request,
                digest,
                subject,
                scope,
                session: false,
                answerable: Answerable::Live,
                ..
            } => Some((request.clone(), digest.clone(), subject.clone(), scope.clone())),
            _ => None,
        })
        .await
        .0
    }
    async fn answer(&mut self, request: RequestId, answer: Answer, digest: String) {
        let call = Call::Answer { session: self.s.clone(), request, answer, digest };
        self.c.call(call).await.unwrap();
    }
    /// The question whether it may be started.
    async fn allow_start(&mut self) -> (String, String) {
        let (id, digest, subject, scope) = self.question().await;
        self.answer(id, Answer::Once, digest).await;
        (subject, scope)
    }
    async fn audit(&mut self) -> Vec<axiom_policy::audit::Entry> {
        let v = self.c.call(Call::Audit { after: 0, limit: 100 }).await.unwrap();
        serde_json::from_value(v).unwrap()
    }
}

fn texts(all: &[Body], kind: ItemKind) -> Vec<String> {
    let of: Vec<&ItemId> = all
        .iter()
        .filter_map(|b| match b {
            Body::ItemStarted { item, kind: k, .. } if *k == kind => Some(item),
            _ => None,
        })
        .collect();
    all.iter()
        .filter_map(|b| match b {
            Body::ItemEnded { item, text, .. } if of.contains(&item) => Some(text.clone()),
            _ => None,
        })
        .collect()
}

// Goose, by the Agent Client Protocol.

fn acp(more: &[Value]) -> Vec<Value> {
    let mut all = vec![
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 1, "agentCapabilities": {}}}}),
        json!({"hear": {"method": "session/new"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "s1"}}}),
    ];
    all.extend_from_slice(more);
    all
}
fn hear(text: &str) -> Value {
    json!({"hear": {"method": "session/prompt", "params": {"prompt": [{"type": "text", "text": text}]}}})
}
fn say(u: Value) -> Value {
    json!({"say": {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s1", "update": u}}})
}
fn words(t: &str) -> Value {
    say(
        json!({"sessionUpdate": "agent_message_chunk", "messageId": t, "content": {"type": "text", "text": t}}),
    )
}
fn ask(id: u64, command: &str) -> Value {
    json!({"say": {"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {"sessionId": "s1",
        "toolCall": {"toolCallId": "c", "title": "Run it", "rawInput": {"command": command}},
        "options": [{"optionId": "yes", "name": "Yes", "kind": "allow_once"},
            {"optionId": "ever", "name": "Ever", "kind": "allow_always"},
            {"optionId": "no", "name": "No", "kind": "reject_once"}]}}})
}
fn answered(id: u64, option: &str) -> Value {
    json!({"hear": {"id": id, "result": {"outcome": {"outcome": "selected", "optionId": option}}}})
}
fn end(id: u64, stop: &str) -> Value {
    json!({"say": {"jsonrpc": "2.0", "id": id, "result": {"stopReason": stop}}})
}

#[tokio::test]
async fn it_is_started_by_a_yes_works_asks_and_stays() {
    let mut t = T::new(&acp(&[
        hear("go"),
        words("I will run it."),
        say(json!({"sessionUpdate": "tool_call", "toolCallId": "c", "title": "Run it", "kind": "execute", "status": "pending", "rawInput": {"command": "make"}})),
        ask(5, "make"),
        answered(5, "yes"),
        say(json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "completed",
            "content": [{"type": "content", "content": {"type": "text", "text": "built"}},
                {"type": "diff", "path": "/w/a.txt", "oldText": "a\n", "newText": "b\n"}]})),
        words("Done."),
        end(2, "end_turn"),
        hear("more"),
        ask(6, "rm x"),
        answered(6, "no"),
        words("Then not."),
        end(3, "end_turn"),
    ]))
    .await;
    let a = t.agent("goose", Access::Ask).await;
    t.prompt(&a, "go").await;
    let (subject, scope) = t.allow_start().await;
    assert_eq!(subject, format!("{} acp", t.at("bin/goose")));
    assert!(scope.contains("nothing holds it back"), "{scope}");
    let (id, digest, subject, scope) = t.question().await;
    assert!(subject.starts_with("Run it") && subject.contains("make"), "{subject}");
    assert!(scope.contains("outside Axiom's sandbox"));
    // Once is all there is: what it would remember is its own.
    let e = t.c.call(Call::Answer {
        session: t.s.clone(),
        request: id.clone(),
        answer: Answer::Session,
        digest: digest.clone(),
    });
    assert!(e.await.is_err());
    t.answer(id, Answer::Once, digest).await;
    let (o, e, all) = t.ended().await;
    assert_eq!((o, e), (Outcome::Completed, None));
    assert_eq!(texts(&t.log, ItemKind::Prose), ["I will run it.", "Done."]);
    let tool = texts(&t.log, ItemKind::Tool);
    assert!(tool[0].contains("built"), "{tool:?}");
    let diff = all.iter().find_map(|b| match b {
        Body::ItemEnded { result: Some(r), .. } => r["diff"].as_str().map(str::to_owned),
        _ => None,
    });
    assert!(diff.unwrap().contains("+b"));
    // It is there still: the next is not asked about.
    t.prompt(&a, "more").await;
    let (id, digest, subject, _) = t.question().await;
    assert!(subject.contains("rm x"));
    t.answer(id, Answer::Deny, digest).await;
    let (o, _, all) = t.ended().await;
    assert_eq!((o, texts(&all, ItemKind::Prose)), (Outcome::Completed, vec!["Then not.".into()]));
    // What was allowed is on record, with who holds it back (SI-23, SI-26).
    let log = t.audit().await;
    let seen: Vec<_> = log.iter().map(|e| (e.tool.as_str(), e.containment)).collect();
    let none = axiom_policy::audit::Containment::None;
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], ("harness/goose", none));
    assert!(seen[1].0.starts_with("goose/") && seen[1].1 == none);
    assert!(log.iter().all(|e| e.answered.is_some()));
    assert_eq!(axiom_policy::audit::broken(&log), None);
    // And what went over the wire.
    let db = rusqlite::Connection::open(t.f.paths.journal()).unwrap();
    let n: i64 = db.query_row("select count(*) from frame", [], |r| r.get(0)).unwrap();
    assert!(n > 12, "{n}");
}

#[tokio::test]
async fn without_a_yes_nothing_is_started() {
    let mut t = T::new(&acp(&[])).await;
    let a = t.agent("goose", Access::Ask).await;
    t.prompt(&a, "go").await;
    let (id, digest, ..) = t.question().await;
    t.answer(id, Answer::Deny, digest).await;
    let (o, e, _) = t.ended().await;
    assert!(o == Outcome::Failed && e.unwrap().contains("did not allow"));
    // Nor when it is stopped instead of answered.
    let v = t.prompt(&a, "go").await;
    t.question().await;
    let run = RunId(v["run"].as_str().unwrap().into());
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    assert_eq!(t.ended().await.0, Outcome::Cancelled);
    assert!(!t.f.dir.path().join("bin/seen.json").exists());
    assert!(t.audit().await.is_empty());
    // Nothing that was said is lost.
    t.script(&acp(&[hear("go\n\ngo\n\nnow"), end(2, "end_turn")]));
    t.prompt(&a, "now").await;
    t.allow_start().await;
    assert_eq!(t.ended().await.0, Outcome::Completed);
}

#[tokio::test]
async fn what_a_delegated_agent_may_not_be() {
    let mut t = T::new(&[]).await;
    let refused = [
        (t.add("goose", Access::Ask, false), "settings"),
        (t.add("goose", Access::Work, true), "no mode"),
        (t.add("claude", Access::Ask, true), "no mode"),
        (t.add("gemini", Access::Ask, true), "gemini"),
        (t.add("other", Access::Ask, true), "does not know"),
    ];
    for (call, why) in refused {
        let e = t.c.call(call).await.unwrap_err().to_string();
        assert!(e.contains(why), "{e}");
    }
    let Call::AgentAdd { session, name, execution, access, trust, .. } =
        t.add("goose", Access::Ask, true)
    else {
        panic!()
    };
    let with = |execution, instructions| Call::AgentAdd {
        session: session.clone(),
        name: name.clone(),
        execution,
        access,
        trust,
        instructions,
        apart: false,
    };
    let x = Execution { connection: Some("c_1".into()), ..execution.clone() };
    assert!(t.c.call(with(x, vec![])).await.unwrap_err().to_string().contains("one account"));
    let x = Execution { model: Some("--yolo".into()), ..execution.clone() };
    assert!(t.c.call(with(x, vec![])).await.is_err());
    // A program that lies in a folder agents write to is not started (SI-22).
    let a = t.agent("goose", Access::Ask).await;
    std::fs::remove_file(t.f.dir.path().join("bin/goose")).unwrap();
    std::fs::copy(FAKE, t.f.dir.path().join("w/goose")).unwrap();
    std::os::unix::fs::symlink(t.at("w/goose"), t.f.dir.path().join("bin/goose")).unwrap();
    let (session, agent) = (t.s.clone(), a.clone());
    let e = t.c.call(Call::Prompt { session, agent, text: "x".into(), key: None }).await;
    assert!(e.unwrap_err().to_string().contains("inside what an agent may write"));
    // Nor one that lies where agents have their copies.
    let copies = t.f.dir.path().join("home/.axiom-work/s");
    std::fs::create_dir_all(&copies).unwrap();
    std::fs::copy(FAKE, copies.join("goose")).unwrap();
    std::fs::remove_file(t.f.dir.path().join("bin/goose")).unwrap();
    std::os::unix::fs::symlink(t.at("home/.axiom-work/s/goose"), t.f.dir.path().join("bin/goose"))
        .unwrap();
    let (session, agent) = (t.s.clone(), a.clone());
    let e = t.c.call(Call::Prompt { session, agent, text: "x".into(), key: None }).await;
    assert!(e.unwrap_err().to_string().contains("inside what an agent may write"));
    // It keeps its harness.
    let execution = Execution { harness: "claude".into(), ..execution };
    let set =
        Call::AgentSet { session: t.s.clone(), agent: a, execution: Some(execution), access: None };
    assert!(t.c.call(set).await.unwrap_err().to_string().contains("keeps its harness"));
}

#[tokio::test]
async fn harnesses_are_listed_without_being_run() {
    let mut t = T::new(&[]).await;
    let v = t.c.call(Call::Harnesses {}).await.unwrap();
    let all: Vec<HarnessInfo> = serde_json::from_value(v).unwrap();
    let of = |id: &str| all.iter().find(|h| h.id == id).unwrap();
    assert_eq!(of("goose").program.as_deref(), Some(t.at("bin/goose").as_str()));
    assert_eq!(
        (of("goose").access.as_slice(), of("goose").needs_trust),
        (&[Access::Ask][..], true)
    );
    assert_eq!(of("claude").access, [Access::Read, Access::Work]);
    assert!(!of("claude").needs_trust);
    assert!(of("codex").program.is_none() && of("codex").problem.is_some());
    assert!(!t.f.dir.path().join("bin/seen.json").exists());
}

/// What it cannot hear while it works it is told after.
#[tokio::test]
async fn it_is_told_more_when_it_can_hear_it() {
    let mut t = T::new(&acp(&[
        hear("first"),
        words("Working."),
        json!({"sleep": 600}),
        end(2, "end_turn"),
        hear("second\n\nthird"),
        words("All."),
        end(3, "end_turn"),
    ]))
    .await;
    let a = t.agent("goose", Access::Ask).await;
    t.prompt(&a, "first").await;
    t.allow_start().await;
    t.until(|b| matches!(b, Body::ItemStarted { kind: ItemKind::Prose, .. }).then_some(())).await;
    assert_eq!(t.prompt(&a, "second").await["queued"], true);
    assert_eq!(t.prompt(&a, "third").await["queued"], true);
    let mut prose = Vec::new();
    while !prose.contains(&"All.".to_string()) {
        let (o, e, all) = t.ended().await;
        assert_eq!((o, e), (Outcome::Completed, None), "{}", t.heard());
        prose.extend(texts(&all, ItemKind::Prose));
    }
}

#[tokio::test]
async fn it_is_stopped() {
    let mark = format!("6{:05}", std::process::id() % 100_000);
    let mut t = T::new(&acp(&[
        hear("go"),
        json!({"child": mark}),
        words("Working."),
        json!({"hear": {"method": "session/cancel"}}),
        end(2, "cancelled"),
        hear("again"),
        words("Here."),
        end(3, "end_turn"),
        json!({"hang": true}),
    ]))
    .await;
    let alive = || {
        let o = std::process::Command::new("/bin/ps").args(["-axo", "args="]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with(&format!("sleep {mark}")))
    };
    let a = t.agent("goose", Access::Ask).await;
    let v = t.prompt(&a, "go").await;
    t.allow_start().await;
    t.until(|b| matches!(b, Body::ItemStarted { kind: ItemKind::Prose, .. }).then_some(())).await;
    let run = RunId(v["run"].as_str().unwrap().into());
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    assert_eq!(t.ended().await.0, Outcome::Cancelled);
    // Its program is there still.
    t.prompt(&a, "again").await;
    let (o, _, all) = t.ended().await;
    assert_eq!((o, texts(&all, ItemKind::Prose)), (Outcome::Completed, vec!["Here.".into()]));
    assert!(alive());
    // Changed, it is another: the program ends, and all it runs.
    let set = Call::AgentSet {
        session: t.s.clone(),
        agent: a,
        execution: None,
        access: Some(Access::Ask),
    };
    let mut r = t.c.call(set.clone()).await;
    for _ in 0..100 {
        if r.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        r = t.c.call(set.clone()).await;
    }
    r.unwrap();
    let now = Instant::now();
    while alive() {
        assert!(now.elapsed() < Duration::from_secs(20));
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn when_it_dies_that_is_said_and_it_begins_anew() {
    let mut t = T::new(&acp(&[
        hear("go"),
        words("Half"),
        json!({"err": "out of luck"}),
        json!({"exit": 7}),
    ]))
    .await;
    let a = t.agent("goose", Access::Ask).await;
    t.prompt(&a, "go").await;
    t.allow_start().await;
    let (o, e, all) = t.ended().await;
    assert_eq!(o, Outcome::Failed, "{e:?}");
    assert_eq!(texts(&all, ItemKind::Prose), ["Half"]);
    t.script(&acp(&[hear("again"), end(2, "end_turn")]));
    t.prompt(&a, "again").await;
    t.allow_start().await;
    let (o, e, all) = t.ended().await;
    assert_eq!((o, e), (Outcome::Completed, None));
    assert!(texts(&all, ItemKind::Notice)[0].contains("begins anew"));
}

// Claude Code, which can be gone on with.

fn claude(more: &[Value]) -> Vec<Value> {
    let mut all = vec![
        json!({"hear": {"type": "control_request", "request": {"subtype": "initialize"}}, "as": "i"}),
        json!({"say": {"type": "control_response", "response": {"subtype": "success", "request_id": "$i/request_id", "response": {"commands": [], "models": []}}}}),
    ];
    all.extend_from_slice(more);
    all
}
fn turn(text: &str, reply: &str) -> Vec<Value> {
    vec![
        json!({"hear": {"type": "user", "message": {"role": "user", "content": text}}, "as": "p"}),
        json!({"say": {"type": "system", "subtype": "init", "uuid": "u1", "session_id": "$p/session_id", "claude_code_version": "2.1.330", "cwd": "/w", "tools": [], "model": "claude-opus-5-5", "permissionMode": "acceptEdits"}}),
        json!({"say": {"type": "assistant", "uuid": "u2", "session_id": "$p/session_id", "parent_tool_use_id": null, "message": {"id": reply, "type": "message", "role": "assistant", "content": [{"type": "text", "text": reply}]}}}),
        json!({"say": {"type": "result", "subtype": "success", "uuid": "u3", "session_id": "$p/session_id", "is_error": false, "num_turns": 1, "result": reply, "stop_reason": "end_turn", "usage": {"input_tokens": 3, "output_tokens": 6}, "user_message_uuids": ["$p/uuid"]}}),
    ]
}

#[tokio::test]
async fn it_goes_on_where_it_was_after_the_host_is_gone() {
    let mut t = T::new(&claude(&turn("go", "One."))).await;
    let call = t.add("claude", Access::Work, false);
    let a = AgentId(t.c.call(call).await.unwrap()["agent"].as_str().unwrap().into());
    t.prompt(&a, "go").await;
    let (subject, scope) = t.allow_start().await;
    assert!(
        subject.contains("--permission-mode acceptEdits")
            && subject.contains("--setting-sources user"),
        "{subject}"
    );
    assert!(
        scope.contains("its own sandbox") && scope.contains("settings for it are ignored"),
        "{scope}"
    );
    let (o, e, all) = t.ended().await;
    assert_eq!((o, e), (Outcome::Completed, None), "{}", t.heard());
    assert_eq!(texts(&all, ItemKind::Prose), ["One."]);
    let seen = |t: &T| std::fs::read_to_string(t.f.dir.path().join("bin/seen.json")).unwrap();
    let first: Value = serde_json::from_str(&seen(&t)).unwrap();
    let args: Vec<&str> =
        first["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    let at = args.iter().position(|a| *a == "--session-id").unwrap();
    let id = args[at + 1].to_owned();
    // It gets nothing of the host's surroundings (SI-11).
    let env: Vec<&str> = first["env"].as_object().unwrap().keys().map(String::as_str).collect();
    assert!(
        env.iter()
            .all(|e| ["HOME", "PATH", "USER", "LANG", "TMPDIR", "__CF_USER_TEXT_ENCODING"]
                .contains(e)),
        "{env:?}"
    );
    t.restart().await;
    t.script(&claude(&turn("more", "Two.")));
    t.prompt(&a, "more").await;
    let (subject, _) = t.allow_start().await;
    assert!(subject.contains(&format!("--resume {id}")), "{subject}");
    let (o, e, all) = t.ended().await;
    assert_eq!((o, e), (Outcome::Completed, None), "{}", t.heard());
    assert_eq!(texts(&all, ItemKind::Prose), ["Two."]);
    let log = t.audit().await;
    assert_eq!(log.len(), 2);
    assert!(log.iter().all(|e| e.containment == axiom_policy::audit::Containment::Harness));
}

/// A program of the person's own starts in the copy; where it is gone on with, it is there too.
#[tokio::test]
async fn it_starts_in_its_own_copy() {
    let mut t = T::new(&claude(&turn("go", "One."))).await;
    std::fs::write(t.f.dir.path().join("w/a.txt"), "a").unwrap();
    let mut call = t.add("claude", Access::Work, false);
    if let Call::AgentAdd { apart, .. } = &mut call {
        *apart = true;
    }
    let a = AgentId(t.c.call(call).await.unwrap()["agent"].as_str().unwrap().into());
    let copy = format!("{}/.axiom-work/{}/{}", t.at("home"), t.s.0, a.0);
    t.prompt(&a, "go").await;
    t.allow_start().await;
    let (o, e, _) = t.ended().await;
    assert_eq!((o, e), (Outcome::Completed, None), "{}", t.heard());
    let cwd = |t: &T| {
        let seen = std::fs::read_to_string(t.f.dir.path().join("bin/seen.json")).unwrap();
        serde_json::from_str::<Value>(&seen).unwrap()["cwd"].as_str().unwrap().to_owned()
    };
    assert_eq!(cwd(&t), copy);
    assert!(Path::new(&copy).join("a.txt").exists());
    t.restart().await;
    t.script(&claude(&turn("more", "Two.")));
    t.prompt(&a, "more").await;
    let (subject, _) = t.allow_start().await;
    assert!(subject.contains("--resume "), "{subject}");
    t.ended().await;
    assert_eq!(cwd(&t), copy);
}

/// PR-4: several accounts of one service, each where the program itself keeps them apart.
#[tokio::test]
async fn a_second_account_is_kept_apart() {
    let mut t = T::new(&claude(&turn("go", "One."))).await;
    let add = |name: &str, secret: Option<&str>| Call::ConnectionAdd {
        name: name.into(),
        provider: "claude-code".into(),
        url: None,
        secret: secret.map(|s| SecretText(s.into())),
    };
    assert!(t.c.call(add("Work", Some("sk-x"))).await.is_err(), "it is given no key");
    let work =
        t.c.call(add("Work", None)).await.unwrap()["connection"].as_str().unwrap().to_owned();
    let more =
        t.c.call(add("More", None)).await.unwrap()["connection"].as_str().unwrap().to_owned();
    let all: Vec<Connection> =
        serde_json::from_value(t.c.call(Call::Connections {}).await.unwrap()).unwrap();
    let dir = t.f.paths.data.join("accounts").join(&work);
    let sign_in = all.iter().find(|c| c.id == work).unwrap().sign_in.clone().unwrap();
    assert_eq!(sign_in, format!("CLAUDE_CONFIG_DIR='{}' claude auth login", dir.display()));
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    let v = t.c.call(Call::Harnesses {}).await.unwrap();
    let list: Vec<HarnessInfo> = serde_json::from_value(v).unwrap();
    let of = |id: &str| list.iter().find(|h| h.id == id).unwrap().accounts.clone();
    assert_eq!(
        (of("claude").as_deref(), of("goose"), of("codex")),
        (Some("claude-code"), None, None)
    );
    let with = |t: &T, harness: &str, access, connection: &str| match t.add(harness, access, true) {
        Call::AgentAdd { session, name, execution, access, trust, instructions, apart } => {
            Call::AgentAdd {
                session,
                name,
                execution: Execution { connection: Some(connection.into()), ..execution },
                access,
                trust,
                instructions,
                apart,
            }
        }
        _ => panic!(),
    };
    let call = Call::ConnectionAdd {
        name: "Here".into(),
        provider: "lm-studio".into(),
        url: None,
        secret: None,
    };
    let v = t.c.call(call).await.unwrap();
    let here = v["connection"].as_str().unwrap();
    for (harness, access, c, why) in [
        ("claude", Access::Work, here, "not one of that program"),
        ("goose", Access::Ask, work.as_str(), "one account"),
        ("claude", Access::Work, "c_none", "no such account"),
    ] {
        let e = t.c.call(with(&t, harness, access, c)).await.unwrap_err().to_string();
        assert!(e.contains(why), "{e}");
    }
    // Nor is it one for Axiom's own loop.
    let e = t.c.call(Call::Models { connection: work.clone() }).await.unwrap_err();
    assert!(e.to_string().contains("not known"), "{e}");
    let v = t.c.call(with(&t, "claude", Access::Work, &work)).await.unwrap();
    let a = AgentId(v["agent"].as_str().unwrap().into());
    t.prompt(&a, "go").await;
    let (_, scope) = t.allow_start().await;
    let real = std::fs::canonicalize(&dir).unwrap();
    assert!(scope.contains(&format!("account is kept in {}", real.display())), "{scope}");
    assert_eq!(t.ended().await.0, Outcome::Completed);
    let seen = |t: &T| -> Value {
        let s = std::fs::read_to_string(t.f.dir.path().join("bin/seen.json")).unwrap();
        serde_json::from_str(&s).unwrap()
    };
    assert_eq!(seen(&t)["env"]["CLAUDE_CONFIG_DIR"], real.to_str().unwrap());
    // On another account it begins anew: what was said is with the first.
    t.script(&claude(&turn("go", "Two.")));
    let set = |connection: Option<&str>| Call::AgentSet {
        session: t.s.clone(),
        agent: a.clone(),
        execution: Some(Execution {
            harness: "claude".into(),
            provider: None,
            model: None,
            connection: connection.map(str::to_owned),
            effort: None,
            environment: None,
        }),
        access: None,
    };
    let (to_more, to_first) = (set(Some(&more)), set(None));
    for (call, dir) in [(to_more, Some(more)), (to_first, None)] {
        let mut r = t.c.call(call.clone()).await;
        for _ in 0..200 {
            if r.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            r = t.c.call(call.clone()).await;
        }
        r.unwrap();
        t.script(&claude(&turn("go", "Two.")));
        t.prompt(&a, "go").await;
        let (subject, _) = t.allow_start().await;
        assert!(subject.contains("--session-id") && !subject.contains("--resume"), "{subject}");
        let (o, _, all) = t.ended().await;
        assert_eq!(o, Outcome::Completed);
        assert!(texts(&all, ItemKind::Notice).iter().any(|n| n.contains("begins anew")));
        let env = seen(&t)["env"]["CLAUDE_CONFIG_DIR"].as_str().map(str::to_owned);
        let want = dir.map(|d| real.with_file_name(d).to_str().unwrap().to_owned());
        assert_eq!(env, want);
    }
    // Removed here, it is still the person's: what the program keeps there is not touched.
    t.c.call(Call::ConnectionRemove { connection: work }).await.unwrap();
    assert!(dir.is_dir());
}
