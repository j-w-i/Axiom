//! The daemon's side of ACP terminals (ADR-0016; M3-ACP-001): a `terminal/create` starts a task
//! through the executor (gated as `acp.terminal`); its output is collected here, capped at the
//! agent's `outputByteLimit` (oldest bytes dropped), for `terminal/output|wait_for_exit|kill|release`.

use crate::app::App;
use axiom_exec::TaskEvent;
use axiom_protocol::TaskId;
use futures_util::future::BoxFuture;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, watch};

type Exit = (Option<i32>, Option<String>);

pub struct Terminal {
    task: TaskId,
    /// Whose it is: the session's principal and workspace, so it goes with them (AUD-099).
    principal: String,
    workspace: axiom_protocol::WorkspaceId,
    output: Mutex<(String, bool)>,
    exit: watch::Receiver<Option<Exit>>,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

impl Terminal {
    /// Registers a started task as a terminal and returns its id.
    pub fn start(
        app: &App,
        ctx: &axiom_agent::tools::ToolCtx,
        task: TaskId,
        mut rx: mpsc::Receiver<TaskEvent>,
        limit: usize,
    ) -> String {
        // Terminals of workspaces closed since: gone (their tasks ended with the workspace).
        app.acp_terminals
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|_, t| app.workspaces.get(&t.workspace).is_ok());
        let id = format!("term-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let (tx, exit) = watch::channel(None);
        let t = Arc::new(Terminal {
            task,
            principal: ctx.principal.clone(),
            workspace: ctx.workspace_id.clone(),
            output: Mutex::new((String::new(), false)),
            exit,
        });
        app.acp_terminals
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id.clone(), t.clone());
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                match ev {
                    TaskEvent::Output(o) => {
                        let mut g = t.output.lock().unwrap_or_else(|p| p.into_inner());
                        g.0.push_str(&o.data);
                        if g.0.len() > limit {
                            let mut cut = g.0.len() - limit;
                            while !g.0.is_char_boundary(cut) {
                                cut += 1;
                            }
                            g.0.drain(..cut);
                            g.1 = true;
                        }
                    }
                    TaskEvent::Exited(x) => {
                        let signal = (x.status.exit_code.is_none()).then(|| "SIGKILL".to_string());
                        let _ = tx.send(Some((x.status.exit_code, signal)));
                    }
                }
            }
        });
        id
    }
}

/// Drops the terminals a session's agent created (the session was deleted), stopping those
/// still running (AUD-099: the table was never cleared).
pub async fn drop_session(app: &App, principal: &str) {
    let gone: Vec<Arc<Terminal>> = {
        let mut m = app.acp_terminals.lock().unwrap_or_else(|p| p.into_inner());
        let ids: Vec<String> = m
            .iter()
            .filter(|(_, t)| t.principal == principal)
            .map(|(id, _)| id.clone())
            .collect();
        ids.iter().filter_map(|id| m.remove(id)).collect()
    };
    for t in gone {
        if t.exit.borrow().is_none() {
            let _ = app.executor.cancel(&t.task).await;
        }
    }
}

/// `axiom_agent::acp::Terminals` over the daemon's terminal table.
pub struct Terminals(pub Arc<App>);

impl Terminals {
    fn get(&self, id: &str) -> Result<Arc<Terminal>, String> {
        self.0
            .acp_terminals
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(id)
            .cloned()
            .ok_or_else(|| format!("no terminal `{id}`"))
    }
}

impl axiom_agent::acp::Terminals for Terminals {
    fn output<'a>(
        &'a self,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(String, bool, Option<Exit>), String>> {
        Box::pin(async move {
            let t = self.get(id)?;
            let (out, truncated) = t.output.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let exit = t.exit.borrow().clone();
            Ok((out, truncated, exit))
        })
    }

    fn wait<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Exit, String>> {
        Box::pin(async move {
            let t = self.get(id)?;
            let mut rx = t.exit.clone();
            let r = rx
                .wait_for(Option::is_some)
                .await
                .map_err(|_| "the terminal is gone".to_string())?
                .clone()
                .unwrap_or((None, None));
            Ok(r)
        })
    }

    fn kill<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let t = self.get(id)?;
            let _ = self.0.executor.cancel(&t.task).await;
            Ok(())
        })
    }

    fn release<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let t = self.get(id)?;
            if t.exit.borrow().is_none() {
                let _ = self.0.executor.cancel(&t.task).await;
            }
            self.0
                .acp_terminals
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(id);
            Ok(())
        })
    }
}
