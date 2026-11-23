//! The processes the agent runtime starts (external agents over ACP, MCP servers). Each leads a
//! process group of its own (Unix), registered with the daemon's exit watchdog from its start
//! until the group is empty or has been killed (AUD-027, review N13): a daemon killed with
//! SIGKILL, or one that crashed, does not leave them, or what they started, behind. Killing one
//! (session delete, idle reap, an MCP restart, dropping its handle) kills its whole group
//! (review D2).
//!
//! This crate does not depend on `exec`: the daemon passes the watchdog's functions in
//! ([`set_group_hooks`]), as it does for language servers.

use std::sync::{Mutex, OnceLock};
use tokio::process::{ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, watch};

/// Called with a process group when its leader starts, and once the leader has been reaped.
pub type GroupHook = fn(u32);

static HOOKS: OnceLock<(GroupHook, GroupHook)> = OnceLock::new();

/// Lets the daemon register each agent's and MCP server's process group with its exit watchdog
/// (`axiom_exec::guard`). The first call wins; without one, nothing is registered.
pub fn set_group_hooks(started: GroupHook, ended: GroupHook) {
    let _ = HOOKS.set((started, ended));
}

/// A started process, waited for by a task of its own. Dropping it kills the process.
pub(crate) struct Supervised {
    kill: Mutex<Option<oneshot::Sender<()>>>,
    exited: watch::Receiver<bool>,
}

/// The pipes of a [`spawn`]ed process that the caller configured.
pub(crate) struct Pipes {
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
}

/// Starts `cmd` (configured by the caller) as the leader of a new process group and
/// supervises it.
pub(crate) fn spawn(cmd: &mut Command) -> std::io::Result<(Supervised, Pipes)> {
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.kill_on_drop(true).spawn()?;
    let pipes = Pipes {
        stdin: child.stdin.take(),
        stdout: child.stdout.take(),
    };
    // Registered while the leader cannot have been reaped: its number is still its own.
    let group = if cfg!(unix) { child.id() } else { None };
    if let (Some(h), Some(g)) = (HOOKS.get(), group) {
        (h.0)(g);
    }
    let (kill_tx, mut kill_rx) = oneshot::channel::<()>();
    let (exit_tx, exited) = watch::channel(false);
    tokio::spawn(async move {
        // Killed when asked, or when the handle is dropped (the sender goes with it).
        let killed = tokio::select! {
            _ = child.wait() => false,
            _ = &mut kill_rx => true,
        };
        if killed {
            // The whole group, not the leader alone: what the agent or server started goes too
            // (review D2). Sent while the leader is not reaped yet, so the number is still the
            // group's.
            if let Some(g) = group {
                kill_group(g);
            }
            let _ = child.kill().await;
        }
        let _ = exit_tx.send(true);
        if let (false, Some(g)) = (killed, group) {
            // The leader exited by itself, and what it started in its group may still run. The
            // group stays registered with the watchdog until it is empty (its number is not
            // reused while it has a member), and is killed with the handle.
            while group_alive(g) {
                tokio::select! {
                    _ = tokio::time::sleep(GROUP_POLL) => {}
                    _ = &mut kill_rx => {
                        kill_group(g);
                        break;
                    }
                }
            }
        }
        // Empty, or after a final kill of the whole group: the watchdog forgets it now.
        if let (Some(h), Some(g)) = (HOOKS.get(), group) {
            (h.1)(g);
        }
    });
    Ok((
        Supervised {
            kill: Mutex::new(Some(kill_tx)),
            exited,
        },
        pipes,
    ))
}

/// How often a group whose leader exited is checked for remaining members.
const GROUP_POLL: std::time::Duration = std::time::Duration::from_millis(500);

/// SIGKILL to every process of the group (one already gone is fine).
#[cfg(unix)]
fn kill_group(g: u32) {
    use nix::sys::signal::{killpg, Signal};
    let _ = killpg(nix::unistd::Pid::from_raw(g as i32), Signal::SIGKILL);
}

#[cfg(not(unix))]
fn kill_group(_g: u32) {}

/// Whether the group still has a member.
#[cfg(unix)]
fn group_alive(g: u32) -> bool {
    nix::sys::signal::killpg(nix::unistd::Pid::from_raw(g as i32), None).is_ok()
}

#[cfg(not(unix))]
fn group_alive(_g: u32) -> bool {
    false
}

impl Supervised {
    /// Kills the process (if it still runs) and its process group, and waits until the process
    /// has been reaped.
    pub(crate) async fn kill(&self) {
        let tx = self.kill.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(tx) = tx {
            let _ = tx.send(());
        }
        let mut rx = self.exited.clone();
        let _ = rx.wait_for(|e| *e).await;
    }
}
