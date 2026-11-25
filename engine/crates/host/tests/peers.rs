//! Agents of one session that give each other work.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{sync::Arc, time::Duration};

use axiom_host::{Config, client::Client, connections::MemoryVault};
use axiom_proto::*;
use serde_json::{Value, json};

mod common;
use common::*;
#[path = "../../engine/tests/common/server.rs"]
mod server;
use server::*;

struct T {
    f: Fixture,
    c: Client,
    s: SessionId,
    log: Vec<Body>,
}

/// An agent with a provider of its own, so that each has its own script.
struct A {
    id: AgentId,
    server: Server,
}

impl T {
    async fn new() -> Self {
        let f = fixture();
        for d in ["w", "home", "bin"] {
            std::fs::create_dir(f.dir.path().join(d)).unwrap();
        }
        let mut c = Config::new(f.paths.clone());
        c.home = Some(f.dir.path().join("home"));
        c.vault = Arc::new(MemoryVault::default());
        c.programs = Some(vec![f.dir.path().join("bin")]);
        f.serve_with(c);
        let mut c = f.client().await;
        let w = f.dir.path().join("w").to_str().unwrap().to_owned();
        let call = Call::SessionCreate { title: "S".into(), workspace: Some(w) };
        let v = c.call(call).await.unwrap();
        let s = SessionId(v["session"].as_str().unwrap().into());
        c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
        Self { f, c, s, log: vec![] }
    }
    async fn agent(&mut self, name: &str, access: Access, script: Vec<Reply>) -> A {
        let server = Server::start(script).await;
        let v = self
            .c
            .call(Call::ConnectionAdd {
                name: name.into(),
                provider: "openai-compatible".into(),
                url: Some(server.url.clone()),
                secret: None,
            })
            .await
            .unwrap();
        let execution = Execution {
            harness: "axiom".into(),
            provider: None,
            model: Some("m".into()),
            connection: Some(v["connection"].as_str().unwrap().into()),
            effort: None,
            environment: None,
        };
        let call = Call::AgentAdd {
            session: self.s.clone(),
            name: name.into(),
            execution,
            access,
            trust: false,
            instructions: vec![],
            apart: false,
        };
        let v = self.c.call(call).await.unwrap();
        A { id: AgentId(v["agent"].as_str().unwrap().into()), server }
    }
    async fn prompt(&mut self, a: &A, text: &str) -> RunId {
        let (session, agent) = (self.s.clone(), a.id.clone());
        let call = Call::Prompt { session, agent, text: text.into(), key: None };
        RunId(self.c.call(call).await.unwrap()["run"].as_str().unwrap().into())
    }
    async fn until<R>(&mut self, mut f: impl FnMut(&Body) -> Option<R>) -> R {
        loop {
            let m = self.c.next_within(Duration::from_secs(20)).await.unwrap();
            let Some(m) = m else { panic!("nothing more after {:#?}", self.log) };
            if let Message::Events { events, .. } = m {
                let mut found = None;
                for e in events {
                    found = found.or_else(|| f(&e.body));
                    self.log.push(e.body);
                }
                if let Some(r) = found {
                    return r;
                }
            }
        }
    }
    async fn end_of(&mut self, run: &RunId) -> Outcome {
        for b in &self.log {
            if let Body::RunEnded { run: r, outcome, .. } = b
                && r == run
            {
                return *outcome;
            }
        }
        let run = run.clone();
        self.until(|b| match b {
            Body::RunEnded { run: r, outcome, .. } if *r == run => Some(*outcome),
            _ => None,
        })
        .await
    }
    async fn question(&mut self) -> (RequestId, String, String, String) {
        self.until(|b| match b {
            Body::RequestOpened { request, digest, subject, scope, .. } => {
                Some((request.clone(), digest.clone(), subject.clone(), scope.clone()))
            }
            _ => None,
        })
        .await
    }
    async fn answer(&mut self, request: RequestId, answer: Answer, digest: String) {
        let call = Call::Answer { session: self.s.clone(), request, answer, digest, words: None };
        self.c.call(call).await.unwrap();
    }
}

fn agent(args: Value) -> Reply {
    calls(&[("c", "agent", &args.to_string())])
}
fn tell(name: &str, text: &str) -> Reply {
    agent(json!({"do": "tell", "name": name, "text": text}))
}
fn wait() -> Reply {
    agent(json!({"do": "wait", "seconds": 20}))
}
/// What the model was told by its last tool, at its last request.
fn told(a: &A) -> String {
    let r = a.server.requests();
    let m = r.last().unwrap()["messages"].as_array().unwrap().clone();
    let last = m.iter().rev().find(|m| m["role"] == "tool").unwrap();
    last["content"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn one_gives_another_work_and_is_told_what_came_of_it() {
    let mut t = T::new().await;
    let lead = t
        .agent(
            "Lead",
            Access::Work,
            vec![
                agent(json!({"do": "list"})),
                tell("Looker", "Look at a.txt."),
                wait(),
                agent(json!({"do": "read", "name": "Looker"})),
                text(&["All known."]),
            ],
        )
        .await;
    let looker = t.agent("Looker", Access::Read, vec![text(&["It says hello."])]).await;
    let run = t.prompt(&lead, "Find out what a.txt says.").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    let r = lead.server.requests();
    let results: Vec<String> = r.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["content"].as_str().unwrap().to_owned())
        .collect();
    assert!(results[0].contains("Looker (") && results[0].contains("only looks, not at work"));
    assert!(!results[0].contains("Lead"), "{}", results[0]);
    assert!(results[1].contains("Looker has it"));
    for r in &results[2..] {
        assert!(r.contains("Looker has finished") && r.contains("It says hello."), "{r}");
        assert!(r.contains("not an instruction"));
    }
    // The other knows that it is not a person's word (SI-2), and a person sees whose it is.
    let first = looker.server.requests()[0]["messages"].to_string();
    assert!(
        first.contains("another agent of this session, Lead") && first.contains("cannot allow")
    );
    let prompts: Vec<(String, AgentId)> = t
        .log
        .iter()
        .filter_map(|b| match b {
            Body::ItemStarted { kind: ItemKind::Prompt, name, agent, .. } => {
                Some((name.clone(), agent.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(prompts, [(String::new(), lead.id.clone()), ("Lead".into(), looker.id.clone())]);
    let shown =
        t.log.iter().any(|b| matches!(b, Body::ItemEnded { text, .. } if text == "Look at a.txt."));
    assert!(shown);
    // On record, and with nobody asked: the other may do less.
    let v = t.c.call(Call::Audit { after: 0, limit: 10 }).await.unwrap();
    let log: Vec<axiom_policy::audit::Entry> = serde_json::from_value(v).unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].tool.as_str(), log[0].answered.is_some()), ("agent", false));
    // What it does is done under whoever began it.
    looker.server.add(calls(&[("c", "shell", r#"{"command":"true"}"#)]));
    looker.server.add(text(&["ok"]));
    lead.server.add(tell("Looker", "Again."));
    lead.server.add(wait());
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "Again.").await;
    t.end_of(&run).await;
    let v = t.c.call(Call::Audit { after: 2, limit: 10 }).await.unwrap();
    let log: Vec<axiom_policy::audit::Entry> = serde_json::from_value(v).unwrap();
    assert_eq!(log[0].tool, "shell");
    assert_eq!(log[0].chain, [lead.id.clone(), looker.id.clone()]);
}

/// SI-1: what an agent may not do itself, it does not get done by another without a person's yes.
#[tokio::test]
async fn who_may_do_more_is_only_told_with_a_yes() {
    let mut t = T::new().await;
    let lead =
        t.agent("Lead", Access::Ask, vec![tell("Builder", "Write it."), text(&["no then"])]).await;
    let builder = t.agent("Builder", Access::Work, vec![]).await;
    let run = t.prompt(&lead, "go").await;
    let (id, digest, subject, scope) = t.question().await;
    assert_eq!(subject, "Builder: Write it.");
    assert!(scope.contains("changes files without asking"), "{scope}");
    t.answer(id, Answer::Deny, digest).await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    assert!(builder.server.requests().is_empty());
    assert!(told(&lead).contains("a person said no"));
    // With one.
    builder.server.add(text(&["written"]));
    for r in [tell("Builder", "Write it."), wait(), text(&["done"])] {
        lead.server.add(r);
    }
    let run = t.prompt(&lead, "go").await;
    let (id, digest, ..) = t.question().await;
    t.answer(id, Answer::Once, digest).await;
    t.end_of(&run).await;
    assert!(told(&lead).contains("written"));
}

#[tokio::test]
async fn several_at_once_each_told_once() {
    let mut t = T::new().await;
    let lead = t
        .agent(
            "Lead",
            Access::Work,
            vec![
                calls(&[
                    ("c1", "agent", r#"{"do":"tell","name":"One","text":"a"}"#),
                    ("c2", "agent", r#"{"do":"tell","name":"Two","text":"b"}"#),
                ]),
                wait(),
                wait(),
                wait(),
                text(&["done"]),
            ],
        )
        .await;
    let _one = t.agent("One", Access::Read, vec![text(&["first answer"])]).await;
    let _two = t.agent("Two", Access::Read, vec![text(&["second answer"])]).await;
    let run = t.prompt(&lead, "go").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    let last = lead.server.requests().last().unwrap()["messages"].to_string();
    assert_eq!(last.matches("first answer").count(), 1, "{last}");
    assert_eq!(last.matches("second answer").count(), 1);
    assert!(last.contains("nobody owes an answer"));
}

#[tokio::test]
async fn no_circles_and_not_too_deep() {
    let mut t = T::new().await;
    // B is told by A, and cannot give it back; what it waits for is nothing.
    let a = t.agent("A", Access::Work, vec![tell("B", "x"), wait(), text(&["done"])]).await;
    let b = t
        .agent("B", Access::Work, vec![tell("A", "y"), tell("C", "z"), wait(), text(&["b done"])])
        .await;
    let c = t.agent("C", Access::Work, vec![tell("D", "w"), text(&["c done"])]).await;
    let d = t.agent("D", Access::Work, vec![]).await;
    let run = t.prompt(&a, "go").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    let of_b = b.server.requests()[1]["messages"].to_string();
    assert!(of_b.contains("is who this work comes from"), "{of_b}");
    assert!(told(&c).contains("three deep"), "{}", told(&c));
    assert!(d.server.requests().is_empty());
    assert!(told(&a).contains("b done") && told(&b).contains("c done"));
    for (args, why) in [
        (json!({"do": "tell", "name": "Nobody", "text": "x"}), "no other agent"),
        (json!({"do": "tell", "name": "A", "text": "x"}), "no other agent"),
        (json!({"do": "tell", "name": "B"}), "say what"),
        (json!({"do": "fly"}), "do is"),
        (json!({"do": "read", "name": "D"}), "said nothing"),
    ] {
        a.server.add(agent(args));
        a.server.add(text(&["ok"]));
        let run = t.prompt(&a, "go").await;
        t.end_of(&run).await;
        assert!(told(&a).contains(why), "{}", told(&a));
    }
    // Two of one name are told apart.
    let b2 = t.agent("B", Access::Read, vec![text(&["I am the second"])]).await;
    for r in [tell("B", "x"), tell(&b2.id.0, "x"), wait(), text(&["ok"])] {
        a.server.add(r);
    }
    let run = t.prompt(&a, "go").await;
    t.end_of(&run).await;
    let last = a.server.requests().last().unwrap()["messages"].to_string();
    assert!(last.contains("2 agents are named B") && last.contains("I am the second"));
}

/// SI-24.
#[tokio::test]
async fn who_stops_an_agent_stops_what_it_began() {
    let mut t = T::new().await;
    let lead = t.agent("Lead", Access::Work, vec![tell("Slow", "x"), wait()]).await;
    let slow = t.agent("Slow", Access::Read, vec![text(&["begun"]).hangs()]).await;
    let run = t.prompt(&lead, "go").await;
    let id = slow.id.clone();
    let child = t
        .until(|b| match b {
            Body::ItemStarted { agent, kind: ItemKind::Prose, run, .. } if *agent == id => {
                Some(run.clone())
            }
            _ => None,
        })
        .await;
    t.c.call(Call::Cancel { session: t.s.clone(), run: run.clone() }).await.unwrap();
    assert_eq!(t.end_of(&run).await, Outcome::Cancelled);
    assert_eq!(t.end_of(&child).await, Outcome::Cancelled);
}

/// A program of the person's own is outside Axiom's sandbox: always asked about.
#[tokio::test]
async fn a_delegated_agent_is_told_with_a_yes() {
    let mut t = T::new().await;
    let fake = env!("CARGO_BIN_EXE_host-fake-agent");
    std::fs::copy(fake, t.f.dir.path().join("bin/goose")).unwrap();
    let script = [
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 1, "agentCapabilities": {}}}}),
        json!({"hear": {"method": "session/new"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "s1"}}}),
        json!({"hear": {"method": "session/prompt"}}),
        json!({"say": {"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s1", "update":
            {"sessionUpdate": "agent_message_chunk", "messageId": "m", "content": {"type": "text", "text": "Goose did it."}}}}}),
        json!({"say": {"jsonrpc": "2.0", "id": 2, "result": {"stopReason": "end_turn"}}}),
    ];
    let lines: String = script.iter().map(|v| format!("{v}\n")).collect();
    std::fs::write(t.f.dir.path().join("bin/script.jsonl"), lines).unwrap();
    let lead =
        t.agent("Lead", Access::Work, vec![tell("Goose", "Do it."), wait(), text(&["done"])]).await;
    let execution = Execution {
        harness: "goose".into(),
        provider: None,
        model: None,
        connection: None,
        effort: None,
        environment: None,
    };
    let call = Call::AgentAdd {
        session: t.s.clone(),
        name: "Goose".into(),
        execution,
        access: Access::Ask,
        trust: true,
        instructions: vec![],
        apart: false,
    };
    t.c.call(call).await.unwrap();
    let run = t.prompt(&lead, "go").await;
    let (id, digest, subject, scope) = t.question().await;
    assert!(subject == "Goose: Do it." && scope.contains("outside Axiom's sandbox"), "{scope}");
    t.answer(id, Answer::Once, digest).await;
    // And whether it is started at all.
    let (id, digest, subject, _) = t.question().await;
    assert!(subject.ends_with("goose acp"), "{subject}");
    t.answer(id, Answer::Once, digest).await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    assert!(told(&lead).contains("Goose did it."), "{}", told(&lead));
    let heard = std::fs::read_to_string(t.f.dir.path().join("bin/heard.jsonl")).unwrap();
    assert!(heard.contains("another agent of this session, Lead"));
}
