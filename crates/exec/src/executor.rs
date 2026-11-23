use crate::backend::{program_name, ExecBackend, HostBackend, SpawnSpec};
use crate::error::ExecError;
use crate::tree::{self, ProcessTree};
use crate::utf8::Utf8Chunker;
use axiom_protocol::task::{
    Command, ExitStatus, ExitedParams, Outcome, OutputParams, Stream, TaskInfo,
};
use axiom_protocol::{TaskId, WorkspaceId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::{mpsc, watch, Notify};

const CHUNK: usize = 8 * 1024;
const EVENT_CAPACITY: usize = 256;
const KILL_GRACE: Duration = Duration::from_secs(3);
/// How long the pipes may stay open after the task's own process has exited (AUD-007).
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(2);
/// How long the pipes may stay open after the task's process group was killed.
const DRAIN_AFTER_KILL: Duration = Duration::from_millis(500);
/// Longest `cancel_all` waits for its tasks (they end within the kill grace plus the drains).
const CANCEL_ALL_BOUND: Duration = Duration::from_secs(10);
const HISTORY_PER_WORKSPACE: usize = 50;

#[derive(Debug, Clone, PartialEq)]
pub enum TaskEvent {
    Output(OutputParams),
    Exited(ExitedParams),
}

type SharedTree = Arc<Mutex<Box<dyn ProcessTree>>>;

struct Entry {
    workspace: WorkspaceId,
    order: u64,
    info: TaskInfo,
    /// Sequence among finished tasks; `None` while running.
    finished_at: Option<u64>,
    cancelled: Arc<AtomicBool>,
    cancel: Arc<Notify>,
    tree: SharedTree,
    done: watch::Receiver<bool>,
}

#[derive(Default)]
struct State {
    next_id: u64,
    finished_counter: u64,
    tasks: HashMap<TaskId, Entry>,
}

struct Inner {
    backend: Box<dyn ExecBackend>,
    state: Mutex<State>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Drop for Inner {
    /// Last clone dropped: kill every task that is still running.
    fn drop(&mut self) {
        let st = lock(&self.state);
        for e in st.tasks.values().filter(|e| e.finished_at.is_none()) {
            lock(&e.tree).kill();
        }
    }
}

/// Supervises processes. Cheap to clone.
#[derive(Clone)]
pub struct Executor {
    inner: Arc<Inner>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn label_of(spec: &SpawnSpec) -> String {
    if let Some(l) = &spec.label {
        return l.clone();
    }
    match &spec.command {
        Command::Argv(v) => v.join(" "),
        Command::Shell(s) => s.clone(),
    }
}

impl Executor {
    pub fn new() -> Self {
        Self::with_backend(Box::new(HostBackend))
    }

    pub(crate) fn with_backend(backend: Box<dyn ExecBackend>) -> Self {
        Executor {
            inner: Arc::new(Inner {
                backend,
                state: Mutex::default(),
            }),
        }
    }

    /// Spawns and returns immediately. The receiver yields `Output`* then
    /// exactly one `Exited`.
    pub async fn start(
        &self,
        workspace: WorkspaceId,
        spec: SpawnSpec,
    ) -> Result<(TaskId, mpsc::Receiver<TaskEvent>), ExecError> {
        if matches!(&spec.command, Command::Argv(v) if v.is_empty()) {
            return Err(ExecError::EmptyCommand);
        }
        if !spec.cwd.is_dir() {
            return Err(ExecError::CwdNotFound);
        }
        let mut cmd = self.inner.backend.prepare(&spec);
        let started = Instant::now();
        // On Windows the process starts suspended, so nothing it starts escapes its job object
        // (AUD-101); `attach` assigns the job, then it is resumed.
        #[cfg(windows)]
        cmd.creation_flags(crate::tree_windows::CREATE_SUSPENDED);
        let mut child = cmd.spawn().map_err(|e| ExecError::SpawnFailed {
            program: program_name(&spec.command),
            os_error: e.to_string(),
        })?;
        let tree: SharedTree = match tree::attach(&child) {
            Some(t) => Arc::new(Mutex::new(t)),
            None => {
                // Never ran (Windows: still suspended); reap it before reporting.
                let _ = child.start_kill();
                let _ = child.wait().await;
                return Err(ExecError::SpawnFailed {
                    program: program_name(&spec.command),
                    os_error: "could not attach process tree control".into(),
                });
            }
        };
        #[cfg(windows)]
        if let Err(e) = crate::tree_windows::resume(&child) {
            lock(&tree).kill();
            let _ = child.wait().await;
            return Err(ExecError::SpawnFailed {
                program: program_name(&spec.command),
                os_error: format!("could not resume the process: {e}"),
            });
        }
        let group = child.id().unwrap_or(0);
        crate::guard::watch_group(group);
        // In the watchdog's pipe before the task is handed out (see `guard::flushed`).
        crate::guard::flushed(std::time::Duration::from_secs(1)).await;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        let (events_tx, events_rx) = mpsc::channel(EVENT_CAPACITY);
        let (done_tx, done_rx) = watch::channel(false);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(Notify::new());

        let id = {
            let mut st = lock(&self.inner.state);
            st.next_id += 1;
            let order = st.next_id;
            let id = TaskId(format!("t{order}"));
            st.tasks.insert(
                id.clone(),
                Entry {
                    workspace: workspace.clone(),
                    order,
                    info: TaskInfo {
                        task_id: id.clone(),
                        label: label_of(&spec),
                        command: spec.command.clone(),
                        started_ms: now_ms(),
                        exit: None,
                    },
                    finished_at: None,
                    cancelled: cancelled.clone(),
                    cancel: cancel.clone(),
                    tree: tree.clone(),
                    done: done_rx,
                },
            );
            id
        };

        tokio::spawn(supervise(Supervised {
            id: id.clone(),
            group,
            child,
            stdout,
            stderr,
            tree,
            cancelled,
            cancel,
            events: events_tx,
            started,
            state: Arc::downgrade(&self.inner),
            done: done_tx,
        }));
        Ok((id, events_rx))
    }

    /// Marks the task cancelled, terminates its tree, and force-kills after 3 s.
    /// Cancelling a finished task is `Ok`.
    pub async fn cancel(&self, id: &TaskId) -> Result<(), ExecError> {
        let st = lock(&self.inner.state);
        let e = st.tasks.get(id).ok_or(ExecError::NotFound)?;
        if e.finished_at.is_none() {
            e.cancelled.store(true, Ordering::SeqCst);
            e.cancel.notify_one();
        }
        Ok(())
    }

    /// Running tasks plus the last 50 finished, newest first.
    pub fn list(&self, workspace: &WorkspaceId) -> Vec<TaskInfo> {
        let st = lock(&self.inner.state);
        let mut v: Vec<(u64, TaskInfo)> = st
            .tasks
            .values()
            .filter(|e| &e.workspace == workspace)
            .map(|e| (e.order, e.info.clone()))
            .collect();
        v.sort_by_key(|(order, _)| std::cmp::Reverse(*order));
        v.into_iter().map(|(_, i)| i).collect()
    }

    /// Cancels every running task of a workspace and waits for them to exit.
    pub async fn cancel_all(&self, workspace: &WorkspaceId) {
        let mut waits = Vec::new();
        {
            let st = lock(&self.inner.state);
            for e in st
                .tasks
                .values()
                .filter(|e| &e.workspace == workspace && e.finished_at.is_none())
            {
                e.cancelled.store(true, Ordering::SeqCst);
                e.cancel.notify_one();
                waits.push(e.done.clone());
            }
        }
        let all = async {
            for mut w in waits {
                // Err means the supervisor is gone, which also means finished.
                let _ = w.wait_for(|done| *done).await;
            }
        };
        // Bounded: a supervisor ends within the kill grace and the drains, but a process the
        // OS cannot reap (uninterruptible sleep) must not hang `workspace/close` or shutdown.
        if tokio::time::timeout(CANCEL_ALL_BOUND, all).await.is_err() {
            tracing::warn!("tasks of {workspace:?} did not end within {CANCEL_ALL_BOUND:?}");
        }
    }

    /// Forgets the finished tasks of a closed workspace (its history is never asked for again:
    /// workspace ids are not reused; AUD-099).
    pub fn forget_workspace(&self, workspace: &WorkspaceId) {
        lock(&self.inner.state)
            .tasks
            .retain(|_, e| &e.workspace != workspace || e.finished_at.is_none());
    }

    /// `cancel_all` for every workspace.
    pub async fn shutdown(&self) {
        let workspaces: Vec<WorkspaceId> = {
            let st = lock(&self.inner.state);
            let mut v: Vec<WorkspaceId> = st.tasks.values().map(|e| e.workspace.clone()).collect();
            v.sort_by(|a, b| a.0.cmp(&b.0));
            v.dedup();
            v
        };
        for w in workspaces {
            self.cancel_all(&w).await;
        }
    }
}

struct Supervised {
    id: TaskId,
    /// The task's process group (Unix) or process id, for the daemon-exit watchdog.
    group: u32,
    child: tokio::process::Child,
    stdout: Option<tokio::process::ChildStdout>,
    stderr: Option<tokio::process::ChildStderr>,
    tree: SharedTree,
    cancelled: Arc<AtomicBool>,
    cancel: Arc<Notify>,
    events: mpsc::Sender<TaskEvent>,
    started: Instant,
    state: Weak<Inner>,
    done: watch::Sender<bool>,
}

/// Hands out `seq` numbers and sends in the same critical section, so chunks
/// from both streams reach the channel in seq order.
struct Sequencer {
    next: u64,
    tx: mpsc::Sender<TaskEvent>,
    open: bool,
}

async fn read_stream<R: AsyncRead + Unpin>(
    mut r: R,
    stream: Stream,
    id: TaskId,
    seq: Arc<tokio::sync::Mutex<Sequencer>>,
) {
    let mut dec = Utf8Chunker::default();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => 0,
            Ok(n) => n,
        };
        let text = if n == 0 {
            dec.finish()
        } else {
            dec.push(&buf[..n])
        };
        if !text.is_empty() {
            let mut s = seq.lock().await;
            let n = s.next;
            s.next += 1;
            if s.open {
                let ev = TaskEvent::Output(OutputParams {
                    task_id: id.clone(),
                    seq: n,
                    stream,
                    data: text,
                });
                // A dropped receiver must not block the child: keep draining.
                if s.tx.send(ev).await.is_err() {
                    s.open = false;
                }
            }
        }
        if n == 0 {
            break;
        }
    }
}

async fn supervise(mut s: Supervised) {
    let seq = Arc::new(tokio::sync::Mutex::new(Sequencer {
        next: 0,
        tx: s.events.clone(),
        open: true,
    }));
    let mut readers = Vec::new();
    if let Some(o) = s.stdout.take() {
        readers.push(tokio::spawn(read_stream(
            o,
            Stream::Stdout,
            s.id.clone(),
            seq.clone(),
        )));
    }
    if let Some(e) = s.stderr.take() {
        readers.push(tokio::spawn(read_stream(
            e,
            Stream::Stderr,
            s.id.clone(),
            seq.clone(),
        )));
    }

    let mut grace_deadline: Option<tokio::time::Instant> = None;
    let mut cancel_seen = false;
    let status = loop {
        let sleep = async {
            match grace_deadline {
                Some(d) => tokio::time::sleep_until(d).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            r = s.child.wait() => break r,
            _ = s.cancel.notified(), if !cancel_seen => {
                cancel_seen = true;
                lock(&s.tree).terminate();
                grace_deadline = Some(tokio::time::Instant::now() + KILL_GRACE);
            }
            _ = sleep, if grace_deadline.is_some() => {
                lock(&s.tree).kill();
                grace_deadline = None;
            }
        }
    };

    // The process is reaped. Let the pipes drain, but never wait for them without a bound
    // (AUD-007): a descendant that inherited stdout/stderr (`server &`, a daemonising tool)
    // keeps them open after the task's own process has exited. After the drain window (or, for
    // a cancelled task, the rest of its grace period) the task's process group is killed; if
    // the pipes are still open even then (a descendant left the group), reading stops.
    // A cancel during the drain kills the group at once.
    let aborts: Vec<tokio::task::AbortHandle> = readers.iter().map(|r| r.abort_handle()).collect();
    let drain = async {
        for r in readers.iter_mut() {
            let _ = r.await;
        }
    };
    tokio::pin!(drain);
    let now = tokio::time::Instant::now();
    let (mut deadline, mut killed) = match (grace_deadline, cancel_seen) {
        // Cancelled, the grace period still running: the descendants get the rest of it.
        (Some(d), _) => (d, false),
        // Cancelled and already killed.
        (None, true) => (now + DRAIN_AFTER_KILL, true),
        (None, false) => (now + DRAIN_AFTER_EXIT, false),
    };
    let mut aborted = false;
    loop {
        tokio::select! {
            _ = &mut drain => break,
            _ = s.cancel.notified(), if !cancel_seen => {
                cancel_seen = true;
                lock(&s.tree).kill();
                killed = true;
                deadline = tokio::time::Instant::now() + DRAIN_AFTER_KILL;
            }
            _ = tokio::time::sleep_until(deadline), if !aborted => {
                if killed {
                    // Still open after the kill: whatever holds them is not ours to wait for.
                    for a in &aborts {
                        a.abort();
                    }
                    aborted = true;
                } else {
                    lock(&s.tree).kill();
                    killed = true;
                    deadline = tokio::time::Instant::now() + DRAIN_AFTER_KILL;
                }
            }
        }
    }
    // A cancelled task must not leave descendants behind.
    if s.cancelled.load(Ordering::SeqCst) {
        lock(&s.tree).kill();
    }
    crate::guard::unwatch_group(s.group);

    let (exit_code, signal) = match &status {
        Ok(st) => (st.code(), signal_of(st)),
        Err(e) => {
            tracing::error!("waiting for task {:?} failed: {e}", s.id);
            (None, None)
        }
    };
    let outcome = if s.cancelled.load(Ordering::SeqCst) {
        Outcome::Cancelled
    } else if exit_code == Some(0) {
        Outcome::Succeeded
    } else {
        Outcome::Failed
    };
    let exit = ExitStatus {
        outcome,
        exit_code,
        signal,
        duration_ms: s.started.elapsed().as_millis() as u64,
    };

    // Record before notifying so `list` already shows the task as finished
    // when a client sees `Exited`.
    if let Some(inner) = s.state.upgrade() {
        let mut st = lock(&inner.state);
        st.finished_counter += 1;
        let counter = st.finished_counter;
        let mut workspace = None;
        if let Some(e) = st.tasks.get_mut(&s.id) {
            e.info.exit = Some(exit.clone());
            e.finished_at = Some(counter);
            workspace = Some(e.workspace.clone());
        }
        if let Some(w) = workspace {
            prune(&mut st, &w);
        }
    }
    let _ = s
        .events
        .send(TaskEvent::Exited(ExitedParams {
            task_id: s.id.clone(),
            status: exit,
        }))
        .await;
    let _ = s.done.send(true);
}

fn prune(st: &mut State, workspace: &WorkspaceId) {
    let mut finished: Vec<(u64, TaskId)> = st
        .tasks
        .iter()
        .filter(|(_, e)| &e.workspace == workspace)
        .filter_map(|(id, e)| e.finished_at.map(|f| (f, id.clone())))
        .collect();
    if finished.len() > HISTORY_PER_WORKSPACE {
        finished.sort_by_key(|(f, _)| *f);
        let excess = finished.len() - HISTORY_PER_WORKSPACE;
        for (_, id) in finished.into_iter().take(excess) {
            st.tasks.remove(&id);
        }
    }
}

#[cfg(unix)]
fn signal_of(st: &std::process::ExitStatus) -> Option<i32> {
    use std::os::unix::process::ExitStatusExt;
    st.signal()
}

#[cfg(not(unix))]
fn signal_of(_: &std::process::ExitStatus) -> Option<i32> {
    None
}
