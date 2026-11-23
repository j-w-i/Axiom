//! The processes the agent runtime starts (external agents over ACP, MCP servers). Each leads a
//! process group of its own (Unix), registered with the daemon's exit watchdog from its start
//! until it has been reaped (AUD-027, review N13): a daemon killed with SIGKILL, or one that
//! crashed, does not leave them, or what they started, behind.
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
    let (kill_tx, kill_rx) = oneshot::channel::<()>();
    let (exit_tx, exited) = watch::channel(false);
    tokio::spawn(async move {
        // Killed when asked, or when the handle is dropped (the sender goes with it).
        let killed = tokio::select! {
            _ = child.wait() => false,
            _ = kill_rx => true,
        };
        if killed {
            let _ = child.kill().await;
        }
        // Reaped: the number may be reused from now on, so the watchdog forgets it now.
        if let (Some(h), Some(g)) = (HOOKS.get(), group) {
            (h.1)(g);
        }
        let _ = exit_tx.send(true);
    });
    Ok((
        Supervised {
            kill: Mutex::new(Some(kill_tx)),
            exited,
        },
        pipes,
    ))
}

impl Supervised {
    /// Kills the process (if it still runs) and waits until it has been reaped.
    pub(crate) async fn kill(&self) {
        let tx = self.kill.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(tx) = tx {
            let _ = tx.send(());
        }
        let mut rx = self.exited.clone();
        let _ = rx.wait_for(|e| *e).await;
    }
}
