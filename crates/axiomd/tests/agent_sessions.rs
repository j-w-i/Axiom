//! Sessions and the agent loop through the real daemon with the scripted model (M3-LOOP-001):
//! "add a function and fix the build" through tools only, checkpoints before writes and a
//! restore, a refused write, resume after a restart, cancel, a conversation without a
//! workspace, edit/regenerate, images, compaction.

mod common;

use axiom_protocol::agent::{
    AgentEvent, Checkpoints, Decision, EventBody, Image, ImageData, ImageParams, Policy, Restore,
    RestoreParams, Send, SendParams, SessionCreate, SessionCreateParams, SessionDelete, SessionId,
    SessionList, SessionListParams, SessionOpen, SessionRef, SessionRename, SessionRenameParams,
    SetPolicy, TurnEndReason,
};
use axiom_protocol::{ErrorKind, WorkspaceId};
use common::{kind, Daemon};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

const TURN_WAIT: Duration = Duration::from_secs(120);

fn script(dir: &Path, name: &str, v: Value) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, v.to_string()).unwrap();
    p
}

async fn daemon(data: &Path, script: &Path) -> Daemon {
    let script = script.to_path_buf();
    let d = Daemon::spawn_configured(data, move |c| {
        c.env("AXIOM_TEST_SCRIPTED_MODEL", &script);
    })
    .await;
    d.initialize().await;
    d
}

async fn allow_all(d: &Daemon) {
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Allow,
            rules: vec![],
        })
        .await
        .unwrap();
}

async fn create(d: &Daemon, ws: Option<&WorkspaceId>) -> SessionId {
    d.client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: ws.cloned(),
            provider_id: "scripted".into(),
            model: "scripted".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id
}

fn send_params(ws: Option<&WorkspaceId>, s: &SessionId, text: &str) -> SendParams {
    SendParams {
        workspace_id: ws.cloned(),
        session_id: s.clone(),
        text: text.into(),
        attachments: vec![],
        images: vec![],
        from_turn: None,
    }
}

/// Every `agent/event` of `s` until its `turnEnd`.
async fn turn_events(d: &Daemon, s: &SessionId) -> Vec<AgentEvent> {
    let mut out = Vec::new();
    loop {
        let v = d.next_notification("agent/event", TURN_WAIT).await;
        let e: AgentEvent = serde_json::from_value(v).unwrap();
        if &e.session_id != s {
            continue;
        }
        let end = matches!(e.event, EventBody::TurnEnd { .. });
        out.push(e);
        if end {
            return out;
        }
    }
}

fn kind_of(e: &AgentEvent) -> String {
    serde_json::to_value(&e.event).unwrap()["type"]
        .as_str()
        .unwrap()
        .to_string()
}

fn end_reason(evs: &[AgentEvent]) -> TurnEndReason {
    match evs.last().map(|e| &e.event) {
        Some(EventBody::TurnEnd { reason }) => *reason,
        other => panic!("no turnEnd: {other:?}"),
    }
}

fn errors(evs: &[AgentEvent]) -> Vec<String> {
    evs.iter()
        .filter_map(|e| match &e.event {
            EventBody::Error { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect()
}

fn cargo_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/main.rs"),
        "fn main() {\n    println!(\"{}\", add(1, 2));\n}\n",
    )
    .unwrap();
    std::fs::write(dir.path().join(".gitignore"), "target/\n").unwrap();
    dir
}

#[tokio::test]
async fn agent_add_a_function_and_fix_the_build_through_tools_only() {
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        "s.json",
        json!({"turns": [
            {"expect": {"userContains": "fix the build", "toolsInclude": ["fs.applyPatch", "build.run"],
                        "systemContains": "AGENTS.md"},
             "events": [{"type":"text","text":"Let me look."},
                        {"type":"toolCall","id":"c1","name":"fs.read","input":{"path":"src/main.rs"}}]},
            {"expect": {"toolResult": {"callId":"c1","contains":"add(1, 2)"}},
             "events": [{"type":"toolCall","id":"c2","name":"build.run","input":{"task":"build"}}]},
            {"expect": {"toolResult": {"callId":"c2","contains":"E0425"}},
             "events": [{"type":"toolCall","id":"c3","name":"fs.applyPatch","input":{
                 "path":"src/main.rs",
                 "edits":[{"search":"fn main() {","replace":"fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\nfn main() {"}]}}]},
            {"expect": {"toolResult": {"callId":"c3","isError":false}},
             "events": [{"type":"toolCall","id":"c4","name":"build.run","input":{"task":"build"}}]},
            {"expect": {"toolResult": {"callId":"c4","contains":"succeeded"}},
             "events": [{"type":"usage","inputTokens":900,"outputTokens":20},
                        {"type":"text","text":"Added `add` and the build passes."}]}
        ]}),
    );
    let d = daemon(tmp.path(), &s).await;
    allow_all(&d).await;
    let fx = cargo_fixture();
    std::fs::write(fx.path().join("AGENTS.md"), "Keep functions small.\n").unwrap();
    let f = d.open_dir(fx).await;
    let sid = create(&d, Some(&f.id)).await;
    d.client
        .request::<Send>(send_params(
            Some(&f.id),
            &sid,
            "Add the missing function and fix the build.",
        ))
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(
        end_reason(&evs),
        TurnEndReason::EndTurn,
        "errors: {:?}",
        errors(&evs)
    );

    // Tools only: the file was changed by fs.applyPatch and the build now passes.
    let main = std::fs::read_to_string(f.root.join("src/main.rs")).unwrap();
    assert!(
        main.starts_with("fn add(a: i32, b: i32) -> i32 {"),
        "{main}"
    );
    let tools: Vec<String> = evs
        .iter()
        .filter_map(|e| match &e.event {
            EventBody::ToolCall { tool, .. } => Some(tool.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tools,
        ["fs.read", "build.run", "fs.applyPatch", "build.run"]
    );

    // Every non-read call was preceded by the turn's checkpoint (one per turn).
    let persisted: Vec<&AgentEvent> = evs.iter().filter(|e| kind_of(e) != "textDelta").collect();
    let ckpt = persisted
        .iter()
        .position(|e| kind_of(e) == "checkpoint")
        .expect("a checkpoint");
    let first_write_result = persisted
        .iter()
        .position(|e| matches!(&e.event, EventBody::ToolResult { call_id, .. } if call_id == "c2"))
        .unwrap();
    assert!(ckpt < first_write_result);
    assert_eq!(
        persisted
            .iter()
            .filter(|e| kind_of(e) == "checkpoint")
            .count(),
        1
    );
    // Seqs strictly increase by one over persisted events.
    let seqs: Vec<u64> = persisted.iter().map(|e| e.seq).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "{seqs:?}");
    // Streaming happened.
    assert!(evs.iter().any(|e| kind_of(e) == "textDelta"));

    // The title came from the first message; the session is listed.
    let list = d
        .client
        .request::<SessionList>(SessionListParams {
            workspace_id: Some(f.id.clone()),
        })
        .await
        .unwrap();
    assert_eq!(
        list.sessions[0].title,
        "Add the missing function and fix the build."
    );

    // Restore the checkpoint: the file is back.
    let cps = d
        .client
        .request::<Checkpoints>(SessionRef {
            workspace_id: Some(f.id.clone()),
            session_id: sid.clone(),
        })
        .await
        .unwrap();
    assert_eq!(cps.checkpoints[0].label, "before build.run");
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
    // The agent's own `cargo build` created Cargo.lock after the checkpoint: it goes too.
    assert_eq!(
        r.restored.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        ["Cargo.lock", "src/main.rs"]
    );
    assert!(!f.root.join("Cargo.lock").exists());
    assert_eq!(
        std::fs::read_to_string(f.root.join("src/main.rs")).unwrap(),
        "fn main() {\n    println!(\"{}\", add(1, 2));\n}\n"
    );
}

#[tokio::test]
async fn agent_refused_write_does_not_run_and_the_model_hears_why() {
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        "s.json",
        json!({"turns": [
            {"events": [{"type":"toolCall","id":"w","name":"fs.applyPatch","input":{"path":"new.txt","create":"x"}}]},
            {"expect": {"toolResult": {"callId":"w","isError":true,"contains":"approval"}},
             "events": [{"type":"text","text":"I could not write it."}]}
        ]}),
    );
    // Default policy (ask), and this client cannot approve: the write is refused.
    let d = daemon(tmp.path(), &s).await;
    let f = d.open_fixture().await;
    let sid = create(&d, Some(&f.id)).await;
    d.client
        .request::<Send>(send_params(Some(&f.id), &sid, "write it"))
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(
        end_reason(&evs),
        TurnEndReason::EndTurn,
        "{:?}",
        errors(&evs)
    );
    assert!(!f.root.join("new.txt").exists());
    assert!(
        !evs.iter().any(|e| kind_of(e) == "checkpoint"),
        "no checkpoint for a refused call"
    );
}

#[tokio::test]
async fn agent_session_resumes_after_a_daemon_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let s1 = script(
        tmp.path(),
        "1.json",
        json!({"turns": [
            {"events": [{"type":"text","text":"The capital is Canberra."}]}
        ]}),
    );
    let s2 = script(
        tmp.path(),
        "2.json",
        json!({"turns": [
            {"expect": {"historyContains": "Canberra", "userContains": "population"},
             "events": [{"type":"text","text":"About 470,000."}]}
        ]}),
    );
    let d = daemon(data.path(), &s1).await;
    let sid = create(&d, None).await;
    d.client
        .request::<Send>(send_params(None, &sid, "Capital of Australia?"))
        .await
        .unwrap();
    assert_eq!(
        end_reason(&turn_events(&d, &sid).await),
        TurnEndReason::EndTurn
    );
    d.shutdown().await;

    let d = daemon(data.path(), &s2).await;
    let opened = d
        .client
        .request::<SessionOpen>(SessionRef {
            workspace_id: None,
            session_id: sid.clone(),
        })
        .await
        .unwrap();
    let kinds: Vec<String> = opened.events.iter().map(kind_of).collect();
    assert_eq!(kinds, ["user", "assistantText", "turnEnd"]);
    d.client
        .request::<Send>(send_params(None, &sid, "And its population?"))
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(
        end_reason(&evs),
        TurnEndReason::EndTurn,
        "{:?}",
        errors(&evs)
    );
    assert_eq!(evs[0].turn, 2);
}

#[tokio::test]
async fn agent_cancel_mid_stream_and_mid_tool() {
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        "s.json",
        json!({"turns": [
            {"events": [{"type":"text","text":"Thinking"},{"type":"hang"}]},
            {"events": [{"type":"toolCall","id":"t","name":"task.run","input":{"argv":["sleep","30"]}}]}
        ]}),
    );
    let d = daemon(tmp.path(), &s).await;
    allow_all(&d).await;
    let f = d.open_fixture().await;
    let sid = create(&d, Some(&f.id)).await;
    let sref = SessionRef {
        workspace_id: Some(f.id.clone()),
        session_id: sid.clone(),
    };

    d.client
        .request::<Send>(send_params(Some(&f.id), &sid, "one"))
        .await
        .unwrap();
    d.next_notification("agent/event", Duration::from_secs(10))
        .await;
    // A second send while running is refused.
    let e = d
        .client
        .request::<Send>(send_params(Some(&f.id), &sid, "again"))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Conflict);
    let t = std::time::Instant::now();
    d.client
        .request::<axiom_protocol::agent::Cancel>(sref.clone())
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(end_reason(&evs), TurnEndReason::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());

    d.client
        .request::<Send>(send_params(Some(&f.id), &sid, "two"))
        .await
        .unwrap();
    // Wait for the tool to start, then cancel.
    loop {
        let v = d
            .next_notification("agent/event", Duration::from_secs(10))
            .await;
        if v["event"]["type"] == "toolCall" {
            break;
        }
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t = std::time::Instant::now();
    d.client
        .request::<axiom_protocol::agent::Cancel>(sref)
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(end_reason(&evs), TurnEndReason::Cancelled);
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    let result = evs.iter().find_map(|e| match &e.event {
        EventBody::ToolResult { output, .. } => Some(output.clone()),
        _ => None,
    });
    assert_eq!(result.unwrap()["outcome"], "cancelled");
}

#[tokio::test]
async fn agent_conversation_without_a_workspace_edit_regenerate_and_images() {
    let tmp = tempfile::tempdir().unwrap();
    let png = axiom_agent::session::base64_lite::encode(b"\x89PNG\r\n\x1a\nfake");
    let s = script(
        tmp.path(),
        "s.json",
        json!({"turns": [
            {"expect": {"noTools": true, "userImages": 1, "userContains": "picture"},
             "events": [{"type":"text","text":"A **picture**:\n\n```rust\nfn main() {}\n```"}]},
            {"expect": {"userContains": "drawing"}, "events": [{"type":"text","text":"A drawing."}]},
            {"expect": {"userContains": "drawing"}, "events": [{"type":"text","text":"Regenerated."}]}
        ]}),
    );
    let d = daemon(tmp.path(), &s).await;
    let sid = create(&d, None).await;
    let mut p = send_params(None, &sid, "What is in this picture?");
    p.images = vec![ImageData {
        media_type: "image/png".into(),
        data: png.clone(),
    }];
    d.client.request::<Send>(p).await.unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(
        end_reason(&evs),
        TurnEndReason::EndTurn,
        "{:?}",
        errors(&evs)
    );
    let sha = match &evs[0].event {
        EventBody::User { images, .. } => images[0].sha256.clone(),
        other => panic!("{other:?}"),
    };
    let img = d
        .client
        .request::<Image>(ImageParams {
            workspace_id: None,
            session_id: sid.clone(),
            sha256: sha,
        })
        .await
        .unwrap();
    assert_eq!(img.data, png);

    // Edit turn 1: a rewind, then turn 1 again.
    let mut p = send_params(None, &sid, "What is in this drawing?");
    p.from_turn = Some(1);
    d.client.request::<Send>(p).await.unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(kind_of(&evs[0]), "rewind");
    assert_eq!(evs[1].turn, 1);
    // Regenerate: the same text from the same turn.
    let mut p = send_params(None, &sid, "What is in this drawing?");
    p.from_turn = Some(1);
    d.client.request::<Send>(p).await.unwrap();
    assert_eq!(
        end_reason(&turn_events(&d, &sid).await),
        TurnEndReason::EndTurn
    );

    // Listed among the workspace-free conversations; rename and delete.
    let list = d
        .client
        .request::<SessionList>(SessionListParams::default())
        .await
        .unwrap();
    assert_eq!(list.sessions.len(), 1);
    let r = d
        .client
        .request::<SessionRename>(SessionRenameParams {
            workspace_id: None,
            session_id: sid.clone(),
            title: "Images".into(),
        })
        .await
        .unwrap();
    assert_eq!(r.title, "Images");
    d.client
        .request::<SessionDelete>(SessionRef {
            workspace_id: None,
            session_id: sid.clone(),
        })
        .await
        .unwrap();
    assert!(d
        .client
        .request::<SessionList>(SessionListParams::default())
        .await
        .unwrap()
        .sessions
        .is_empty());
    let e = d
        .client
        .request::<SessionOpen>(SessionRef {
            workspace_id: None,
            session_id: sid,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
}

#[tokio::test]
async fn agent_compacts_when_the_context_fills() {
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        "s.json",
        json!({"turns": [
            {"events": [{"type":"text","text":"one"}]},
            {"events": [{"type":"text","text":"two"}]},
            {"events": [{"type":"usage","inputTokens":30000,"outputTokens":5},{"type":"text","text":"three"}]},
            {"expect": {"systemContains": "summaries"}, "events": [{"type":"text","text":"SUMMARY: counted to three."}]},
            {"expect": {"historyContains": "SUMMARY: counted to three."}, "events": [{"type":"text","text":"four"}]}
        ]}),
    );
    let d = daemon(tmp.path(), &s).await;
    let sid = create(&d, None).await;
    for (i, msg) in ["a", "b", "c"].iter().enumerate() {
        d.client
            .request::<Send>(send_params(None, &sid, msg))
            .await
            .unwrap();
        let evs = turn_events(&d, &sid).await;
        assert_eq!(
            end_reason(&evs),
            TurnEndReason::EndTurn,
            "turn {i}: {:?}",
            errors(&evs)
        );
    }
    let compaction = d
        .next_notification("agent/event", Duration::from_secs(10))
        .await;
    assert_eq!(compaction["event"]["type"], "compaction", "{compaction}");
    d.client
        .request::<Send>(send_params(None, &sid, "d"))
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(
        end_reason(&evs),
        TurnEndReason::EndTurn,
        "{:?}",
        errors(&evs)
    );
}

#[tokio::test]
async fn agent_sessions_are_bound_to_their_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    let s = script(tmp.path(), "s.json", json!({"turns": []}));
    let d = daemon(tmp.path(), &s).await;
    let f = d.open_fixture().await;
    let sid = create(&d, Some(&f.id)).await;
    // Not reachable as a workspace-free conversation.
    let e = d
        .client
        .request::<SessionOpen>(SessionRef {
            workspace_id: None,
            session_id: sid,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    // An unknown provider.
    let e = d
        .client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: None,
            provider_id: "nope".into(),
            model: "m".into(),
            title: None,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
}

fn fake_mcp() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join(if cfg!(windows) {
        "fake-mcp.exe"
    } else {
        "fake-mcp"
    });
    if !p.exists() {
        assert!(std::process::Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "fake-mcp"])
            .status()
            .unwrap()
            .success());
    }
    p
}

#[tokio::test]
async fn agent_uses_the_tools_of_a_configured_mcp_server() {
    let tmp = tempfile::tempdir().unwrap();
    let s = script(
        tmp.path(),
        "s.json",
        json!({"turns": [
            {"expect": {"toolsInclude": ["mcp.fake.echo", "mcp.fake.fail", "fs.read"]},
             "events": [{"type":"toolCall","id":"m1","name":"mcp.fake.echo","input":{"text":"hi"}}]},
            {"expect": {"toolResult": {"callId":"m1","contains":"echo: hi","isError":false}},
             "events": [{"type":"toolCall","id":"m2","name":"mcp.fake.fail","input":{}}]},
            {"expect": {"toolResult": {"callId":"m2","contains":"it failed","isError":true}},
             "events": [{"type":"text","text":"done"}]}
        ]}),
    );
    let d = daemon(tmp.path(), &s).await;
    allow_all(&d).await;
    d.client
        .request::<axiom_protocol::settings::Set>(axiom_protocol::settings::SetParams {
            values: [(
                "agent.mcpServers".to_string(),
                json!([{"name": "fake", "argv": [fake_mcp().to_string_lossy()]}]),
            )]
            .into_iter()
            .collect(),
        })
        .await
        .unwrap();
    let f = d.open_fixture().await;
    let sid = create(&d, Some(&f.id)).await;
    d.client
        .request::<Send>(send_params(Some(&f.id), &sid, "use the server"))
        .await
        .unwrap();
    let evs = turn_events(&d, &sid).await;
    assert_eq!(
        end_reason(&evs),
        TurnEndReason::EndTurn,
        "{:?}",
        errors(&evs)
    );
    let effect = evs.iter().find_map(|e| match &e.event {
        EventBody::ToolCall { tool, effect, .. } if tool == "mcp.fake.echo" => Some(*effect),
        _ => None,
    });
    assert_eq!(
        effect,
        Some(axiom_protocol::agent::Effect::Execute),
        "annotations never lower the effect"
    );
}
