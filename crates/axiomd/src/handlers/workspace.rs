use crate::app::App;
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::workspace::{
    GetStateParams, OpenParams, OpenResult, RecentResult, RecentWorkspace, SetStateParams,
    StateResult, WorkspaceRef,
};
use axiom_protocol::RpcError;
use axiom_rpc::RequestCtx;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

const RECENT_LIMIT: usize = 20;

pub async fn open(app: Arc<App>, p: OpenParams, ctx: RequestCtx) -> Result<OpenResult, RpcError> {
    let opened = app.workspaces.open(&p.path).await?;
    app.add_client(&opened.workspace_id, ctx.notifier.clone());
    let (root, name) = (opened.root.clone(), opened.name.clone());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    app.store_call(move |s| {
        s.recent_touch(&root, &name, now)?;
        let mut m = BTreeMap::new();
        m.insert(
            "workspace.lastRoot".to_string(),
            Some(Value::String(root.clone())),
        );
        s.settings_set(&m)
    })
    .await?;
    Ok(opened)
}

pub async fn close(app: Arc<App>, p: WorkspaceRef) -> Result<Empty, RpcError> {
    app.workspaces.get(&p.workspace_id)?;
    app.executor.cancel_all(&p.workspace_id).await;
    app.stop_watcher(&p.workspace_id);
    app.lsp.close_workspace(&p.workspace_id).await;
    app.external_reads
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&p.workspace_id);
    app.clients
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(&p.workspace_id);
    app.workspaces.close(&p.workspace_id).await?;
    Ok(Empty {})
}

pub async fn recent(app: Arc<App>, _: Empty) -> Result<RecentResult, RpcError> {
    let list = app.store_call(|s| s.recent_list(RECENT_LIMIT)).await?;
    let workspaces = tokio::task::spawn_blocking(move || {
        list.into_iter()
            .filter(|r| std::path::Path::new(&r.root).is_dir())
            .map(|r| RecentWorkspace {
                root: r.root,
                name: r.name,
                last_opened_ms: r.last_opened_ms,
            })
            .collect()
    })
    .await
    .map_err(|_| crate::app::internal())?;
    Ok(RecentResult { workspaces })
}

pub async fn get_state(app: Arc<App>, p: GetStateParams) -> Result<StateResult, RpcError> {
    let root = app
        .workspaces
        .get(&p.workspace_id)?
        .root()
        .to_string_lossy()
        .into_owned();
    let values = app
        .store_call(move |s| s.workspace_state_get(&root, &p.keys))
        .await?;
    Ok(StateResult { values })
}

pub async fn set_state(app: Arc<App>, p: SetStateParams) -> Result<Empty, RpcError> {
    let root = app
        .workspaces
        .get(&p.workspace_id)?
        .root()
        .to_string_lossy()
        .into_owned();
    let values: BTreeMap<String, Option<Value>> = p
        .values
        .into_iter()
        .map(|(k, v)| (k, if v.is_null() { None } else { Some(v) }))
        .collect();
    app.store_call(move |s| s.workspace_state_set(&root, &values))
        .await?;
    Ok(Empty {})
}
