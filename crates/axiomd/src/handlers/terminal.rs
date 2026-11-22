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
use axiom_term::{Spec, TermEvent};
use std::sync::Arc;

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
    if let Some(pid) = t.pid {
        axiom_exec::guard::watch_group(pid);
    }
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
                    // A client that missed output redraws from `terminal/list` (seq shows the gap).
                    tracing::warn!("{n} terminal events were dropped (slow clients)");
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
                    pid,
                } => {
                    if let Some(pid) = pid {
                        axiom_exec::guard::unwatch_group(pid);
                    }
                    let n = ExitedParams { terminal_id, code };
                    app.notify_clients::<Exited>(&workspace_id, n).await;
                }
            }
        }
    });
}
