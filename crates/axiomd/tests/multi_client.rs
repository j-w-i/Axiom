//! Several connections on one daemon (processes.md §Multi-client): `fs/watch` per connection
//! (AUD-028), workspaces shared and reference-counted (AUD-029), one notification per
//! connection however often it opened the workspace (AUD-030). Unix only (the socket).
#![cfg(unix)]

mod common;

use axiom_protocol::fs::{Watch, WatchParams};
use axiom_protocol::lifecycle::{Initialize, InitializeParams, PeerInfo};
use axiom_protocol::task::{Command, ListTasks, ListTasksParams, Start, StartParams};
use axiom_protocol::terminal::{Create, CreateParams, Write, WriteParams};
use axiom_protocol::workspace::{Close, Open, OpenParams, WorkspaceRef};
use axiom_protocol::{ErrorKind, WorkspaceId};
use axiom_rpc::Client;
use common::{kind, Daemon, WAIT};
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

async fn socket_client(data: &Path) -> (Client, tokio::sync::broadcast::Receiver<(String, Value)>) {
    let run = data.join("run");
    for _ in 0..100 {
        if run.join("axiomd.token").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let stream = tokio::net::UnixStream::connect(run.join("axiomd.sock"))
        .await
        .unwrap();
    let (r, w) = stream.into_split();
    let c = Client::new(r, w);
    let rx = c.notifications();
    let token = std::fs::read_to_string(run.join("axiomd.token")).unwrap();
    c.request::<Initialize>(InitializeParams {
        protocol_version: "0.6".into(),
        client: PeerInfo {
            name: "multi-client-test".into(),
            version: "0".into(),
        },
        auth_token: Some(token),
        capabilities: None,
    })
    .await
    .unwrap();
    (c, rx)
}

async fn open(c: &Client, root: &Path) -> WorkspaceId {
    c.request::<Open>(OpenParams {
        path: root.to_string_lossy().into_owned(),
    })
    .await
    .unwrap()
    .workspace_id
}

/// The next notification `method` on `rx` within `t`, if any.
async fn next(
    rx: &mut tokio::sync::broadcast::Receiver<(String, Value)>,
    method: &str,
    t: Duration,
) -> Option<Value> {
    let end = tokio::time::Instant::now() + t;
    loop {
        match tokio::time::timeout_at(end, rx.recv()).await {
            Ok(Ok((m, p))) if m == method => return Some(p),
            Ok(Ok(_)) | Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            _ => return None,
        }
    }
}

/// The next `fs/didChange` that names `file`. Batches for other paths are skipped: on macOS,
/// FSEvents can report the fixture's own files (written just before the watch) late.
async fn next_change_for(
    rx: &mut tokio::sync::broadcast::Receiver<(String, Value)>,
    file: &str,
    t: Duration,
) -> Option<Value> {
    let end = tokio::time::Instant::now() + t;
    loop {
        let left = end.saturating_duration_since(tokio::time::Instant::now());
        let n = next(rx, "fs/didChange", left).await?;
        if n.to_string().contains(file) {
            return Some(n);
        }
    }
}

async fn enable_watch(c: &Client, ws: &WorkspaceId) {
    c.request::<Watch>(WatchParams {
        workspace_id: ws.clone(),
        enabled: true,
    })
    .await
    .unwrap();
}

/// AUD-028: each connection that enables `fs/watch` gets the changes; one that goes away does
/// not take the others' (or a later connection's) events with it.
#[tokio::test]
async fn every_watching_connection_gets_the_changes() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let (a, mut arx) = socket_client(d.data_dir()).await;
    let (b, mut brx) = socket_client(d.data_dir()).await;
    for c in [&a, &b] {
        assert_eq!(open(c, &f.root).await, f.id);
        enable_watch(c, &f.id).await;
        // Twice on one connection: still one subscription.
        enable_watch(c, &f.id).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(f.path("one.txt"), "1").unwrap();
    for rx in [&mut arx, &mut brx] {
        next_change_for(rx, "one.txt", WAIT)
            .await
            .expect("a change for one.txt");
        // Exactly one batch for the write, not two.
        let again = next_change_for(rx, "one.txt", Duration::from_millis(400)).await;
        assert!(again.is_none(), "a duplicate: {again:?}");
    }
    // A goes away; B still gets events, and a new connection C can watch.
    drop((a, arx));
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (c, mut crx) = socket_client(d.data_dir()).await;
    open(&c, &f.root).await;
    enable_watch(&c, &f.id).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(f.path("two.txt"), "2").unwrap();
    for rx in [&mut brx, &mut crx] {
        next_change_for(rx, "two.txt", WAIT)
            .await
            .expect("a change for two.txt");
    }
}

/// AUD-029: one connection closing a shared workspace does not stop another's work; the last
/// one to close it tears it down.
#[tokio::test]
async fn closing_a_shared_workspace_only_releases_the_caller() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let (b, _brx) = socket_client(d.data_dir()).await;
    assert_eq!(open(&b, &f.root).await, f.id);
    let started = b
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            command: Command::Argv(vec!["sleep".into(), "30".into()]),
            cwd: None,
            env: Default::default(),
            label: None,
        })
        .await
        .unwrap();
    // A (the stdio client) closes: B's task keeps running and B can still use the workspace.
    d.client
        .request::<Close>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let list = b
        .request::<ListTasks>(ListTasksParams {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    let t = list
        .tasks
        .iter()
        .find(|t| t.task_id == started.task_id)
        .unwrap();
    assert!(t.exit.is_none(), "B's task was stopped: {t:?}");
    assert!(f.list("").await.is_ok());
    // B closes too: now it is torn down.
    b.request::<Close>(WorkspaceRef {
        workspace_id: f.id.clone(),
    })
    .await
    .unwrap();
    let e = b
        .request::<ListTasks>(ListTasksParams {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::WorkspaceNotOpen);
}

/// AUD-029: a socket connection that opened a workspace alone and went away releases it.
#[tokio::test]
async fn a_disconnect_releases_what_only_that_connection_held() {
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    let (b, brx) = socket_client(d.data_dir()).await;
    let ws = open(&b, dir.path()).await;
    drop((b, brx));
    let mut gone = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let r = d
            .client
            .request::<ListTasks>(ListTasksParams {
                workspace_id: ws.clone(),
            })
            .await;
        if r.is_err_and(|e| kind(&e) == ErrorKind::WorkspaceNotOpen) {
            gone = true;
            break;
        }
    }
    assert!(
        gone,
        "the workspace stayed open after its only connection left"
    );
}

/// AUD-030: a connection that opened the same workspace twice gets each terminal output once.
#[tokio::test]
async fn opening_twice_does_not_duplicate_notifications() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    // The stdio client opens it a second time.
    assert_eq!(open(&d.client, &f.root).await, f.id);
    let t = d
        .client
        .request::<Create>(CreateParams {
            workspace_id: f.id.clone(),
            argv: Some(vec!["sh".into(), "-i".into()]),
            cols: 80,
            rows: 24,
        })
        .await
        .unwrap();
    d.client
        .request::<Write>(WriteParams {
            terminal_id: t.terminal_id.clone(),
            data: "echo mark-$((40+2))\r".into(),
        })
        .await
        .unwrap();
    let mut seqs = Vec::new();
    let mut out = String::new();
    let end = tokio::time::Instant::now() + Duration::from_secs(20);
    while !out.contains("mark-42") && tokio::time::Instant::now() < end {
        let v = d
            .next_notification("terminal/output", Duration::from_secs(20))
            .await;
        seqs.push(v["seq"].as_u64().unwrap());
        out.push_str(v["data"].as_str().unwrap());
    }
    assert!(out.contains("mark-42"), "{out:?}");
    // Nothing more for a moment (a duplicate would follow at once).
    let end = tokio::time::Instant::now() + Duration::from_millis(300);
    while tokio::time::Instant::now() < end {
        let left = end - tokio::time::Instant::now();
        if d.no_notification("terminal/output", left).await {
            break;
        }
    }
    let mut sorted = seqs.clone();
    sorted.dedup();
    assert_eq!(sorted, seqs, "a seq arrived twice: {seqs:?}");
}
