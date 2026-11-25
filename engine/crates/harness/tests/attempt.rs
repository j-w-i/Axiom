//! What goes wrong, and what is stopped, with an agent in a process of its own.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use axiom_harness::{
    Acp, Answer, By, Error, Event, Harness, Level, MOST_LINE, MOST_PROMPT, MOST_SUBJECT, Refused,
    Settled, Took, Trust, Way, Why, frames,
};
use axiom_policy::{Access, Place};
use axiom_proto::Outcome;
use serde_json::{Value, json};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

// Questions.

#[tokio::test]
async fn a_question_is_refused() {
    let no = json!({"hear": {"id": 5, "result": {"outcome": {"outcome": "selected", "optionId": "no"}}}});
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": ask(5, "rm -rf /")}),
        no.clone(),
        json!({"say": ask(6, "rm -rf /")}),
        json!({"hear": {"id": 6, "result": {"outcome": {"optionId": "no"}}}}),
        end_turn("end_turn"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    let h = &r.handle;
    assert_eq!(h.answer("other", Answer::Refuse).await, Err(Refused::NotAsked));
    let odd = Answer::Choose("maybe".into());
    assert_eq!(h.answer(&q, odd).await, Err(Refused::NoSuchOption));
    h.answer(&q, Answer::Refuse).await.unwrap();
    assert_eq!(h.answer(&q, Answer::Refuse).await, Err(Refused::NotAsked));
    let q = r.asked().await;
    r.choose(&q, "no").await;
    r.done().await;
    r.handle.shut().await.unwrap();
    assert_eq!(r.closed().await.why, Why::Asked);
    assert_eq!(r.count(|e| matches!(e, Event::AskClosed { how: Settled::Answered, .. })), 2);
}

#[tokio::test]
async fn nobody_answers_in_time() {
    let mut w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": ask(5, "x")}),
        json!({"hear": {"id": 5, "result": {"outcome": {"optionId": "no"}}}}),
        end_turn("end_turn"),
    ]));
    w.limits.ask = Some(ms(300));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    let t = Instant::now();
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    assert!(t.elapsed() > ms(200));
    assert!(r.all.contains(&Event::AskClosed { id: q.clone(), how: Settled::TimedOut }));
    assert_eq!(r.handle.answer(&q, Answer::Choose("yes".into())).await, Err(Refused::NotAsked));
    r.handle.shut().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn an_answer_in_time_stops_the_clock() {
    let mut w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": ask(5, "x")}),
        json!({"hear": {"id": 5, "result": {"outcome": {"optionId": "yes"}}}}),
        json!({"sleep": 900}),
        end_turn("end_turn"),
    ]));
    w.limits.ask = Some(ms(500));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    r.choose(&q, "yes").await;
    r.done().await;
    assert_eq!(r.count(|e| matches!(e, Event::AskClosed { how: Settled::Answered, .. })), 1);
    assert_eq!(w.heard().len(), 4, "no second answer");
    r.handle.cancel().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn who_may_only_read_is_never_allowed_more() {
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": ask(5, "x")}),
        json!({"hear": {"id": 5, "result": {"outcome": {"optionId": "no"}}}}),
        end_turn("end_turn"),
    ]));
    // The script's agent has no modes: that it is given one is only for this.
    let script = w.read("bin/script.jsonl").replace(
        "\"sessionId\":\"s1\"}}}",
        "\"sessionId\":\"s1\",\"modes\":{\"currentModeId\":\"x\",\"availableModes\":[{\"id\":\"plan\",\"name\":\"Plan\"}]}}}}\n{\"hear\":{\"id\":2,\"method\":\"session/set_mode\"}}\n{\"say\":{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}}",
    );
    let script = script.replace(
        "\"id\":2,\"jsonrpc\":\"2.0\",\"result\":{\"stopReason\"",
        "\"id\":3,\"jsonrpc\":\"2.0\",\"result\":{\"stopReason\"",
    );
    std::fs::write(w.dir.path().join("bin/script.jsonl"), script).unwrap();
    let a = Acp { args: vec!["acp".into()], read: Some("plan".into()), ..Acp::default() };
    let mut r = w.run(&Harness::Acp(a), Access::Read);
    r.ready().await;
    r.prompt("go").await;
    r.done().await;
    assert_eq!(r.count(|e| matches!(e, Event::Asked(_) | Event::AskClosed { .. })), 0);
    assert!(r.all.contains(&Event::Notice {
        level: Level::Warning,
        text: "Not allowed, it may only read: Run".into()
    }));
    r.handle.cancel().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn what_cannot_be_shown_whole_is_not_asked() {
    let big = "x".repeat(MOST_SUBJECT + 1);
    // All of it in the question; or in the tool call that it names.
    let mut by_name = ask(6, "");
    by_name["params"]["toolCall"] = json!({"toolCallId": "t"});
    let mut long = ask(8, "");
    long["params"]["toolCall"] = json!({"toolCallId": "u", "locations": [{"path": big}]});
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": ask(5, &big)}),
        json!({"hear": {"id": 5, "result": {"outcome": {"optionId": "no"}}}}),
        json!({"say": say(json!({"sessionUpdate": "tool_call", "toolCallId": "t", "title": "T", "rawInput": {"a": big}}))}),
        json!({"say": by_name}),
        json!({"hear": {"id": 6, "result": {"outcome": {"optionId": "no"}}}}),
        json!({"say": long}),
        json!({"hear": {"id": 8, "result": {"outcome": {"optionId": "no"}}}}),
        json!({"say": ask(7, "small")}),
        json!({"hear": {"id": 7}}),
        end_turn("end_turn"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    r.choose(&q, "yes").await;
    r.done().await;
    let why = "Not allowed, it is about more than can be shown: ";
    let n = r.count(|e| matches!(e, Event::Notice { text, .. } if text.starts_with(why)));
    assert_eq!(n, 3);
    r.handle.shut().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn a_hundred_questions_at_once() {
    let mut all = vec![hear_prompt()];
    all.extend((0..100).map(|n| json!({"say": ask(100 + n, "x")})));
    all.push(json!({"until": {"method": "session/cancel"}}));
    all.push(json!({"say": ask(300, "one more")}));
    all.push(json!({"until": {"id": 300, "result": {"outcome": {"optionId": "no"}}}}));
    all.push(end_turn("cancelled"));
    let w = world(&acp_1(&all));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    for _ in 64..100 {
        r.until(|e| matches!(e, Event::Notice { .. })).await;
    }
    assert_eq!(r.count(|e| matches!(e, Event::Asked(_))), 64);
    r.handle.interrupt().await.unwrap();
    assert_eq!(r.done().await.outcome, Outcome::Cancelled);
    // What is asked while it is being stopped is not put to anyone.
    assert_eq!(r.count(|e| matches!(e, Event::Asked(_))), 64);
    let text = "Not allowed, it is being stopped: Run".to_owned();
    assert!(r.all.contains(&Event::Notice { level: Level::Warning, text }));
    assert_eq!(r.count(|e| matches!(e, Event::AskClosed { how: Settled::Withdrawn, .. })), 64);
    r.handle.cancel().await.unwrap();
    r.closed().await;
}

// Being stopped.

#[tokio::test]
async fn a_turn_is_cancelled_in_the_middle_of_a_tool() {
    let m = mark(1);
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"child": m}),
        json!({"say": say(json!({"sessionUpdate": "tool_call", "toolCallId": "c", "title": "T", "status": "in_progress"}))}),
        json!({"say": ask(5, "x")}),
        json!({"hear": {"method": "session/cancel", "params": {"sessionId": "s1"}}}),
        json!({"hear": {"id": 5, "result": {"outcome": {"outcome": "cancelled"}}}}),
        json!({"say": say(json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "failed"}))}),
        end_turn("cancelled"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.asked().await;
    within("its child runs", || alive(&m) == 1).await;
    r.handle.cancel().await.unwrap();
    let d = r.done().await;
    assert_eq!((d.outcome, d.stop.as_str()), (Outcome::Cancelled, "cancelled"));
    let g = r.closed().await;
    assert_eq!((g.why, g.words), (Why::Asked, None));
    assert_eq!(alive(&m), 0, "what it had started is ended with it");
    assert_eq!(r.count(|e| matches!(e, Event::ToolEnded(t) if t.outcome == Outcome::Failed)), 1);
    assert_eq!(r.handle.prompt("more", By::Person).await, Err(Refused::Closed));
    assert_eq!(r.handle.cancel().await, Err(Refused::Closed));
}

#[tokio::test]
async fn one_that_does_not_stop_is_ended() {
    let m = mark(2);
    let mut w = world(&acp_1(&[
        hear_prompt(),
        json!({"child": m}),
        json!({"say": say(json!({"sessionUpdate": "tool_call", "toolCallId": "c", "title": "T"}))}),
        json!({"hang": true}),
    ]));
    w.limits.cancel = ms(300);
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.until(|e| matches!(e, Event::ToolStarted(_))).await;
    within("its child runs", || alive(&m) == 1).await;
    let t = Instant::now();
    // To be asked for twice changes nothing.
    r.handle.interrupt().await.unwrap();
    r.handle.interrupt().await.unwrap();
    assert_eq!(r.handle.prompt("more", By::Person).await, Err(Refused::Busy));
    let d = r.done().await;
    assert_eq!(d.outcome, Outcome::Cancelled);
    assert!(t.elapsed() > ms(250));
    let g = r.closed().await;
    assert_eq!(g.why, Why::Asked);
    assert_eq!(g.words.as_deref(), Some("it did not stop when it was asked to"));
    assert_eq!(alive(&m), 0);
    assert_eq!(r.count(|e| matches!(e, Event::ToolEnded(t) if t.outcome == Outcome::Cancelled)), 1);
}

#[tokio::test]
async fn an_interrupted_turn_leaves_the_session() {
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"hear": {"method": "session/cancel"}}),
        end_turn("cancelled"),
        hear_prompt(),
        json!({"say": {"jsonrpc": "2.0", "id": 3, "result": {"stopReason": "end_turn"}}}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    // With nothing to stop, nothing is.
    r.handle.interrupt().await.unwrap();
    r.prompt("go").await;
    r.handle.interrupt().await.unwrap();
    assert_eq!(r.done().await.outcome, Outcome::Cancelled);
    assert_eq!(r.prompt("again").await, Took::Began);
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    r.handle.cancel().await.unwrap();
    assert_eq!(r.closed().await.words, None);
}

#[tokio::test]
async fn an_interruption_becomes_a_cancellation() {
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"hear": {"method": "session/cancel"}}),
        json!({"sleep": 300}),
        end_turn("cancelled"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.handle.interrupt().await.unwrap();
    r.handle.cancel().await.unwrap();
    assert_eq!(r.done().await.outcome, Outcome::Cancelled);
    assert_eq!(r.closed().await.why, Why::Asked);
}

#[tokio::test]
async fn stopped_before_it_began() {
    let w = world(&lines(&[json!({"hang": true})]));
    let mut r = w.run(&acp(), Access::Ask);
    assert_eq!(r.handle.prompt("go", By::Person).await, Err(Refused::NotReady));
    r.handle.interrupt().await.unwrap();
    let g = r.closed().await;
    assert_eq!((g.why, g.words), (Why::Asked, None));
    assert_eq!(r.all.len(), 1);
}

#[tokio::test]
async fn what_takes_too_long_to_go_is_ended() {
    let m = mark(3);
    let mut w = world(&acp_1(&[json!({"child": m}), json!({"hang": true})]));
    w.limits.stop = ms(300);
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    within("its child runs", || alive(&m) == 1).await;
    r.handle.shut().await.unwrap();
    let g = r.closed().await;
    assert_eq!(g.why, Why::Asked);
    assert!(g.ended.unwrap().signal.is_some());
    assert_eq!(alive(&m), 0);
}

#[tokio::test]
async fn one_that_goes_by_itself_is_let_go() {
    let w = world(&acp_1(&[json!({"until": {"never": true}})]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.handle.shut().await.unwrap();
    // It ended because its input did, with a code of its own.
    assert_eq!(r.closed().await.ended.unwrap().code, Some(96));
}

#[tokio::test]
async fn nobody_listens_any_more() {
    let m = mark(4);
    let w = world(&acp_1(&[hear_prompt(), json!({"child": m}), json!({"hang": true})]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    within("its child runs", || alive(&m) == 1).await;
    let Run { events, handle, .. } = r;
    drop(events);
    within("all of it ends", || alive(&m) == 0).await;
    within("nothing more is taken", || handle.is_over()).await;
}

#[tokio::test]
async fn nobody_to_tell_it_anything_is_no_reason_to_end() {
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"sleep": 200}),
        end_turn("end_turn"),
        json!({"exit": 0}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let Run { mut events, handle, .. } = r;
    drop(handle);
    let mut done = false;
    while let Some(e) = events.next().await {
        match e {
            Event::Done(d) => done = d.outcome == Outcome::Completed,
            Event::Closed(g) => assert_eq!(g.words.unwrap(), "it ended by itself (exit 0)"),
            _ => {}
        }
    }
    assert!(done);
}

// What dies, hangs, floods, or speaks nonsense.

#[tokio::test]
async fn it_dies_in_the_middle() {
    for (how, words) in [
        (json!({"exit": 3}), "it ended by itself (exit 3)"),
        (json!({"abort": true}), "it ended by itself (signal 6)"),
    ] {
        let w = world(&acp_1(&[
            hear_prompt(),
            json!({"err": "a library warns", "times": 3}),
            json!({"say": message("half a wo")}),
            json!({"say": say(json!({"sessionUpdate": "tool_call", "toolCallId": "c", "title": "T"}))}),
            json!({"say": ask(5, "x")}),
            json!({"raw": "{\"jsonrpc\": \"2.0\", \"meth"}),
            json!({"err": "fatal: out of luck"}),
            how,
        ]));
        let mut r = w.run(&acp(), Access::Ask);
        r.ready().await;
        r.prompt("go").await;
        let d = r.done().await;
        assert_eq!((d.outcome, d.error.as_deref()), (Outcome::Failed, Some(words)));
        let g = r.closed().await;
        assert_eq!((g.why, g.words.as_deref()), (Why::Died, Some(words)));
        assert!(g.stderr.ends_with("fatal: out of luck\n"), "{:?}", g.stderr);
        // Everything before is told, and the last that was said is there to look at.
        assert_eq!(r.texts()[0].1, "half a wo");
        assert!(g.frames.last().unwrap().text.contains("session/request_permission"));
        assert!(g.frames.iter().any(|f| f.way == Way::Out && f.text.contains("session/prompt")));
        assert!(g.frames.len() <= 12);
    }
}

#[tokio::test]
async fn it_dies_before_it_says_anything() {
    let w = world(&lines(&[json!({"err": "no node here"}), json!({"exit": 127})]));
    let mut r = w.run(&acp(), Access::Ask);
    let g = r.closed().await;
    assert_eq!(g.words.as_deref(), Some("it ended by itself (exit 127)"));
    assert_eq!((g.why, g.stderr.as_str()), (Why::Died, "no node here\n"));
    assert_eq!(r.all.len(), 1);
}

#[tokio::test]
async fn it_dies_and_what_it_started_holds_its_output() {
    let m = mark(5);
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"child": m}),
        json!({"say": message("last")}),
        json!({"exit": 0}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!(g.words.as_deref(), Some("it ended by itself (exit 0)"));
    assert_eq!(g.ended.unwrap().left, 1);
    assert_eq!(alive(&m), 0);
    assert_eq!(r.texts()[0].1, "last");
}

#[tokio::test]
async fn it_says_nothing() {
    let mut w = world(&lines(&[json!({"hang": true})]));
    w.limits.opening = ms(300);
    let mut r = w.run(&acp(), Access::Ask);
    let g = r.closed().await;
    assert_eq!((g.why, g.words.as_deref()), (Why::Silent, Some("it did not begin in time")));
    let mut w = world(&acp_1(&[hear_prompt(), json!({"hang": true})]));
    w.limits.silence = Some(ms(400));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    // While it has nothing to do, it need not say anything.
    tokio::time::sleep(ms(700)).await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!((g.why, g.words.as_deref()), (Why::Silent, Some("it said nothing for too long")));
    assert_eq!(r.count(|e| matches!(e, Event::Done(d) if d.outcome == Outcome::Failed)), 1);
}

#[tokio::test]
async fn silence_is_counted_from_when_it_could_speak() {
    let mut w = world(&acp_1(&[hear_prompt(), json!({"sleep": 300}), end_turn("end_turn")]));
    w.limits.silence = Some(ms(1000));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    tokio::time::sleep(ms(1300)).await;
    r.prompt("go").await;
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    r.handle.cancel().await.unwrap();
    r.closed().await;
    // Two of these do not fit: the second waits for who listens.
    let big = message(&"x".repeat(3 * 1024 * 1024));
    let mut w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": big, "times": 2}),
        json!({"sleep": 2000}),
        end_turn("end_turn"),
    ]));
    w.limits.silence = Some(ms(1000));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    tokio::time::sleep(ms(1500)).await;
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    r.handle.cancel().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn waiting_for_a_person_is_not_silence() {
    let mut w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": ask(5, "x")}),
        json!({"hear": {"id": 5}}),
        json!({"say": message("a")}),
        json!({"sleep": 250}),
        json!({"say": message("b")}),
        json!({"sleep": 250}),
        end_turn("end_turn"),
    ]));
    w.limits.silence = Some(ms(450));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    tokio::time::sleep(ms(900)).await;
    r.choose(&q, "yes").await;
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    r.handle.cancel().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn a_few_lines_of_something_else() {
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"raw": "Welcome to Agent 3!\n\n   \n\t\n"}),
        json!({"raw": "[1, 2]\n\"a\"\nnull\n{}\n{\"jsonrpc\": \"2.0\"}\n"}),
        json!({"bytes": [255, 254, 0, 10]}),
        json!({"raw": "\r\n"}),
        json!({"say": message("a")}),
        end_turn("end_turn"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    let raw: Vec<_> = r
        .all
        .iter()
        .filter_map(|e| match e {
            Event::Unknown { what, raw, .. } => Some((what.as_str(), raw.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(raw.len(), 7, "{raw:?}");
    assert_eq!(raw[0], ("no message", "Welcome to Agent 3!"));
    r.handle.shut().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn only_nonsense() {
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"raw": "garbage\n", "times": 1000}),
        json!({"hang": true}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!(
        (g.why, g.words.as_deref()),
        (Why::Nonsense, Some("it writes what is not its protocol"))
    );
    assert_eq!(r.count(|e| matches!(e, Event::Unknown { .. })), 16);
    assert!(g.frames.iter().all(|f| f.text == "garbage"));
}

#[tokio::test]
async fn a_line_without_an_end() {
    let w =
        world(&acp_1(&[hear_prompt(), json!({"long": MOST_LINE + 4096}), json!({"hang": true})]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!(g.why, Why::Nonsense);
    assert!(g.words.unwrap().contains("exceeds"));
    // One that fits is read, however long.
    let long = "y".repeat(2 * 1024 * 1024);
    let w = world(&acp_1(&[hear_prompt(), json!({"say": message(&long)}), end_turn("end_turn")]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.done().await;
    assert_eq!(r.texts()[0].1.len(), long.len());
    r.handle.shut().await.unwrap();
    r.closed().await;
    let frames = r.kept.frames.lock().unwrap();
    let f = frames.iter().max_by_key(|f| f.bytes).unwrap();
    assert!(f.bytes > 2 * 1024 * 1024 && f.text.len() == frames::MOST);
}

#[tokio::test]
async fn lines_in_pieces() {
    let m = message("héllo").to_string();
    let (a, b) = m.as_bytes().split_at(m.find('é').unwrap() + 1);
    let two = format!("{}\n{}", message("1"), message("2"));
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"bytes": a}),
        json!({"sleep": 50}),
        json!({"bytes": b}),
        json!({"sleep": 50}),
        json!({"raw": "\r\n"}),
        json!({"raw": two}),
        json!({"sleep": 50}),
        json!({"raw": "\n"}),
        end_turn("end_turn"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.done().await;
    assert_eq!(r.texts()[0].1, "héllo12");
    r.handle.shut().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn a_flood_waits_for_who_listens() {
    let n = 15_000;
    let words = "0123456789".repeat(20);
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": message(&words), "times": n}),
        json!({"touch": "all said"}),
        end_turn("end_turn"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    // Nobody takes anything: it is read only as far as there is room.
    tokio::time::sleep(ms(700)).await;
    let read = r.kept.frames.lock().unwrap().len();
    tokio::time::sleep(ms(500)).await;
    assert_eq!(r.kept.frames.lock().unwrap().len(), read, "it waits");
    assert!(read < 3000, "{read}");
    assert!(!w.dir.path().join("bin/all said").exists(), "nor is it taken in unread");
    let mut got = 0;
    loop {
        match r.events.next().await.unwrap() {
            Event::Say(s) => {
                assert_eq!(s.text, words);
                got += 1;
            }
            Event::Done(d) => {
                assert_eq!(d.outcome, Outcome::Completed);
                break;
            }
            _ => {}
        }
    }
    assert_eq!(got, n, "and nothing is lost");
}

#[tokio::test]
async fn a_flood_of_large_things_is_held_by_their_size() {
    let words = "x".repeat(1024 * 1024);
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": message(&words), "times": 40}),
        end_turn("end_turn"),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    tokio::time::sleep(ms(1500)).await;
    let read = r.kept.frames.lock().unwrap().iter().filter(|f| f.bytes > 1024 * 1024).count();
    assert!((3..=6).contains(&read), "{read}");
    r.done().await;
    assert_eq!(r.count(|e| matches!(e, Event::Say(_))), 40);
    r.handle.cancel().await.unwrap();
    r.closed().await;
}

#[tokio::test]
async fn it_can_be_stopped_though_nobody_listens() {
    let m = mark(6);
    let mut w = world(&acp_1(&[
        hear_prompt(),
        json!({"child": m}),
        json!({"say": message("0123456789"), "times": 200_000}),
        json!({"hang": true}),
    ]));
    w.limits.cancel = ms(300);
    w.limits.silence = Some(ms(200));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    within("its child runs", || alive(&m) == 1).await;
    // That it is not read is no silence of its own.
    tokio::time::sleep(ms(900)).await;
    assert_eq!(alive(&m), 1);
    r.handle.cancel().await.unwrap();
    within("all of it ends", || alive(&m) == 0).await;
    let g = r.closed().await;
    assert_eq!(g.why, Why::Asked);
}

#[tokio::test]
async fn too_many_tool_calls_at_once() {
    let call = say(json!({"sessionUpdate": "tool_call", "toolCallId": "$n", "title": "T"}));
    let w =
        world(&acp_1(&[hear_prompt(), json!({"say": call, "times": 600}), json!({"hang": true})]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!(g.why, Why::Flood);
    assert_eq!(r.count(|e| matches!(e, Event::ToolStarted(_))), 512);
    assert_eq!(r.count(|e| matches!(e, Event::ToolEnded(_))), 512);
}

#[tokio::test]
async fn it_does_not_read_what_it_is_told() {
    // Each of its questions is answered, until there is no room for answers.
    let odd = json!({"jsonrpc": "2.0", "id": "$n", "method": "fs/read_text_file", "params": {"path": "x".repeat(200)}});
    let w = world(&acp_1(&[
        hear_prompt(),
        json!({"say": odd, "times": 100_000}),
        json!({"hang": true}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!((g.why, g.words.as_deref()), (Why::Deaf, Some("it does not read what it is told")));
    // Or one of them is never read.
    let mut w = world(&acp_1(&[json!({"hang": true})]));
    w.limits.write = ms(400);
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt(&"p".repeat(MOST_PROMPT)).await;
    let g = r.closed().await;
    assert_eq!((g.why, g.words.as_deref()), (Why::Deaf, Some("it does not read what it is told")));
}

#[tokio::test]
async fn what_it_is_not_told() {
    let w = world(&acp_1(&[]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    let h = &r.handle;
    assert_eq!(h.prompt("", By::Person).await, Err(Refused::Empty));
    assert_eq!(h.prompt(" \n", By::Person).await, Err(Refused::Empty));
    let long = "x".repeat(MOST_PROMPT + 1);
    assert_eq!(h.prompt(&long, By::Person).await, Err(Refused::TooLong(MOST_PROMPT)));
    r.handle.shut().await.unwrap();
    r.closed().await;
    assert_eq!(w.heard().len(), 2, "its two first words");
}

#[tokio::test]
async fn it_closes_its_output_and_lives() {
    let m = mark(7);
    // A shell is better at this than the script: it gives its output away.
    let w = world("");
    let agent = w.dir.path().join("bin/agent");
    std::fs::remove_file(&agent).unwrap();
    std::fs::write(&agent, format!("#!/bin/sh\nexec 1>&-\nsleep {m}\n")).unwrap();
    std::fs::set_permissions(&agent, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let mut r = w.run(&acp(), Access::Ask);
    let g = r.closed().await;
    assert_eq!((g.why, g.words.as_deref()), (Why::Died, Some("it closed its output")));
    assert_eq!(alive(&m), 0);
}

#[tokio::test]
async fn noise_beside() {
    let w = world(&acp_1(&[
        json!({"err": "e".repeat(200), "times": 20_000}),
        hear_prompt(),
        json!({"say": message("a")}),
        json!({"err": "the last line"}),
        end_turn("end_turn"),
        json!({"exit": 0}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.done().await;
    let g = r.closed().await;
    assert!(g.stderr.len() <= 8 * 1024 && g.stderr.ends_with("the last line\n"));
    assert_eq!(r.count(|e| matches!(e, Event::Unknown { .. })), 0);
}

// The record.

#[tokio::test]
async fn everything_said_is_on_record_before_it_is_told() {
    let w = world(&acp_1(&[hear_prompt(), json!({"say": message("a")}), end_turn("end_turn")]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    r.until(|e| matches!(e, Event::Say(_))).await;
    {
        let f = r.kept.frames.lock().unwrap();
        assert!(f.iter().any(|f| f.way == Way::In && f.text.contains("agent_message_chunk")));
        let out: Vec<Value> = f
            .iter()
            .filter(|f| f.way == Way::Out)
            .map(|f| serde_json::from_str(&f.text).unwrap())
            .collect();
        assert_eq!(out, w.heard());
    }
    r.done().await;
    r.handle.shut().await.unwrap();
    let g = r.closed().await;
    let all = r.kept.frames.lock().unwrap();
    assert_eq!(g.frames.len(), all.len().min(12));
    assert_eq!(g.frames.last().unwrap(), all.last().unwrap());
    assert!(all.iter().all(|f| f.at > 1_600_000_000_000 && f.bytes == f.text.len() as u64));
}

#[tokio::test]
async fn without_a_record_it_ends() {
    for stuck in [false, true] {
        let mut w = world(&acp_1(&[hear_prompt(), json!({"hang": true})]));
        w.limits.write = ms(300);
        let mut r = w.run(&acp(), Access::Ask);
        r.ready().await;
        if stuck { &r.kept.stuck } else { &r.kept.broken }.store(true, Ordering::SeqCst);
        // It is not told anything that is not on record.
        assert!(r.handle.prompt("go", By::Person).await.is_ok());
        let g = r.closed().await;
        assert_eq!(g.why, Why::Journal);
        assert_eq!(w.heard().len(), 2);
    }
    // Its last word cannot be kept: that is why it ends.
    let w = world(&lines(&[
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 2, "capabilities": {"session": {}}}}}),
        json!({"hear": {"method": "session/new"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "s1"}}}),
    ]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.kept.broken.store(true, Ordering::SeqCst);
    r.handle.shut().await.unwrap();
    assert_eq!(r.closed().await.why, Why::Journal);
    let w = world(&acp_1(&[]));
    let kept = Kept::default();
    kept.broken.store(true, Ordering::SeqCst);
    let mut r = w.try_run(&acp(), w.start(Access::Ask), kept).unwrap();
    assert_eq!(r.closed().await.why, Why::Journal);
    assert!(w.heard().is_empty());
}

// How it is started.

#[tokio::test]
async fn what_it_is_started_with() {
    let w = world(&acp_1(&[]));
    let mut r = w.run(&acp(), Access::Ask);
    r.ready().await;
    r.handle.shut().await.unwrap();
    r.closed().await;
    let seen = w.seen();
    let env = seen["env"].as_object().unwrap();
    let mut names: Vec<&str> = env.keys().map(String::as_str).collect();
    // What the system's own libraries put there on a Mac.
    names.retain(|n| *n != "__CF_USER_TEXT_ENCODING");
    names.sort_unstable();
    assert_eq!(names, ["HOME", "LANG", "PATH", "TMPDIR", "USER"]);
    assert_eq!(env["HOME"], w.at("home").as_str());
    assert_eq!(env["PATH"], "/usr/bin:/bin");
    assert_eq!(seen["cwd"], w.at("w").as_str());
    assert_eq!(seen["args"], json!(["acp"]));
}

#[test]
fn nothing_that_lets_go_is_ever_passed() {
    let w = world("");
    let mut all = vec![];
    for h in [acp(), Harness::Codex, Harness::Claude(axiom_harness::Prompts::Refused), claude()]
        .into_iter()
        .chain(axiom_harness::KNOWN.iter().map(|k| Harness::Acp(k.acp())))
    {
        for a in [Access::Read, Access::Ask, Access::Work] {
            for trust in [Trust::Given, Trust::Withheld] {
                for more in [None, Some("x")] {
                    let mut s = w.start(a);
                    s.trust = trust;
                    (s.model, s.effort) = (more.map(String::from), more.map(String::from));
                    s.resume = more.map(String::from);
                    if let Ok(l) = h.launch(&s) {
                        all.push(l.args);
                    }
                }
            }
        }
    }
    assert!(all.len() > 40, "{}", all.len());
    for a in all.iter().flatten() {
        let low = a.to_lowercase();
        for word in [
            "yolo",
            "dangerous",
            "bypass",
            "danger-full-access",
            "dangerfullaccess",
            "skip-permission",
            "never",
            "--allow",
            "--force",
            "--trust",
            "auto-approve",
            "\"enabled\":false",
            "allowunsandboxedcommands\":true",
        ] {
            assert!(!low.contains(word), "{a}");
        }
    }
    // Nor by a name that a person, or anything else, gives.
    for bad in ["--yolo", "-c", "a b", "x\ny", ""] {
        for at in 0..3 {
            let mut s = w.start(Access::Work);
            *[&mut s.model, &mut s.effort, &mut s.resume][at] = Some(bad.into());
            for h in [Harness::Codex, claude()] {
                assert!(matches!(h.launch(&s), Err(Error::BadValue(..))), "{bad:?}");
            }
        }
    }
    let mut s = w.start(Access::Work);
    s.model = Some("bypassPermissions".into());
    assert!(matches!(claude().launch(&s), Err(Error::Unsafe(_))));
}

#[tokio::test]
async fn what_is_not_allowed_is_not_started() {
    let w = world(&acp_1(&[]));
    let start = |f: &dyn Fn(&mut axiom_harness::Start), h: &Harness| {
        let mut s = w.start(Access::Ask);
        f(&mut s);
        w.try_run(h, s, Kept::default()).err()
    };
    assert!(matches!(start(&|s| s.access = Access::Work, &acp()), Some(Error::NoMode(_))));
    assert!(matches!(start(&|s| s.trust = Trust::Withheld, &acp()), Some(Error::Untrusted)));
    assert!(matches!(start(&|s| s.resume = Some("--x".into()), &acp()), Some(Error::BadValue(..))));
    let person = Harness::Acp(Acp { person_only: true, ..Acp::default() });
    assert!(matches!(start(&|s| s.by = By::Agent, &person), Some(Error::PersonOnly)));
    assert!(person.person_only() && !acp().person_only());
    // The workspace may be one that the program is in, or what it finds others by.
    assert!(
        matches!(start(&|s| s.workspace = w.at("bin"), &acp()), Some(Error::Inside(p)) if p == w.at("bin/agent"))
    );
    let mut site = w.site();
    site.path.push(w.at("w"));
    let e = acp().start(&site, w.start(Access::Ask), token(), Kept::default()).err();
    assert!(matches!(e, Some(Error::Inside(p)) if p == w.at("w")));
    let mut site = w.site();
    site.written = axiom_policy::Reach::of([Place::new(w.at("bin").as_path()).unwrap()]);
    let e = acp().start(&site, w.start(Access::Ask), token(), Kept::default()).err();
    assert!(matches!(e, Some(Error::Inside(_))));
    let mut site = w.site();
    site.user = String::new();
    let e = acp().start(&site, w.start(Access::Ask), token(), Kept::default()).err();
    assert!(matches!(e, Some(Error::Spawn(_))));
    let gone = start(&|s| s.workspace = w.at("not there"), &acp());
    assert!(matches!(gone, Some(Error::Spawn(_))));
    assert!(w.read("bin/seen.json").is_empty(), "it was never started");
}

#[tokio::test]
async fn words_that_are_no_persons() {
    let w = world(&acp_1(&[hear_prompt(), end_turn("end_turn")]));
    let h = Harness::Acp(Acp { args: vec!["acp".into()], person_only: true, ..Acp::default() });
    let mut r = w.run(&h, Access::Ask);
    r.ready().await;
    assert_eq!(r.handle.prompt("go", By::Agent).await, Err(Refused::PersonOnly));
    assert_eq!(r.handle.prompt("go", By::Person).await, Ok(Took::Began));
    r.done().await;
    r.handle.shut().await.unwrap();
    r.closed().await;
}

// Being asked what it can do.

#[tokio::test]
async fn it_is_asked_what_it_can_do() {
    let w = world(&lines(&[
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 2, "capabilities": {"session": {}}, "info": {"name": "a", "title": "An A", "version": "3"}}}}),
    ]));
    let p = acp().probe(&w.site(), token()).await.unwrap();
    assert_eq!(
        (p.name.as_deref(), p.version.as_deref(), p.protocol.as_str()),
        (Some("An A"), Some("3"), "acp/2")
    );
    assert!(p.caps.resume);
    assert_eq!(p.program, w.at("bin/agent"));
    assert_eq!(w.seen()["cwd"], w.at("tmp").as_str(), "not in a workspace");
    assert_eq!(w.heard().len(), 1, "no session was opened");

    let w = world(&lines(&[
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"id": 0, "result": {"userAgent": "x/1.2"}}}),
        json!({"until": {"method": "model/list"}}),
        json!({"say": {"id": 1, "result": {"data": [{"model": "m"}]}}}),
    ]));
    let p = Harness::Codex.probe(&w.site(), token()).await.unwrap();
    assert!(p.caps.models.has("m"));
    assert_eq!(p.version.as_deref(), Some("1.2"));
    assert_eq!(w.seen()["args"], json!(["app-server"]));
    assert_eq!(w.heard().len(), 3);

    let w = world("");
    std::fs::write(w.dir.path().join("bin/version"), "2.1.325 (Claude Code)\n").unwrap();
    let p = claude().probe(&w.site(), token()).await.unwrap();
    assert_eq!((p.version.as_deref(), p.name.as_deref()), (Some("2.1.325"), Some("Claude Code")));
    assert!(p.caps.approvals);
    assert_eq!(w.seen()["args"], json!(["--version"]));
    assert_eq!(w.seen()["cwd"], w.at("tmp").as_str());
}

#[tokio::test]
async fn it_does_not_say_what_it_can_do() {
    let bad = |e: Result<axiom_harness::Probe, Error>| match e {
        Err(Error::Probe(w)) => w,
        e => panic!("{e:?}"),
    };
    let w = world(&lines(&[json!({"exit": 2})]));
    assert_eq!(bad(acp().probe(&w.site(), token()).await), "it ended by itself (exit 2)");
    let w = world("");
    assert_eq!(
        bad(claude().probe(&w.site(), token()).await),
        "it does not say which version it is"
    );
    std::fs::write(w.dir.path().join("bin/version"), "agent\n").unwrap();
    assert_eq!(
        bad(claude().probe(&w.site(), token()).await),
        "it does not say which version it is"
    );
    let mut w = world(&lines(&[json!({"hang": true})]));
    w.limits.opening = ms(300);
    assert_eq!(bad(acp().probe(&w.site(), token()).await), "it did not begin in time");
    let loose = Harness::Acp(Acp { args: vec!["--yolo".into()], ..Acp::default() });
    assert!(matches!(loose.probe(&w.site(), token()).await, Err(Error::Unsafe(_))));
    // One that never ends is ended.
    let m = mark(8);
    let mut w = world("");
    w.limits.opening = ms(500);
    let agent = w.dir.path().join("bin/agent");
    std::fs::remove_file(&agent).unwrap();
    std::fs::write(&agent, format!("#!/bin/sh\nsleep {m}\n")).unwrap();
    std::fs::set_permissions(&agent, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    assert_eq!(bad(claude().probe(&w.site(), token()).await), "it did not answer in time");
    assert_eq!(alive(&m), 0);
}

// The other protocols, from end to end.

#[tokio::test]
async fn codex_goes_on_with_a_thread_is_refused_and_stopped() {
    let w = world(&lines(&[
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"id": 0, "result": {"userAgent": "x/1.2"}}}),
        json!({"until": {"method": "model/list"}}),
        json!({"say": {"id": 1, "error": {"code": -1, "message": "no list"}}}),
        json!({"hear": {"method": "thread/resume", "params": {"threadId": "thr_9", "excludeTurns": true}}}),
        json!({"say": {"id": 2, "result": {"thread": {"id": "thr_9"}, "sandbox": {"type": "readOnly"}, "approvalPolicy": "on-request"}}}),
        json!({"hear": {"method": "turn/start"}}),
        json!({"say": {"id": 3, "result": {"turn": {"id": "t"}}}}),
        json!({"say": {"id": 9, "method": "item/fileChange/requestApproval", "params": {"threadId": "thr_9", "turnId": "t", "itemId": "i"}}}),
        json!({"hear": {"id": 9, "result": {"decision": "decline"}}}),
        json!({"hear": {"method": "turn/interrupt", "params": {"turnId": "t"}}}),
        json!({"say": {"id": 4, "result": {}}}),
        json!({"say": {"method": "turn/completed", "params": {"threadId": "thr_9", "turn": {"id": "t", "status": "interrupted"}}}}),
    ]));
    let mut s = w.start(Access::Ask);
    s.resume = Some("thr_9".into());
    let mut r = w.run_with(&Harness::Codex, s);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    r.handle.answer(&q, Answer::Refuse).await.unwrap();
    r.handle.cancel().await.unwrap();
    assert_eq!(r.done().await.outcome, Outcome::Cancelled);
    let g = r.closed().await;
    assert_eq!((g.why, g.words), (Why::Asked, None), "{}", g.stderr);
}

#[tokio::test]
async fn codex_without_the_sandbox_it_was_told_to_use() {
    let w = world(&lines(&[
        json!({"until": {"method": "model/list"}}),
        json!({"say": {"id": 0, "result": {}}}),
        json!({"hang": true}),
    ]));
    let script = lines(&[
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"id": 0, "result": {}}}),
        json!({"until": {"method": "model/list"}}),
        json!({"say": {"id": 1, "result": {"data": []}}}),
        json!({"hear": {"method": "thread/start"}}),
        json!({"say": {"id": 2, "result": {"thread": {"id": "t"}, "sandbox": {"type": "dangerFullAccess"}, "approvalPolicy": "on-request"}}}),
    ]);
    std::fs::write(w.dir.path().join("bin/script.jsonl"), script).unwrap();
    let mut r = w.run(&Harness::Codex, Access::Work);
    let g = r.closed().await;
    assert_eq!(g.why, Why::Boundary);
    assert_eq!(r.all.len(), 1, "it was never ready");
}

#[tokio::test]
async fn claude_goes_on_with_a_session_is_refused_and_stopped() {
    let w = world(&lines(&[
        json!({"hear": {"type": "control_request", "request": {"subtype": "initialize"}}}),
        json!({"say": {"type": "control_response", "response": {"subtype": "success", "request_id": "axiom-1", "response": {}}}}),
        json!({"hear": {"type": "user", "session_id": "old"}}),
        json!({"say": {"type": "system", "subtype": "init", "session_id": "old", "permissionMode": "default"}}),
        json!({"say": {"type": "control_request", "request_id": "r", "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "x"}, "tool_use_id": "t"}}}),
        json!({"hear": {"type": "control_response", "response": {"request_id": "r", "response": {"behavior": "deny"}}}}),
        json!({"say": {"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t", "content": "denied", "is_error": true}]}}}),
        json!({"hear": {"type": "control_request", "request": {"subtype": "interrupt"}}, "as": "i"}),
        json!({"sleep": 500}),
        json!({"say": {"type": "control_response", "response": {"subtype": "success", "request_id": "$i/request_id", "response": {}}}}),
        json!({"say": {"type": "result", "subtype": "error_during_execution", "is_error": true, "errors": [], "session_id": "old"}}),
        json!({"hang": true}),
    ]));
    let mut s = w.start(Access::Ask);
    s.resume = Some("old".into());
    let mut r = w.run_with(&claude(), s);
    r.ready().await;
    r.prompt("go").await;
    let q = r.asked().await;
    r.handle.answer(&q, Answer::Refuse).await.unwrap();
    r.until(|e| matches!(e, Event::ToolEnded(_))).await;
    r.handle.interrupt().await.unwrap();
    // One that could be told more is not, while it is being stopped.
    assert_eq!(r.handle.prompt("more", By::Person).await, Err(Refused::Busy));
    assert_eq!(r.done().await.outcome, Outcome::Cancelled);
    r.handle.cancel().await.unwrap();
    let g = r.closed().await;
    assert_eq!((g.why, g.words), (Why::Asked, None), "{}", g.stderr);
    let args = w.seen()["args"].clone();
    let args: Vec<&str> = args.as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(args.windows(2).any(|a| a == ["--resume", "old"]));
    assert!(args.windows(2).any(|a| a == ["--permission-prompt-tool", "stdio"]));
}

#[tokio::test]
async fn claude_in_another_mode_is_ended() {
    let w = world(&lines(&[
        json!({"hear": {"type": "control_request"}}),
        json!({"say": {"type": "control_response", "response": {"subtype": "success", "request_id": "axiom-1", "response": {}}}}),
        json!({"hear": {"type": "user"}}),
        json!({"say": {"type": "system", "subtype": "init", "session_id": "s", "permissionMode": "bypassPermissions"}}),
        json!({"say": {"type": "assistant", "message": {"id": "m", "content": [{"type": "text", "text": "done already"}]}}}),
        json!({"hang": true}),
    ]));
    let mut r = w.run(&claude(), Access::Ask);
    r.ready().await;
    r.prompt("go").await;
    let g = r.closed().await;
    assert_eq!(g.why, Why::Boundary);
    assert!(g.words.unwrap().contains("bypassPermissions"));
    assert_eq!(r.texts().len(), 0);
}
