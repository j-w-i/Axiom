//! `agent/restore` through the real daemon: what a restore leaves as it is (a path inside a git
//! directory) is told to the person. The wire result has no field for it (IPC frozen), so the
//! session's transcript carries it as a notice (`error` event, kind `notice`) right after the
//! `restored` event (review E round: skipped paths were only logged).

mod common;

use axiom_protocol::agent::{
    AgentEvent, Decision, EventBody, Policy, Restore, RestoreParams, Send, SendParams,
    SessionCreate, SessionCreateParams, SessionId, SessionOpen, SessionRef, SetPolicy,
};
use axiom_protocol::WorkspaceId;
use common::Daemon;
use serde_json::json;
use std::path::Path;
use std::time::Duration;

const WAIT: Duration = Duration::from_secs(120);

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

fn send(ws: &WorkspaceId, s: &SessionId, text: &str) -> SendParams {
    SendParams {
        workspace_id: Some(ws.clone()),
        session_id: s.clone(),
        text: text.into(),
        attachments: vec![],
        images: vec![],
        from_turn: None,
    }
}

/// The next `agent/event` of `s` matching `want` within `wait`, skipping the others.
async fn next_event(
    d: &Daemon,
    s: &SessionId,
    wait: Duration,
    want: impl Fn(&EventBody) -> bool,
) -> AgentEvent {
    let end = tokio::time::Instant::now() + wait;
    loop {
        let left = end.saturating_duration_since(tokio::time::Instant::now());
        let v = d.next_notification("agent/event", left).await;
        let e: AgentEvent = serde_json::from_value(v).unwrap();
        if &e.session_id == s && want(&e.event) {
            return e;
        }
    }
}

fn is_notice(e: &EventBody) -> bool {
    matches!(e, EventBody::Error { kind, .. } if kind == "notice")
}

#[tokio::test]
async fn a_restore_tells_the_person_what_it_left_inside_a_git_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("s.json");
    std::fs::write(
        &script,
        json!({"turns": [
            {"events": [{"type":"toolCall","id":"w1","name":"fs.applyPatch","input":{"path":"one.txt","create":"1\n"}}]},
            {"expect": {"toolResult": {"callId":"w1","isError":false}},
             "events": [{"type":"text","text":"Wrote one."}]},
            {"events": [{"type":"toolCall","id":"w2","name":"fs.applyPatch","input":{"path":"two.txt","create":"2\n"}}]},
            {"expect": {"toolResult": {"callId":"w2","isError":false}},
             "events": [{"type":"text","text":"Wrote two."}]}
        ]})
        .to_string(),
    )
    .unwrap();
    let d = Daemon::spawn_configured(tmp.path(), {
        let script = script.clone();
        move |c| {
            c.env("AXIOM_TEST_SCRIPTED_MODEL", &script);
        }
    })
    .await;
    d.initialize().await;
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Allow,
            rules: vec![],
        })
        .await
        .unwrap();
    // A bare repository in the workspace: its files are inside a git directory.
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "--bare", "bare.git"]);
    std::fs::write(dir.path().join("bare.git/description"), "orig\n").unwrap();
    let f = d.open_dir(dir).await;
    let sid = d
        .client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: Some(f.id.clone()),
            provider_id: "scripted".into(),
            model: "scripted".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id;
    let turn_end = |e: &EventBody| matches!(e, EventBody::TurnEnd { .. });

    // Turn 1: checkpoint 1 (description "orig"), then one.txt.
    d.client
        .request::<Send>(send(&f.id, &sid, "write one"))
        .await
        .unwrap();
    next_event(&d, &sid, WAIT, turn_end).await;
    // The description changes between turns; turn 2 leaves it so (checkpoint 2, two.txt).
    std::fs::write(f.root.join("bare.git/description"), "changed\n").unwrap();
    d.client
        .request::<Send>(send(&f.id, &sid, "write two"))
        .await
        .unwrap();
    next_event(&d, &sid, WAIT, turn_end).await;

    let r = d
        .client
        .request::<Restore>(RestoreParams {
            workspace_id: Some(f.id.clone()),
            session_id: sid.clone(),
            checkpoint: 1,
            skip_conflicts: false,
        })
        .await
        .unwrap();
    let mut restored: Vec<&str> = r.restored.iter().map(|p| p.0.as_str()).collect();
    restored.sort();
    assert_eq!(restored, ["one.txt", "two.txt"]);
    assert!(r.conflicts.is_empty(), "{:?}", r.conflicts);
    assert_eq!(
        std::fs::read_to_string(f.root.join("bare.git/description")).unwrap(),
        "changed\n"
    );

    // The `restored` event, then the notice naming what was left.
    let restored_ev = next_event(&d, &sid, WAIT, |e| matches!(e, EventBody::Restored { .. })).await;
    // Emitted right after it: a short wait.
    let notice = next_event(&d, &sid, Duration::from_secs(10), is_notice).await;
    assert_eq!(notice.seq, restored_ev.seq + 1);
    let EventBody::Error { message, .. } = &notice.event else {
        unreachable!()
    };
    assert!(
        message.contains("checkpoint 1")
            && message.contains("git directory")
            && message.contains("bare.git/description"),
        "{message}"
    );

    // It is in the transcript, not only sent.
    let open = d
        .client
        .request::<SessionOpen>(SessionRef {
            workspace_id: Some(f.id.clone()),
            session_id: sid.clone(),
        })
        .await
        .unwrap();
    assert!(
        open.events
            .iter()
            .any(|e| e.seq == notice.seq && is_notice(&e.event)),
        "the notice is not in the journal"
    );
}
