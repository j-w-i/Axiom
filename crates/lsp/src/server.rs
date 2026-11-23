//! One language server for one workspace: the process, the LSP conversation with it (the daemon's
//! own id space, shared by every client), the document mirror, diagnostics, and the restart
//! policy (ADR-0018 §2).

use crate::docs::Documents;
use crate::framing::{read_message, write_message};
use crate::restricted::Restriction;
use crate::{Event, LspError};
use axiom_protocol::lsp::{ServerInfo, ServerState};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, watch};
use tokio_util::sync::CancellationToken;

/// Crashes allowed within [`CRASH_WINDOW`] before the server is left `failed`.
pub const MAX_CRASHES: usize = 3;
pub const CRASH_WINDOW: Duration = Duration::from_secs(180);
/// How long a request waits for the server to finish starting.
pub const READY_TIMEOUT: Duration = Duration::from_secs(60);
/// `shutdown` + `exit`, then a kill.
pub const STOP_GRACE: Duration = Duration::from_secs(2);

/// Messages waiting for a server to read them, at most: more means it stopped reading.
const OUT_QUEUE: usize = 8192;
const STDERR_LINES: usize = 200;

/// A server definition from a provider manifest (`[[lsp]]`), with the provider's id.
#[derive(Debug, Clone, PartialEq)]
pub struct Definition {
    pub id: String,
    pub provider_id: String,
    pub languages: Vec<String>,
    pub command: Vec<String>,
    pub initialization_options: Option<Value>,
    pub install_hint: Option<String>,
    /// Tool ids that must resolve for the server to run (M5.5, ADR-0021 §6).
    pub requires: Vec<String>,
}

/// Where and how the process runs: resolved by the daemon (the workspace's environment).
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub root: PathBuf,
    /// The complete environment of the process (it replaces the daemon's).
    pub env: BTreeMap<String, String>,
    /// Settings `lsp.<id>.settings`: the answer to `workspace/configuration`.
    pub settings: Value,
    /// Settings `lsp.<id>.initializationOptions`, merged over the manifest's.
    pub initialization_options: Option<Value>,
    /// Why the server cannot run yet, and the provider whose install would fix it (a required
    /// tool that does not resolve). The server is `missing` with that detail.
    pub blocked: Option<(String, String)>,
    /// The provider to install when the command itself is not found (the server's own).
    pub install_provider: Option<String>,
    /// How it runs in a folder the person has not trusted (AUD-008): set by
    /// [`crate::restricted::restrict`].
    pub restriction: crate::restricted::Restriction,
}

/// The client capabilities Axiom declares: what the editor's client implements (M4-LSP-002).
pub fn client_capabilities() -> Value {
    json!({
        "general": { "positionEncodings": ["utf-16"] },
        // csharp-ls answers a definition in a .NET assembly with a `csharp:/metadata/…` URI,
        // whose source the shell asks for with `csharp/metadata`, only when told the client
        // can (M5.5-QA-001); other servers ignore it.
        "experimental": { "csharp": { "metadataUris": true } },
        "workspace": {
            "configuration": true,
            "workspaceFolders": true,
            "applyEdit": false,
            "workspaceEdit": { "documentChanges": true }
        },
        "window": { "workDoneProgress": true, "showMessage": {} },
        "textDocument": {
            "synchronization": { "didSave": true, "dynamicRegistration": false },
            "completion": {
                "contextSupport": true,
                "completionItem": {
                    "snippetSupport": true,
                    "insertReplaceSupport": true,
                    "labelDetailsSupport": true,
                    "documentationFormat": ["markdown", "plaintext"],
                    "resolveSupport": { "properties": ["documentation", "detail", "additionalTextEdits"] }
                }
            },
            "hover": { "contentFormat": ["markdown", "plaintext"] },
            "signatureHelp": {
                "signatureInformation": {
                    "documentationFormat": ["markdown", "plaintext"],
                    "parameterInformation": { "labelOffsetSupport": true },
                    "activeParameterSupport": true
                }
            },
            "definition": { "linkSupport": true },
            "references": {},
            "documentHighlight": {},
            "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": { "valueSet": ["", "quickfix", "refactor", "refactor.extract", "refactor.inline", "refactor.rewrite", "source", "source.organizeImports"] }
                },
                "resolveSupport": { "properties": ["edit"] }
            },
            "formatting": {},
            "rangeFormatting": {},
            "rename": { "prepareSupport": true },
            "publishDiagnostics": { "relatedInformation": true, "versionSupport": true }
        }
    })
}

/// A file URI for a host path.
pub fn file_uri(p: &Path) -> String {
    url::Url::from_file_path(p)
        .map_or_else(|_| format!("file://{}", p.display()), |u| u.to_string())
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// The first file named `program` in `PATH` (from `env`), or `program` itself if it is a path.
pub fn which(program: &str, env: &BTreeMap<String, String>) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.components().count() > 1 {
        return p.is_file().then(|| p.to_path_buf());
    }
    let path = env.get("PATH").map(String::as_str).unwrap_or_default();
    let exts: Vec<String> = if cfg!(windows) {
        env.get("PATHEXT")
            .map_or(".EXE;.CMD;.BAT", String::as_str)
            .split(';')
            .map(|e| e.to_ascii_lowercase())
            .chain([String::new()])
            .collect()
    } else {
        vec![String::new()]
    };
    std::env::split_paths(path).find_map(|dir| {
        exts.iter().find_map(|e| {
            let c = dir.join(format!("{program}{e}"));
            is_executable(&c).then_some(c)
        })
    })
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

type Reply = Result<Value, LspError>;

/// One running process and its conversation.
struct Conn {
    out: mpsc::Sender<Value>,
    pending: Mutex<HashMap<i64, oneshot::Sender<Reply>>>,
    next_id: AtomicI64,
    stderr: Arc<Mutex<VecDeque<String>>>,
    child: tokio::sync::Mutex<Child>,
    /// The server's process group (it is spawned as the group's leader), so its children (node
    /// workers, a JVM's, gradle daemons) are stopped with it. Unix only.
    #[cfg_attr(not(unix), allow(dead_code))]
    pgid: Option<u32>,
    /// The server's output pipe (`pipe:[inode]`, Linux): a process that left the server's group
    /// and tree (kotlin-lsp's launcher starts its JVM with setsid and exits) still holds it.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pipe: Option<String>,
    /// Becomes true when the process has exited (or its stdout closed).
    exited: watch::Receiver<bool>,
}

impl Conn {
    /// The server's descendants now, and the processes holding its output pipe, each with its
    /// start time (Linux; elsewhere its process group covers them).
    fn descendants(&self) -> Vec<(u32, String)> {
        #[cfg(target_os = "linux")]
        if let Some(pgid) = self.pgid {
            let mut pids = descendants(pgid);
            pids.extend(self.pipe_holders());
            pids.sort_unstable();
            pids.dedup();
            return pids
                .into_iter()
                .filter_map(|pid| start_time(pid).map(|s| (pid, s)))
                .collect();
        }
        Vec::new()
    }

    /// Other processes that hold the server's output pipe open (Linux).
    #[cfg(target_os = "linux")]
    fn pipe_holders(&self) -> Vec<u32> {
        let Some(pipe) = &self.pipe else {
            return Vec::new();
        };
        let me = std::process::id();
        let mut out = Vec::new();
        let Ok(procs) = std::fs::read_dir("/proc") else {
            return out;
        };
        for p in procs.flatten() {
            let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            if pid == me {
                continue;
            }
            let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else {
                continue;
            };
            if fds.flatten().any(|fd| {
                std::fs::read_link(fd.path()).is_ok_and(|l| l.as_os_str() == pipe.as_str())
            }) {
                out.push(pid);
            }
        }
        out
    }

    /// Signals the server's whole process group: SIGKILL when [hard], else SIGTERM. A group
    /// already gone is fine.
    fn signal_group(&self, hard: bool) {
        #[cfg(unix)]
        if let Some(pgid) = self.pgid {
            use nix::sys::signal::{killpg, Signal};
            let sig = if hard {
                Signal::SIGKILL
            } else {
                Signal::SIGTERM
            };
            // Descendants that left the group for a session of their own (kotlin-lsp's launcher
            // starts its JVM with setsid) are found by their parents while the server runs.
            #[cfg(target_os = "linux")]
            for pid in descendants(pgid).into_iter().chain(self.pipe_holders()) {
                let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), sig);
            }
            let _ = killpg(nix::unistd::Pid::from_raw(pgid as i32), sig);
        }
        #[cfg(not(unix))]
        let _ = hard;
    }
}

/// SIGKILLs a stopped server's descendants that outlived it: each only while its start time is
/// the one recorded, so a process that reused the number is never touched.
fn signal_pids(pids: &[(u32, String)]) {
    #[cfg(target_os = "linux")]
    for (pid, started) in pids {
        if start_time(*pid).as_ref() == Some(started) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(*pid as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = pids;
}

/// A process's start time (`/proc/<pid>/stat` field 22), which a reused number does not share.
#[cfg(target_os = "linux")]
fn start_time(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Fields after `(comm) `: state is field 3, so the start time (22) is the 20th.
    let (_, rest) = stat.rsplit_once(") ")?;
    rest.split_whitespace().nth(19).map(String::from)
}

/// Every process descended from [root], from `/proc`'s parent links.
#[cfg(target_os = "linux")]
fn descendants(root: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    if let Ok(rd) = std::fs::read_dir("/proc") {
        for e in rd.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            // `pid (comm) state ppid …`; the name may hold spaces and parentheses.
            let ppid = stat
                .rsplit_once(") ")
                .and_then(|(_, rest)| rest.split_whitespace().nth(1))
                .and_then(|p| p.parse::<u32>().ok());
            if let Some(ppid) = ppid {
                children.entry(ppid).or_default().push(pid);
            }
        }
    }
    let mut out = Vec::new();
    let mut todo = vec![root];
    while let Some(p) = todo.pop() {
        for &c in children.get(&p).map(Vec::as_slice).unwrap_or_default() {
            out.push(c);
            todo.push(c);
        }
    }
    out
}

impl Drop for Conn {
    fn drop(&mut self) {
        // The leader alone would be killed on drop; its children would live on.
        self.signal_group(true);
    }
}

impl Conn {
    fn send(&self, v: Value) -> bool {
        match self.out.try_send(v) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                // The server has not read its input for [OUT_QUEUE] messages: it is hung. It is
                // killed, which counts as a crash and restarts it (audit finding 15).
                tracing::warn!("a language server stopped reading its input; killing it");
                self.signal_group(true);
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    fn notify(&self, method: &str, params: Value) -> bool {
        self.send(json!({"jsonrpc":"2.0","method":method,"params":params}))
    }

    /// Sends a request; the reply arrives on the returned receiver.
    fn request(&self, method: &str, params: Value) -> (i64, oneshot::Receiver<Reply>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        // The reader sets `exited` before it fails what is pending, so a request registered after
        // that sees the flag here.
        if *self.exited.borrow()
            || !self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
        {
            if let Some(tx) = lock(&self.pending).remove(&id) {
                let _ = tx.send(Err(LspError::Exited("The language server stopped.".into())));
            }
        }
        (id, rx)
    }

    fn stderr_tail(&self, n: usize) -> String {
        let s = lock(&self.stderr);
        let skip = s.len().saturating_sub(n);
        s.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
    }
}

struct Inner {
    state: ServerState,
    detail: Option<String>,
    capabilities: Option<Value>,
    conn: Option<Arc<Conn>>,
    crashes: VecDeque<Instant>,
    /// Incremented per start; a crash of an older generation is ignored.
    generation: u64,
}

/// A language server of one workspace.
pub struct LanguageServer {
    pub def: Definition,
    workspace_id: axiom_protocol::WorkspaceId,
    launch: Mutex<Launch>,
    inner: Mutex<Inner>,
    /// Sends of sync notifications, in order, across a (re)start.
    send_lock: tokio::sync::Mutex<()>,
    docs: Mutex<Documents>,
    diagnostics: Mutex<BTreeMap<String, Value>>,
    ready: watch::Sender<u64>,
    events: tokio::sync::broadcast::Sender<Event>,
    stopping: AtomicBool,
    /// Its workspace closed or the daemon is stopping: it is never started again.
    closed: AtomicBool,
}

impl LanguageServer {
    pub fn new(
        workspace_id: axiom_protocol::WorkspaceId,
        def: Definition,
        launch: Launch,
        events: tokio::sync::broadcast::Sender<Event>,
    ) -> Arc<Self> {
        // Refused in an untrusted folder: `stopped` with why from the start (AUD-008).
        let (state, detail) = match &launch.restriction {
            Restriction::Refused { detail } => (ServerState::Stopped, Some(detail.clone())),
            _ => (ServerState::Idle, None),
        };
        Arc::new(Self {
            def,
            workspace_id,
            launch: Mutex::new(launch),
            inner: Mutex::new(Inner {
                state,
                detail,
                capabilities: None,
                conn: None,
                crashes: VecDeque::new(),
                generation: 0,
            }),
            send_lock: tokio::sync::Mutex::new(()),
            docs: Mutex::new(Documents::default()),
            diagnostics: Mutex::new(BTreeMap::new()),
            ready: watch::channel(0).0,
            events,
            stopping: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        })
    }

    /// Another server serves this one's languages: it is `standby` and never starts (M5.5,
    /// ADR-0021 §3); `false` returns it to `idle` when it was on standby.
    pub fn set_standby(&self, on: bool) {
        let now = self.state();
        if on
            && matches!(
                now,
                ServerState::Idle | ServerState::Missing | ServerState::Stopped
            )
        {
            self.set_state(ServerState::Standby, None);
        } else if !on && now == ServerState::Standby {
            self.set_state(ServerState::Idle, None);
        }
    }

    /// Replaces how the next start runs (the environment, settings or trust changed). A server
    /// that was refused for an untrusted folder and no longer is becomes `idle` again.
    pub fn set_launch(&self, launch: Launch) {
        let refused = match &launch.restriction {
            Restriction::Refused { detail } => Some(detail.clone()),
            _ => None,
        };
        let was = std::mem::replace(&mut *lock(&self.launch), launch);
        let was_refused = matches!(was.restriction, Restriction::Refused { .. });
        match (refused, self.state()) {
            (Some(detail), ServerState::Idle | ServerState::Stopped) => {
                self.set_state(ServerState::Stopped, Some(detail));
            }
            (None, ServerState::Stopped) if was_refused => {
                self.set_state(ServerState::Idle, None);
            }
            _ => {}
        }
    }

    /// How it runs (or would) in its folder: as configured, restricted, refused (AUD-008).
    pub fn restriction(&self) -> Restriction {
        lock(&self.launch).restriction.clone()
    }

    /// Refused until the folder is trusted: why.
    fn refused(&self) -> Option<String> {
        match &lock(&self.launch).restriction {
            Restriction::Refused { detail } => Some(detail.clone()),
            _ => None,
        }
    }

    pub fn info(&self) -> ServerInfo {
        let i = lock(&self.inner);
        ServerInfo {
            server_id: self.def.id.clone(),
            provider_id: self.def.provider_id.clone(),
            languages: self.def.languages.clone(),
            state: i.state,
            detail: i.detail.clone(),
            install_hint: (i.state == ServerState::Missing)
                .then(|| self.def.install_hint.clone())
                .flatten(),
            install_provider: (i.state == ServerState::Missing)
                .then(|| {
                    let l = lock(&self.launch);
                    l.blocked
                        .as_ref()
                        .map(|(_, p)| p.clone())
                        .or_else(|| l.install_provider.clone())
                })
                .flatten(),
            capabilities: i.capabilities.clone(),
        }
    }

    pub fn state(&self) -> ServerState {
        lock(&self.inner).state
    }

    /// Diagnostics last published for each URI (non-empty lists only).
    pub fn diagnostics(&self) -> BTreeMap<String, Value> {
        lock(&self.diagnostics).clone()
    }

    /// The workspace this server belongs to.
    pub fn workspace_id(&self) -> &axiom_protocol::WorkspaceId {
        &self.workspace_id
    }

    /// The URIs of the documents clients have open on it.
    pub fn open_uris(&self) -> Vec<String> {
        lock(&self.docs).all().map(|d| d.uri.clone()).collect()
    }

    /// The mirrored text of an open document.
    pub fn document(&self, uri: &str) -> Option<crate::docs::Document> {
        lock(&self.docs).get(uri).cloned()
    }

    pub fn handles(&self, language_id: &str) -> bool {
        self.def.languages.iter().any(|l| l == language_id)
    }

    fn set_state(&self, state: ServerState, detail: Option<String>) {
        {
            let mut i = lock(&self.inner);
            if i.state == state && i.detail == detail {
                return;
            }
            i.state = state;
            i.detail = detail;
        }
        let _ = self.events.send(Event::State {
            workspace_id: self.workspace_id.clone(),
            info: self.info(),
        });
    }

    /// Ends the start of generation [gen] in [state] (missing or failed). The generation moves
    /// on, so the exit of the process it may have spawned is not counted as a crash (which would
    /// restart it and lose this state); a newer start is left alone.
    fn end_start(&self, gen: u64, state: ServerState, detail: Option<String>) {
        {
            let mut i = lock(&self.inner);
            if i.generation != gen {
                return;
            }
            i.generation += 1;
            i.conn = None;
            i.capabilities = None;
            i.state = state;
            i.detail = detail;
        }
        let _ = self.events.send(Event::State {
            workspace_id: self.workspace_id.clone(),
            info: self.info(),
        });
        self.ready.send_modify(|v| *v += 1);
    }

    /// The connection of a ready server, starting it (and waiting, bounded) if needed.
    async fn ready_conn(self: &Arc<Self>) -> Result<Arc<Conn>, LspError> {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        let mut rx = self.ready.subscribe();
        loop {
            let start = {
                let i = lock(&self.inner);
                match i.state {
                    ServerState::Ready => {
                        if let Some(c) = &i.conn {
                            return Ok(c.clone());
                        }
                        false
                    }
                    ServerState::Idle | ServerState::Stopped => {
                        // Refused in an untrusted folder: say so now, not after a 60 s wait.
                        if let Some(detail) = self.refused() {
                            return Err(LspError::Unavailable(ServerState::Stopped, Some(detail)));
                        }
                        true
                    }
                    ServerState::Starting => false,
                    s => return Err(LspError::Unavailable(s, i.detail.clone())),
                }
            };
            if start {
                self.stopping.store(false, Ordering::SeqCst);
                self.spawn_start();
            }
            tokio::select! {
                r = rx.changed() => { if r.is_err() { return Err(LspError::Exited("The language server stopped.".into())); } }
                () = tokio::time::sleep_until(deadline) => {
                    return Err(LspError::Unavailable(ServerState::Starting, Some("The language server did not finish starting within 60 seconds.".into())));
                }
            }
        }
    }

    fn spawn_start(self: &Arc<Self>) {
        if self.closed.load(Ordering::SeqCst) {
            return;
        }
        let gen = {
            let mut i = lock(&self.inner);
            if i.state == ServerState::Starting {
                return;
            }
            i.generation += 1;
            i.state = ServerState::Starting;
            i.detail = None;
            i.generation
        };
        let _ = self.events.send(Event::State {
            workspace_id: self.workspace_id.clone(),
            info: self.info(),
        });
        let me = self.clone();
        tokio::spawn(async move { me.start(gen).await });
    }

    async fn start(self: Arc<Self>, gen: u64) {
        let launch = lock(&self.launch).clone();
        let Some(program) = self.def.command.first() else {
            self.end_start(
                gen,
                ServerState::Failed,
                Some("The server definition has no command.".into()),
            );
            return;
        };
        if let Some((detail, _)) = &launch.blocked {
            self.end_start(gen, ServerState::Missing, Some(detail.clone()));
            return;
        }
        // Not in an untrusted folder (AUD-008): stopped, with why.
        if let Restriction::Refused { detail } = &launch.restriction {
            self.end_start(gen, ServerState::Stopped, Some(detail.clone()));
            return;
        }
        let Some(exe) = which(program, &launch.env) else {
            self.end_start(
                gen,
                ServerState::Missing,
                Some(format!("{program} was not found.")),
            );
            return;
        };
        let conn = match spawn_conn(&exe, &self.def.command[1..], &launch, self.clone(), gen) {
            Ok(c) => c,
            Err(e) => {
                self.end_start(
                    gen,
                    ServerState::Failed,
                    Some(format!("{program} could not be started: {e}")),
                );
                return;
            }
        };
        let root_uri = file_uri(&launch.root);
        let name = launch
            .root
            .file_name()
            .map_or_else(|| "workspace".into(), |n| n.to_string_lossy().into_owned());
        let mut init_opts = self.def.initialization_options.clone();
        if let Some(o) = launch.initialization_options.clone() {
            init_opts = Some(merge(init_opts.unwrap_or(Value::Null), o));
        }
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": {"name": "Axiom", "version": env!("CARGO_PKG_VERSION")},
            "rootUri": root_uri,
            "rootPath": launch.root.to_string_lossy(),
            "workspaceFolders": [{"uri": root_uri, "name": name}],
            "capabilities": client_capabilities(),
            "initializationOptions": init_opts.unwrap_or(Value::Null),
            "trace": "off",
        });
        let (_, rx) = conn.request("initialize", params);
        let mut exited = conn.exited.clone();
        let result = tokio::select! {
            r = rx => r.unwrap_or_else(|_| Err(LspError::Exited("The language server stopped while starting.".into()))),
            _ = exited.wait_for(|e| *e) => Err(LspError::Exited("The language server exited while starting.".into())),
            () = tokio::time::sleep(READY_TIMEOUT) => Err(LspError::Exited("The language server did not answer initialize within 60 seconds.".into())),
        };
        let caps = match result {
            Ok(v) => v.get("capabilities").cloned().unwrap_or(Value::Null),
            Err(e) => {
                let mut tail = conn.stderr_tail(5);
                if tail.is_empty() {
                    // A process that exits at once may not have had its stderr read yet.
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    tail = conn.stderr_tail(5);
                }
                let detail = if tail.is_empty() {
                    e.to_string()
                } else {
                    format!("{e} {tail}")
                };
                // A toolchain manager's stub for a component that is not installed (rustup's
                // proxy: "Unknown binary 'rust-analyzer' in official toolchain") means the server
                // is missing, not broken: the shell offers its install (ADR-0021 §5).
                // Homebrew's rustup says "`rust-analyzer` is unavailable for the active
                // toolchain" (and then loops on its own fallback).
                let missing = tail.contains("Unknown binary")
                    || tail.contains("is not installed for the toolchain")
                    || tail.contains("is unavailable for the active toolchain");
                // The generation moves on first, so the process's exit is not taken for a crash.
                self.end_start(
                    gen,
                    if missing {
                        ServerState::Missing
                    } else {
                        ServerState::Failed
                    },
                    Some(detail),
                );
                let _ = conn.child.lock().await.start_kill();
                return;
            }
        };
        conn.notify("initialized", json!({}));
        {
            // Documents opened before (or during) the start, or before a crash, are opened again,
            // in order, ahead of anything a client sends next.
            let _g = self.send_lock.lock().await;
            for d in lock(&self.docs).all() {
                conn.notify(
                    "textDocument/didOpen",
                    json!({"textDocument":{"uri":d.uri,"languageId":d.language_id,"version":d.version,"text":d.text}}),
                );
            }
            let mut i = lock(&self.inner);
            if i.generation != gen {
                return;
            }
            i.conn = Some(conn);
            i.capabilities = Some(caps);
            // Ready in the same step as the connection, under the send lock: a notification
            // waiting on that lock finds the server ready and sends, rather than seeing it
            // starting and leaving the change to a replay that has already happened.
            i.state = ServerState::Ready;
            // A restricted server says so while it runs.
            i.detail = match &launch.restriction {
                Restriction::Restricted { note } => Some(note.clone()),
                _ => None,
            };
        }
        let _ = self.events.send(Event::State {
            workspace_id: self.workspace_id.clone(),
            info: self.info(),
        });
        self.ready.send_modify(|v| *v += 1);
    }

    /// The process of generation `gen` ended.
    fn on_exit(self: &Arc<Self>, gen: u64, stderr: String) {
        let restart = {
            let mut i = lock(&self.inner);
            if i.generation != gen {
                return;
            }
            i.conn = None;
            i.capabilities = None;
            if self.stopping.load(Ordering::SeqCst) {
                false
            } else {
                let now = Instant::now();
                i.crashes.push_back(now);
                while i
                    .crashes
                    .front()
                    .is_some_and(|t| now.duration_since(*t) > CRASH_WINDOW)
                {
                    i.crashes.pop_front();
                }
                i.crashes.len() < MAX_CRASHES
            }
        };
        lock(&self.diagnostics).clear();
        if self.stopping.load(Ordering::SeqCst) {
            self.set_state(ServerState::Stopped, None);
        } else if restart {
            tracing::warn!(server = %self.def.id, "language server exited; restarting");
            self.set_state(
                ServerState::Stopped,
                Some("It exited; starting it again.".into()),
            );
            self.spawn_start();
        } else {
            let detail =
                format!(
                "It stopped {MAX_CRASHES} times in 3 minutes, so Axiom stopped restarting it.{}",
                if stderr.is_empty() { String::new() } else { format!(" Last output: {stderr}") }
            );
            self.set_state(ServerState::Failed, Some(detail));
        }
        self.ready.send_modify(|v| *v += 1);
    }

    /// One request; cancelling `cancel` sends `$/cancelRequest` and returns `Cancelled`.
    pub async fn request(
        self: &Arc<Self>,
        method: &str,
        params: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, LspError> {
        let conn = tokio::select! {
            c = self.ready_conn() => c?,
            () = cancel.cancelled() => return Err(LspError::Cancelled),
        };
        let (id, rx) = conn.request(method, params);
        tokio::select! {
            r = rx => r.unwrap_or_else(|_| Err(LspError::Exited("The language server stopped.".into()))),
            () = cancel.cancelled() => {
                lock(&conn.pending).remove(&id);
                conn.notify("$/cancelRequest", json!({"id": id}));
                Err(LspError::Cancelled)
            }
        }
    }

    /// A notification from a client: mirrored, then sent (starting the server if needed).
    pub async fn notify(self: &Arc<Self>, method: &str, params: Value) -> Result<(), LspError> {
        let _g = self.send_lock.lock().await;
        {
            let mut docs = lock(&self.docs);
            if docs.is_stale(method, &params) {
                // Older than what the mirror and the server already have: applying it would
                // put the change on the wrong text.
                tracing::warn!(server = %self.def.id, "dropped a didChange that arrived out of order");
                return Ok(());
            }
            docs.apply(method, &params);
        }
        if method == "textDocument/didClose" {
            if let Some(uri) = params["textDocument"]["uri"].as_str() {
                lock(&self.diagnostics).remove(uri);
            }
        }
        let conn = {
            let i = lock(&self.inner);
            (i.state == ServerState::Ready)
                .then(|| i.conn.clone())
                .flatten()
        };
        match conn {
            Some(c) => {
                c.notify(method, params);
                Ok(())
            }
            None => {
                // Not ready: the mirror has it, and a start (re)opens mirrored documents. Start
                // now if idle; report a server that cannot run.
                match self.state() {
                    // Closing a document never starts a server (it may be closing with its
                    // workspace).
                    _ if method == "textDocument/didClose" => Ok(()),
                    ServerState::Idle | ServerState::Stopped => {
                        if let Some(detail) = self.refused() {
                            return Err(LspError::Unavailable(ServerState::Stopped, Some(detail)));
                        }
                        self.stopping.store(false, Ordering::SeqCst);
                        self.spawn_start();
                        Ok(())
                    }
                    ServerState::Starting => Ok(()),
                    s => Err(LspError::Unavailable(s, lock(&self.inner).detail.clone())),
                }
            }
        }
    }

    /// Stops the server (`shutdown`, `exit`, then a kill after the grace period).
    pub async fn stop(self: &Arc<Self>) {
        self.stopping.store(true, Ordering::SeqCst);
        let conn = lock(&self.inner).conn.clone();
        if let Some(c) = conn {
            // Its descendants, taken while it runs: once it has exited, those in a session of
            // their own are no longer linked to it.
            let family = c.descendants();
            let (_, rx) = c.request("shutdown", Value::Null);
            let _ = tokio::time::timeout(STOP_GRACE, rx).await;
            c.notify("exit", Value::Null);
            let mut exited = c.exited.clone();
            if tokio::time::timeout(STOP_GRACE, exited.wait_for(|e| *e))
                .await
                .is_err()
            {
                c.signal_group(true);
            }
            let _ =
                tokio::time::timeout(STOP_GRACE, async { c.child.lock().await.wait().await }).await;
            // What the server started and left behind goes with it.
            c.signal_group(true);
            signal_pids(&family);
        }
        let mut i = lock(&self.inner);
        i.conn = None;
        i.capabilities = None;
        i.generation += 1;
        drop(i);
        lock(&self.diagnostics).clear();
        self.set_state(ServerState::Stopped, None);
        self.ready.send_modify(|v| *v += 1);
    }

    /// Never starts again (its workspace closed, or the daemon is stopping).
    pub fn close(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.stopping.store(true, Ordering::SeqCst);
    }

    /// Kills the server's process group now, without the shutdown handshake.
    pub fn kill_now(&self) {
        if let Some(c) = lock(&self.inner).conn.clone() {
            c.signal_group(true);
        }
    }

    /// Stops and starts again, also out of `failed` or `missing`.
    pub async fn restart(self: &Arc<Self>) {
        self.stop().await;
        lock(&self.inner).crashes.clear();
        self.stopping.store(false, Ordering::SeqCst);
        self.spawn_start();
    }

    /// The answer to a request the server sent.
    fn answer(&self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        let launch = lock(&self.launch);
        match method {
            "workspace/configuration" => {
                let items = params["items"].as_array().cloned().unwrap_or_default();
                Ok(Value::Array(
                    items
                        .iter()
                        .map(|it| match it["section"].as_str() {
                            Some(sec) if !sec.is_empty() => sec
                                .split('.')
                                .try_fold(&launch.settings, |v, k| v.get(k))
                                .cloned()
                                .unwrap_or(Value::Null),
                            _ => launch.settings.clone(),
                        })
                        .collect(),
                ))
            }
            "client/registerCapability"
            | "client/unregisterCapability"
            | "window/workDoneProgress/create"
            | "window/showMessageRequest"
            | "workspace/semanticTokens/refresh"
            | "workspace/inlayHint/refresh"
            | "workspace/codeLens/refresh"
            | "workspace/diagnostic/refresh" => Ok(Value::Null),
            "workspace/workspaceFolders" => {
                let uri = file_uri(&launch.root);
                Ok(
                    json!([{"uri": uri, "name": launch.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()}]),
                )
            }
            "workspace/applyEdit" => Ok(json!({
                "applied": false,
                "failureReason": "Axiom applies edits in the editor, not on the language server's request."
            })),
            _ => Err((-32601, format!("Axiom does not handle {method}."))),
        }
    }

    fn on_notification(&self, method: &str, params: Value) {
        if method == "textDocument/publishDiagnostics" {
            if let Some(uri) = params["uri"].as_str() {
                let mut d = lock(&self.diagnostics);
                match params["diagnostics"].as_array() {
                    Some(a) if !a.is_empty() => {
                        d.insert(uri.to_string(), Value::Array(a.clone()));
                    }
                    _ => {
                        d.remove(uri);
                    }
                }
            }
        }
        if matches!(
            method,
            "textDocument/publishDiagnostics"
                | "$/progress"
                | "window/showMessage"
                | "window/logMessage"
        ) {
            let _ = self.events.send(Event::Notification {
                workspace_id: self.workspace_id.clone(),
                server_id: self.def.id.clone(),
                method: method.to_string(),
                params,
            });
        }
    }
}

/// Deep-merges `b` over `a` (objects key by key; anything else replaced).
fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let old = a.remove(&k).unwrap_or(Value::Null);
                a.insert(k, merge(old, v));
            }
            Value::Object(a)
        }
        (_, b) => b,
    }
}

fn spawn_conn(
    exe: &Path,
    args: &[String],
    launch: &Launch,
    server: Arc<LanguageServer>,
    gen: u64,
) -> std::io::Result<Arc<Conn>> {
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .current_dir(&launch.root)
        .env_clear()
        .envs(&launch.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    cmd.process_group(0);
    let mut child = cmd.spawn()?;
    let pgid = child.id();
    // The daemon's orphan watchdog ends the group if the daemon dies (AUD-027).
    #[cfg(unix)]
    if let (Some(h), Some(g)) = (GROUP_HOOKS.get(), pgid) {
        (h.0)(g);
    }
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no stdin"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("no stdout"))?;
    #[cfg(target_os = "linux")]
    let pipe = {
        use std::os::fd::AsRawFd;
        std::fs::read_link(format!("/proc/self/fd/{}", stdout.as_raw_fd()))
            .ok()
            .map(|l| l.to_string_lossy().into_owned())
    };
    #[cfg(not(target_os = "linux"))]
    let pipe = None;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("no stderr"))?;
    let (out_tx, mut out_rx) = mpsc::channel::<Value>(OUT_QUEUE);
    let (exit_tx, exit_rx) = watch::channel(false);
    let conn = Arc::new(Conn {
        out: out_tx,
        pending: Mutex::new(HashMap::new()),
        next_id: AtomicI64::new(1),
        stderr: Arc::new(Mutex::new(VecDeque::new())),
        pgid,
        pipe,
        child: tokio::sync::Mutex::new(child),
        exited: exit_rx,
    });
    // Writer.
    tokio::spawn(async move {
        let mut stdin = stdin;
        while let Some(v) = out_rx.recv().await {
            if write_message(&mut stdin, &v).await.is_err() {
                break;
            }
        }
    });
    // Stderr: kept as the last lines, for errors.
    let tail = conn.stderr.clone();
    let id = server.def.id.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(l)) = lines.next_line().await {
            tracing::debug!(server = %id, "stderr: {l}");
            let mut t = lock(&tail);
            if t.len() >= STDERR_LINES {
                t.pop_front();
            }
            t.push_back(l);
        }
    });
    // Reader: responses, the server's requests and notifications.
    let weak = Arc::downgrade(&conn);
    tokio::spawn(async move {
        let mut r = BufReader::new(stdout);
        loop {
            let msg = match read_message(&mut r).await {
                Ok(Some(m)) => m,
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(server = %server.def.id, "bad message from the language server: {e}");
                    break;
                }
            };
            let Some(conn) = weak.upgrade() else { break };
            let method = msg
                .get("method")
                .and_then(Value::as_str)
                .map(str::to_string);
            let id = msg.get("id").cloned().filter(|v| !v.is_null());
            match (method, id) {
                (Some(m), Some(id)) => {
                    let reply = match server.answer(&m, &msg["params"]) {
                        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                        Err((code, message)) => {
                            json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
                        }
                    };
                    conn.send(reply);
                }
                (Some(m), None) => {
                    server.on_notification(&m, msg.get("params").cloned().unwrap_or(Value::Null))
                }
                (None, Some(id)) => {
                    let Some(n) = id.as_i64() else { continue };
                    let Some(tx) = lock(&conn.pending).remove(&n) else {
                        continue;
                    };
                    let reply = match msg.get("error") {
                        Some(e) if !e.is_null() => Err(LspError::Server {
                            code: e["code"].as_i64().unwrap_or(-32603),
                            message: e["message"]
                                .as_str()
                                .unwrap_or("The language server reported an error.")
                                .to_string(),
                            data: e.get("data").cloned(),
                        }),
                        _ => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                    };
                    let _ = tx.send(reply);
                }
                (None, None) => {}
            }
        }
        // The stream ended. In this order: mark the connection dead, report the exit (the server
        // drops the connection, so no new request reaches it), fail what is pending, reap.
        let _ = exit_tx.send(true);
        if let Some(conn) = weak.upgrade() {
            // Give stderr a moment to deliver the last lines for the report.
            tokio::time::sleep(Duration::from_millis(50)).await;
            let tail = conn.stderr_tail(5);
            server.on_exit(gen, tail);
            for (_, tx) in lock(&conn.pending).drain() {
                let _ = tx.send(Err(LspError::Exited("The language server stopped.".into())));
            }
            let _ =
                tokio::time::timeout(STOP_GRACE, async { conn.child.lock().await.wait().await })
                    .await;
            let _ = conn.child.lock().await.start_kill();
        }
        #[cfg(unix)]
        if let (Some(h), Some(g)) = (GROUP_HOOKS.get(), pgid) {
            (h.1)(g);
        }
    });
    Ok(conn)
}

/// Called with a server's process group when it starts and when it has ended.
type GroupHook = fn(u32);
static GROUP_HOOKS: std::sync::OnceLock<(GroupHook, GroupHook)> = std::sync::OnceLock::new();

/// Lets the daemon register each server's process group with its orphan watchdog
/// (`axiom_exec::guard`, AUD-027) without this crate depending on `exec`. The first call wins.
pub fn set_group_hooks(started: GroupHook, ended: GroupHook) {
    let _ = GROUP_HOOKS.set((started, ended));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_is_deep() {
        let a = json!({"a":1,"b":{"c":1,"d":2}});
        let b = json!({"b":{"c":3},"e":4});
        assert_eq!(merge(a, b), json!({"a":1,"b":{"c":3,"d":2},"e":4}));
    }

    #[test]
    fn which_uses_the_given_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("fake-server");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut env = BTreeMap::new();
        env.insert(
            "PATH".to_string(),
            dir.path().to_string_lossy().into_owned(),
        );
        assert_eq!(which("fake-server", &env), Some(exe));
        assert_eq!(which("not-there", &env), None);
    }
}
