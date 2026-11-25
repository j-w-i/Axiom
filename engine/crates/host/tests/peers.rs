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
            plan: false,
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
        (json!({"do": "read", "name": "D"}), "not finished anything"),
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

/// Runs of an agent that have begun.
fn runs(t: &T, a: &A) -> usize {
    let of = |b: &&Body| matches!(b, Body::RunStarted { agent, .. } if *agent == a.id);
    t.log.iter().filter(of).count()
}
/// How often the model was last told something.
fn heard(a: &A, what: &str) -> usize {
    let r = a.server.requests();
    r.last().unwrap()["messages"].to_string().matches(what).count()
}
fn slow(answer: &str) -> Vec<Reply> {
    vec![calls(&[("c", "shell", r#"{"command":"sleep 1"}"#)]), text(&[answer])]
}
impl T {
    /// The next run of an agent to end, after what has been seen.
    async fn next_end(&mut self, a: &A) -> Outcome {
        let mut mine: Option<RunId> = None;
        let id = a.id.clone();
        self.until(|b| match b {
            Body::RunStarted { run, agent } if *agent == id => {
                mine = Some(run.clone());
                None
            }
            Body::RunEnded { run, outcome, .. } if Some(run) == mine.as_ref() => Some(*outcome),
            _ => None,
        })
        .await
    }
    /// Nothing more begins by itself.
    async fn still(&mut self) {
        tokio::time::sleep(Duration::from_millis(900)).await;
        quiet(&mut self.c).await;
        while let Ok(Some(m)) = self.c.next_within(Duration::from_millis(50)).await {
            if let Message::Events { events, .. } = m {
                self.log.extend(events.into_iter().map(|e| e.body));
            }
        }
    }
    async fn restart(&mut self) {
        self.c.call(Call::Shutdown { force: true }).await.unwrap();
        while self.f.paths.token().exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let mut c = Config::new(self.f.paths.clone());
        c.home = Some(self.f.dir.path().join("home"));
        c.programs = Some(vec![self.f.dir.path().join("bin")]);
        self.f.serve_with(c);
        self.c = self.f.client().await;
        let v = self.c.call(Call::SessionList { limit: 1, offset: 0 }).await.unwrap();
        let after = v[0]["last_seq"].as_u64().unwrap();
        self.c.call(Call::Follow { session: self.s.clone(), after }).await.unwrap();
    }
}

/// PR-21: what came of it reaches who asked for it, by itself and once.
#[tokio::test]
async fn what_came_of_it_is_delivered_by_itself() {
    let mut t = T::new().await;
    let lead = t
        .agent(
            "Lead",
            Access::Work,
            vec![tell("Slow", "x"), text(&["I have told it."]), text(&["Now I know."])],
        )
        .await;
    let slow = t.agent("Slow", Access::Work, slow("the answer")).await;
    let run = t.prompt(&lead, "go").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    assert_eq!(heard(&lead, "the answer"), 0);
    assert_eq!(t.next_end(&lead).await, Outcome::Completed);
    // As another agent's word (SI-2), shown as its.
    assert_eq!((heard(&lead, "the answer"), heard(&lead, "Slow has finished")), (1, 1));
    assert_eq!(heard(&lead, "another agent of this session, Slow"), 1);
    let shown = t.log.iter().any(|b| {
        matches!(b, Body::ItemStarted { kind: ItemKind::Prompt, name, agent, .. }
            if name == "Slow" && *agent == lead.id)
    });
    assert!(shown);
    t.still().await;
    assert_eq!((runs(&t, &lead), runs(&t, &slow)), (2, 1));
    // Its person is told once, when all of it has come to an end, and of what they began only.
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let v = t.c.call(Call::Waiting {}).await.unwrap();
    let all: Vec<Waits> = serde_json::from_value(v).unwrap();
    let shown: Vec<(&str, &str)> =
        all.iter().map(|w| (w.name.as_str(), w.subject.as_str())).collect();
    assert_eq!(shown, [("Lead", "Now I know.")]);
    // Once: not with the next word either, nor by waiting, nor by another host.
    lead.server.add(wait());
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "more").await;
    t.end_of(&run).await;
    assert!(told(&lead).contains("nobody owes"), "{}", told(&lead));
    t.restart().await;
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "more").await;
    t.end_of(&run).await;
    assert_eq!(heard(&lead, "the answer"), 1);
    // What it said last is known there too.
    lead.server.add(agent(json!({"do": "read", "name": "Slow"})));
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "again").await;
    t.end_of(&run).await;
    assert!(told(&lead).contains("the answer"), "{}", told(&lead));
}

/// Who still waits for another has not come to an end for its person.
#[tokio::test]
async fn its_person_is_told_when_all_has_ended() {
    let mut t = T::new().await;
    let script = vec![tell("Slow", "x"), text(&["told"]), text(&["Now I know."])];
    let lead = t.agent("Lead", Access::Work, script).await;
    let sleep = calls(&[("c", "shell", r#"{"command":"sleep 4"}"#)]);
    t.agent("Slow", Access::Work, vec![sleep, text(&["the answer"])]).await;
    let run = t.prompt(&lead, "go").await;
    t.end_of(&run).await;
    tokio::time::sleep(Duration::from_millis(2800)).await;
    assert_eq!(t.c.call(Call::Waiting {}).await.unwrap(), json!([]));
    t.next_end(&lead).await;
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let v = t.c.call(Call::Waiting {}).await.unwrap();
    assert_eq!(v.as_array().map(Vec::len), Some(1), "{v}");
}

#[tokio::test]
async fn ends_that_come_together_are_told_together() {
    let mut t = T::new().await;
    let both = calls(&[
        ("c1", "agent", r#"{"do":"tell","name":"One","text":"a"}"#),
        ("c2", "agent", r#"{"do":"tell","name":"Two","text":"b"}"#),
    ]);
    let lead = t.agent("Lead", Access::Work, vec![both, text(&["told"]), text(&["both"])]).await;
    t.agent("One", Access::Read, vec![text(&["first answer"])]).await;
    t.agent("Two", Access::Read, vec![text(&["second answer"])]).await;
    let run = t.prompt(&lead, "go").await;
    t.end_of(&run).await;
    assert_eq!(t.next_end(&lead).await, Outcome::Completed);
    assert_eq!((heard(&lead, "first answer"), heard(&lead, "second answer")), (1, 1));
    t.still().await;
    assert_eq!(runs(&t, &lead), 2);
}

/// What went wrong, or was stopped, does not begin again by itself: it is told when it is spoken to.
#[tokio::test]
async fn it_is_held_for_one_that_failed() {
    let mut t = T::new().await;
    let lead =
        t.agent("Lead", Access::Work, vec![tell("Slow", "x"), status(401, "no such key")]).await;
    t.agent("Slow", Access::Work, slow("the answer")).await;
    let run = t.prompt(&lead, "go").await;
    assert_eq!(t.end_of(&run).await, Outcome::Failed);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    t.still().await;
    assert_eq!(runs(&t, &lead), 1);
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "go on").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    assert_eq!((heard(&lead, "the answer"), heard(&lead, "go on")), (1, 1));
    // It is shown in the order in which it is told.
    let names: Vec<&str> = t
        .log
        .iter()
        .filter_map(|b| match b {
            Body::ItemStarted { kind: ItemKind::Prompt, agent, name, .. } if *agent == lead.id => {
                Some(name.as_str())
            }
            _ => None,
        })
        .collect();
    assert_eq!(names, ["", "Slow", ""]);
    t.still().await;
    assert_eq!(runs(&t, &lead), 2);
    // Stopped by a person, it is owed nothing more.
    lead.server.add(tell("Slow", "y"));
    lead.server.add(text(&["thinking"]).hangs());
    let run = t.prompt(&lead, "once more").await;
    let id = lead.id.clone();
    t.until(|b| match b {
        Body::ItemStarted { kind: ItemKind::Prose, agent, .. } if *agent == id => Some(()),
        _ => None,
    })
    .await;
    t.c.call(Call::Cancel { session: t.s.clone(), run: run.clone() }).await.unwrap();
    assert_eq!(t.end_of(&run).await, Outcome::Cancelled);
    t.still().await;
    let n = runs(&t, &lead);
    lead.server.clear();
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "and now").await;
    t.end_of(&run).await;
    assert_eq!(heard(&lead, "has finished"), 1, "only what it was told before");
    t.still().await;
    assert_eq!(runs(&t, &lead), n + 1);
    t.restart().await;
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "and then").await;
    t.end_of(&run).await;
    assert_eq!(heard(&lead, "of this session, Slow"), 1, "nor by another host");
}

/// One that goes on without asking is told when it has ended; one that asked is not told again.
#[tokio::test]
async fn it_is_told_when_its_own_run_ends() {
    let mut t = T::new().await;
    let lead = t
        .agent(
            "Lead",
            Access::Work,
            vec![
                tell("Quick", "x"),
                calls(&[("c", "shell", r#"{"command":"sleep 2"}"#)]),
                text(&["Busy until now."]),
                text(&["Now I know."]),
            ],
        )
        .await;
    let quick = t.agent("Quick", Access::Read, vec![text(&["the answer"])]).await;
    let run = t.prompt(&lead, "go").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    assert_eq!(t.next_end(&lead).await, Outcome::Completed);
    assert_eq!((heard(&lead, "the answer"), runs(&t, &lead)), (1, 2));
    quick.server.add(text(&["another answer"]));
    for r in [tell("Quick", "y"), wait(), text(&["ok"])] {
        lead.server.add(r);
    }
    let run = t.prompt(&lead, "and").await;
    t.end_of(&run).await;
    t.restart().await;
    t.log.clear();
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "and").await;
    t.end_of(&run).await;
    assert_eq!((heard(&lead, "another answer"), runs(&t, &lead)), (1, 1));
}

/// What was on when the host ended is told as that, and nothing begins by itself after it.
#[tokio::test]
async fn what_was_cut_short_is_told_as_that() {
    let mut t = T::new().await;
    let lead = t.agent("Lead", Access::Work, vec![tell("Slow", "x"), text(&["told"])]).await;
    let slow = t.agent("Slow", Access::Read, vec![text(&["begun"]).hangs()]).await;
    let run = t.prompt(&lead, "go").await;
    t.end_of(&run).await;
    let id = slow.id.clone();
    if runs(&t, &slow) == 0 {
        t.until(|b| matches!(b, Body::RunStarted { agent, .. } if *agent == id).then_some(()))
            .await;
    }
    // Killed, as it were: what the stop would say of it is not there.
    let db = rusqlite::Connection::open(t.f.paths.journal()).unwrap();
    t.c.call(Call::Shutdown { force: true }).await.unwrap();
    while t.f.paths.token().exists() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    for _ in 0..200 {
        if db.execute("update owed set ended = null", []).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    drop(db);
    let mut c = Config::new(t.f.paths.clone());
    c.home = Some(t.f.dir.path().join("home"));
    t.f.serve_with(c);
    t.c = t.f.client().await;
    let v = t.c.call(Call::SessionList { limit: 1, offset: 0 }).await.unwrap();
    let after = v[0]["last_seq"].as_u64().unwrap();
    t.c.call(Call::Follow { session: t.s.clone(), after }).await.unwrap();
    t.log.clear();
    t.still().await;
    assert_eq!(runs(&t, &lead), 0);
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "go on").await;
    assert_eq!(t.end_of(&run).await, Outcome::Completed);
    assert_eq!(heard(&lead, "Slow was interrupted"), 1);
    assert_eq!(heard(&lead, "the host ended meanwhile"), 1);
}

/// What another said before is no answer to what it was told now.
#[tokio::test]
async fn an_old_answer_is_not_a_new_one() {
    let mut t = T::new().await;
    let lead = t
        .agent(
            "Lead",
            Access::Work,
            vec![tell("Other", "x"), wait(), tell("Other", "y"), wait(), text(&["ok"])],
        )
        .await;
    t.agent("Other", Access::Read, vec![text(&["the old answer"]), status(401, "no such key")])
        .await;
    let run = t.prompt(&lead, "go").await;
    t.end_of(&run).await;
    assert!(
        told(&lead).contains("Other failed") && !told(&lead).contains("the old answer"),
        "{}",
        told(&lead)
    );
}

/// Agents do not keep each other going for ever with nobody there.
#[tokio::test]
async fn it_does_not_go_on_for_ever() {
    let mut t = T::new().await;
    let mut script = Vec::new();
    for _ in 0..20 {
        script.push(tell("Echo", "again"));
        script.push(text(&["told"]));
    }
    let lead = t.agent("Lead", Access::Work, script).await;
    let echo = t.agent("Echo", Access::Read, (0..20).map(|_| text(&["done"])).collect()).await;
    let run = t.prompt(&lead, "go").await;
    t.end_of(&run).await;
    for _ in 0..40 {
        let n = runs(&t, &lead);
        t.still().await;
        if runs(&t, &lead) == n {
            break;
        }
    }
    assert_eq!((runs(&t, &lead), runs(&t, &echo)), (9, 9));
    // A person's word begins it again, and the last is told with it.
    lead.server.clear();
    lead.server.add(text(&["ok"]));
    let run = t.prompt(&lead, "enough").await;
    t.end_of(&run).await;
    t.still().await;
    assert_eq!(runs(&t, &lead), 10);
    // And from there on it goes by itself again.
    for r in [tell("Echo", "again"), text(&["told"]), text(&["thanks"])] {
        lead.server.add(r);
    }
    let run = t.prompt(&lead, "go on").await;
    t.end_of(&run).await;
    assert_eq!(t.next_end(&lead).await, Outcome::Completed);
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
        plan: false,
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
