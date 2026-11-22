//! What the daemon started does not outlive it (AUD-027): its exit watchdog ends tasks and
//! terminals when the daemon is killed, and a shutdown waits for the terminals' escalation.
#![cfg(unix)]

mod common;

use axiom_protocol::task::{Command, Start, StartParams};
use axiom_protocol::terminal::{Create, CreateParams};
use common::Daemon;
use std::time::Duration;

fn alive(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

async fn dies_within(pid: i32, t: Duration) -> bool {
    let end = tokio::time::Instant::now() + t;
    while tokio::time::Instant::now() < end {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    !alive(pid)
}

async fn terminal_pid(d: &Daemon, ws: &axiom_protocol::WorkspaceId, script: &str) -> i32 {
    let t = d
        .client
        .request::<Create>(CreateParams {
            workspace_id: ws.clone(),
            argv: Some(vec!["sh".into(), "-c".into(), script.into()]),
            cols: 80,
            rows: 24,
        })
        .await
        .unwrap();
    t.pid.unwrap() as i32
}

#[tokio::test]
async fn a_killed_daemon_takes_its_tasks_and_terminals_with_it() {
    let mut d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let started = d
        .client
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            command: Command::Argv(vec![
                "sh".into(),
                "-c".into(),
                "echo $$; exec sleep 60".into(),
            ]),
            cwd: None,
            env: Default::default(),
            label: None,
        })
        .await
        .unwrap();
    let out = d
        .next_notification("task/output", Duration::from_secs(10))
        .await;
    assert_eq!(out["taskId"], started.task_id.0);
    let task_pid: i32 = out["data"].as_str().unwrap().trim().parse().unwrap();
    // Ignoring SIGHUP (kept across `exec`): the hang-up of the PTY's end alone does not end it.
    let term_pid = terminal_pid(&d, &f.id, "trap '' HUP; exec sleep 60").await;
    assert!(alive(task_pid) && alive(term_pid));

    d.child.start_kill().unwrap();
    let _ = d.child.wait().await;
    assert!(
        dies_within(task_pid, Duration::from_secs(3)).await,
        "the task {task_pid} outlived the daemon"
    );
    assert!(
        dies_within(term_pid, Duration::from_secs(3)).await,
        "the terminal {term_pid} outlived the daemon"
    );
}

#[tokio::test]
async fn shutdown_kills_a_terminal_that_ignores_the_hang_up() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let pid = terminal_pid(
        &d,
        &f.id,
        "trap '' HUP; echo ready; while :; do sleep 1; done",
    )
    .await;
    d.next_notification("terminal/output", Duration::from_secs(10))
        .await;
    d.shutdown().await;
    assert!(
        dies_within(pid, Duration::from_secs(3)).await,
        "the shell {pid} survived the daemon's shutdown"
    );
}
