//! `lsp/*` (protocol 0.4, ADR-0018 §2; M4-LSP-001): the workspace's language servers from its
//! providers' `[[lsp]]` definitions, run in the workspace's environment, shared by every client.

use crate::app::{internal, App};
use crate::handlers::env::{process_environment, registry, workspace};
use axiom_lsp::{Definition, LanguageServer, Launch};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::lsp::{
    LspNotification, NotifyParams, RequestParams, RequestResult, ServerRef, ServerStateChanged,
    ServerStateParams, ServersResult,
};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId};
use axiom_rpc::RequestCtx;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;

/// Settings key: per-server `settings` (the answer to `workspace/configuration`) and
/// `initializationOptions`, keyed by server id.
pub const SETTINGS_KEY: &str = "lsp.servers";

/// The server definitions of the providers that apply to the workspace.
async fn definitions(app: &App, ws: &WorkspaceId) -> Result<(PathBuf, Vec<Definition>), RpcError> {
    let w = workspace(app, ws).await?;
    let root = w.0.clone();
    let r = registry(app, Some(w)).await?;
    let root2 = root.clone();
    let defs = tokio::task::spawn_blocking(move || {
        let mut out: Vec<Definition> = Vec::new();
        for (_, m) in r.active() {
            if m.lsp.is_empty() || !axiom_env::detect::applies(m, &root2) {
                continue;
            }
            for s in &m.lsp {
                if out.iter().any(|d| d.id == s.id) {
                    continue;
                }
                out.push(Definition {
                    id: s.id.clone(),
                    provider_id: m.id.clone(),
                    languages: s.languages.clone(),
                    command: s.command.clone(),
                    initialization_options: s.initialization_options.clone(),
                    install_hint: s.install_hint.clone(),
                });
            }
        }
        out
    })
    .await
    .map_err(|_| internal())?;
    Ok((root, defs))
}

/// How a server of this workspace runs: the project's environment over the daemon's `PATH`,
/// and its settings.
async fn launch(
    app: &App,
    ws: &WorkspaceId,
    root: PathBuf,
    server_id: &str,
) -> Result<Launch, RpcError> {
    let env = process_environment(app, ws).await?;
    let all = app.store_call(|s| s.setting_get(SETTINGS_KEY)).await?;
    let mine = all
        .as_ref()
        .and_then(|v| v.get(server_id))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(Launch {
        root,
        env,
        settings: mine.get("settings").cloned().unwrap_or_else(|| json!({})),
        initialization_options: mine.get("initializationOptions").cloned(),
    })
}

/// The workspace's server with this id, created (idle) on first use.
async fn server(
    app: &App,
    ws: &WorkspaceId,
    server_id: &str,
) -> Result<Arc<LanguageServer>, RpcError> {
    if let Some(s) = app.lsp.get(ws, server_id) {
        return Ok(s);
    }
    let (root, defs) = definitions(app, ws).await?;
    let def = defs
        .into_iter()
        .find(|d| d.id == server_id)
        .ok_or_else(|| {
            RpcError::new(
                ErrorKind::NotFound,
                format!("No language server \"{server_id}\" applies to this workspace."),
            )
        })?;
    let l = launch(app, ws, root, server_id).await?;
    Ok(app.lsp.server(ws, def, l))
}

pub async fn servers(app: Arc<App>, p: WorkspaceRef) -> Result<ServersResult, RpcError> {
    let (root, defs) = definitions(&app, &p.workspace_id).await?;
    let mut servers = Vec::new();
    for def in defs {
        let s = match app.lsp.get(&p.workspace_id, &def.id) {
            Some(s) => s,
            None => {
                let l = launch(&app, &p.workspace_id, root.clone(), &def.id).await?;
                app.lsp.server(&p.workspace_id, def, l)
            }
        };
        servers.push(s.info());
    }
    Ok(ServersResult { servers })
}

pub async fn request(
    app: Arc<App>,
    p: RequestParams,
    ctx: RequestCtx,
) -> Result<RequestResult, RpcError> {
    let s = server(&app, &p.workspace_id, &p.server_id).await?;
    let result = s.request(&p.method, p.params, &ctx.cancel).await?;
    // Locations outside the workspace may be opened read-only (ADR-0018 §2).
    let mut uris = Vec::new();
    axiom_lsp::collect_uris(&result, &mut uris);
    if !uris.is_empty() {
        let root = app.workspaces.get(&p.workspace_id)?.root().to_path_buf();
        app.allow_external_reads(&p.workspace_id, &root, &uris);
    }
    Ok(RequestResult { result })
}

pub async fn notify(app: Arc<App>, p: NotifyParams) -> Result<Empty, RpcError> {
    let s = server(&app, &p.workspace_id, &p.server_id).await?;
    s.notify(&p.method, p.params).await?;
    Ok(Empty {})
}

pub async fn restart(app: Arc<App>, p: ServerRef) -> Result<Empty, RpcError> {
    let s = server(&app, &p.workspace_id, &p.server_id).await?;
    // The environment or settings may have changed since it was created.
    let root = app.workspaces.get(&p.workspace_id)?.root().to_path_buf();
    s.set_launch(launch(&app, &p.workspace_id, root, &p.server_id).await?);
    s.restart().await;
    Ok(Empty {})
}

/// Relays server events to the workspace's clients, for the life of the daemon.
pub fn spawn_forwarder(app: &Arc<App>) {
    let Ok(rt) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let mut rx = app.lsp.subscribe();
    let weak = Arc::downgrade(app);
    rt.spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("{n} language-server events were dropped (slow clients)");
                    continue;
                }
                Err(_) => return,
            };
            let Some(app) = weak.upgrade() else { return };
            match ev {
                axiom_lsp::Event::Notification {
                    workspace_id,
                    server_id,
                    method,
                    params,
                } => {
                    let n = axiom_protocol::lsp::LspNotificationParams {
                        workspace_id: workspace_id.clone(),
                        server_id,
                        method,
                        params,
                    };
                    for c in app.clients_of(&workspace_id) {
                        let _ = c.notify::<LspNotification>(n.clone()).await;
                    }
                }
                axiom_lsp::Event::State { workspace_id, info } => {
                    let n = ServerStateParams {
                        workspace_id: workspace_id.clone(),
                        server: info,
                    };
                    for c in app.clients_of(&workspace_id) {
                        let _ = c.notify::<ServerStateChanged>(n.clone()).await;
                    }
                }
            }
        }
    });
}
