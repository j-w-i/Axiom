//! `terminal/*` (protocol 0.4, ADR-0018 §4; M4-TERM-001): the person's shell in a PTY, in the
//! workspace root with the project's environment. Not an agent tool; no gate.

use crate::app::App;
use crate::handlers::env::process_environment;
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::terminal::{
    CreateParams, Exited, ExitedParams, ListResult, Output, OutputParams, ResizeParams,
    TerminalInfo, TerminalRef, WriteParams,
};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::RpcError;
use axiom_term::{Spec, TermEvent, Terminal};
use std::sync::{Arc, Mutex, Weak};

/// The terminals registered with the exit watchdog (AUD-027), with their process groups. A group
/// is unregistered once its terminal's process has been reaped, whether or not the `Exited`
/// event reached the forwarder (a lagging forwarder drops events): a group left registered
/// after its number was reused would be someone else's when the daemon dies (review N13).
static WATCHED: Mutex<Vec<(Weak<Terminal>, u32)>> = Mutex::new(Vec::new());

fn watched() -> std::sync::MutexGuard<'static, Vec<(Weak<Terminal>, u32)>> {
    WATCHED.lock().unwrap_or_else(|p| p.into_inner())
}

/// Registers a new terminal's process group with the watchdog.
fn watch(t: &Arc<Terminal>) {
    if let Some(pid) = t.pid {
        watched().push((Arc::downgrade(t), pid));
        axiom_exec::guard::watch_group(pid);
    }
    // It may have ended, and its `Exited` been handled, before it was registered.
    unwatch_ended();
}

/// Unregisters every terminal whose process has been reaped (`running` is cleared after the
/// wait) or that is gone.
fn unwatch_ended() {
    watched().retain(|(t, pid)| {
        let running = t.upgrade().is_some_and(|t| t.is_running());
        if !running {
            axiom_exec::guard::unwatch_group(*pid);
        }
        running
    });
}

pub async fn create(app: Arc<App>, p: CreateParams) -> Result<TerminalInfo, RpcError> {
    let root = app.workspaces.get(&p.workspace_id)?.root().to_path_buf();
    let env = process_environment(&app, &p.workspace_id).await?;
    let spec = Spec {
        workspace_id: p.workspace_id,
        argv: p.argv,
        cwd: root,
        env,
        cols: p.cols,
        rows: p.rows,
    };
    let terms = app.clone();
    let t = tokio::task::spawn_blocking(move || terms.terminals.create(spec))
        .await
        .map_err(|_| crate::app::internal())??;
    // Ended by the watchdog if the daemon dies without closing it (AUD-027).
    watch(&t);
    Ok(t.info())
}

pub async fn write(app: Arc<App>, p: WriteParams) -> Result<Empty, RpcError> {
    let t = app.terminals.get(&p.terminal_id)?;
    // A full PTY buffer blocks the write until the shell reads: off the async threads.
    tokio::task::spawn_blocking(move || t.write(&p.data))
        .await
        .map_err(|_| crate::app::internal())??;
    Ok(Empty {})
}

pub async fn resize(app: Arc<App>, p: ResizeParams) -> Result<Empty, RpcError> {
    app.terminals.get(&p.terminal_id)?.resize(p.cols, p.rows)?;
    Ok(Empty {})
}

pub async fn close(app: Arc<App>, p: TerminalRef) -> Result<Empty, RpcError> {
    app.terminals.close(&p.terminal_id)?;
    Ok(Empty {})
}

pub async fn list(app: Arc<App>, p: WorkspaceRef) -> Result<ListResult, RpcError> {
    app.workspaces.get(&p.workspace_id)?;
    Ok(ListResult {
        terminals: app
            .terminals
            .of_workspace(&p.workspace_id)
            .iter()
            .map(|t| t.listed())
            .collect(),
    })
}

/// The forwarder missed `n` events.
fn lagged(n: u64) {
    // A client that missed output redraws from `terminal/list` (seq shows the gap).
    tracing::warn!("{n} terminal events were dropped (slow clients)");
    // A dropped `Exited` must not leave its group registered.
    unwatch_ended();
}

/// Relays terminal events to the workspace's clients, for the life of the daemon.
pub fn spawn_forwarder(app: &Arc<App>) {
    let Ok(rt) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let mut rx = app.terminals.subscribe();
    let weak = Arc::downgrade(app);
    rt.spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    lagged(n);
                    continue;
                }
                Err(_) => return,
            };
            let Some(app) = weak.upgrade() else { return };
            match ev {
                TermEvent::Output {
                    workspace_id,
                    terminal_id,
                    seq,
                    data,
                    permit,
                } => {
                    let n = OutputParams {
                        terminal_id,
                        seq,
                        data,
                    };
                    // The permit is held until the output is queued for every connection: a
                    // terminal producing faster than its readers take waits (AUD-036); a
                    // connection that does not read does not hold up the others (AUD-030).
                    app.notify_clients::<Output>(&workspace_id, n).await;
                    drop(permit);
                }
                TermEvent::Exited {
                    workspace_id,
                    terminal_id,
                    code,
                    pid: _,
                } => {
                    unwatch_ended();
                    let n = ExitedParams { terminal_id, code };
                    app.notify_clients::<Exited>(&workspace_id, n).await;
                }
            }
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// A terminal whose `Exited` event never reached the forwarder (dropped while it lagged;
    /// here, no forwarder runs at all) is unregistered from the exit watchdog once its process
    /// has ended (review N13).
    #[tokio::test]
    async fn a_terminal_whose_exit_was_missed_is_unwatched() {
        let data = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let app = App::with_data_dir(
            axiom_store::Store::open_in_memory().unwrap(),
            data.path().into(),
        );
        let ws = app
            .workspaces
            .open(dir.path().to_str().unwrap())
            .await
            .unwrap()
            .workspace_id;
        let info = create(
            app.clone(),
            CreateParams {
                workspace_id: ws,
                argv: Some(vec!["sh".into(), "-c".into(), "sleep 0.2".into()]),
                cols: 80,
                rows: 24,
            },
        )
        .await
        .unwrap();
        let pid = info.pid.unwrap();
        assert!(axiom_exec::guard::is_watched(pid));
        let t = app.terminals.get(&info.terminal_id).unwrap();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while t.is_running() {
            assert!(std::time::Instant::now() < end, "the terminal did not end");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        // What the forwarder does when it lagged.
        lagged(1);
        assert!(!axiom_exec::guard::is_watched(pid));
    }
}
