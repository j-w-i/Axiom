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
    let mut st = st;
    if st.repo {
        start_watch(&app, &p.workspace_id);
        st.polling = app
            .git_polling
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(&p.workspace_id);
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

/// When a watch could not be set up (the inotify limit, a `git` that timed out), the
/// repository is looked at this often instead (AUD-012).
const POLL: Duration = Duration::from_secs(10);

/// Starts the workspace's change watch once: worktree changes (the workspace watcher) and the
/// repository's own files, debounced, to every client of the workspace. The check and the
/// insert happen under one lock; the setup (a recursive watch of the tree, `git rev-parse`, a
/// watch of the git dir: seconds in a large repository) runs on a blocking thread, never under
/// the lock or on an async worker (AUD-031).
fn start_watch(app: &Arc<App>, ws: &WorkspaceId) {
    let mut watches = app.git_watches.lock().unwrap_or_else(|p| p.into_inner());
    if watches.contains_key(ws) {
        return;
    }
    let Ok(w) = app.workspaces.get(ws) else {
        return;
    };
    let weak = Arc::downgrade(app);
    let id = ws.clone();
    let task = tokio::spawn(async move {
        let setup = tokio::task::spawn_blocking(move || {
            let tree = w.watch().map_err(|e| e.to_string());
            let dir = git::discover(w.root())
                .and_then(|r| git::watch_git_dir(&r.git_dir))
                .map_err(|e| e.to_string());
            (tree, dir)
        })
        .await;
        let Ok((tree, dir)) = setup else { return };
        let (tree, _tree_watch) = match tree {
            Ok(mut h) => {
                let rx = std::mem::replace(&mut h.changes, tokio::sync::mpsc::channel(1).1);
                (Some(rx), Some(h))
            }
            Err(e) => {
                tracing::warn!("git changes of {id:?}: no worktree watch ({e}); polling");
                (None, None)
            }
        };
        let (dir, _dir_watch) = match dir {
            Ok((w, rx)) => (Some(rx), Some(w)),
            Err(e) => {
                tracing::warn!("git changes of {id:?}: no repository watch ({e}); polling");
                (None, None)
            }
        };
        let poll = (tree.is_none() || dir.is_none()).then_some(POLL);
        if poll.is_some() {
            if let Some(app) = weak.upgrade() {
                app.git_polling
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(id.clone());
            }
        }
        relay(tree, dir, poll, || {
            let weak = weak.clone();
            let id = id.clone();
            async move {
                let Some(app) = weak.upgrade() else {
                    return false;
                };
                app.notify_clients::<DidChange>(
                    &id,
                    WorkspaceRef {
                        workspace_id: id.clone(),
                    },
                )
                .await;
                true
            }
        })
        .await;
    });
    watches.insert(ws.clone(), task.abort_handle());
}

/// Waits for a change from either source, lets the burst settle ([`DEBOUNCE`]) and calls `emit`
/// (which returns false to stop). A missing source is never polled (AUD-012: a closed channel
/// standing in for it made this loop spin); with `poll`, `emit` also runs that often. Ends when
/// every present source has ended.
async fn relay<T, U, F, Fut>(
    mut tree: Option<tokio::sync::mpsc::Receiver<T>>,
    mut dir: Option<tokio::sync::mpsc::Receiver<U>>,
    poll: Option<Duration>,
    mut emit: F,
) where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    async fn next<V>(rx: &mut Option<tokio::sync::mpsc::Receiver<V>>) -> bool {
        match rx {
            Some(r) => {
                if r.recv().await.is_some() {
                    true
                } else {
                    *rx = None;
                    false
                }
            }
            None => std::future::pending().await,
        }
    }
    let had_source = tree.is_some() || dir.is_some();
    loop {
        if had_source && tree.is_none() && dir.is_none() && poll.is_none() {
            return;
        }
        if !had_source && poll.is_none() {
            return;
        }
        let tick = async {
            match poll {
                Some(p) => tokio::time::sleep(p).await,
                None => std::future::pending().await,
            }
        };
        // Wait for a first change, then let the burst settle.
        let changed = tokio::select! {
            c = next(&mut tree) => c,
            d = next(&mut dir) => d,
            () = tick => true,
        };
        if !changed {
            // A source ended (the workspace closed): look again whether anything is left.
            continue;
        }
        let settle = tokio::time::sleep(DEBOUNCE);
        tokio::pin!(settle);
        loop {
            tokio::select! {
                () = &mut settle => break,
                _ = next(&mut tree) => {}
                _ = next(&mut dir) => {}
            }
        }
        if !emit().await {
            return;
        }
    }
}

/// Ends the workspace's change watch (the workspace closed).
pub fn stop_watch(app: &App, ws: &WorkspaceId) {
    app.git_polling
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(ws);
    if let Some(h) = app
        .git_watches
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .remove(ws)
    {
        h.abort();
    }
}

/// Ends every change watch (the daemon is shutting down).
pub fn stop_all_watches(app: &App) {
    let all: Vec<_> = app
        .git_watches
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .drain()
        .map(|(_, h)| h)
        .collect();
    for h in all {
        h.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn counter() -> (Arc<AtomicUsize>, impl FnMut() -> std::future::Ready<bool>) {
        let n = Arc::new(AtomicUsize::new(0));
        let n2 = n.clone();
        (n, move || {
            n2.fetch_add(1, Ordering::SeqCst);
            std::future::ready(true)
        })
    }

    /// AUD-012: without a repository watch (and no changes) nothing is sent and the relay
    /// sleeps; it used to spin and send `git/didChange` every 300 ms.
    #[tokio::test(start_paused = true)]
    async fn no_repository_watch_does_not_spin_or_send() {
        let (_tx, tree) = tokio::sync::mpsc::channel::<()>(4);
        let (n, emit) = counter();
        let r = tokio::spawn(relay::<(), (), _, _>(Some(tree), None, None, emit));
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert_eq!(n.load(Ordering::SeqCst), 0);
        assert!(!r.is_finished());
        r.abort();
    }

    /// AUD-012: with neither watch, the fallback poll sends at most one change per interval.
    #[tokio::test(start_paused = true)]
    async fn the_fallback_poll_is_one_change_per_interval() {
        let (n, emit) = counter();
        let r = tokio::spawn(relay::<(), (), _, _>(None, None, Some(POLL), emit));
        tokio::time::sleep(POLL * 3 + Duration::from_secs(1)).await;
        let sent = n.load(Ordering::SeqCst);
        assert!((2..=3).contains(&sent), "{sent}");
        r.abort();
    }

    /// A burst of changes becomes one notification; the relay ends with its sources.
    #[tokio::test(start_paused = true)]
    async fn a_burst_is_debounced_and_the_end_of_the_sources_ends_it() {
        let (tx, tree) = tokio::sync::mpsc::channel::<()>(16);
        let (dtx, dir) = tokio::sync::mpsc::channel::<()>(16);
        let (n, emit) = counter();
        let r = tokio::spawn(relay(Some(tree), Some(dir), None, emit));
        for _ in 0..5 {
            tx.send(()).await.unwrap();
            dtx.send(()).await.unwrap();
        }
        tokio::time::sleep(DEBOUNCE * 3).await;
        assert_eq!(n.load(Ordering::SeqCst), 1);
        drop((tx, dtx));
        tokio::time::timeout(Duration::from_secs(5), r)
            .await
            .expect("ends")
            .unwrap();
    }
}
