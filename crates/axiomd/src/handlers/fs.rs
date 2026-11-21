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
    if p.path.0.starts_with("file://") {
        return read_external(&app, &p).await;
    }
    Ok(ws.read_text(&p.path).await?)
}

/// A file outside the workspace that one of its language servers returned in a location
/// (protocol 0.4, ADR-0018 §2): read-only, and only those.
async fn read_external(app: &App, p: &ReadFileParams) -> Result<ReadFileResult, RpcError> {
    let denied = || {
        RpcError::new(
            axiom_protocol::ErrorKind::PermissionDenied,
            "Only files a language server pointed to can be read outside the workspace.",
        )
    };
    let path = url::Url::parse(&p.path.0)
        .ok()
        .and_then(|u| u.to_file_path().ok())
        .ok_or_else(denied)?;
    if !app.external_read_allowed(&p.workspace_id, &path) {
        return Err(denied());
    }
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        RpcError::new(
            if e.kind() == std::io::ErrorKind::NotFound {
                axiom_protocol::ErrorKind::NotFound
            } else {
                axiom_protocol::ErrorKind::Io
            },
            format!("{} cannot be read: {e}", path.display()),
        )
    })?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(RpcError::new(
            axiom_protocol::ErrorKind::TooLarge,
            "The file is over 16 MiB.",
        ));
    }
    let version = {
        use sha2::Digest;
        axiom_protocol::fs::FileVersion(hex::encode(sha2::Sha256::digest(&bytes)))
    };
    let text = String::from_utf8(bytes).map_err(|_| {
        RpcError::new(
            axiom_protocol::ErrorKind::UnsupportedEncoding,
            "The file is not UTF-8 text.",
        )
    })?;
    Ok(ReadFileResult { text, version })
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
