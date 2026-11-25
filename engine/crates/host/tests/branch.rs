//! Branching a conversation (decision 0015), through the real socket.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_host::client::ClientError;
use axiom_proto::*;
use serde_json::{Value, json};

mod common;
#[path = "../../engine/tests/common/server.rs"]
mod server;
use server::*;
#[path = "common/apart.rs"]
mod apart;
use apart::*;

impl T {
    async fn talks(&mut self, a: &AgentId, words: &str, answer: &str) {
        self.server.add(text(&[answer]));
        self.say(a, words).await.unwrap();
        assert_eq!(self.ended().await, (Outcome::Completed, None));
    }
    async fn branch(
        &mut self,
        agent: &AgentId,
        item: &ItemId,
        files: bool,
    ) -> Result<AgentId, WireError> {
        let (session, name) = (self.s.clone(), "Other".into());
        let (agent, item) = (agent.clone(), item.clone());
        match self.c.call(Call::Branch { session, agent, item, name, files }).await {
            Ok(v) => Ok(AgentId(v["agent"].as_str().unwrap().into())),
            Err(ClientError::Refused(e)) => Err(e),
            Err(e) => panic!("{e}"),
        }
    }
    async fn look(&mut self, agent: &AgentId, item: &ItemId) -> Result<BranchInfo, WireError> {
        let (session, agent, item) = (self.s.clone(), agent.clone(), item.clone());
        match self.c.call(Call::BranchLook { session, agent, item }).await {
            Ok(v) => Ok(serde_json::from_value(v).unwrap()),
            Err(ClientError::Refused(e)) => Err(e),
            Err(e) => panic!("{e}"),
        }
    }
    /// What the model was sent last, without what every agent is told.
    fn sent(&self) -> Vec<String> {
        let last = self.server.requests().pop().unwrap();
        let all = last["messages"].as_array().unwrap().clone();
        let words = |m: &Value| match (&m["content"], &m["tool_calls"]) {
            (_, c @ Value::Array(_)) => c.to_string(),
            (Value::String(s), _) => s.clone(),
            (v, _) => v.to_string(),
        };
        all.iter()
            .filter(|m| m["role"] != "system")
            .map(|m| format!("{}: {}", m["role"].as_str().unwrap(), words(m)))
            .collect()
    }
    fn agents(&self) -> usize {
        self.history().iter().filter(|b| matches!(b, Body::AgentAdded { .. })).count()
    }
    fn places(&self) -> usize {
        let all = self.f.dir.path().join("home/.axiom-work").join(&self.s.0);
        std::fs::read_dir(all).map_or(0, Iterator::count)
    }
    fn records(&self) -> Vec<String> {
        let names = |d: &str| -> Vec<String> {
            let all = std::fs::read_dir(self.f.paths.data.join(d)).unwrap();
            all.map(|e| e.unwrap().file_name().into_string().unwrap()).collect()
        };
        let mut all = [names("copies"), names("points")].concat();
        all.sort();
        all
    }
}

#[tokio::test]
async fn a_conversation_goes_on_from_where_it_was() {
    let mut t = T::new(vec![]).await;
    let a = t.agent("First", Access::Work, false).await;
    t.talks(&a, "one", "A one").await;
    t.does(&a, "echo made > f").await;
    t.talks(&a, "three", "A three").await;
    let (said, answers) = (t.items(&a, ItemKind::Prompt), t.items(&a, ItemKind::Prose));
    let before = t.history();

    // Said differently: all before it.
    assert_eq!(t.look(&a, &said[1]).await.unwrap(), BranchInfo { apart: false, files: false });
    let b = t.branch(&a, &said[1], false).await.unwrap();
    let history = t.history();
    let added = history.iter().rev().find_map(|x| match x {
        Body::AgentAdded { agent, name, from, access, apart, execution, .. } if *agent == b => {
            Some((name.clone(), from.as_deref().cloned(), *access, *apart, execution.model.clone()))
        }
        _ => None,
    });
    let from = Branched { agent: a.clone(), item: said[1].clone(), with: false };
    let want = ("Other".into(), Some(from), Some(Access::Work), false, Some("m".into()));
    assert_eq!(added, Some(want));
    // Nothing was said to it, and nothing of the first's is another.
    assert_eq!(history[..before.len()], before);
    assert!(t.items(&b, ItemKind::Prompt).is_empty());
    t.talks(&b, "two, differently", "B two").await;
    assert_eq!(t.sent(), ["user: one", "assistant: A one", "user: two, differently"]);

    // From an answer on: all up to it, with what was done.
    let c = t.branch(&a, &answers[1], false).await.unwrap();
    t.talks(&c, "then", "C").await;
    let sent = t.sent();
    assert_eq!(sent.len(), 7, "{sent:?}");
    assert!(sent[3].contains("echo made") && sent[4].starts_with("tool: "), "{sent:?}");
    assert_eq!(sent[5..], ["assistant: done", "user: then"]);

    // The first goes on as if there were none.
    t.talks(&a, "four", "A four").await;
    let sent = t.sent();
    assert_eq!(sent[sent.len() - 3..], ["user: three", "assistant: A three", "user: four"]);

    // After the host was away; of a branch, where it was the first's, and where it is its own.
    t.restart().await;
    let d = t.branch(&b, &said[0], false).await.unwrap();
    t.talks(&d, "other", "D").await;
    assert_eq!(t.sent(), ["user: other"]);
    let own = t.items(&b, ItemKind::Prose);
    let e = t.branch(&b, &own[0], false).await.unwrap();
    t.talks(&e, "more", "E").await;
    assert_eq!(t.sent().len(), 5);
    assert_eq!(t.sent()[2..], ["user: two, differently", "assistant: B two", "user: more"]);
    // What came after it was the first's, and is none of the branch's.
    assert!(t.branch(&b, &said[1], false).await.is_err());
    assert!(t.branch(&b, &answers[2], false).await.is_err());
}

#[tokio::test]
async fn it_is_branched_only_between_runs() {
    let mut t = T::new(vec![]).await;
    let a = t.agent("First", Access::Work, false).await;
    t.server.add(calls(&[("c1", "list", r#"{"path": "."}"#)]));
    t.server.add(text(&["hangs"]).hangs());
    t.say(&a, "begins").await.unwrap();
    while t.items(&a, ItemKind::Prose).is_empty() {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Where it began, also while it is at work.
    let said = t.items(&a, ItemKind::Prompt);
    assert!(t.look(&a, &said[0]).await.is_ok());
    let queued = t.say(&a, "while it works").await.unwrap();
    assert_eq!(queued["queued"], true);
    let run = RunId(queued["run"].as_str().unwrap().into());
    t.c.call(Call::Cancel { session: t.s.clone(), run }).await.unwrap();
    assert_eq!(t.ended().await.0, Outcome::Cancelled);
    let said = t.items(&a, ItemKind::Prompt);
    let no = "cannot be branched here";
    let prose = t.items(&a, ItemKind::Prose)[0].clone();
    let tool = t.items(&a, ItemKind::Tool)[0].clone();
    let agents = t.agents();
    for item in [&said[1], &prose, &tool, &ItemId::new()] {
        let e = t.branch(&a, item, false).await.unwrap_err();
        assert!(e.message.contains(no), "{e:?}");
        assert!(t.look(&a, item).await.unwrap_err().message.contains(no));
    }
    // Of another agent, of nobody, with no name, with files where there is no copy.
    let other = t.agent("Second", Access::Work, false).await;
    assert!(t.branch(&other, &said[0], false).await.is_err());
    assert!(t.branch(&AgentId::new(), &said[0], false).await.is_err());
    assert!(t.branch(&a, &said[0], true).await.unwrap_err().message.contains("not kept"));
    let (session, agent, item) = (t.s.clone(), a.clone(), said[0].clone());
    let call = Call::Branch { session, agent, item, name: " ".into(), files: false };
    assert!(t.c.call(call).await.is_err());
    assert_eq!(t.agents(), agents + 1);
}

#[tokio::test]
async fn what_was_summarised_is_there_before_it() {
    let mut t = T::new(vec![]).await;
    let a = t.agent("First", Access::Work, false).await;
    t.talks(&a, "one", "A one").await;
    t.talks(&a, "two", "A two").await;
    t.server.add(text(&["notes"]));
    t.c.call(Call::Compact { session: t.s.clone(), agent: a.clone() }).await.unwrap();
    t.ended().await;
    t.talks(&a, "three", "A three").await;
    assert!(t.sent()[0].contains("notes"));
    let said = t.items(&a, ItemKind::Prompt);
    let b = t.branch(&a, &said[1], false).await.unwrap();
    t.talks(&b, "other", "B").await;
    assert_eq!(t.sent(), ["user: one", "assistant: A one", "user: other"]);
    // And after it, the summary.
    let c = t.branch(&a, &said[2], false).await.unwrap();
    t.talks(&c, "other", "C").await;
    assert!(t.sent()[0].contains("notes") && t.sent().len() <= 2, "{:?}", t.sent());
}

#[tokio::test]
async fn its_files_are_as_they_were() {
    let mut t = T::new(vec![]).await;
    t.write("a", "0\n");
    t.write("same", "s\n");
    t.write("gone", "g\n");
    let a = t.agent("First", Access::Work, true).await;
    t.does(&a, "echo 1 > a; rm gone; echo n > new").await;
    t.does(&a, "echo 2 > a; echo g2 > gone; rm new; echo l > later").await;
    let (said, answers) = (t.items(&a, ItemKind::Prompt), t.items(&a, ItemKind::Prose));
    assert_eq!(t.look(&a, &said[1]).await.unwrap(), BranchInfo { apart: true, files: true });

    let b = t.branch(&a, &said[1], true).await.unwrap();
    let files = |t: &T, x: &AgentId| ["a", "same", "gone", "new", "later"].map(|n| t.copied(x, n));
    let some = |s: &str| Some(format!("{s}\n"));
    assert_eq!(files(&t, &b), [some("1"), some("s"), None, some("n"), None]);
    // What it has changed is what had been done by then.
    let c = t.changes(&b).await;
    assert_eq!(paths(&c), [("a", "changed"), ("gone", "removed"), ("new", "new")]);
    assert!(c.files.iter().all(|f| !f.clash));
    // It works there and not in the first one's.
    t.does(&b, "echo b >> a").await;
    assert_eq!(t.copied(&b, "a").unwrap(), "1\nb\n");
    assert_eq!(files(&t, &a), [some("2"), some("s"), some("g2"), None, some("l")]);
    let r = t.bring(&b, &["a"]).await;
    assert_eq!(r["brought"], json!(["a"]));

    // Before all of it: nothing had been done.
    let c = t.branch(&a, &said[0], true).await.unwrap();
    assert_eq!(files(&t, &c), [some("0"), some("s"), some("g"), None, None]);
    // Where the first one left it, and it is not at work.
    let d = t.branch(&a, &answers[1], true).await.unwrap();
    assert_eq!(files(&t, &d), files(&t, &a));
    assert!(t.record().await.iter().any(|r| r.contains("as it was earlier")));
    // Without files it begins with the folder, when it is spoken to.
    let e = t.branch(&a, &said[1], false).await.unwrap();
    assert!(!t.copy_of(&e).exists());
    t.does(&e, "true").await;
    assert_eq!(files(&t, &e), [some("1\nb"), some("s"), some("g"), None, None]);
}

#[tokio::test]
async fn its_files_are_there_over_a_restart_and_in_a_branch_of_it() {
    let mut t = T::new(vec![]).await;
    t.write("a", "0\n");
    let a = t.agent("First", Access::Work, true).await;
    t.does(&a, "echo 1 > a").await;
    t.does(&a, "echo 2 > a").await;
    t.does(&a, "echo 3 > a").await;
    t.restart().await;
    let said = t.items(&a, ItemKind::Prompt);
    let b = t.branch(&a, &said[2], true).await.unwrap();
    assert_eq!(t.copied(&b, "a").unwrap(), "2\n");
    // Not spoken to yet: where it was branched is where it is.
    let answers = t.items(&a, ItemKind::Prose);
    let c = t.branch(&b, &answers[1], true).await.unwrap();
    assert_eq!(t.copied(&c, "a").unwrap(), "2\n");
    t.does(&b, "echo b > a").await;
    // What the first one's files were is known of the branch too, and then its own.
    let d = t.branch(&b, &said[1], true).await.unwrap();
    assert_eq!(t.copied(&d, "a").unwrap(), "1\n");
    let own = t.items(&b, ItemKind::Prompt);
    let e = t.branch(&b, &own[0], true).await.unwrap();
    assert_eq!(t.copied(&e, "a").unwrap(), "2\n");
    t.does(&b, "echo bb > a").await;
    let own = t.items(&b, ItemKind::Prompt);
    let f = t.branch(&b, &own[1], true).await.unwrap();
    assert_eq!(t.copied(&f, "a").unwrap(), "b\n");
}

#[tokio::test]
async fn its_files_are_those_of_its_own_place() {
    let mut t = T::new(vec![]).await;
    t.write("a", "0\n");
    let a = t.agent("First", Access::Work, true).await;
    t.does(&a, "echo 1 > b").await;
    // A run that comes to nothing, and then the copy is brought up to date: the next begins with that.
    t.server.add(status(400, "no"));
    t.say(&a, "lost").await.unwrap();
    assert_eq!(t.ended().await.0, Outcome::Failed);
    t.write("a", "new\n");
    let renew = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    t.c.call(renew).await.unwrap();
    t.does(&a, "echo 2 > b").await;
    let said = t.items(&a, ItemKind::Prompt);
    let b = t.branch(&a, &said[2], true).await.unwrap();
    assert_eq!((t.copied(&b, "a"), t.copied(&b, "b")), (Some("new\n".into()), Some("1\n".into())));
    assert_eq!(paths(&t.changes(&b).await), [("b", "new")]);
    let c = t.branch(&a, &said[1], true).await.unwrap();
    assert_eq!(t.copied(&c, "a").unwrap(), "0\n");

    // A branch that began with a new copy has its own from there on, and the other's before.
    let answers = t.items(&a, ItemKind::Prose);
    let d = t.branch(&a, &answers[1], false).await.unwrap();
    assert!(t.look(&d, &answers[1]).await.unwrap().files, "the other's, until it has any");
    t.does(&d, "echo d > b").await;
    let own = t.items(&d, ItemKind::Prompt);
    let e = t.branch(&d, &own[0], true).await.unwrap();
    assert_eq!((t.copied(&e, "a"), t.copied(&e, "b")), (Some("new\n".into()), None));
    let f = t.branch(&d, &said[2], true).await.unwrap();
    assert_eq!(t.copied(&f, "b").unwrap(), "1\n");
}

#[tokio::test]
async fn a_clash_that_is_not_settled_is_not_in_the_branch_either() {
    let mut t = T::new(vec![]).await;
    t.write("a", "0\n");
    t.write("b", "0\n");
    let a = t.agent("First", Access::Work, true).await;
    t.does(&a, "echo mine > a; echo mine > b").await;
    for f in ["a", "b"] {
        t.write(f, "theirs\n");
    }
    let renew = Call::CopyRenew { session: t.s.clone(), agent: a.clone() };
    t.c.call(renew).await.unwrap();
    t.does(&a, "echo both > b").await;
    let answers = t.items(&a, ItemKind::Prose);
    let b = t.branch(&a, &answers[1], true).await.unwrap();
    let c = t.changes(&b).await;
    assert_eq!((file(&c, "a").unsettled, file(&c, "b").unsettled), (true, false));
    // Before that there was none to settle: both have changed it, and it was neither.
    let said = t.items(&a, ItemKind::Prompt);
    let d = t.branch(&a, &said[1], true).await.unwrap();
    let c = t.changes(&d).await;
    assert!(file(&c, "a").unsettled && file(&c, "b").unsettled);
}

#[tokio::test]
async fn files_that_are_not_to_be_had_any_more_leave_nothing_behind() {
    let mut t = T::new(vec![]).await;
    t.write("a", "0\n");
    t.write("b", "00\n");
    let a = t.agent("First", Access::Work, true).await;
    t.does(&a, "true").await;
    t.does(&a, "echo 1 > b").await;
    t.does(&a, "true").await;
    let said = t.items(&a, ItemKind::Prompt);
    // Nobody has what `a` was: both have another.
    t.write("a", "folder\n");
    t.does(&a, "echo 1 > a").await;
    let (agents, places, records) = (t.agents(), t.places(), t.records());
    let e = t.branch(&a, &said[0], true).await.unwrap_err();
    assert!(e.message.contains("a is not to be had any more as it was then"), "{e:?}");
    assert_eq!((t.agents(), t.places(), t.records()), (agents, places, records));
    // Without them it can be, and with those that are kept.
    t.branch(&a, &said[0], false).await.unwrap();
    t.write("a", "0\n");
    let b = t.branch(&a, &said[1], true).await.unwrap();
    let is = (t.copied(&b, "a").unwrap(), t.copied(&b, "b").unwrap());
    assert_eq!(is, ("0\n".into(), "00\n".into()));
    // What was never kept, or went with the copy, is not offered.
    t.remove(&a, true).await.unwrap();
    assert!(!t.records().iter().any(|r| r.contains(&a.0)));
    assert!(!t.look(&a, &said[1]).await.unwrap().files);
    let e = t.branch(&a, &said[1], true).await.unwrap_err();
    assert!(e.message.contains("not kept"), "{e:?}");
}

#[tokio::test]
async fn what_is_kept_of_files_has_bounds() {
    let mut t = T::new(vec![]).await;
    t.write("a", "0\n");
    let a = t.agent("First", Access::Work, true).await;
    t.does(&a, "echo 1 > a").await;
    // More than may differ: the run goes on, and that point is not there.
    t.does(&a, "for i in $(seq 2100); do echo $i > f$i; done").await;
    t.does(&a, "rm f*").await;
    t.does(&a, "true").await;
    let said = t.items(&a, ItemKind::Prompt);
    let kept: Vec<bool> = {
        let mut all = Vec::new();
        for s in &said {
            all.push(t.look(&a, s).await.unwrap().files);
        }
        all
    };
    assert_eq!(kept, [true, true, false, true]);
    // Each content is kept once, and what no point needs goes.
    let points = t.f.paths.data.join(format!("points/{}-{}", t.s.0, a.0));
    let names = || -> Vec<String> {
        let all = std::fs::read_dir(&points).unwrap();
        all.map(|e| e.unwrap().file_name().into_string().unwrap()).collect()
    };
    let blobs = names().iter().filter(|n| !n.contains('.')).count();
    assert_eq!(blobs, 2, "{:?}", names());
    for i in 0..60 {
        t.talks(&a, "more", &format!("{i}")).await;
    }
    let v: Value =
        serde_json::from_slice(&std::fs::read(points.join("points.json")).unwrap()).unwrap();
    assert_eq!(v["all"].as_array().unwrap().len(), 50);
    assert!(!t.look(&a, &said[0]).await.unwrap().files, "the oldest go");
    assert_eq!(names().len(), 4, "{:?}", names());
}
