//! Whole conversations as the documentation has them, with an agent in a process of its own
//! (fixtures/README.md).
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use axiom_harness::{Acp, Event, Harness, Kind, Progress, Prompts, Settled, Took, Why};
use axiom_policy::{Access, audit::Containment};
use axiom_proto::{Outcome, Provenance};

fn acp_with_modes() -> Harness {
    Harness::Acp(Acp { args: vec!["acp".into()], ask: Some("ask".into()), ..Acp::default() })
}

async fn analyse(version: u32) -> Run {
    let w = world(&fixture(&format!("acp-v{version}-from-documentation.jsonl")));
    let mut r = w.run(&acp_with_modes(), Access::Ask);
    let Event::Ready(ready) = r.until(|e| matches!(e, Event::Ready(_))).await else { panic!() };
    assert_eq!(ready.session, "sess_abc123def456");
    assert_eq!(ready.protocol, format!("acp/{version}"));
    assert_eq!(ready.version.as_deref(), Some("1.0.0"));
    assert!(ready.caps.resume);
    let text = "Can you analyze this code for potential issues?";
    assert_eq!(r.prompt(text).await, Took::Began);
    let q = r.asked().await;
    r.choose(&q, "allow-once").await;
    let d = r.done().await;
    assert_eq!((d.outcome, d.stop.as_str(), d.error), (Outcome::Completed, "end_turn", None));
    r.handle.shut().await.unwrap();
    let g = r.closed().await;
    assert_eq!((g.why, g.words.clone()), (Why::Asked, None), "{g:#?}");
    assert_eq!(g.ended.unwrap().code, Some(0), "{}", g.stderr);
    assert_eq!(w.seen()["args"], serde_json::json!(["acp"]));
    let Event::ToolEnded(t) = r.all.iter().find(|e| matches!(e, Event::ToolEnded(_))).unwrap()
    else {
        panic!()
    };
    assert_eq!((t.id.as_str(), t.outcome), ("call_001", Outcome::Completed));
    assert!(t.output.as_ref().unwrap().starts_with("Analysis complete"));
    assert_eq!(t.diffs[0].path, "/home/user/project/src/config.json");
    assert!(r.all.contains(&Event::AskClosed { id: q, how: Settled::Answered }));
    let spent = r.all.iter().find_map(|e| match e {
        Event::Usage(s) => Some(s),
        _ => None,
    });
    let spent = spent.unwrap();
    assert_eq!(spent.context, Some((53000, 200_000)));
    assert_eq!(spent.cost.as_ref().unwrap().provenance, Provenance::Reported);
    let plan = r.all.iter().find_map(|e| match e {
        Event::Plan(p) => Some(p),
        _ => None,
    });
    assert_eq!(plan.unwrap().steps[0], ("Check for syntax errors".into(), Progress::Todo));
    assert_eq!(r.count(|e| matches!(e, Event::Unknown { .. } | Event::NotHeard { .. })), 0);
    r
}

#[tokio::test]
async fn acp_version_1() {
    let r = analyse(1).await;
    let said = "I'll analyze your code for potential issues. Let me examine it...";
    assert_eq!(r.texts(), [(Kind::Prose, said.into())]);
}

#[tokio::test]
async fn acp_version_2() {
    let r = analyse(2).await;
    assert_eq!(
        r.texts(),
        [
            (Kind::Reasoning, "Need to inspect the loop body before suggesting a fix.".into()),
            (
                Kind::Prose,
                "I'll analyze your code for potential issues. Let me examine it...".into()
            ),
        ]
    );
    let out: Vec<_> = r
        .all
        .iter()
        .filter_map(|e| match e {
            Event::ToolOutput { text, whole: false, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(out, ["Checked syntax...", "passed\r\n"]);
}

#[tokio::test]
async fn codex() {
    let w = world(&fixture("codex-from-documentation.jsonl"));
    let mut s = w.start(Access::Work);
    (s.model, s.effort) = (Some("gpt-6.1-sol".into()), Some("medium".into()));
    assert_eq!(Harness::Codex.launch(&s).unwrap().containment, Containment::Harness);
    let mut r = w.run_with(&Harness::Codex, s);
    let Event::Ready(ready) = r.until(|e| matches!(e, Event::Ready(_))).await else { panic!() };
    assert_eq!((ready.session.as_str(), ready.model.as_deref()), ("thr_123", Some("gpt-6.1-sol")));
    assert!(ready.caps.models.has("gpt-6.1-sol"));
    assert_eq!(r.prompt("Run tests").await, Took::Began);
    let q = r.asked().await;
    r.choose(&q, "accept").await;
    r.until(|e| matches!(e, Event::ToolEnded(_))).await;
    let more = "Actually focus on failing tests first.";
    assert_eq!(r.prompt(more).await, Took::Steered);
    assert_eq!(r.done().await.outcome, Outcome::Completed);
    r.handle.shut().await.unwrap();
    let g = r.closed().await;
    assert_eq!(g.ended.clone().unwrap().code, Some(0), "{g:#?}");
    assert_eq!(r.texts(), [(Kind::Reasoning, "Think".into()), (Kind::Prose, "All pass.".into())]);
    let Event::ToolEnded(t) = r.all.iter().find(|e| matches!(e, Event::ToolEnded(_))).unwrap()
    else {
        panic!()
    };
    assert_eq!((t.output.as_deref(), t.outcome), (Some("ok\n"), Outcome::Completed));
    assert_eq!(r.count(|e| matches!(e, Event::Limit(l) if l.used == Some(0.25))), 1);
    assert_eq!(r.count(|e| matches!(e, Event::Usage(s) if s.usage.output == Some(20))), 1);
    assert_eq!(r.count(|e| matches!(e, Event::Unknown { .. })), 0, "{:#?}", r.all);
    assert_eq!(w.seen()["args"], serde_json::json!(["app-server"]));
}

#[tokio::test]
async fn claude_code() {
    let w = world(&fixture("claude-from-documentation.jsonl"));
    let mut r = w.run(&Harness::Claude(Prompts::Stdio), Access::Ask);
    let Event::Ready(ready) = r.until(|e| matches!(e, Event::Ready(_))).await else { panic!() };
    assert_eq!(ready.version.as_deref(), Some("2.1.330"));
    assert!(ready.caps.models.has("opus") && ready.caps.efforts.has("max"));
    assert_eq!(r.prompt("List the files in my project").await, Took::Began);
    let q = r.asked().await;
    r.choose(&q, "allow").await;
    r.until(|e| matches!(e, Event::ToolEnded(_))).await;
    let more = "Skip the integration tests and summarize what you have so far";
    assert_eq!(r.prompt(more).await, Took::Steered);
    let d = r.done().await;
    assert_eq!((d.outcome, d.stop.as_str()), (Outcome::Completed, "completed"));
    r.handle.shut().await.unwrap();
    let g = r.closed().await;
    assert_eq!(g.ended.clone().unwrap().code, Some(0), "{g:#?}");
    let said = [(Kind::Prose, "I'll look.".into()), (Kind::Prose, "All pass.".into())];
    assert_eq!(r.texts(), said);
    assert_eq!(r.count(|e| matches!(e, Event::Limit(l) if l.used == Some(0.83))), 1);
    let cost = r.all.iter().find_map(|e| match e {
        Event::Usage(s) => s.cost.clone(),
        _ => None,
    });
    assert_eq!(cost.unwrap().provenance, Provenance::Estimated);
    assert_eq!(r.count(|e| matches!(e, Event::Unknown { .. } | Event::Session(_))), 0);
    // Who the person is, which it told without being asked, is not on record.
    let frames = r.kept.frames.lock().unwrap();
    assert!(frames.iter().all(|f| !f.text.contains("person@example.com")));
    assert!(frames.iter().any(|f| f.text.contains("(kept back)")));
    assert!(g.frames.iter().all(|f| !f.text.contains("person@example.com")));
}
