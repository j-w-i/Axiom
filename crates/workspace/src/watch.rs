//! Debounced, deduplicated, ignore-filtered change notifications
//! (`fs/didChange`, docs/protocols/ipc.md).

use crate::error::WsError;
use crate::workspace::{Inner, Workspace};
use axiom_protocol::fs::{ChangeKind, FileChange};
use axiom_protocol::WorkspacePath;
use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{error::TrySendError, Receiver, Sender};

const DEBOUNCE: Duration = Duration::from_millis(100);
const SUBSCRIBER_CAPACITY: usize = 64;

struct Sub {
    id: u64,
    tx: Sender<Vec<FileChange>>,
    /// A batch could not be delivered; the next one is replaced by a
    /// "re-list everything" marker so nothing is silently lost.
    lagged: bool,
}

#[derive(Default)]
struct Subs {
    next_id: u64,
    list: Vec<Sub>,
}

/// State shared by all handles of one workspace; dropping the last handle
/// drops this and with it the OS watcher.
pub(crate) struct WatchShared {
    subs: Arc<Mutex<Subs>>,
    _watcher: RecommendedWatcher,
}

/// One subscription. Dropping it unsubscribes; the OS watcher stops when the
/// last handle of the workspace is dropped.
pub struct WatchHandle {
    pub changes: Receiver<Vec<FileChange>>,
    id: u64,
    shared: Arc<WatchShared>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        lock(&self.shared.subs).list.retain(|s| s.id != self.id);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Workspace {
    /// Subscribes to changes (recursive), delivered as batches. Each handle is
    /// an independent subscriber that receives every batch emitted while it
    /// exists. One OS watcher is shared by all handles of a workspace.
    pub fn watch(&self) -> Result<WatchHandle, WsError> {
        let mut slot = lock(&self.inner.watch);
        let shared = match slot.upgrade() {
            Some(s) => s,
            None => {
                let s = start(self.inner.clone())?;
                *slot = Arc::downgrade(&s);
                s
            }
        };
        let (tx, rx) = tokio::sync::mpsc::channel(SUBSCRIBER_CAPACITY);
        let mut subs = lock(&shared.subs);
        subs.next_id += 1;
        let id = subs.next_id;
        subs.list.push(Sub {
            id,
            tx,
            lagged: false,
        });
        drop(subs);
        Ok(WatchHandle {
            changes: rx,
            id,
            shared,
        })
    }
}

fn start(inner: Arc<Inner>) -> Result<Arc<WatchShared>, WsError> {
    let (raw_tx, raw_rx) = mpsc::channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = raw_tx.send(res);
    })
    .map_err(notify_err)?;
    watcher
        .watch(&inner.root, RecursiveMode::Recursive)
        .map_err(notify_err)?;
    let subs: Arc<Mutex<Subs>> = Arc::default();
    let thread_subs = subs.clone();
    std::thread::Builder::new()
        .name("axiom-fs-watch".into())
        .spawn(move || run(inner, raw_rx, thread_subs))
        .map_err(WsError::Io)?;
    Ok(Arc::new(WatchShared {
        subs,
        _watcher: watcher,
    }))
}

fn notify_err(e: notify::Error) -> WsError {
    match e.kind {
        notify::ErrorKind::Io(io) => WsError::Io(io),
        _ => WsError::Io(std::io::Error::other(e.to_string())),
    }
}

/// Accumulates one debounce window.
#[derive(Default)]
struct Batch {
    changes: BTreeMap<String, ChangeKind>,
    overflow: bool,
}

fn merge(prev: Option<ChangeKind>, new: ChangeKind) -> ChangeKind {
    use ChangeKind::*;
    match (prev, new) {
        (None, n) => n,
        (_, Removed) => Removed,
        (Some(Removed), _) => Modified,
        (Some(Created), _) => Created,
        (Some(Modified), _) => Modified,
    }
}

impl Batch {
    fn add(&mut self, path: String, kind: ChangeKind) {
        let merged = merge(self.changes.get(&path).copied(), kind);
        self.changes.insert(path, merged);
    }

    fn finish(self) -> Vec<FileChange> {
        if self.overflow {
            return vec![overflow_change()];
        }
        self.changes
            .into_iter()
            .map(|(p, kind)| FileChange {
                path: WorkspacePath(p),
                kind,
            })
            .collect()
    }
}

fn overflow_change() -> FileChange {
    FileChange {
        path: WorkspacePath::root(),
        kind: ChangeKind::Modified,
    }
}

fn run(inner: Arc<Inner>, raw: mpsc::Receiver<notify::Result<Event>>, subs: Arc<Mutex<Subs>>) {
    let mut batch: Option<(Batch, Instant)> = None;
    loop {
        let received = match &batch {
            None => raw.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some((_, deadline)) => {
                raw.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            }
        };
        match received {
            Ok(res) => {
                let (b, _) =
                    batch.get_or_insert_with(|| (Batch::default(), Instant::now() + DEBOUNCE));
                match res {
                    Ok(ev) => handle_event(&inner, ev, b),
                    Err(e) => {
                        tracing_free_log(&e);
                        b.overflow = true;
                    }
                }
                // An event that was entirely filtered out must not start a window.
                if let Some((b, _)) = &batch {
                    if b.changes.is_empty() && !b.overflow {
                        batch = None;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if let Some((b, _)) = batch.take() {
                    deliver(&subs, b.finish());
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn tracing_free_log(_e: &notify::Error) {
    // The workspace crate has no logging dependency; an error simply becomes
    // a "re-list everything" batch.
}

fn deliver(subs: &Mutex<Subs>, changes: Vec<FileChange>) {
    if changes.is_empty() {
        return;
    }
    let mut guard = lock(subs);
    for sub in guard.list.iter_mut() {
        let payload = if sub.lagged {
            vec![overflow_change()]
        } else {
            changes.clone()
        };
        match sub.tx.try_send(payload) {
            Ok(()) => sub.lagged = false,
            Err(TrySendError::Full(_)) => sub.lagged = true,
            Err(TrySendError::Closed(_)) => {}
        }
    }
}

fn rel_of(root: &Path, p: &Path) -> Option<String> {
    let rel = p.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for c in rel.components() {
        match c {
            std::path::Component::Normal(s) => parts.push(s.to_str()?),
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn handle_event(inner: &Inner, ev: Event, batch: &mut Batch) {
    if ev.need_rescan() {
        batch.overflow = true;
        return;
    }
    // (host path, kind) pairs this event stands for.
    let mut items: Vec<(&Path, ChangeKind)> = Vec::new();
    match ev.kind {
        EventKind::Create(_) => ev
            .paths
            .iter()
            .for_each(|p| items.push((p, ChangeKind::Created))),
        EventKind::Remove(_) => ev
            .paths
            .iter()
            .for_each(|p| items.push((p, ChangeKind::Removed))),
        EventKind::Modify(ModifyKind::Name(mode)) => match mode {
            RenameMode::From => ev
                .paths
                .iter()
                .for_each(|p| items.push((p, ChangeKind::Removed))),
            RenameMode::To => ev
                .paths
                .iter()
                .for_each(|p| items.push((p, ChangeKind::Created))),
            RenameMode::Both if ev.paths.len() == 2 => {
                items.push((&ev.paths[0], ChangeKind::Removed));
                items.push((&ev.paths[1], ChangeKind::Created));
            }
            // Platforms that do not say which side: look at the disk.
            _ => ev.paths.iter().for_each(|p| {
                items.push((
                    p,
                    if exists(p) {
                        ChangeKind::Created
                    } else {
                        ChangeKind::Removed
                    },
                ))
            }),
        },
        EventKind::Modify(_) => ev
            .paths
            .iter()
            .for_each(|p| items.push((p, ChangeKind::Modified))),
        EventKind::Any | EventKind::Other => ev.paths.iter().for_each(|p| {
            items.push((
                p,
                if exists(p) {
                    ChangeKind::Modified
                } else {
                    ChangeKind::Removed
                },
            ))
        }),
        EventKind::Access(_) => {}
    }
    for (path, kind) in items {
        let Some(rel) = rel_of(&inner.root, path) else {
            continue;
        };
        if rel.is_empty() {
            continue;
        }
        // Ignore files changed: forget cached matchers before judging anything.
        inner.ignores.invalidate_for_write(&rel);
        if is_filtered(inner, &rel) {
            continue;
        }
        batch.add(rel, kind);
    }
}

/// ipc.md §`fs/didChange`: drop `.git` and anything under it, and anything
/// under a directory the ignore rules mark ignored. The entry itself is
/// judged only by name (`.git`): an ignored file, or an ignored directory
/// being created/removed, is reported.
fn is_filtered(inner: &Inner, rel: &str) -> bool {
    if rel.split('/').any(|c| c == ".git") {
        return true;
    }
    let Some((parent, _)) = rel.rsplit_once('/') else {
        return false; // directly under the root
    };
    // Is `parent` (a directory) ignored, directly or through an ancestor?
    let (grand, name) = match parent.rsplit_once('/') {
        Some((g, n)) => (g, n),
        None => ("", parent),
    };
    inner
        .ignores
        .classify(&inner.root, grand, &[(name.to_string(), true)])
        .first()
        .copied()
        .unwrap_or(false)
}
