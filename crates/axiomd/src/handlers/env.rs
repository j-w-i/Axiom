use crate::app::{internal, App};
use axiom_env::{detect, HostPaths, Registry, Sources};
use axiom_protocol::env::{DetectResult, ProvidersParams, ProvidersResult, TrustParams};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{RpcError, WorkspaceId};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// The workspace's root (as stored) and its trust flag.
async fn workspace(app: &App, id: &WorkspaceId) -> Result<(PathBuf, bool), RpcError> {
    let root = app.workspaces.get(id)?.root().to_path_buf();
    let key = root.to_string_lossy().into_owned();
    let trusted = app.store_call(move |s| s.workspace_trusted(&key)).await?;
    Ok((root, trusted))
}

async fn registry(app: &App, ws: Option<(PathBuf, bool)>) -> Result<Registry, RpcError> {
    let sources = Sources {
        builtin: app.env.builtin.clone(),
        user_dir: Some(app.env.data_dir.join("providers")),
        workspace_dir: ws.as_ref().map(|(r, _)| r.join(".axiom").join("providers")),
        workspace_trusted: ws.is_some_and(|(_, t)| t),
    };
    tokio::task::spawn_blocking(move || Registry::load(&sources))
        .await
        .map_err(|_| internal())
}

pub async fn providers(app: Arc<App>, p: ProvidersParams) -> Result<ProvidersResult, RpcError> {
    let ws = match &p.workspace_id {
        Some(id) => Some(workspace(&app, id).await?),
        None => None,
    };
    let r = registry(&app, ws).await?;
    Ok(ProvidersResult {
        providers: r.infos(),
    })
}

pub async fn detect_tools(app: Arc<App>, p: WorkspaceRef) -> Result<DetectResult, RpcError> {
    let ws = workspace(&app, &p.workspace_id).await?;
    let root = ws.0.clone();
    let r = registry(&app, Some(ws)).await?;
    let host = HostPaths::from_env(app.env.data_dir.join("toolchains"));
    Ok(detect(&r, &root, &host).await)
}

pub async fn trust(app: Arc<App>, p: TrustParams) -> Result<Empty, RpcError> {
    let key = app
        .workspaces
        .get(&p.workspace_id)?
        .root()
        .to_string_lossy()
        .into_owned();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    app.store_call(move |s| s.set_workspace_trusted(&key, p.trusted, now))
        .await?;
    Ok(Empty {})
}
