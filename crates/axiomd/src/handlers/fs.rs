use crate::app::App;
use axiom_protocol::fs::DidChangeParams;
use axiom_protocol::fs::{
    CreateDirectoryParams, CreateFileParams, DidChange, ListParams, ListResult, ReadFileParams,
    ReadFileResult, RenameParams, TrashParams, WatchParams, WriteFileParams, WriteFileResult,
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
    // A regular file of at most 16 MiB, checked before reading and read with a limit: a
    // location naming `/dev/zero`, a FIFO or a huge file is refused at once (AUD-097).
    let shown = path.display().to_string();
    let bytes = tokio::task::spawn_blocking(move || {
        axiom_workspace::read_regular_file(&path, &shown, axiom_protocol::fs::MAX_TEXT_FILE_BYTES)
    })
    .await
    .map_err(|_| crate::app::internal())??;
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

/// `fs/watch`: per connection (AUD-028). Each connection that enables it gets every batch;
/// enabling twice on one connection is idempotent; a connection's forwarder ends with it.
pub async fn watch(app: Arc<App>, p: WatchParams, ctx: RequestCtx) -> Result<Empty, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    if !p.enabled {
        app.stop_watcher_of(&p.workspace_id, &ctx.notifier);
        return Ok(Empty {});
    }
    let watching = |app: &App| {
        let mut m = app.watchers.lock().unwrap_or_else(|e| e.into_inner());
        let v = m.entry(p.workspace_id.clone()).or_default();
        v.retain(|(n, h)| !n.is_closed() && !h.is_finished());
        v.iter().any(|(n, _)| n.same_connection(&ctx.notifier))
    };
    if watching(&app) {
        return Ok(Empty {});
    }
    // The first watch of a workspace walks its whole tree: off the async threads (AUD-031).
    let w = ws.clone();
    let mut handle = tokio::task::spawn_blocking(move || w.watch())
        .await
        .map_err(|_| crate::app::internal())??;
    let id = p.workspace_id.clone();
    let notifier = ctx.notifier.clone();
    let mut m = app.watchers.lock().unwrap_or_else(|e| e.into_inner());
    let v = m.entry(p.workspace_id.clone()).or_default();
    // Checked again under the same lock as the insert: two concurrent enables make one.
    if v.iter().any(|(n, _)| n.same_connection(&ctx.notifier)) {
        return Ok(Empty {});
    }
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
    v.push((ctx.notifier.clone(), task.abort_handle()));
    Ok(Empty {})
}

/// `fs/createFile` (protocol 0.6, AUD-022).
pub async fn create_file(app: Arc<App>, p: CreateFileParams) -> Result<WriteFileResult, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    let version = ws.create_file(&p.path, &p.text).await?;
    Ok(WriteFileResult { version })
}

/// `fs/createDirectory` (protocol 0.6).
pub async fn create_directory(app: Arc<App>, p: CreateDirectoryParams) -> Result<Empty, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    ws.create_dir(&p.path, p.parents).await?;
    Ok(Empty {})
}

/// `fs/rename` (protocol 0.6).
pub async fn rename(app: Arc<App>, p: RenameParams) -> Result<Empty, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    ws.rename(&p.from, &p.to).await?;
    Ok(Empty {})
}

/// `fs/trash` (protocol 0.6): to the person's trash, never a permanent delete.
pub async fn trash(app: Arc<App>, p: TrashParams) -> Result<Empty, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    ws.trash(&p.path).await?;
    Ok(Empty {})
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use axiom_protocol::{ErrorKind, WorkspacePath};
    use std::time::Duration;

    /// AUD-097: a language server's location naming `/dev/zero` or a FIFO is refused at once
    /// (it was read to the end before the size check: out of memory, or a blocked thread).
    #[tokio::test]
    async fn external_reads_refuse_devices_and_fifos_at_once() {
        let app = App::new(axiom_store::Store::open_in_memory().unwrap());
        let ws_dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let fifo = other.path().join("p");
        assert!(std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success());
        let opened = app
            .workspaces
            .open(ws_dir.path().to_str().unwrap())
            .await
            .unwrap();
        let fifo_uri = url::Url::from_file_path(&fifo).unwrap().to_string();
        app.allow_external_reads(
            &opened.workspace_id,
            std::path::Path::new(&opened.root),
            &["file:///dev/zero".to_string(), fifo_uri.clone()],
        );
        for uri in ["file:///dev/zero".to_string(), fifo_uri] {
            let r = tokio::time::timeout(
                Duration::from_secs(5),
                read_file(
                    app.clone(),
                    ReadFileParams {
                        workspace_id: opened.workspace_id.clone(),
                        path: WorkspacePath(uri.clone()),
                    },
                ),
            )
            .await
            .unwrap_or_else(|_| panic!("reading {uri} blocked"));
            assert_eq!(r.unwrap_err().data.kind, ErrorKind::InvalidParams, "{uri}");
        }
    }
}
