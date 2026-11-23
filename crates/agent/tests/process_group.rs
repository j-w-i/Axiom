//! Stopping an external agent ends what it started (review D2): its process group is killed,
//! not only the process Axiom started.
#![cfg(unix)]

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

/// The groups the watchdog hooks were told about: `(started, group)`.
static HOOKED: Mutex<Vec<(bool, u32)>> = Mutex::new(Vec::new());

fn hook_started(g: u32) {
    HOOKED.lock().unwrap().push((true, g));
}

fn hook_ended(g: u32) {
    HOOKED.lock().unwrap().push((false, g));
}

fn watched(g: u32) -> bool {
    let h = HOOKED.lock().unwrap();
    h.contains(&(true, g)) && !h.contains(&(false, g))
}

fn alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

/// The pid the agent's script wrote to `file`.
async fn pid_in(file: &Path) -> i32 {
    for _ in 0..500 {
        if let Some(p) = std::fs::read_to_string(file)
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
        {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the agent never started its child");
}

/// Waits up to five seconds for `pid` to end; whether it did. Kills it either way.
async fn ends(pid: i32) -> bool {
    let mut gone = false;
    for _ in 0..250 {
        if !alive(pid) {
            gone = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status();
    gone
}

fn connect(script: String, dir: &Path) -> axiom_agent::acp::Connection {
    axiom_agent::process::set_group_hooks(hook_started, hook_ended);
    let argv = vec!["sh".to_string(), "-c".to_string(), script];
    axiom_agent::acp::Connection::spawn(&argv, &Default::default(), &[], dir).unwrap()
}

/// Session delete, idle reap and an MCP restart kill the agent: a child it started in the
/// background goes with it.
#[tokio::test]
async fn killing_an_agent_kills_what_it_started() {
    let d = tempfile::tempdir().unwrap();
    let pid_file = d.path().join("pid");
    let conn = connect(
        format!("sleep 600 & echo $! > '{}'; exec cat", pid_file.display()),
        d.path(),
    );
    let pid = pid_in(&pid_file).await;
    conn.kill().await;
    drop(conn);
    assert!(ends(pid).await, "the agent's child {pid} survived its kill");
    assert!(HOOKED.lock().unwrap().iter().any(|(s, _)| *s));
}

/// An agent that exits by itself, leaving a child in its group: the group stays registered
/// with the exit watchdog while the child runs, and the child is ended when the handle goes
/// (the session is deleted, the agent is started again).
#[tokio::test]
async fn an_agent_that_exited_leaves_nothing_behind_once_dropped() {
    let d = tempfile::tempdir().unwrap();
    let (pid_file, group_file) = (d.path().join("pid"), d.path().join("group"));
    let conn = connect(
        format!(
            "sleep 600 </dev/null >/dev/null 2>&1 & echo $! > '{}'; echo $$ > '{}'; exit 0",
            pid_file.display(),
            group_file.display()
        ),
        d.path(),
    );
    let pid = pid_in(&pid_file).await;
    let group = pid_in(&group_file).await as u32;
    // The leader has exited (and been reaped) by now; its child still runs in its group.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(alive(pid));
    assert!(watched(group), "unregistered while its group still runs");
    drop(conn);
    assert!(ends(pid).await, "the exited agent's child {pid} survived");
    for _ in 0..250 {
        if !watched(group) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the group was never unregistered");
}
