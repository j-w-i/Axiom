use crate::app::App;
use axiom_protocol::fs::DidChangeParams;
use axiom_protocol::fs::{
    DidChange, ListParams, ListResult, ReadFileParams, ReadFileResult, WatchParams,
    WriteFileParams, WriteFileResult,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::RpcError;
use axiom_rpc::RequestCtx;
use std::sync::Arc;

pub async fn list(app: Arc<App>, p: ListParams) -> Result<ListResult, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    Ok(ListResult {
        entries: ws.list(&p.path).await?,
    })
}

pub async fn read_file(app: Arc<App>, p: ReadFileParams) -> Result<ReadFileResult, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    Ok(ws.read_text(&p.path).await?)
}

pub async fn write_file(app: Arc<App>, p: WriteFileParams) -> Result<WriteFileResult, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    let version = ws
        .write_text(&p.path, &p.text, p.expected_version.as_ref(), p.overwrite)
        .await?;
    Ok(WriteFileResult { version })
}

pub async fn watch(app: Arc<App>, p: WatchParams, ctx: RequestCtx) -> Result<Empty, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    if !p.enabled {
        app.stop_watcher(&p.workspace_id);
        return Ok(Empty {});
    }
    // Idempotent: already watching.
    if app
        .watchers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(&p.workspace_id)
    {
        return Ok(Empty {});
    }
    let mut handle = ws.watch()?;
    let id = p.workspace_id.clone();
    let notifier = ctx.notifier.clone();
    let task = tokio::spawn(async move {
        while let Some(changes) = handle.changes.recv().await {
            let params = DidChangeParams {
                workspace_id: id.clone(),
                changes,
            };
            if notifier.notify::<DidChange>(params).await.is_err() {
                break;
            }
        }
    });
    app.watchers
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(p.workspace_id, task.abort_handle());
    Ok(Empty {})
}
