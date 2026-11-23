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
    let opened = {
        // Not while a teardown is deciding or running (review B5).
        let _lifecycle = app.workspace_lifecycle.lock().await;
        let opened = app.workspaces.open(&p.path).await?;
        app.add_client(&opened.workspace_id, ctx.notifier.clone());
        opened
    };
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

/// `workspace/close`: this connection lets go of the workspace. Workspace handles are shared by
/// the connections that opened the same folder (processes.md §Multi-client): only when the last
/// one closes it (or disconnects, [`release_connection`]) are its tasks, servers, terminals and
/// watches stopped (AUD-029).
pub async fn close(app: Arc<App>, p: WorkspaceRef, ctx: RequestCtx) -> Result<Empty, RpcError> {
    app.workspaces.get(&p.workspace_id)?;
    app.stop_watcher_of(&p.workspace_id, &ctx.notifier);
    // "The last holder" and the teardown are one step for `workspace/open` (review B5).
    let _lifecycle = app.workspace_lifecycle.lock().await;
    if app.release_client(&p.workspace_id, &ctx.notifier) {
        teardown(&app, &p.workspace_id).await?;
    }
    Ok(Empty {})
}

/// A connection ended: it lets go of every workspace it had open; those no other connection
/// holds are torn down.
pub async fn release_connection(app: Arc<App>, n: axiom_rpc::Notifier) {
    for ws in app.workspaces_of(&n) {
        app.stop_watcher_of(&ws, &n);
        let _lifecycle = app.workspace_lifecycle.lock().await;
        if app.release_client(&ws, &n) {
            if let Err(e) = teardown(&app, &ws).await {
                tracing::debug!("closing {ws:?} after its last connection: {e:?}");
            }
        }
    }
}

/// Stops everything of the workspace and forgets it.
pub async fn teardown(app: &Arc<App>, id: &axiom_protocol::WorkspaceId) -> Result<(), RpcError> {
    app.executor.cancel_all(id).await;
    app.stop_watcher(id);
    app.lsp.close_workspace(id).await;
    app.terminals.close_workspace(id);
    crate::handlers::git::stop_watch(app, id);
    crate::handlers::preview::close_workspace(app, id).await;
    app.external_reads
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(id);
    app.clients
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(id);
    app.executor.forget_workspace(id);
    app.workspaces.close(id).await?;
    Ok(())
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

/// `workspace/search` (protocol 0.4, M4-UX-001): text matches or the file list. Cancelling the
/// request (`$/cancel`) stops the walk between files.
pub async fn search(
    app: Arc<App>,
    p: axiom_protocol::workspace::SearchParams,
    ctx: axiom_rpc::RequestCtx,
) -> Result<axiom_protocol::workspace::SearchResult, RpcError> {
    use axiom_protocol::workspace::{SearchMatch, SearchMode, SearchResult};
    let ws = app.workspaces.get(&p.workspace_id)?;
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f2 = flag.clone();
    let token = ctx.cancel.clone();
    let watch = tokio::spawn(async move {
        token.cancelled().await;
        f2.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let q = axiom_workspace::FindQuery {
        query: p.query,
        regex: p.regex,
        case_sensitive: p.case_sensitive,
        whole_word: p.whole_word,
        include: p.include,
        exclude: p.exclude,
        files: p.mode == SearchMode::Files,
        limit: p.limit.unwrap_or(0) as usize,
    };
    let r = ws.find(q, flag).await;
    watch.abort();
    let r = r.map_err(|e| match e {
        axiom_workspace::WsError::InvalidPath(m) => {
            RpcError::new(axiom_protocol::ErrorKind::InvalidParams, m)
        }
        other => other.into(),
    })?;
    Ok(SearchResult {
        matches: r
            .matches
            .into_iter()
            .map(|m| SearchMatch {
                path: m.path,
                line: m.line,
                column: m.column,
                end_column: m.end_column,
                preview: m.preview,
                preview_column: m.preview_column,
            })
            .collect(),
        files: r.files,
        truncated: r.truncated,
        files_searched: r.files_searched,
    })
}
