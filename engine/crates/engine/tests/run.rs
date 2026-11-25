//! Whole runs: a model (a server that answers from a script), real tools on real files, a journal in
//! memory that can fail at any point.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use std::time::{Duration, Instant};

use axiom_engine::{Context, Limits, Say, Turn};
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
    assert_eq!(r[0]["tools"].as_array().unwrap().len(), 8);
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
}

#[tokio::test]
async fn a_person_can_speak_while_it_works() {
    let t = T::new(
        Access::Work,
        vec![calls(&[("c1", "list", "{}")]), text(&["first"]), text(&["second"])],
    )
    .await;
    let mut a = t.agent();
    t.inbox.try_send("also this".into()).unwrap();
    let e = t.run(&mut a, "go").await;
    assert_eq!(e.outcome, Outcome::Completed);
    assert_eq!(t.stage.texts(ItemKind::Prompt), ["go", "also this"]);
    assert!(t.server.requests()[0]["messages"].to_string().contains("also this"));
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
    let (c, new) = Context::load(&t.stage.turns()).unwrap();
    assert!(new.is_empty());
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
        let (c, new) = Context::load(&kept).unwrap();
        for turn in new {
            t.stage.keep(&turn);
        }
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
    let (c, new) = Context::load(&kept).unwrap();
    assert!(new.is_empty());
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
