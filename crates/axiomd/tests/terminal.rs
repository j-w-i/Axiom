//! `terminal/*` through the daemon (M4-TERM-001): a shell in the workspace root with the project's
//! environment, output to the workspace's clients with increasing `seq`, the backlog on
//! `terminal/list`, resize, exit, close, and closing the workspace ends its terminals.
#![cfg(unix)]

mod common;

use axiom_protocol::terminal::{
    Close, Create, CreateParams, List, Resize, ResizeParams, TerminalRef, Write, WriteParams,
};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::ErrorKind;
use common::{kind, Daemon};
use serde_json::Value;
use std::time::Duration;

/// Output of terminal `id` until `want` appears; returns it and the seqs seen.
async fn output_until(d: &Daemon, id: &str, want: &str) -> (String, Vec<u64>) {
    let mut out = String::new();
    let mut seqs = Vec::new();
    while !out.contains(want) {
        let v: Value = d
            .next_notification("terminal/output", Duration::from_secs(20))
            .await;
        if v["terminalId"] == id {
            out.push_str(v["data"].as_str().unwrap());
            seqs.push(v["seq"].as_u64().unwrap());
        }
    }
    (out, seqs)
}

#[tokio::test]
async fn a_shell_in_the_workspace_streams_to_its_clients() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
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
    assert!(t.running && t.title == "sh");
    let id = t.terminal_id.0.clone();
    let w = |data: &str| WriteParams {
        terminal_id: t.terminal_id.clone(),
        data: data.into(),
    };
    d.client
        .request::<Write>(w("echo $((6*7)); pwd\r"))
        .await
        .unwrap();
    let root = f.root.file_name().unwrap().to_str().unwrap().to_string();
    let (out, seqs) = output_until(&d, &id, &root).await;
    assert!(out.contains("42"), "{out:?}");
    assert!(seqs.windows(2).all(|p| p[1] == p[0] + 1), "{seqs:?}");
    d.client
        .request::<Resize>(ResizeParams {
            terminal_id: t.terminal_id.clone(),
            cols: 100,
            rows: 30,
        })
        .await
        .unwrap();
    d.client.request::<Write>(w("stty size\r")).await.unwrap();
    output_until(&d, &id, "30 100").await;
    // The backlog redraws a reconnecting client.
    let l = d
        .client
        .request::<List>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(l.terminals.len(), 1);
    assert!(l.terminals[0].backlog.contains("42") && l.terminals[0].seq >= 2);
    d.client.request::<Write>(w("exit 3\r")).await.unwrap();
    let ex = d
        .next_notification("terminal/exited", Duration::from_secs(20))
        .await;
    assert_eq!(
        (ex["terminalId"].as_str(), ex["code"].as_i64()),
        (Some(id.as_str()), Some(3))
    );
    // A zero size is refused; an unknown terminal is not found.
    let e = d
        .client
        .request::<Create>(CreateParams {
            workspace_id: f.id.clone(),
            argv: None,
            cols: 0,
            rows: 24,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
    d.client
        .request::<Close>(TerminalRef {
            terminal_id: t.terminal_id.clone(),
        })
        .await
        .unwrap();
    let e = d
        .client
        .request::<Close>(TerminalRef {
            terminal_id: t.terminal_id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
}

#[tokio::test]
async fn closing_the_workspace_ends_its_terminals() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let t = d
        .client
        .request::<Create>(CreateParams {
            workspace_id: f.id.clone(),
            argv: Some(vec!["sh".into(), "-c".into(), "echo up; sleep 1000".into()]),
            cols: 80,
            rows: 24,
        })
        .await
        .unwrap();
    output_until(&d, &t.terminal_id.0, "up").await;
    let pid = t.pid.unwrap() as i32;
    d.client
        .request::<axiom_protocol::workspace::Close>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    let alive = |p: i32| {
        std::path::Path::new(&format!("/proc/{p}")).exists()
            || cfg!(target_os = "macos")
                && std::process::Command::new("kill")
                    .args(["-0", &p.to_string()])
                    .status()
                    .is_ok_and(|s| s.success())
    };
    let end = std::time::Instant::now() + Duration::from_secs(5);
    while alive(pid) && std::time::Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!alive(pid), "the shell {pid} survived the workspace");
}
