use crate::app::{internal, App};
use axiom_env::project::{self, ProjectFile};
use axiom_env::{detect, HostPaths, Registry, Sources};
use axiom_protocol::env::{
    DetectResult, Environment, InstallParams, InstallPlan, InstallResult, Installed,
    PlanInstallParams, Progress, ProvidersParams, ProvidersResult, TrustParams, UninstallParams,
    VersionsParams, VersionsResult,
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
pub(crate) async fn workspace(app: &App, id: &WorkspaceId) -> Result<(PathBuf, bool), RpcError> {
    let root = app.workspaces.get(id)?.root().to_path_buf();
    let key = root.to_string_lossy().into_owned();
    let trusted = app.store_call(move |s| s.workspace_trusted(&key)).await?;
    Ok((root, trusted))
}

pub(crate) async fn registry(app: &App, ws: Option<(PathBuf, bool)>) -> Result<Registry, RpcError> {
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

/// The workspace's environment from `.axiom/project.toml`, or the empty one without a project
/// file. Shared by `build/run` and the agent's `task.run`/`build.run` tools.
pub(crate) async fn project_environment(
    app: &App,
    id: &WorkspaceId,
    r: &Registry,
) -> Result<Environment, RpcError> {
    Ok(match project_file(app, id).await? {
        Some(selection) => {
            let path = std::env::var_os("PATH");
            project::resolve(
                r,
                &selection,
                &app.env.data_dir.join("toolchains"),
                path.as_deref(),
            )
            .await?
            .0
        }
        None => Environment::default(),
    })
}

/// The complete environment of a process the person runs in the workspace (a language server, a
/// terminal): the daemon's own with the project's on top, as builds get it.
pub(crate) async fn process_environment(
    app: &App,
    id: &WorkspaceId,
) -> Result<std::collections::BTreeMap<String, String>, RpcError> {
    let w = workspace(app, id).await?;
    let r = registry(app, Some(w)).await?;
    let environment = project_environment(app, id, &r).await?;
    let mut env = utf8_environment(std::env::vars_os());
    env.extend(axiom_env::project::process_env(
        &environment,
        std::env::var_os("PATH"),
    ));
    Ok(env)
}

/// The daemon's environment as UTF-8 pairs. `std::env::vars()` panics on a variable that is not
/// UTF-8 (a path in a legacy encoding), which broke every terminal, language server and preview
/// (AUD-037); such a variable is left out of the map (a task still inherits it: its spawn adds
/// to the inherited environment) and logged once.
pub(crate) fn utf8_environment(
    vars: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> std::collections::BTreeMap<String, String> {
    static WARNED: std::sync::Once = std::sync::Once::new();
    let mut skipped = Vec::new();
    let env = vars
        .filter_map(|(k, v)| match (k.into_string(), v.into_string()) {
            (Ok(k), Ok(v)) => Some((k, v)),
            (k, _) => {
                skipped.push(k.unwrap_or_else(|k| k.to_string_lossy().into_owned()));
                None
            }
        })
        .collect();
    if !skipped.is_empty() {
        WARNED.call_once(|| {
            tracing::warn!(
                "environment variables that are not UTF-8 are not passed on: {skipped:?}"
            )
        });
    }
    env
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
    let mut d = detect(&r, &root, &host).await;
    // What `.axiom/project.toml` selects; a malformed file is reported by `env/resolve`.
    if let Ok(Some(selection)) = project_file(&app, &p.workspace_id).await {
        for pd in &mut d.providers {
            pd.selected_version = selection.environment.get(&pd.provider_id).cloned();
        }
    }
    Ok(d)
}

/// `.axiom/project.toml`, read through the workspace (containment); `None` if absent.
pub(crate) async fn project_file(
    app: &App,
    id: &WorkspaceId,
) -> Result<Option<ProjectFile>, RpcError> {
    let ws = app.workspaces.get(id)?;
    let path = axiom_protocol::WorkspacePath(project::PROJECT_FILE.into());
    match ws.read_text(&path).await {
        Ok(f) => Ok(Some(project::parse(&f.text)?)),
        Err(axiom_workspace::WsError::NotFound(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// `env/resolve`. Without a project file the environment is empty. The lock is written
/// (atomically, inside the workspace) only when its content changes.
pub async fn resolve(app: Arc<App>, p: WorkspaceRef) -> Result<Environment, RpcError> {
    let ws = workspace(&app, &p.workspace_id).await?;
    let Some(selection) = project_file(&app, &p.workspace_id).await? else {
        return Ok(Environment::default());
    };
    let r = registry(&app, Some(ws)).await?;
    let path = std::env::var_os("PATH");
    let (env, lock) = project::resolve(
        &r,
        &selection,
        &app.env.data_dir.join("toolchains"),
        path.as_deref(),
    )
    .await?;
    let text = lock.to_text();
    let w = app.workspaces.get(&p.workspace_id)?;
    let lock_path = axiom_protocol::WorkspacePath(project::LOCK_FILE.into());
    let current = w.read_text(&lock_path).await.ok().map(|f| f.text);
    if current.as_deref() != Some(text.as_str()) {
        w.write_text(&lock_path, &text, None, true).await?;
    }
    Ok(env)
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
pub(crate) async fn provider(app: &App, id: &str) -> Result<Option<ProviderManifest>, RpcError> {
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
    // The manifest, if the provider still exists, supplies a delegate's uninstall command.
    let m = provider(&app, &p.provider_id).await?;
    app.env
        .installer
        .uninstall(&p.provider_id, &p.version, m.as_ref())
        .await?;
    Ok(Empty {})
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn a_variable_that_is_not_utf8_is_left_out_without_a_panic() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        let vars = vec![
            (OsString::from("GOOD"), OsString::from("yes")),
            (OsString::from("BAD"), OsString::from_vec(b"\xff".to_vec())),
            (OsString::from_vec(b"K\xfe".to_vec()), OsString::from("v")),
        ];
        let env = super::utf8_environment(vars.into_iter());
        assert_eq!(env.len(), 1);
        assert_eq!(env["GOOD"], "yes");
    }
}
