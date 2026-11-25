//! Whole runs: a model (a server that answers from a script), real tools on real files, a journal in
//! memory that can fail at any point.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use std::time::{Duration, Instant};

use axiom_engine::{Context, Limits, Say, Turn, Word};
use axiom_model::{Content, Role};
use axiom_policy::{Access, Answer};
use axiom_proto::{Body, ItemKind, Outcome};

#[tokio::test]
async fn an_answer_is_said_and_kept() {
    let t = T::new(Access::Work, vec![text(&["Hel", "lo."])]).await;
    let mut a = t.agent();
    let e = t.run(&mut a, "hi").await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    assert_eq!(t.stage.texts(ItemKind::Prompt), ["hi"]);
    assert_eq!(t.stage.texts(ItemKind::Prose), ["Hello."]);
    let c = a.context.messages();
    assert_eq!((c.len(), c[0].role, c[1].role), (2, Role::User, Role::Assistant));
    let r = t.server.requests();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0]["model"], "m");
    assert!(r[0]["messages"][0]["content"].as_str().unwrap().contains("sandbox"));
    assert_eq!(r[0]["tools"].as_array().unwrap().len(), 10);
    assert!(t.stage.ended().contains(&Outcome::Completed));
    // It goes on from there.
    t.server.add(text(&["Again."]));
    t.run(&mut a, "more").await;
    let r = t.server.requests();
    let (one, two) = (r[0]["messages"].as_array().unwrap(), r[1]["messages"].as_array().unwrap());
    assert_eq!(one[..], two[..one.len()], "what was sent is sent again unchanged");
    assert_eq!(two.len(), one.len() + 2);
    assert_eq!(t.stage.usage(), 2);
}

#[tokio::test]
async fn it_works_until_there_is_nothing_to_do() {
    let t = T::new(
        Access::Work,
        vec![
            calls(&[("c1", "read", r#"{"path":"a.txt"}"#), ("c2", "list", "{}")]),
            calls(&[("c3", "edit", r#"{"path":"a.txt","old":"two","new":"2"}"#)]),
            calls(&[("c4", "shell", r#"{"command":"cat a.txt"}"#)]),
            text(&["Done."]),
        ],
    )
    .await;
    let mut a = t.agent();
    let e = t.run(&mut a, "change it").await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    assert_eq!(t.file("a.txt").as_deref(), Some("one\n2\n"));
    assert_eq!(
        t.stage.tools(),
        [
            ("read".into(), Outcome::Completed),
            ("list".into(), Outcome::Completed),
            ("edit".into(), Outcome::Completed),
            ("shell".into(), Outcome::Completed)
        ]
    );
    let r = t.server.requests();
    let last = r[3]["messages"].as_array().unwrap();
    let tool: Vec<&str> = last
        .iter()
        .filter(|m| m["role"] == "tool")
        .map(|m| m["tool_call_id"].as_str().unwrap())
        .collect();
    assert_eq!(tool, ["c1", "c2", "c3", "c4"]);
    assert!(last.last().unwrap()["content"].as_str().unwrap().contains("one\n2\n"));
    // Only what changes something, or may, is in the audit record, and before it was done.
    let deeds = t.stage.deeds();
    assert_eq!(deeds.iter().map(|d| d.tool.as_str()).collect::<Vec<_>>(), ["edit", "shell"]);
    assert!(
        t.stage.before("audit edit", "end edit") && t.stage.before("start shell", "audit shell")
    );
    assert!(t.stage.before("audit shell", "sync shell"), "{:?}", t.stage.log());
}

/// In Anthropic's form: what it signed goes back as it was, and every call has its result.
#[tokio::test]
async fn the_same_in_anthropics_form() {
    let t = T::anthropic(
        Access::Work,
        vec![
            anthropic(
                Some(("I look first.", "sig-1==")),
                "Reading.",
                &[("t1", "read", r#"{"path":"a.txt"}"#), ("t2", "list", "{}")],
            ),
            anthropic(None, "", &[("t3", "edit", r#"{"path":"a.txt","old":"two","new":"2"}"#)]),
            anthropic(Some(("Done now.", "sig-2==")), "It is done.", &[]),
        ],
    )
    .await;
    let mut a = t.agent();
    a.model = "claude-test".into();
    let e = t.run(&mut a, "Make it 2.").await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    assert_eq!(t.file("a.txt").as_deref(), Some("one\n2\n"));
    assert_eq!(t.stage.texts(ItemKind::Reasoning), ["I look first.", "Done now."]);
    let r = t.server.requests();
    assert_eq!(r.len(), 3);
    assert!(r[0]["system"].to_string().contains("sandbox"));
    assert_eq!(r[0]["tools"].as_array().unwrap().len(), 10);
    let m = r[2]["messages"].as_array().unwrap();
    let roles: Vec<&str> = m.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "assistant", "user", "assistant", "user"]);
    let first = m[1]["content"].as_array().unwrap();
    assert_eq!(first[0]["type"], "thinking");
    assert_eq!(
        (first[0]["thinking"].as_str(), first[0]["signature"].as_str()),
        (Some("I look first."), Some("sig-1=="))
    );
    assert_eq!(first.iter().filter(|b| b["type"] == "tool_use").count(), 2);
    // Both results in one message, in the order of the calls.
    let results: Vec<&str> = m[2]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["tool_use_id"].as_str().unwrap())
        .collect();
    assert_eq!(results, ["t1", "t2"]);
    // What was sent is sent again unchanged, cache marks aside.
    let bare = |v: &serde_json::Value| {
        let mut v = v.clone();
        for m in v.as_array_mut().unwrap() {
            for b in m["content"].as_array_mut().into_iter().flatten() {
                b.as_object_mut().unwrap().remove("cache_control");
            }
        }
        v
    };
    let (one, two) = (bare(&r[1]["messages"]), bare(&r[2]["messages"]));
    assert_eq!(
        one.as_array().unwrap()[..],
        two.as_array().unwrap()[..one.as_array().unwrap().len()]
    );
    // A host that comes back has the same, to the byte.
    let c = Context::load(&t.stage.turns()).unwrap();
    let mut again = T::again(&t, vec![anthropic(None, "Yes.", &[])]).await;
    again.profile = t.profile.clone();
    let mut b = again.agent();
    b.model = "claude-test".into();
    b.context = c;
    again.run(&mut b, "Still there?").await;
    let three = bare(&again.server.requests()[0]["messages"]);
    assert_eq!(
        three.as_array().unwrap()[..two.as_array().unwrap().len()],
        two.as_array().unwrap()[..]
    );
    assert_eq!(t.stage.usage(), 4);
}

#[tokio::test]
async fn cancelled_in_anthropics_form_the_conversation_goes_on() {
    let t = T::anthropic(
        Access::Work,
        vec![
            anthropic(
                Some(("Hm.", "s")),
                "Running.",
                &[("t1", "shell", r#"{"command":"sleep 30"}"#), ("t2", "list", "{}")],
            ),
            anthropic(None, "Going on.", &[]),
        ],
    )
    .await;
    let mut a = t.agent();
    a.model = "claude-test".into();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let person = async {
        t.stage.until(|s| s.before("audit shell", "sync shell")).await;
        tx.send(true).unwrap();
    };
    let (e, ()) = tokio::join!(t.run_with(&mut a, "go", rx), person);
    assert_eq!(e.outcome, Outcome::Cancelled);
    assert_eq!(t.run(&mut a, "go on").await.outcome, Outcome::Completed);
    let m = t.server.requests()[1]["messages"].as_array().unwrap().clone();
    let roles: Vec<&str> = m.iter().map(|m| m["role"].as_str().unwrap()).collect();
    // The results come first in what follows the calls; what the person said may be beside them.
    assert_eq!(&roles[..3], ["user", "assistant", "user"]);
    let after = m[2]["content"].as_array().unwrap();
    assert_eq!(after[0]["tool_use_id"], "t1");
    assert_eq!(after[1]["tool_use_id"], "t2");
    assert!(m.last().unwrap().to_string().contains("go on"));
}

#[tokio::test]
async fn a_person_is_asked_and_their_answer_counts() {
    let edit = r#"{"path":"n.txt","content":"x"}"#;
    let script = || vec![calls(&[("c1", "write", edit)]), text(&["ok"])];
    for (answer, wrote) in [(Answer::Once, true), (Answer::Deny, false)] {
        let t = T::new(Access::Ask, script()).await;
        let mut a = t.agent();
        let run = t.run(&mut a, "go");
        let person = async {
            let (id, digest, subject) = t.stage.request().await;
            assert!(subject.contains("+x"), "{subject}");
            assert!(!t.root.join("w/n.txt").exists(), "not before the answer");
            assert!(t.desk.answer(&id, Answer::Once, "other").is_err());
            t.desk.answer(&id, answer, &digest).unwrap();
        };
        let (e, ()) = tokio::join!(run, person);
        assert_eq!(e.outcome, Outcome::Completed);
        assert_eq!(t.root.join("w/n.txt").exists(), wrote);
        let told =
            t.server.requests()[1]["messages"].as_array().unwrap().last().unwrap()["content"]
                .as_str()
                .unwrap()
                .to_owned();
        assert_eq!(told.contains("a person said no"), !wrote, "{told}");
        assert_eq!(t.stage.deeds().len(), usize::from(wrote));
        if wrote {
            assert!(t.stage.deeds()[0].answered.is_some());
        }
    }
    // For the session: the second time it is not asked.
    let same = r#"{"command":"echo $(true)"}"#;
    let t = T::new(
        Access::Ask,
        vec![calls(&[("c1", "shell", same)]), calls(&[("c2", "shell", same)]), text(&["ok"])],
    )
    .await;
    let mut a = t.agent();
    let person = async {
        let (id, digest, _) = t.stage.request().await;
        t.desk.answer(&id, Answer::Session, &digest).unwrap();
    };
    let (e, ()) = tokio::join!(t.run(&mut a, "go"), person);
    assert_eq!(e.outcome, Outcome::Completed);
    assert_eq!(t.stage.requests(), 1);
    assert_eq!(t.stage.deeds().len(), 2);
}

#[tokio::test]
async fn what_cannot_be_run_is_said_back() {
    let t = T::new(
        Access::Work,
        vec![
            calls(&[
                ("c1", "nothing", "{}"),
                ("c2", "read", r#"{"path":1}"#),
                ("c3", "read", "{\"path\":"),
                ("c4", "read", r#"{"path":"../../etc/passwd"}"#),
            ]),
            text(&["ok"]),
        ],
    )
    .await;
    let mut a = t.agent();
    assert_eq!(t.run(&mut a, "go").await.outcome, Outcome::Completed);
    let Content::ToolResult { text, failed, .. } = &a.context.messages()[2].blocks[0].content
    else {
        panic!()
    };
    assert!(*failed && text.contains("no tool nothing"), "{text}");
    assert!(t.stage.tools().iter().all(|t| t.1 == Outcome::Failed));
    assert!(t.stage.deeds().is_empty());
}

/// A call that cannot be read is not sent back to any provider: the model is told in words, and not
/// for ever.
#[tokio::test]
async fn calls_that_cannot_be_read_are_told_and_not_for_ever() {
    let broken = || calls(&[("c1", "read", "{\"path\":")]);
    let t = T::new(Access::Work, vec![broken(), text(&["I see."])]).await;
    let e = t.run(&mut t.agent(), "go").await;
    assert_eq!(e.outcome, Outcome::Completed);
    let r = t.server.requests();
    let last = r[1]["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["role"], "user");
    assert!(last["content"].as_str().unwrap().contains("Your call of read"), "{last}");
    assert_ne!(r[0]["messages"], r[1]["messages"]);
    let t = T::new(Access::Work, (0..9).map(|_| broken()).collect()).await;
    let e = t.run(&mut t.agent(), "go").await;
    assert!(e.outcome == Outcome::Failed && e.error.unwrap().contains("cannot be read"));
    assert_eq!(t.server.requests().len(), 3);
    // One of them can: its result comes first, and the count begins again.
    let mixed = || calls(&[("c1", "read", "{\"path\":"), ("c2", "list", "{}")]);
    let t = T::new(Access::Work, vec![mixed(), mixed(), mixed(), mixed(), text(&["ok"])]).await;
    assert_eq!(t.run(&mut t.agent(), "go").await.outcome, Outcome::Completed);
    let m = t.server.requests()[1]["messages"].as_array().unwrap().clone();
    let tail: Vec<&str> = m[m.len() - 2..].iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(tail, ["tool", "user"]);
    // Two with one name: one of them is done, the other told.
    let twice = calls(&[("c1", "list", "{}"), ("c1", "read", r#"{"path":"a.txt"}"#)]);
    let t = T::new(Access::Work, vec![twice, text(&["ok"])]).await;
    assert_eq!(t.run(&mut t.agent(), "go").await.outcome, Outcome::Completed);
    let m = t.server.requests()[1]["messages"].as_array().unwrap().clone();
    assert_eq!(m.iter().filter(|m| m["role"] == "tool").count(), 1);
}

#[tokio::test]
async fn an_answer_that_reached_its_length_goes_on() {
    let t = T::new(Access::Work, vec![ends_with("length", &["half"]), text(&["rest"])]).await;
    let e = t.run(&mut t.agent(), "go").await;
    assert_eq!(e.outcome, Outcome::Completed);
    let m = t.server.requests()[1]["messages"].as_array().unwrap().clone();
    assert_eq!(m.last().unwrap()["content"], "Go on.");
    assert_eq!(t.stage.texts(ItemKind::Notice).len(), 1);
    // Inside a call: nothing of it is run.
    let mut c = calls(&[("c1", "shell", r#"{"command":"touch made"}"#)]);
    c.stop("length");
    let t = T::new(Access::Work, vec![c, text(&["ok"])]).await;
    assert_eq!(t.run(&mut t.agent(), "go").await.outcome, Outcome::Completed);
    assert_eq!(t.file("made"), None);
    assert!(t.server.requests()[1]["messages"].to_string().contains("Your call of shell"));
    for (stop, words) in [("content_filter", "declined"), ("weird", "weird")] {
        let t = T::new(Access::Work, vec![ends_with(stop, &["x"])]).await;
        let e = t.run(&mut t.agent(), "go").await;
        assert!(e.outcome == Outcome::Failed && e.error.unwrap().contains(words), "{stop}");
    }
}

/// SI-23: what is about to be done is on the drive before it is done.
#[tokio::test]
async fn nothing_is_done_before_its_intent_is_kept() {
    let t = T::new(
        Access::Work,
        vec![calls(&[("c1", "write", r#"{"path":"n.txt","content":"x"}"#)]), text(&["ok"])],
    )
    .await;
    let (stage, file) = (t.stage.clone(), t.root.join("w/n.txt"));
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let s = seen.clone();
    stage.on_sync(move |log| s.lock().unwrap().push((log.join(","), file.exists())));
    t.run(&mut t.agent(), "go").await;
    let seen = seen.lock().unwrap();
    let synced = seen.iter().any(|(log, made)| log.contains("audit write") && !made);
    assert!(synced, "a sync between the audit entry and the effect: {seen:?}");
}

#[tokio::test]
async fn what_a_person_takes_is_not_the_runs_time() {
    let t = T::new(
        Access::Ask,
        vec![
            calls(&[("c1", "list", "{}")]),
            calls(&[("c2", "shell", r#"{"command":"touch made"}"#)]),
            text(&["ok"]),
        ],
    )
    .await;
    let mut a = t.agent();
    a.limits = Limits { time: Duration::from_millis(1500), ..Limits::default() };
    let person = async {
        let (id, digest, _) = t.stage.request().await;
        tokio::time::sleep(Duration::from_millis(2000)).await;
        t.desk.answer(&id, Answer::Once, &digest).unwrap();
    };
    let (e, ()) = tokio::join!(t.run(&mut a, "go"), person);
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    a.limits = Limits { time: Duration::ZERO, ..Limits::default() };
    assert!(t.run(&mut a, "go").await.error.unwrap().contains("as long as it may"));
}

/// The session is closed under a run that waits: the run ends, and its question with it.
#[tokio::test]
async fn a_session_that_closes_ends_what_waits_in_it() {
    let t =
        T::new(Access::Ask, vec![calls(&[("c1", "shell", r#"{"command":"touch made"}"#)])]).await;
    let mut a = t.agent();
    let person = async {
        let q = t.stage.request().await;
        t.stage.shut();
        q
    };
    let (e, (id, digest, _)) = tokio::join!(t.run(&mut a, "go"), person);
    assert_eq!(e.outcome, Outcome::Interrupted);
    assert!(t.desk.answer(&id, Answer::Once, &digest).is_err(), "nobody waits for it");
    assert_eq!(t.file("made"), None);
    // And under one that waits for the provider, or for a command.
    for script in
        [vec![text(&["x"]).hangs()], vec![calls(&[("c1", "shell", r#"{"command":"sleep 30"}"#)])]]
    {
        let t = T::new(Access::Work, script).await;
        let mut a = t.agent();
        let close = async {
            t.stage
                .until(|s| {
                    s.before("audit shell", "sync shell") || !s.texts(ItemKind::Prose).is_empty()
                })
                .await;
            t.stage.shut();
        };
        let now = Instant::now();
        let (e, ()) = tokio::join!(t.run(&mut a, "go"), close);
        assert_eq!(e.outcome, Outcome::Interrupted);
        assert!(now.elapsed() < Duration::from_secs(15));
    }
}

#[tokio::test]
async fn a_run_that_cannot_begin_has_ended() {
    let t = T::new(Access::Work, vec![]).await;
    let mut a = t.agent();
    t.stage.refuses_turns();
    let r = a
        .begin(&axiom_proto::RunId::new(), vec![], Some(axiom_engine::Word::new("go".into())))
        .await;
    assert!(r.is_err());
    assert_eq!(t.stage.ended(), [Outcome::Failed]);
    assert!(a.context.is_empty());
}

#[tokio::test]
async fn a_failed_request_is_tried_again_if_that_may_help() {
    let t = T::new(
        Access::Work,
        vec![
            status(529, "{}"),
            status(429, "{}").header("retry-after", "0"),
            cut("half an ans"),
            text(&["Whole."]),
        ],
    )
    .await;
    let mut a = t.agent();
    let e = t.run(&mut a, "hi").await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    assert_eq!(t.server.requests().len(), 4);
    assert_eq!(t.stage.texts(ItemKind::Notice).len(), 3);
    assert_eq!(t.stage.texts(ItemKind::Prose), ["half an ans", "Whole."]);
    assert_eq!(t.stage.outcomes(ItemKind::Prose), [Outcome::Failed, Outcome::Completed]);
    assert_eq!(a.context.messages().len(), 2, "the broken answer is not part of it");
    for (code, words) in [(401, "wrong key"), (400, "bad")] {
        let t = T::new(
            Access::Work,
            vec![status(code, &format!(r#"{{"error":{{"message":"{words}"}}}}"#))],
        )
        .await;
        let mut a = t.agent();
        let e = t.run(&mut a, "hi").await;
        assert_eq!(e.outcome, Outcome::Failed);
        assert!(e.error.unwrap().contains(words));
        assert_eq!(t.server.requests().len(), 1);
    }
    // Asked to be left alone for longer than anybody waits.
    let t = T::new(Access::Work, vec![status(429, "{}").header("retry-after", "86400")]).await;
    let now = Instant::now();
    let e = t.run(&mut t.agent(), "hi").await;
    assert!(e.error.unwrap().contains("86400") && now.elapsed() < Duration::from_secs(20));
    // And not for ever.
    let t = T::new(Access::Work, (0..9).map(|_| status(529, "{}")).collect()).await;
    let e = t.run(&mut t.agent(), "hi").await;
    assert_eq!(e.outcome, Outcome::Failed);
    assert_eq!(t.server.requests().len(), 4, "once and three times again");
}

#[tokio::test]
async fn calls_of_an_answer_that_broke_off_are_never_run() {
    let mut c = calls(&[("c1", "shell", r#"{"command":"touch log"}"#)]);
    c.cut = true;
    let t = T::new(Access::Work, vec![c, text(&["ok"])]).await;
    let mut a = t.agent();
    assert_eq!(t.run(&mut a, "go").await.outcome, Outcome::Completed);
    assert_eq!(t.file("log"), None);
    assert!(t.stage.tools().is_empty());
}

#[tokio::test]
async fn stopping_stops_the_answer() {
    let t = T::new(Access::Work, vec![text(&["I was say"]).hangs(), text(&["ok"])]).await;
    let mut a = t.agent();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let person = async {
        t.stage.until(|s| s.texts(ItemKind::Prose).first().is_some_and(|x| !x.is_empty())).await;
        let now = Instant::now();
        tx.send(true).unwrap();
        t.server.closed().await;
        now.elapsed()
    };
    let (e, took) = tokio::join!(t.run_with(&mut a, "go", rx), person);
    assert_eq!(e.outcome, Outcome::Cancelled);
    assert!(took < Duration::from_millis(1500), "{took:?}");
    assert_eq!(t.stage.outcomes(ItemKind::Prose), [Outcome::Cancelled]);
    // What it had said it knows.
    let m = a.context.messages();
    assert!(matches!(&m[1].blocks[0].content, Content::Text { text } if text == "I was say"));
    assert_eq!(t.run(&mut a, "go on").await.outcome, Outcome::Completed);
    assert_eq!(t.server.requests()[1]["messages"].as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn stopping_stops_the_command() {
    let m = format!("6{:05}", std::process::id() % 100_000);
    let t = T::new(
        Access::Work,
        vec![
            calls(&[
                ("c1", "shell", &format!(r#"{{"command":"touch up && sleep {m} && touch late"}}"#)),
                ("c2", "shell", r#"{"command":"touch never"}"#),
            ]),
            text(&["ok"]),
        ],
    )
    .await;
    let mut a = t.agent();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let person = async {
        t.stage.until(|_| t.file("up").is_some()).await;
        tx.send(true).unwrap();
        Instant::now()
    };
    let (e, sent) = tokio::join!(t.run_with(&mut a, "go", rx), person);
    assert_eq!(e.outcome, Outcome::Cancelled);
    assert!(sent.elapsed() < Duration::from_secs(3));
    let o = std::process::Command::new("ps").args(["-axo", "args="]).output().unwrap();
    assert!(
        !String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with(&format!("sleep {m}")))
    );
    assert_eq!((t.file("late"), t.file("never")), (None, None));
    assert_eq!(t.stage.tools(), [("shell".into(), Outcome::Cancelled)]);
    // Both calls have results, so the conversation can go on.
    assert_eq!(t.run(&mut a, "go on").await.outcome, Outcome::Completed);
    let r = t.server.requests()[1]["messages"].as_array().unwrap().clone();
    assert_eq!(r.iter().filter(|m| m["role"] == "tool").count(), 2);
}

#[tokio::test]
async fn stopping_withdraws_the_question() {
    let t = T::new(Access::Ask, vec![calls(&[("c1", "shell", r#"{"command":"ls"}"#)])]).await;
    let mut a = t.agent();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let person = async {
        let (id, digest, _) = t.stage.request().await;
        tx.send(true).unwrap();
        (id, digest)
    };
    let (e, (id, digest)) = tokio::join!(t.run_with(&mut a, "go", rx), person);
    assert_eq!(e.outcome, Outcome::Cancelled);
    assert!(t.desk.answer(&id, Answer::Once, &digest).is_err());
    assert_eq!(t.stage.answers(), ["expired"]);
    assert!(t.stage.deeds().is_empty());
    // Nothing of it was begun, and that is what the model is told.
    let told = format!("{:?}", a.context.messages());
    assert!(told.contains("while a person was being asked") && !told.contains("in part"), "{told}");
}

#[tokio::test]
async fn a_person_can_speak_while_it_works() {
    let t = T::new(
        Access::Work,
        vec![calls(&[("c1", "list", "{}")]), text(&["first"]), text(&["second"])],
    )
    .await;
    let mut a = t.agent();
    t.inbox.try_send(Word { text: "also this".into(), item: None, shown: None }).unwrap();
    t.inbox
        .try_send(Word {
            text: "and that".into(),
            item: Some(axiom_proto::ItemId("i_9".into())),
            shown: None,
        })
        .unwrap();
    let e = t.run(&mut a, "go").await;
    assert_eq!(e.outcome, Outcome::Completed);
    assert_eq!(t.stage.texts(ItemKind::Prompt), ["go", "also this"]);
    let sent = t.server.requests()[0]["messages"].to_string();
    assert!(sent.contains("also this") && sent.contains("and that"), "{sent}");
}

/// Where a run began with a person's words and where it ended with an answer, and nowhere else.
#[tokio::test]
async fn it_can_be_branched_between_runs() {
    let t = T::new(
        Access::Work,
        vec![calls(&[("c", "list", "{}")]), text(&["one"]), text(&["two"]), text(&["three"])],
    )
    .await;
    let mut a = t.agent();
    let inbox = t.inbox.clone();
    let mut sent = false;
    t.stage.on_turn(move || {
        if !std::mem::replace(&mut sent, true) {
            inbox.try_send(word("meanwhile", "i_1")).unwrap();
        }
    });
    t.run(&mut a, "go").await;
    let turns = t.stage.turns().len() as u64;
    let prompts = t.stage.ids(ItemKind::Prompt);
    let prose = t.stage.ids(ItemKind::Prose);
    let (begins, ends) = ((prompts[0].clone(), 0, false), (prose[0].clone(), turns, true));
    assert_eq!(t.stage.cuts(), [begins.clone(), ends.clone()]);
    // Another agent's word begins no branch of a person's.
    let word = Word { shown: Some(("B".into(), "x".into())), ..Word::new("from B".into()) };
    let id = axiom_proto::RunId::new();
    a.begin(&id, vec![], Some(word)).await.unwrap();
    let (_tx, rx) = tokio::sync::watch::channel(false);
    a.run(id, rx).await;
    let prose = t.stage.ids(ItemKind::Prose);
    let second = (prose[1].clone(), turns + 2, true);
    assert_eq!(t.stage.cuts(), [begins, ends, second]);
}

/// What has not come to its end has none.
#[tokio::test]
async fn an_answer_that_is_not_one_is_no_place() {
    for (script, places) in [
        (vec![text(&["half"]).hangs()], 1),
        (vec![status(400, "no")], 1),
        // Words, and then an answer without any.
        (vec![calls(&[("c", "list", "{}")]), ends_with("stop", &[])], 1),
        (vec![text(&["all"])], 2),
    ] {
        let t = T::new(Access::Work, script).await;
        let mut a = t.agent();
        let (tx, rx) = tokio::sync::watch::channel(false);
        let stage = t.stage.clone();
        let stop = tokio::spawn(async move {
            stage.until(|s| !s.ids(ItemKind::Prose).is_empty()).await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            let _ = tx.send(true);
        });
        t.run_with(&mut a, "go", rx).await;
        stop.abort();
        assert_eq!(t.stage.cuts().len(), places, "{:?}", t.stage.log());
    }
}

/// What a person says and takes back.
fn word(text: &str, item: &str) -> Word {
    Word { text: text.into(), item: Some(axiom_proto::ItemId(item.into())), shown: None }
}

#[tokio::test]
async fn what_was_taken_back_is_told_to_nobody() {
    let t = T::new(Access::Work, vec![text(&["one"])]).await;
    let mut a = t.agent();
    t.stage.take_back(axiom_proto::ItemId("i_1".into()));
    t.inbox.try_send(word("not this", "i_1")).unwrap();
    t.inbox.try_send(word("but this", "i_2")).unwrap();
    assert_eq!(t.run(&mut a, "go").await.outcome, Outcome::Completed);
    let r = t.server.requests();
    let sent = r[0]["messages"].to_string();
    assert!(!sent.contains("not this") && sent.contains("but this"), "{sent}");
    let kept = t.stage.turns().join("");
    assert!(!kept.contains("not this") && kept.contains("but this"), "{kept}");
}

/// Said during the last answer, and taken back before it was over: there is nothing to answer.
#[tokio::test]
async fn a_run_goes_on_for_what_still_waits_only() {
    for (taken, requests) in [(vec!["i_1", "i_2"], 1), (vec!["i_1"], 2), (vec![], 2)] {
        let t = T::new(Access::Work, vec![text(&["one"]), text(&["two"])]).await;
        let mut a = t.agent();
        for i in &taken {
            t.stage.take_back(axiom_proto::ItemId((*i).into()));
        }
        let (inbox, server) = (t.inbox.clone(), t.server.clone());
        let mut sent = false;
        t.stage.on_turn(move || {
            if !sent && !server.requests().is_empty() {
                sent = true;
                inbox.try_send(word("first", "i_1")).unwrap();
                inbox.try_send(word("second", "i_2")).unwrap();
            }
        });
        assert_eq!(t.run(&mut a, "go").await.outcome, Outcome::Completed);
        let r = t.server.requests();
        assert_eq!(r.len(), requests, "{taken:?}");
        let last = r.last().unwrap()["messages"].to_string();
        assert_eq!(last.contains("first"), taken.is_empty(), "{last}");
        assert_eq!(last.contains("second"), taken.len() < 2, "{last}");
    }
}

/// Taken in from what waited it is not taken in again from the inbox; taken back by then, it asks
/// for no answer either.
#[tokio::test]
async fn what_waited_from_before_is_heard_once_or_not_at_all() {
    for taken in [false, true] {
        let t = T::new(Access::Work, vec![text(&["one"]), text(&["two"])]).await;
        let mut a = t.agent();
        let i = axiom_proto::ItemId("i_1".into());
        if taken {
            t.stage.take_back(i.clone());
        }
        a.note(vec![(i, "from before".into())]).await.unwrap();
        let (inbox, server) = (t.inbox.clone(), t.server.clone());
        let mut sent = false;
        t.stage.on_turn(move || {
            if !sent && !server.requests().is_empty() {
                sent = true;
                inbox.try_send(word("from before", "i_1")).unwrap();
            }
        });
        assert_eq!(t.run(&mut a, "go").await.outcome, Outcome::Completed);
        let kept = t.stage.turns().join("");
        assert_eq!(kept.matches("from before").count(), usize::from(!taken), "{kept}");
        assert_eq!(t.server.requests().len(), 1);
    }
}

#[tokio::test]
async fn what_goes_on_in_the_background_is_said_and_ends_when_stopped() {
    let m = format!("8{:05}", std::process::id() % 100_000);
    let start = format!(r#"{{"do":"start","command":"sleep {m}"}}"#);
    let t = T::new(
        Access::Work,
        vec![calls(&[("c1", "job", &start)]), text(&["ok"]), text(&["x"]).hangs()],
    )
    .await;
    let mut a = t.agent();
    t.run(&mut a, "go").await;
    assert!(t.stage.texts(ItemKind::Notice)[0].contains(&format!("1: sleep {m}")));
    assert_eq!(t.stage.deeds().len(), 1);
    let runs = || {
        let o = std::process::Command::new("ps").args(["-axo", "args="]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with(&format!("sleep {m}")))
    };
    let now = Instant::now();
    while !runs() && now.elapsed() < Duration::from_secs(10) {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let (tx, rx) = tokio::sync::watch::channel(false);
    let person = async {
        t.stage.until(|s| s.texts(ItemKind::Prose).len() > 1).await;
        tx.send(true).unwrap();
    };
    tokio::join!(t.run_with(&mut a, "go on", rx), person);
    let now = Instant::now();
    while runs() {
        assert!(now.elapsed() < Duration::from_secs(10));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn another_model_that_answers_is_named() {
    for (asked, told) in [("m", 0), ("M-2026-01-01", 0), ("lab/m", 0), ("other", 1)] {
        let t = T::new(Access::Work, vec![calls(&[("c1", "list", "{}")]), text(&["ok"])]).await;
        let mut a = t.agent();
        a.model = asked.into();
        t.run(&mut a, "go").await;
        let n = t.stage.texts(ItemKind::Notice);
        assert_eq!(n.len(), told, "{asked}: {n:?}");
    }
}

#[tokio::test]
async fn a_run_has_an_end() {
    let t = T::new(Access::Work, (0..9).map(|_| calls(&[("c", "list", "{}")])).collect()).await;
    let mut a = t.agent();
    a.limits = Limits { steps: 4, ..Limits::default() };
    let e = t.run(&mut a, "go").await;
    assert!(e.outcome == Outcome::Failed && e.error.unwrap().contains("4 steps"));
    assert_eq!(t.server.requests().len(), 4);
    let t = T::new(Access::Work, (0..9).map(|_| calls(&[("c", "list", "{}")])).collect()).await;
    let mut a = t.agent();
    a.limits = Limits { output_tokens: Some(12), ..Limits::default() };
    let e = t.run(&mut a, "go").await;
    assert!(e.error.unwrap().contains("tokens"));
    assert_eq!(t.server.requests().len(), 3, "five tokens each");
}

#[tokio::test]
async fn an_agent_has_only_its_tools() {
    let t = T::new(
        Access::Work,
        vec![calls(&[("c1", "shell", r#"{"command":"touch made"}"#)]), text(&["ok"])],
    )
    .await;
    let mut a = t.agent();
    a.site.caps.tools = axiom_policy::Tools::Only(["read".to_string()].into());
    t.run(&mut a, "go").await;
    assert_eq!(t.server.requests()[0]["tools"].as_array().unwrap().len(), 1);
    assert_eq!((t.file("made"), t.stage.deeds().len()), (None, 0));
}

#[tokio::test]
async fn a_full_window_is_made_room_in() {
    // Each answer of the script says 55 tokens were used, of 60.
    let t = T::new(
        Access::Work,
        vec![
            calls(&[("c1", "read", r#"{"path":"a.txt"}"#)]),
            text(&["Notes: the file has two lines."]),
            calls(&[("c2", "edit", r#"{"path":"a.txt","old":"two","new":"2"}"#)]),
            text(&["Notes: it must be read first."]),
            text(&["Done."]),
        ],
    )
    .await;
    let mut a = t.agent();
    a.window = Some(60);
    let e = t.run(&mut a, "Make it 2.").await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    let r = t.server.requests();
    assert_eq!(r.len(), 5);
    // The summary is asked for aside: without tools, and with everything told.
    assert!(r[1].get("tools").is_none_or(|t| t.as_array().is_none_or(Vec::is_empty)));
    let told = r[1]["messages"].to_string();
    assert!(told.contains("Make it 2.") && told.contains("Agent calls read"), "{told}");
    // The agent goes on from its notes and what the person said.
    let m = r[2]["messages"].as_array().unwrap();
    assert_eq!(m.len(), 2, "the system's words and the notes");
    let notes = m[1]["content"].as_str().unwrap();
    assert!(
        notes.contains("Notes: the file has two lines.") && notes.ends_with("Make it 2."),
        "{notes}"
    );
    // What it knew of the file went with the conversation.
    assert!(r[3]["messages"].to_string().contains("read a.txt before"));
    assert_eq!(t.stage.texts(ItemKind::Prose), ["Done."], "the notes are not shown as its words");
    assert!(t.stage.texts(ItemKind::Notice)[0].contains("summarised"));
    // Kept so: the next host goes on from the same.
    let c = Context::load(&t.stage.turns()).unwrap();
    assert_eq!(c.messages(), a.context.messages());
}

#[tokio::test]
async fn room_is_made_because_a_person_asks() {
    let t = T::new(Access::Work, vec![text(&["Hello."]), text(&["Kept: hello."]), text(&["On."])])
        .await;
    let mut a = t.agent();
    // Before anything was said there is nothing to summarise, and nobody is asked to.
    a.compact = true;
    let e = t.run_unsaid(&mut a).await;
    assert_eq!((e.outcome, t.server.requests().len()), (Outcome::Failed, 0), "{e:?}");
    assert_eq!(t.run(&mut a, "Say hello.").await.outcome, Outcome::Completed);
    a.compact = true;
    let e = t.run_unsaid(&mut a).await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    assert_eq!(t.server.requests().len(), 2, "it is summarised, and nothing is answered");
    let said = t.stage.texts(ItemKind::Notice).pop().unwrap();
    assert!(said.contains("a person asked") && said.ends_with("Kept: hello."), "{said}");
    assert!(format!("{:?}", a.context.messages()[0]).contains("Kept: hello."));
    // Once: the next run answers, from what was kept.
    assert_eq!(t.run(&mut a, "Go on.").await.outcome, Outcome::Completed);
    let sent = t.server.requests()[2]["messages"].to_string();
    assert!(sent.contains("Kept: hello.") && sent.contains("Go on."), "{sent}");
    let c = Context::load(&t.stage.turns()).unwrap();
    assert_eq!(c.messages(), a.context.messages());
}

#[tokio::test]
async fn what_is_too_long_is_summarised_once_and_then_fails() {
    let long = || {
        status(
            400,
            r#"{"error":{"message":"maximum context length exceeded","code":"context_length_exceeded"}}"#,
        )
    };
    let t = T::new(
        Access::Work,
        vec![text(&["one"]), long(), long(), text(&["Notes."]), text(&["two"])],
    )
    .await;
    let mut a = t.agent();
    a.window = None;
    t.run(&mut a, "first").await;
    let e = t.run(&mut a, "second").await;
    assert_eq!((e.outcome, e.error), (Outcome::Completed, None));
    let r = t.server.requests();
    assert_eq!(r.len(), 5, "the summary was itself too long once, and asked for with less");
    assert!(r[3]["messages"].to_string().len() <= r[2]["messages"].to_string().len());
    let t = T::new(Access::Work, vec![text(&["one"]), long(), text(&["Notes."]), long()]).await;
    let mut a = t.agent();
    t.run(&mut a, "first").await;
    let e = t.run(&mut a, "second").await;
    assert!(e.outcome == Outcome::Failed && e.error.unwrap().contains("context"));
    let t = T::new(Access::Work, vec![long()]).await;
    let e = t.run(&mut t.agent(), "a single thing that is too long").await;
    assert_eq!(e.outcome, Outcome::Failed);
    assert_eq!(t.server.requests().len(), 1, "there is nothing to leave out");
}

/// The host is killed at every point there is. Whatever was done is on record, and nothing is done
/// twice when the agent goes on.
#[tokio::test]
async fn killed_at_any_point_nothing_is_lost_and_nothing_repeated() {
    let script = || {
        vec![
            calls(&[
                ("c1", "shell", r#"{"command":"echo one | tee -a log"}"#),
                ("c2", "shell", r#"{"command":"echo two | tee -a log"}"#),
            ]),
            calls(&[("c3", "write", r#"{"path":"made.txt","content":"x\n"}"#)]),
            text(&["Done."]),
        ]
    };
    let whole = T::new(Access::Work, script()).await;
    whole.run(&mut whole.agent(), "go").await;
    let points = whole.stage.ops();
    assert!(points > 25, "{points}");
    let mut seen = std::collections::BTreeSet::new();
    for k in 0..points {
        let t = T::new(Access::Work, script()).await;
        t.stage.dies_at(k);
        let mut a = t.agent();
        let e = t.run(&mut a, "go").await;
        drop(a);
        let log = t.file("log").unwrap_or_default();
        seen.insert(log.clone());
        let done = log.lines().count() + usize::from(t.file("made.txt").is_some());
        let deeds = t.stage.deeds().len();
        // Intent before effect: nothing is done that is not on record; at most the last was not done.
        assert!(done <= deeds && deeds <= done + 1, "at {k}: {done} done, {deeds} on record");
        if k + 4 < points {
            assert_ne!(e.outcome, Outcome::Completed, "at {k}");
        }
        // The next host: what was kept is read, and the agent goes on.
        t.stage.lives();
        let kept = t.stage.turns();
        let c = Context::load(&kept).unwrap();
        let next = T::again(&t, vec![text(&["I see."])]).await;
        let mut a = next.agent();
        a.context = c;
        let had: usize = a.context.messages().len();
        let e = next.run(&mut a, "go on").await;
        assert_eq!((e.outcome, e.error), (Outcome::Completed, None), "at {k}, {had} messages");
        assert_eq!(next.file("log").unwrap_or_default(), log, "at {k}: nothing is done again");
        // Every call has exactly one result, in the model's eyes.
        let m = next.server.requests()[0]["messages"].as_array().unwrap().clone();
        let asked: Vec<String> = m
            .iter()
            .flat_map(|m| m["tool_calls"].as_array().cloned().unwrap_or_default())
            .map(|c| c["id"].as_str().unwrap().into())
            .collect();
        let answered: Vec<String> = m
            .iter()
            .filter(|m| m["role"] == "tool")
            .map(|m| m["tool_call_id"].as_str().unwrap().into())
            .collect();
        assert_eq!(asked, answered, "at {k}");
        // What was done and finished is told as done.
        let told = m
            .iter()
            .filter(|m| m["role"] == "tool")
            .map(|m| m["content"].as_str().unwrap().contains("Interrupted"))
            .filter(|x| *x)
            .count();
        assert!(told <= 2, "at {k}");
    }
    assert!(seen.contains("") && seen.contains("one\n") && seen.contains("one\ntwo\n"), "{seen:?}");
}

#[tokio::test]
async fn turns_are_kept_as_they_were() {
    let t = T::new(Access::Work, vec![calls(&[("c1", "list", "{}")]), text(&["ok"])]).await;
    let mut a = t.agent();
    t.run(&mut a, "go").await;
    let kept = t.stage.turns();
    assert_eq!(kept.len() as u64, a.context.turns());
    let c = Context::load(&kept).unwrap();
    assert_eq!(c.messages(), a.context.messages());
    for k in &kept {
        let t: Turn = serde_json::from_str(k).unwrap();
        assert_eq!(&serde_json::to_string(&t).unwrap(), k);
    }
    assert!(t.stage.all().iter().any(|s| matches!(s, Say::Event(Body::RunEnded { .. }))));
}

/// By hand: an agent on a model of this machine changes a file.
#[tokio::test]
#[ignore = "needs LM Studio on this machine"]
async fn live_on_lm_studio() {
    let model = std::env::var("AXIOM_LIVE_MODEL").unwrap_or("qwen/qwen3-4b-2507".into());
    let t = T::live().await;
    let mut a = t.agent();
    a.model = model;
    a.settings.max_output = Some(800);
    let e = t.run(&mut a, "In a.txt, replace the line `two` with `2`. Then run `cat a.txt` to check, and say what it shows.").await;
    println!(
        "{e:?}\n{:#?}\n{:?}\n{:?}",
        t.stage.tools(),
        t.stage.texts(ItemKind::Prose),
        t.file("a.txt")
    );
    assert_eq!(e.outcome, Outcome::Completed);
    assert_eq!(t.file("a.txt").as_deref(), Some("one\n2\n"));
}
