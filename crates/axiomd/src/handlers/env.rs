use crate::app::{internal, App};
use axiom_env::{detect, HostPaths, Registry, Sources};
use axiom_protocol::env::{
    DetectResult, InstallParams, InstallPlan, InstallResult, Installed, PlanInstallParams,
    Progress, ProvidersParams, ProvidersResult, TrustParams, UninstallParams, VersionsParams,
    VersionsResult,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::provider::ProviderManifest;
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::ErrorKind;
use axiom_protocol::{RpcError, WorkspaceId};
use axiom_rpc::RequestCtx;
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

/// The effective built-in or user provider with this id (installs use no workspace providers).
async fn provider(app: &App, id: &str) -> Result<Option<ProviderManifest>, RpcError> {
    Ok(registry(app, None).await?.get(id).cloned())
}

async fn known_provider(app: &App, id: &str) -> Result<ProviderManifest, RpcError> {
    provider(app, id).await?.ok_or_else(|| {
        RpcError::new(
            ErrorKind::NotFound,
            format!("There is no provider \"{id}\"."),
        )
    })
}

pub async fn versions(app: Arc<App>, p: VersionsParams) -> Result<VersionsResult, RpcError> {
    let m = known_provider(&app, &p.provider_id).await?;
    Ok(app.env.installer.versions(&m).await?)
}

pub async fn plan_install(app: Arc<App>, p: PlanInstallParams) -> Result<InstallPlan, RpcError> {
    let m = known_provider(&app, &p.provider_id).await?;
    Ok(app.env.installer.plan(&m, &p.version, p.method).await?)
}

/// Runs the install as its own task: when the request is cancelled the router stops waiting
/// after a short grace period, but the install still sees the cancellation, removes its
/// staging directory and partial download, and only then ends.
pub async fn install(
    app: Arc<App>,
    p: InstallParams,
    ctx: RequestCtx,
) -> Result<InstallResult, RpcError> {
    let current = match app.env.installer.plan_provider(&p.plan_id) {
        Some(id) => provider(&app, &id).await?,
        None => None,
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let notifier = ctx.notifier.clone();
    let forward = tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = notifier.notify::<Progress>(progress).await;
        }
    });
    let installer = app.env.installer.clone();
    let cancel = ctx.cancel.clone();
    let result = tokio::spawn(async move {
        let report = move |progress| {
            let _ = tx.send(progress);
        };
        installer
            .install(&p.plan_id, current.as_ref(), &cancel, &report)
            .await
    })
    .await
    .map_err(|_| internal())?;
    // Every progress notification is queued before `env/installed` and the response.
    let _ = forward.await;
    let r = result?;
    let _ = ctx.notifier.notify::<Installed>(r.clone()).await;
    Ok(r)
}

pub async fn uninstall(app: Arc<App>, p: UninstallParams) -> Result<Empty, RpcError> {
    let installer = app.env.installer.clone();
    tokio::task::spawn_blocking(move || installer.uninstall(&p.provider_id, &p.version))
        .await
        .map_err(|_| internal())??;
    Ok(Empty {})
}
