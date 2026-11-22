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
use std::collections::BTreeMap;
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
        // M5.5 (ADR-0021 §3): every active provider's servers, whether or not it detects the
        // workspace; the shell starts one when a document of its language opens.
        let _ = &root2;
        // The more specific source first (workspace, user, built-in; each by id): a server the
        // person added claims its languages before a built-in one (ADR-0021 §3).
        let mut active: Vec<_> = r.active().collect();
        active.sort_by_key(|(e, m)| {
            let rank = match e.info.source {
                axiom_protocol::env::ProviderSource::Workspace => 0,
                axiom_protocol::env::ProviderSource::User => 1,
                axiom_protocol::env::ProviderSource::Builtin => 2,
            };
            (rank, m.id.clone())
        });
        for (_, m) in active {
            if m.lsp.is_empty() {
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
                    requires: s.requires.clone(),
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
    def: &Definition,
) -> Result<Launch, RpcError> {
    let base = LaunchBase::read(app, ws).await?;
    Ok(launch_in(app, &base, root, def))
}

/// What every server of a workspace launches from, read once per listing (M5.5: with every
/// provider's servers listed, reading it per server slowed opening a folder).
struct LaunchBase {
    env: BTreeMap<String, String>,
    registry: axiom_env::registry::Registry,
    settings: Option<Value>,
}

impl LaunchBase {
    async fn read(app: &App, ws: &WorkspaceId) -> Result<Self, RpcError> {
        Ok(Self {
            env: process_environment(app, ws).await?,
            registry: registry(app, Some(workspace(app, ws).await?)).await?,
            settings: app.store_call(|s| s.setting_get(SETTINGS_KEY)).await?,
        })
    }
}

fn launch_in(app: &App, base: &LaunchBase, root: PathBuf, def: &Definition) -> Launch {
    let server_id = def.id.as_str();
    let mut env = base.env.clone();
    // M5.5 (ADR-0021 §4, §6): not in the workspace's environment → the newest managed install of
    // the server's provider; each required tool likewise from its provider's install.
    let r = &base.registry;
    let installer = app.env.installer.clone();
    let overlay = |provider: &str, env: &mut BTreeMap<String, String>| -> bool {
        let Some(m) = r.get(provider) else {
            return false;
        };
        let Some(version) = installer.installed(provider).into_iter().next() else {
            return false;
        };
        let at = installer.toolchains_dir().join(provider).join(&version);
        let (dirs, vars) = axiom_env::project::managed_overlay(m, &at, &version);
        let path = env.get("PATH").cloned().unwrap_or_default();
        let mut parts = dirs;
        if !path.is_empty() {
            parts.push(path);
        }
        env.insert(
            "PATH".into(),
            parts.join(if cfg!(windows) { ";" } else { ":" }),
        );
        for (k, v) in vars {
            env.entry(k).or_insert(v);
        }
        true
    };
    // The server's own managed install wins over PATH: a person who installed it through Axiom
    // gets that one (rustup's `rust-analyzer` stub on PATH runs nothing without the component).
    overlay(&def.provider_id, &mut env);
    let mut blocked = None;
    for tool in &def.requires {
        // Another provider's supply first: the server's own may list the tool only to run its
        // install (lsp-csharp's `dotnet tool install`), and installs nothing of it itself.
        let supplies =
            |m: &axiom_protocol::provider::ProviderManifest| m.tools.iter().any(|t| &t.id == tool);
        let supplier = r
            .active()
            .find(|(_, m)| m.id != def.provider_id && supplies(m))
            .or_else(|| r.active().find(|(_, m)| supplies(m)))
            .map(|(_, m)| {
                (
                    m.id.clone(),
                    m.name.clone(),
                    m.tools
                        .iter()
                        .find(|t| &t.id == tool)
                        .map(|t| t.program.clone())
                        .unwrap_or_default(),
                )
            });
        let Some((pid, pname, prog)) = supplier else {
            blocked = Some((
                format!("{server_id} needs {tool}, which no provider supplies."),
                def.provider_id.clone(),
            ));
            break;
        };
        if axiom_lsp::server::which(&prog, &env).is_none() && !overlay(&pid, &mut env) {
            blocked = Some((
                format!("{server_id} needs {pname} ({tool}), which is not installed."),
                pid,
            ));
            break;
        }
    }
    let mine = base
        .settings
        .as_ref()
        .and_then(|v| v.get(server_id))
        .cloned()
        .unwrap_or(Value::Null);
    Launch {
        root,
        env,
        settings: mine.get("settings").cloned().unwrap_or_else(|| json!({})),
        initialization_options: mine.get("initializationOptions").cloned(),
        blocked,
        install_provider: Some(def.provider_id.clone()),
    }
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
    let l = launch(app, ws, root, &def).await?;
    Ok(app.lsp.server(ws, def, l))
}

pub async fn servers(app: Arc<App>, p: WorkspaceRef) -> Result<ServersResult, RpcError> {
    let (root, defs) = definitions(&app, &p.workspace_id).await?;
    let mut servers = Vec::new();
    // The first server (registry order) to claim a language serves it; a server whose every
    // language is already claimed is on standby (ADR-0021 §3).
    let mut claimed: std::collections::BTreeSet<String> = Default::default();
    let mut base: Option<LaunchBase> = None;
    for def in defs {
        let standby =
            !def.languages.is_empty() && def.languages.iter().all(|l| claimed.contains(l));
        claimed.extend(def.languages.iter().cloned());
        let s = match app.lsp.get(&p.workspace_id, &def.id) {
            Some(s) => s,
            None => {
                if base.is_none() {
                    base = Some(LaunchBase::read(&app, &p.workspace_id).await?);
                }
                let Some(b) = base.as_ref() else {
                    return Err(internal());
                };
                let l = launch_in(&app, b, root.clone(), &def);
                app.lsp.server(&p.workspace_id, def, l)
            }
        };
        s.set_standby(standby);
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
    s.set_launch(launch(&app, &p.workspace_id, root, &s.def).await?);
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
                    // What was dropped may have been a state or diagnostics, which are not sent
                    // again by themselves: send every server's current ones (audit finding 13).
                    if let Some(app) = weak.upgrade() {
                        resync(&app).await;
                    }
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

/// Sends every server's state, and the diagnostics of each document it has them for or has open
/// (empty when none, so ones cleared meanwhile go), to its workspace's clients.
async fn resync(app: &Arc<App>) {
    for s in app.lsp.all() {
        let ws = s.workspace_id().clone();
        let state = ServerStateParams {
            workspace_id: ws.clone(),
            server: s.info(),
        };
        let diagnostics = s.diagnostics();
        let mut uris: std::collections::BTreeSet<String> = diagnostics.keys().cloned().collect();
        uris.extend(s.open_uris());
        for c in app.clients_of(&ws) {
            let _ = c.notify::<ServerStateChanged>(state.clone()).await;
            for uri in &uris {
                let list = diagnostics.get(uri).cloned().unwrap_or_else(|| json!([]));
                let _ = c
                    .notify::<LspNotification>(axiom_protocol::lsp::LspNotificationParams {
                        workspace_id: ws.clone(),
                        server_id: s.def.id.clone(),
                        method: "textDocument/publishDiagnostics".into(),
                        params: json!({"uri": uri, "diagnostics": list}),
                    })
                    .await;
            }
        }
    }
}
