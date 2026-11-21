//! `git/*` (protocol 0.4, ADR-0018 §5; M4-GIT-001): status and file text through the `git`
//! program, and `git/didChange` when the worktree or the repository changes (debounced 300 ms).
//! The change watch starts with a workspace's first `git/status` and ends with the workspace.

use crate::app::{internal, App};
use axiom_protocol::git::{DidChange, GitRev, GitStatus, ShowParams, ShowResult};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{RpcError, WorkspaceId};
use axiom_workspace::git;
use std::sync::Arc;
use std::time::Duration;

const DEBOUNCE: Duration = Duration::from_millis(300);

pub async fn status(app: Arc<App>, p: WorkspaceRef) -> Result<GitStatus, RpcError> {
    let root = app.workspaces.get(&p.workspace_id)?.root().to_path_buf();
    let st = tokio::task::spawn_blocking(move || git::status(&root))
        .await
        .map_err(|_| internal())??;
    if st.repo {
        start_watch(&app, &p.workspace_id);
    }
    Ok(st)
}

pub async fn show(app: Arc<App>, p: ShowParams) -> Result<ShowResult, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    // The path must be a valid workspace path (it is joined to the repository prefix).
    ws.resolve(&p.path)?;
    let root = ws.root().to_path_buf();
    let path = p.path.0.clone();
    let head = p.rev == GitRev::Head;
    let (text, binary) = tokio::task::spawn_blocking(move || git::show(&root, &path, head))
        .await
        .map_err(|_| internal())??;
    Ok(ShowResult { text, binary })
}

/// Starts the workspace's change watch once: worktree changes (the workspace watcher) and the
/// repository's own files, debounced, to every client of the workspace.
fn start_watch(app: &Arc<App>, ws: &WorkspaceId) {
    let mut watches = app.git_watches.lock().unwrap_or_else(|p| p.into_inner());
    if watches.contains_key(ws) {
        return;
    }
    let Ok(w) = app.workspaces.get(ws) else {
        return;
    };
    let root = w.root().to_path_buf();
    let Ok(mut tree) = w.watch() else { return };
    let repo = git::discover(&root).ok();
    let gitdir = repo.and_then(|r| git::watch_git_dir(&r.git_dir).ok());
    let weak = Arc::downgrade(app);
    let id = ws.clone();
    let task = tokio::spawn(async move {
        let (_keep, mut dir_rx) = match gitdir {
            Some((w, rx)) => (Some(w), rx),
            None => (None, tokio::sync::mpsc::channel(1).1),
        };
        loop {
            // Wait for a first change, then let the burst settle.
            tokio::select! {
                c = tree.changes.recv() => if c.is_none() { return },
                d = dir_rx.recv() => if d.is_none() && _keep.is_some() { return },
            }
            let settle = tokio::time::sleep(DEBOUNCE);
            tokio::pin!(settle);
            loop {
                tokio::select! {
                    () = &mut settle => break,
                    _ = tree.changes.recv() => {}
                    _ = dir_rx.recv() => {}
                }
            }
            let Some(app) = weak.upgrade() else { return };
            for c in app.clients_of(&id) {
                let _ = c
                    .notify::<DidChange>(WorkspaceRef {
                        workspace_id: id.clone(),
                    })
                    .await;
            }
        }
    });
    watches.insert(ws.clone(), task.abort_handle());
}

/// Ends the workspace's change watch (the workspace closed).
pub fn stop_watch(app: &App, ws: &WorkspaceId) {
    if let Some(h) = app
        .git_watches
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(ws)
    {
        h.abort();
    }
}
