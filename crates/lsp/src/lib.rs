//! Language servers as daemon children (ADR-0018 §2; M4-LSP-001).
//!
//! One [`LanguageServer`] per (workspace, server definition). The daemon owns the conversation
//! with it: its own request ids shared by every client, answers to the server's requests, a
//! mirror of the open documents, the diagnostics last published, and a bounded restart policy.
//! Clients reach it through the daemon (`lsp/request`, `lsp/notify`), never directly.

pub mod docs;
pub mod framing;
pub mod server;

pub use server::{client_capabilities, file_uri, which, Definition, LanguageServer, Launch};

use axiom_protocol::lsp::{ServerInfo, ServerState};
use axiom_protocol::WorkspaceId;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, thiserror::Error)]
pub enum LspError {
    /// The server cannot take requests (missing, failed, stopped, still starting).
    #[error("{}", .1.clone().unwrap_or_else(|| format!("The language server is {:?}.", .0)))]
    Unavailable(ServerState, Option<String>),
    /// The server answered with an error.
    #[error("{message}")]
    Server {
        code: i64,
        message: String,
        data: Option<Value>,
    },
    /// The process ended while the request was pending.
    #[error("{0}")]
    Exited(String),
    #[error("The request was cancelled.")]
    Cancelled,
}

/// The daemon's error for each failure (`ipc.md` §Protocol 0.4).
impl From<LspError> for axiom_protocol::RpcError {
    fn from(e: LspError) -> Self {
        match e {
            LspError::Unavailable(state, _) => axiom_protocol::RpcError::new(
                axiom_protocol::ErrorKind::ServerUnavailable,
                e.to_string(),
            )
            .with_details(serde_json::json!({ "state": state })),
            LspError::Server {
                code,
                message,
                data,
            } => {
                let mut d = serde_json::json!({ "code": code });
                if let Some(x) = data {
                    d["data"] = x;
                }
                axiom_protocol::RpcError::new(axiom_protocol::ErrorKind::LspError, message)
                    .with_details(d)
            }
            LspError::Exited(m) => {
                axiom_protocol::RpcError::new(axiom_protocol::ErrorKind::ServerUnavailable, m)
                    .with_details(serde_json::json!({ "state": "stopped" }))
                    .retryable()
            }
            LspError::Cancelled => axiom_protocol::RpcError::new(
                axiom_protocol::ErrorKind::Cancelled,
                "The request was cancelled.",
            ),
        }
    }
}

/// What servers send to the workspace's clients.
#[derive(Debug, Clone)]
pub enum Event {
    Notification {
        workspace_id: WorkspaceId,
        server_id: String,
        method: String,
        params: Value,
    },
    State {
        workspace_id: WorkspaceId,
        info: ServerInfo,
    },
}

/// Every language server of the daemon.
pub struct Manager {
    servers: Mutex<HashMap<(WorkspaceId, String), Arc<LanguageServer>>>,
    events: tokio::sync::broadcast::Sender<Event>,
}

impl Default for Manager {
    fn default() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            events: tokio::sync::broadcast::channel(4096).0,
        }
    }
}

impl Manager {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<(WorkspaceId, String), Arc<LanguageServer>>> {
        self.servers.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The workspace's server for `def`, created (idle) on first use. An existing one keeps
    /// running; its next start uses `launch`.
    pub fn server(&self, ws: &WorkspaceId, def: Definition, launch: Launch) -> Arc<LanguageServer> {
        let mut m = self.lock();
        let key = (ws.clone(), def.id.clone());
        if let Some(s) = m.get(&key) {
            s.set_launch(launch);
            return s.clone();
        }
        let s = LanguageServer::new(ws.clone(), def, launch, self.events.clone());
        m.insert(key, s.clone());
        s
    }

    /// The workspace's existing server with this id.
    pub fn get(&self, ws: &WorkspaceId, server_id: &str) -> Option<Arc<LanguageServer>> {
        self.lock()
            .get(&(ws.clone(), server_id.to_string()))
            .cloned()
    }

    /// Every server of every workspace.
    pub fn all(&self) -> Vec<Arc<LanguageServer>> {
        self.lock().values().cloned().collect()
    }

    /// The workspace's servers.
    pub fn of_workspace(&self, ws: &WorkspaceId) -> Vec<Arc<LanguageServer>> {
        let mut v: Vec<_> = self
            .lock()
            .iter()
            .filter(|((w, _), _)| w == ws)
            .map(|(_, s)| s.clone())
            .collect();
        v.sort_by(|a, b| a.def.id.cmp(&b.def.id));
        v
    }

    /// Stops and forgets the workspace's servers (the workspace closed). They are marked closed
    /// first, so a late `didClose` or request cannot start one again; they stop together.
    pub async fn close_workspace(&self, ws: &WorkspaceId) {
        let servers: Vec<_> = {
            let mut m = self.lock();
            let keys: Vec<_> = m.keys().filter(|(w, _)| w == ws).cloned().collect();
            keys.into_iter().filter_map(|k| m.remove(&k)).collect()
        };
        stop_together(servers, None).await;
    }

    /// Stops every server (daemon shutdown): together, and what has not stopped within
    /// [SHUTDOWN_BUDGET] is killed with its process group. The shell waits a few seconds for
    /// the daemon before killing it, which would leave the servers behind.
    pub async fn stop_all(&self) {
        let servers: Vec<_> = self.lock().drain().map(|(_, s)| s).collect();
        stop_together(servers, Some(SHUTDOWN_BUDGET)).await;
    }
}

/// How long the daemon's shutdown waits for its language servers to stop by themselves.
pub const SHUTDOWN_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

async fn stop_together(servers: Vec<Arc<LanguageServer>>, budget: Option<std::time::Duration>) {
    for s in &servers {
        s.close();
    }
    let mut set = tokio::task::JoinSet::new();
    for s in &servers {
        let s = s.clone();
        set.spawn(async move { s.stop().await });
    }
    let all = async { while set.join_next().await.is_some() {} };
    match budget {
        Some(b) => {
            if tokio::time::timeout(b, all).await.is_err() {
                for s in &servers {
                    s.kill_now();
                }
            }
        }
        None => all.await,
    }
}

/// Every `uri` / `targetUri` string in an LSP result (locations, location links, workspace
/// edits), for the daemon's out-of-workspace read allowance.
pub fn collect_uris(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(o) => {
            for (k, x) in o {
                match (k.as_str(), x) {
                    ("uri" | "targetUri", Value::String(s)) => out.push(s.clone()),
                    _ => collect_uris(x, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| collect_uris(x, out)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn uris_from_locations_and_links() {
        let mut out = Vec::new();
        collect_uris(
            &json!([{"uri":"file:///a","range":{}},{"targetUri":"file:///b","originSelectionRange":{}},{"changes":{"file:///c":[]}}]),
            &mut out,
        );
        assert_eq!(out, vec!["file:///a", "file:///b"]);
    }
}
