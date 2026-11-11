//! `axiom-desktop-bridge` — supervises one `axiomd` child process and speaks
//! protocol 0.1 (docs/protocols/ipc.md) over its stdio on behalf of a GUI
//! host. It is the Tauri-era equivalent of the Qt shell's `DaemonClient`
//! (apps/desktop/src/daemon/DaemonClient.cpp) and keeps its observable
//! behaviour:
//!
//! - lifecycle per docs/architecture/processes.md: spawn with piped stdio,
//!   `initialize` within 10 s, `shutdown` → wait 3 s → close stdin → wait
//!   2 s → kill;
//! - every request is answered exactly once (result, daemon error, `timeout`
//!   after which `$/cancel` is sent, or `internal` "Backend stopped");
//! - a crashed or exited daemon moves the bridge to `failed` with the stderr
//!   tail, and `restart` starts a fresh process.
//!
//! One difference is deliberate: requests issued *for the UI* are not
//! answered through a future but forwarded into the same ordered event sink
//! as notifications ([`BridgeEvent::Response`]), pushed by the reader at the
//! moment the line is read. The UI therefore observes responses and
//! notifications in exact wire order — e.g. the `task/start` response always
//! precedes that task's first `task/output`, as ipc.md guarantees on the wire.
//!
//! The crate has no GUI dependency, so it is tested against the real daemon
//! without a webview (tests/bridge.rs).

use axiom_protocol::{ErrorKind, Message, RequestId, RpcError};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, watch, Notify};

/// Protocol 0.1 framing limit (ipc.md §Transport): a longer line is fatal.
pub const MAX_LINE_BYTES: usize = 64 * 1024 * 1024;
const STDERR_TAIL_LINES: usize = 5;

/// How the bridge launches and talks to the daemon.
#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub program: Option<PathBuf>,
    pub args: Vec<String>,
    pub client_name: String,
    pub client_version: String,
    pub initialize_timeout: Duration,
    pub default_timeout: Duration,
    pub shutdown_answer_wait: Duration,
    pub shutdown_exit_wait: Duration,
}

impl BridgeConfig {
    pub fn new(program: Option<PathBuf>) -> Self {
        Self {
            program,
            args: Vec::new(),
            client_name: "axiom-desktop-tauri".into(),
            client_version: env!("CARGO_PKG_VERSION").into(),
            initialize_timeout: Duration::from_secs(10),
            default_timeout: Duration::from_secs(30),
            shutdown_answer_wait: Duration::from_secs(3),
            shutdown_exit_wait: Duration::from_secs(2),
        }
    }
}

/// Connection state, as presented to the UI.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "state"
)]
pub enum DaemonState {
    Stopped,
    Starting,
    Ready {
        server_version: String,
        protocol_version: String,
        methods: Vec<String>,
    },
    Failed {
        message: String,
    },
}

/// Everything the bridge tells its host, in one ordered stream.
#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "type"
)]
pub enum BridgeEvent {
    /// The connection state changed. `generation` increments per process.
    State { generation: u64, state: DaemonState },
    /// A daemon → client notification, verbatim.
    Notification { method: String, params: Value },
    /// The answer to a request issued with [`DaemonBridge::request_forward`].
    Response {
        key: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<RpcError>,
    },
    /// One line of the daemon's stderr (free-form log text).
    Log { line: String },
}

pub type EventSink = Arc<dyn Fn(BridgeEvent) + Send + Sync>;

type Answer = Result<Value, RpcError>;

enum Waiter {
    Internal(oneshot::Sender<Answer>),
    Forward { key: u64 },
}

struct Conn {
    generation: u64,
    tx: Mutex<Option<mpsc::UnboundedSender<String>>>,
    pending: Mutex<HashMap<i64, Waiter>>,
    stopping: AtomicBool,
    kill: Notify,
    exited: watch::Receiver<bool>,
    pid: Option<u32>,
}

struct Inner {
    /// Runtime every background task runs on, so the bridge can be driven
    /// from synchronous callers (e.g. Tauri commands on the main thread).
    rt: tokio::runtime::Handle,
    config: BridgeConfig,
    sink: EventSink,
    conn: Mutex<Option<Arc<Conn>>>,
    state: Mutex<(u64, DaemonState)>,
    generation: AtomicU64,
    next_id: AtomicI64,
    stderr_tail: Mutex<VecDeque<String>>,
    /// Forwarded keys still in flight → (generation, request id), for cancel.
    forwarded: Mutex<HashMap<u64, (u64, i64)>>,
}

/// Cheap to clone; all clones share one daemon connection.
#[derive(Clone)]
pub struct DaemonBridge {
    inner: Arc<Inner>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn stopped() -> RpcError {
    RpcError::new(ErrorKind::Internal, "Backend stopped")
}

fn timed_out() -> RpcError {
    RpcError::new(ErrorKind::Timeout, "The backend did not respond in time.")
}

impl Conn {
    fn send(&self, line: String) -> bool {
        match lock(&self.tx).as_ref() {
            Some(tx) => tx.send(line).is_ok(),
            None => false,
        }
    }

    fn close_stdin(&self) {
        lock(&self.tx).take();
    }

    fn take_pending(&self, id: i64) -> Option<Waiter> {
        lock(&self.pending).remove(&id)
    }

    fn drain_pending(&self) -> Vec<Waiter> {
        lock(&self.pending).drain().map(|(_, w)| w).collect()
    }

    async fn wait_exit(&self, within: Duration) -> bool {
        let mut rx = self.exited.clone();
        let done = matches!(
            tokio::time::timeout(within, rx.wait_for(|e| *e)).await,
            Ok(Ok(_))
        );
        done
    }
}

impl DaemonBridge {
    /// Must be called inside a Tokio runtime (uses `Handle::current`).
    pub fn new(config: BridgeConfig, sink: EventSink) -> Self {
        Self::with_runtime(tokio::runtime::Handle::current(), config, sink)
    }

    pub fn with_runtime(rt: tokio::runtime::Handle, config: BridgeConfig, sink: EventSink) -> Self {
        Self {
            inner: Arc::new(Inner {
                rt,
                config,
                sink,
                conn: Mutex::new(None),
                state: Mutex::new((0, DaemonState::Stopped)),
                generation: AtomicU64::new(0),
                next_id: AtomicI64::new(0),
                stderr_tail: Mutex::new(VecDeque::new()),
                forwarded: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn state(&self) -> (u64, DaemonState) {
        lock(&self.inner.state).clone()
    }

    /// OS process id of the running daemon, if any (diagnostics, benchmarks).
    pub fn daemon_pid(&self) -> Option<u32> {
        self.current().and_then(|c| c.pid)
    }

    fn current(&self) -> Option<Arc<Conn>> {
        lock(&self.inner.conn).clone()
    }

    fn set_state(&self, generation: u64, state: DaemonState) {
        {
            let mut s = lock(&self.inner.state);
            // A late transition of a replaced process must not overwrite the current one.
            if generation < s.0 {
                return;
            }
            if s.0 == generation && s.1 == state {
                return;
            }
            *s = (generation, state.clone());
        }
        (self.inner.sink)(BridgeEvent::State { generation, state });
    }

    /// Spawns the daemon and performs `initialize`. Resolves when the state is
    /// `ready` or `failed`.
    pub async fn start(&self) -> DaemonState {
        let generation = self.inner.generation.fetch_add(1, Ordering::SeqCst) + 1;
        lock(&self.inner.stderr_tail).clear();
        self.set_state(generation, DaemonState::Starting);

        let Some(program) = self.inner.config.program.clone() else {
            let state = DaemonState::Failed {
                message: "The backend program (axiomd) was not found. Pass --daemon <path>, \
                          set AXIOMD, or install axiomd next to axiom."
                    .into(),
            };
            self.set_state(generation, state.clone());
            return state;
        };

        let mut cmd = Command::new(&program);
        cmd.args(&self.inner.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let state = DaemonState::Failed {
                    message: format!("Could not start the backend ({}): {e}", program.display()),
                };
                self.set_state(generation, state.clone());
                return state;
            }
        };
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.start_kill();
            let state = DaemonState::Failed {
                message: "Could not connect to the backend's standard streams.".into(),
            };
            self.set_state(generation, state.clone());
            return state;
        };

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let (exit_tx, exit_rx) = watch::channel(false);
        let conn = Arc::new(Conn {
            generation,
            tx: Mutex::new(Some(tx)),
            pending: Mutex::new(HashMap::new()),
            stopping: AtomicBool::new(false),
            kill: Notify::new(),
            exited: exit_rx,
            pid: child.id(),
        });
        *lock(&self.inner.conn) = Some(conn.clone());

        // Writer: one line per message; closing the channel closes stdin (EOF = shutdown).
        self.inner.rt.spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
            let _ = stdin.shutdown().await;
        });

        // Reader: responses and notifications, in wire order.
        {
            let this = self.clone();
            let conn = conn.clone();
            self.inner
                .rt
                .spawn(async move { this.read_loop(conn, stdout).await });
        }

        // stderr: forwarded as log lines, last few kept for error messages.
        {
            let this = self.clone();
            self.inner.rt.spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    {
                        let mut tail = lock(&this.inner.stderr_tail);
                        tail.push_back(line.clone());
                        while tail.len() > STDERR_TAIL_LINES {
                            tail.pop_front();
                        }
                    }
                    (this.inner.sink)(BridgeEvent::Log { line });
                }
            });
        }

        // Waiter: owns the child; reports its exit.
        {
            let this = self.clone();
            let conn = conn.clone();
            self.inner
                .rt
                .spawn(async move { this.wait_loop(conn, child, exit_tx).await });
        }

        // Handshake.
        let params = json!({
            "protocolVersion": "0.1",
            "client": { "name": self.inner.config.client_name, "version": self.inner.config.client_version },
        });
        let answer = self
            .request_on(
                &conn,
                "initialize",
                params,
                self.inner.config.initialize_timeout,
            )
            .await;
        if self.inner.generation.load(Ordering::SeqCst) != generation {
            return self.state().1; // superseded by a restart while starting
        }
        if conn.stopping.load(Ordering::SeqCst) || *conn.exited.borrow() {
            return self.state().1; // the waiter has already reported
        }
        let state = match answer {
            Ok(v) => DaemonState::Ready {
                server_version: v["server"]["version"].as_str().unwrap_or("").to_string(),
                protocol_version: v["protocolVersion"].as_str().unwrap_or("").to_string(),
                methods: v["methods"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| m.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            Err(e) => {
                let message = if e.data.kind == ErrorKind::Timeout {
                    "The backend did not answer 'initialize' within 10 seconds.".to_string()
                } else {
                    format!("The backend refused to initialize: {}", e.message)
                };
                // Mark first so the waiter does not report "exited unexpectedly".
                conn.stopping.store(true, Ordering::SeqCst);
                conn.kill.notify_one();
                self.fail_pending(&conn);
                DaemonState::Failed { message }
            }
        };
        self.set_state(generation, state.clone());
        state
    }

    async fn read_loop<R: AsyncRead + Unpin>(&self, conn: Arc<Conn>, stdout: R) {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match read_bounded_line(&mut reader, &mut line).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(()) => {
                    // Fatal framing error (over-long line): close the connection.
                    (self.inner.sink)(BridgeEvent::Log {
                        line: "[bridge] protocol line exceeds 64 MiB; closing the connection"
                            .into(),
                    });
                    conn.kill.notify_one();
                    break;
                }
            }
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let msg: Message = match serde_json::from_slice(&line) {
                Ok(m) => m,
                Err(e) => {
                    (self.inner.sink)(BridgeEvent::Log {
                        line: format!("[bridge] ignoring a malformed protocol line: {e}"),
                    });
                    continue;
                }
            };
            match msg {
                Message::Notification(n) => (self.inner.sink)(BridgeEvent::Notification {
                    method: n.method,
                    params: n.params.unwrap_or(Value::Null),
                }),
                Message::Response(r) => {
                    let Some(RequestId::Number(id)) = r.id else {
                        continue; // id-less error (parse/invalid request): logged by the daemon
                    };
                    let answer = match (r.result, r.error) {
                        (_, Some(e)) => Err(e),
                        (Some(v), None) => Ok(v),
                        (None, None) => Ok(Value::Null),
                    };
                    match conn.take_pending(id) {
                        Some(Waiter::Internal(tx)) => {
                            let _ = tx.send(answer);
                        }
                        Some(Waiter::Forward { key }) => self.emit_response(key, answer),
                        None => {} // late answer to a request that timed out
                    }
                }
                Message::Request(_) => {} // the daemon sends no requests in 0.1
            }
        }
    }

    async fn wait_loop(&self, conn: Arc<Conn>, mut child: Child, exit_tx: watch::Sender<bool>) {
        let status = tokio::select! {
            s = child.wait() => s,
            _ = conn.kill.notified() => {
                let _ = child.start_kill();
                child.wait().await
            }
        };
        let _ = exit_tx.send(true);
        self.fail_pending(&conn);
        if conn.stopping.load(Ordering::SeqCst) {
            if self.inner.generation.load(Ordering::SeqCst) == conn.generation {
                let current = self.state().1;
                if !matches!(current, DaemonState::Failed { .. }) {
                    self.set_state(conn.generation, DaemonState::Stopped);
                }
            }
            return;
        }
        let mut message = match status {
            Ok(s) => match s.code() {
                Some(code) => format!("The backend exited unexpectedly (exit code {code})."),
                None => "The backend crashed.".to_string(),
            },
            Err(e) => format!("Lost the backend process: {e}"),
        };
        // stderr may still be draining; give it a moment for the tail.
        tokio::time::sleep(Duration::from_millis(50)).await;
        if let Some(last) = lock(&self.inner.stderr_tail).back() {
            message.push_str(&format!(" Last output: {last}"));
        }
        self.set_state(conn.generation, DaemonState::Failed { message });
    }

    fn fail_pending(&self, conn: &Conn) {
        for w in conn.drain_pending() {
            match w {
                Waiter::Internal(tx) => {
                    let _ = tx.send(Err(stopped()));
                }
                Waiter::Forward { key } => self.emit_response(key, Err(stopped())),
            }
        }
    }

    fn emit_response(&self, key: u64, answer: Answer) {
        lock(&self.inner.forwarded).remove(&key);
        let (result, error) = match answer {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e)),
        };
        (self.inner.sink)(BridgeEvent::Response { key, result, error });
    }

    fn encode(id: i64, method: &str, params: &Value) -> String {
        let mut s =
            json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
        s.push('\n');
        s
    }

    fn send_cancel(conn: &Conn, id: i64) {
        let mut s =
            json!({ "jsonrpc": "2.0", "method": "$/cancel", "params": { "id": id } }).to_string();
        s.push('\n');
        conn.send(s);
    }

    async fn request_on(
        &self,
        conn: &Arc<Conn>,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Answer {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = oneshot::channel();
        lock(&conn.pending).insert(id, Waiter::Internal(tx));
        if !conn.send(Self::encode(id, method, &params)) {
            conn.take_pending(id);
            return Err(stopped());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err(stopped()),
            Err(_) => {
                if conn.take_pending(id).is_some() {
                    Self::send_cancel(conn, id);
                }
                Err(timed_out())
            }
        }
    }

    /// Sends a request and awaits its answer (Rust-side callers and tests).
    pub async fn request(&self, method: &str, params: Value, timeout: Option<Duration>) -> Answer {
        let Some(conn) = self.current() else {
            return Err(stopped());
        };
        if *conn.exited.borrow() {
            return Err(stopped());
        }
        let timeout = timeout.unwrap_or(self.inner.config.default_timeout);
        self.request_on(&conn, method, params, timeout).await
    }

    /// Sends a request whose answer is delivered as [`BridgeEvent::Response`]
    /// with `key`, in wire order relative to notifications. Exactly one
    /// response event is emitted per call.
    pub fn request_forward(
        &self,
        key: u64,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) {
        let Some(conn) = self.current().filter(|c| !*c.exited.borrow()) else {
            self.emit_response(key, Err(stopped()));
            return;
        };
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        lock(&conn.pending).insert(id, Waiter::Forward { key });
        lock(&self.inner.forwarded).insert(key, (conn.generation, id));
        if !conn.send(Self::encode(id, method, &params)) {
            if conn.take_pending(id).is_some() {
                self.emit_response(key, Err(stopped()));
            }
            return;
        }
        let timeout = timeout.unwrap_or(self.inner.config.default_timeout);
        let this = self.clone();
        self.inner.rt.spawn(async move {
            let mut exited = conn.exited.clone();
            tokio::select! {
                _ = tokio::time::sleep(timeout) => {
                    if conn.take_pending(id).is_some() {
                        Self::send_cancel(&conn, id);
                        this.emit_response(key, Err(timed_out()));
                    }
                }
                _ = exited.wait_for(|e| *e) => {} // fail_pending answers it
            }
        });
    }

    /// Asks the daemon to stop working on a forwarded request (`$/cancel`).
    /// The request is still answered exactly once (normally with `cancelled`).
    pub fn cancel_forward(&self, key: u64) {
        let Some((generation, id)) = lock(&self.inner.forwarded).get(&key).copied() else {
            return;
        };
        if let Some(conn) = self.current().filter(|c| c.generation == generation) {
            Self::send_cancel(&conn, id);
        }
    }

    /// processes.md §Daemon lifecycle step 5. Bounded: at most
    /// `shutdown_answer_wait + shutdown_exit_wait + 1 s`.
    pub async fn shutdown(&self) {
        let Some(conn) = lock(&self.inner.conn).take() else {
            let generation = self.inner.generation.load(Ordering::SeqCst);
            self.set_state(generation, DaemonState::Stopped);
            return;
        };
        conn.stopping.store(true, Ordering::SeqCst);
        let ready = matches!(self.state().1, DaemonState::Ready { .. });
        if ready && !*conn.exited.borrow() {
            let _ = self
                .request_on(
                    &conn,
                    "shutdown",
                    json!({}),
                    self.inner.config.shutdown_answer_wait,
                )
                .await;
        }
        conn.close_stdin();
        if !conn.wait_exit(self.inner.config.shutdown_exit_wait).await {
            conn.kill.notify_one();
            conn.wait_exit(Duration::from_secs(1)).await;
        }
        self.fail_pending(&conn);
        self.set_state(conn.generation, DaemonState::Stopped);
    }

    /// Replaces the daemon with a fresh process. Requests to the old one are
    /// answered with `internal` "Backend stopped".
    pub async fn restart(&self) -> DaemonState {
        if let Some(old) = lock(&self.inner.conn).take() {
            old.stopping.store(true, Ordering::SeqCst);
            old.close_stdin();
            self.fail_pending(&old);
            let wait = self.inner.config.shutdown_exit_wait;
            self.inner.rt.spawn(async move {
                if !old.wait_exit(wait).await {
                    old.kill.notify_one();
                }
            });
        }
        self.start().await
    }
}

/// Reads one `\n`-terminated line into `out` (without the terminator, `\r`
/// trimmed). `Ok(false)` at EOF, `Err(())` when the line exceeds the limit.
async fn read_bounded_line<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    out: &mut Vec<u8>,
) -> Result<bool, ()> {
    loop {
        let buf = match reader.fill_buf().await {
            Ok(b) => b,
            Err(_) => return Ok(false),
        };
        if buf.is_empty() {
            return Ok(!out.is_empty());
        }
        let (take, done) = match buf.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        out.extend_from_slice(&buf[..take]);
        reader.consume(take);
        if out.len() > MAX_LINE_BYTES + 2 {
            return Err(());
        }
        if done {
            while matches!(out.last(), Some(b'\n' | b'\r')) {
                out.pop();
            }
            return Ok(true);
        }
    }
}

/// The 0.1 methods a UI may call through [`DaemonBridge::request_forward`]:
/// all of them except the lifecycle ones the bridge owns.
pub fn ui_callable(method: &str) -> bool {
    method != "initialize"
        && method != "shutdown"
        && axiom_protocol::registry::method_names().contains(&method)
}

/// Finds axiomd like the Qt shell does (processes.md step 1): explicit path,
/// `$AXIOMD`, a sibling of the current executable (Tauri's `externalBin`
/// installs sidecars next to the app binary without the target-triple
/// suffix), then `PATH`.
pub fn locate_daemon(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = explicit.filter(|p| p.is_file()) {
        return Some(p);
    }
    if let Some(p) = std::env::var_os("AXIOMD").map(PathBuf::from) {
        if p.is_file() {
            return Some(p);
        }
    }
    let exe_name = format!("axiomd{}", std::env::consts::EXE_SUFFIX);
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(PathBuf::from))
    {
        let sibling = dir.join(&exe_name);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(&exe_name))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod framing {
    use super::{read_bounded_line, MAX_LINE_BYTES};

    async fn lines(input: &[u8]) -> Vec<Vec<u8>> {
        let mut reader = tokio::io::BufReader::new(std::io::Cursor::new(input.to_vec()));
        let mut out = Vec::new();
        let mut line = Vec::new();
        loop {
            line.clear();
            match read_bounded_line(&mut reader, &mut line).await {
                Ok(true) => out.push(line.clone()),
                Ok(false) => break,
                Err(()) => {
                    out.push(b"<too-long>".to_vec());
                    break;
                }
            }
        }
        out
    }

    // The nine cases of the Qt shell's tst_linebuffer, which the bridge replaces.
    #[tokio::test]
    async fn one_line() {
        assert_eq!(lines(b"one\n").await, vec![b"one".to_vec()]);
    }

    #[tokio::test]
    async fn several_lines_in_one_read() {
        assert_eq!(
            lines(b"a\nb\nc\n").await,
            vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
        );
    }

    #[tokio::test]
    async fn line_split_across_reads() {
        // A reader that yields one byte at a time.
        struct Dribble(Vec<u8>);
        impl tokio::io::AsyncRead for Dribble {
            fn poll_read(
                mut self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                if self.0.is_empty() {
                    return std::task::Poll::Ready(Ok(()));
                }
                let b = self.0.remove(0);
                buf.put_slice(&[b]);
                std::task::Poll::Ready(Ok(()))
            }
        }
        let mut reader = tokio::io::BufReader::new(Dribble(b"hello\n".to_vec()));
        let mut line = Vec::new();
        assert!(read_bounded_line(&mut reader, &mut line).await.is_ok());
        assert_eq!(line, b"hello");
    }

    #[tokio::test]
    async fn crlf_is_accepted_and_empty_lines_are_kept() {
        // Blank lines are filtered by the caller (they are valid JSON padding).
        assert_eq!(
            lines(b"a\r\n\r\nb\r\n").await,
            vec![b"a".to_vec(), Vec::new(), b"b".to_vec()]
        );
    }

    #[tokio::test]
    async fn terminator_split_from_its_line() {
        assert_eq!(lines(b"a\r\nb").await, vec![b"a".to_vec(), b"b".to_vec()]);
    }

    #[tokio::test]
    async fn complete_line_followed_by_partial() {
        assert_eq!(
            lines(b"done\ntail").await,
            vec![b"done".to_vec(), b"tail".to_vec()]
        );
    }

    #[tokio::test]
    async fn empty_input_yields_nothing() {
        assert!(lines(b"").await.is_empty());
    }

    #[tokio::test]
    async fn large_line_is_accepted_up_to_the_limit() {
        let mut big = vec![b'x'; 1024 * 1024];
        big.push(b'\n');
        let got = lines(&big).await;
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].len(), 1024 * 1024);
    }

    #[tokio::test]
    async fn a_line_over_the_limit_is_fatal() {
        let mut huge = vec![b'x'; MAX_LINE_BYTES + 8];
        huge.push(b'\n');
        let got = lines(&huge).await;
        assert_eq!(got, vec![b"<too-long>".to_vec()]);
    }
}
