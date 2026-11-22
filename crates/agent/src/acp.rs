//! External agents over the Agent Client Protocol (ADR-0016; M3-ACP-001). Axiom is the ACP
//! *client*: it starts the agent (Claude Code through `claude-agent-acp`, Codex through
//! `codex-acp`, Gemini CLI natively), opens an ACP session in the workspace, sends prompts, and
//! answers the agent's requests:
//!
//! - `fs/read_text_file`, `fs/write_text_file` and `terminal/create` become calls of the hidden
//!   tools `acp.readFile`, `acp.writeFile` and `acp.terminal`, so the same gate, checkpoint and
//!   audit log apply as to Axiom's own agent;
//! - `session/request_permission` is decided by the same gate (and shown as the same approval
//!   card); an approval also covers the file writes / the command it was about, so the person is
//!   not asked twice;
//! - `session/update` becomes the session's `agent/event` stream and journal.
//!
//! Authentication stays the agent's: Axiom never passes or sees its credentials.

use crate::agent_loop::Checkpoint;
use crate::checkpoint::Shadow;
use crate::gate::{self, policy, Gate, Policies, Via};
use crate::live::Live;
use crate::tools::{Registry, ToolCtx};
use axiom_protocol::agent::{
    Decision, Effect, EventBody, PolicyRule, ProviderConfig, TextKind, TurnEndReason,
};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot};

pub const PROTOCOL_VERSION: u64 = 1;
const SETUP_TIMEOUT: Duration = Duration::from_secs(60);
/// Messages from the agent not yet handled by a turn: past this, notifications are dropped (an
/// agent that floods the daemon cannot grow its memory without bound; AUD-109).
const INCOMING_QUEUE: usize = 4096;

/// The built-in external agents (ADR-0016 §2): id, name, command, install hint.
pub const BUILTIN: &[(&str, &str, &[&str], &str)] = &[
    (
        "claude-code",
        "Claude Code",
        &["claude-agent-acp"],
        "npm install -g @agentclientprotocol/claude-agent-acp, then sign in by running claude once",
    ),
    (
        "codex",
        "Codex",
        &["codex-acp"],
        "npm install -g @agentclientprotocol/codex-acp, then sign in with codex login",
    ),
    (
        "gemini",
        "Gemini CLI",
        &["gemini", "--experimental-acp"],
        "npm install -g @google/gemini-cli, then sign in by running gemini once",
    ),
];

pub fn builtin_configs() -> Vec<ProviderConfig> {
    BUILTIN
        .iter()
        .map(|(id, name, cmd, _)| ProviderConfig {
            id: (*id).into(),
            kind: axiom_protocol::agent::ProviderKind::Acp,
            name: (*name).into(),
            base_url: None,
            api_key_env: None,
            models: vec!["default".into()],
            capabilities: None,
            command: cmd.iter().map(|s| s.to_string()).collect(),
            env: BTreeMap::new(),
        })
        .collect()
}

pub fn install_hint(id: &str) -> Option<&'static str> {
    BUILTIN.iter().find(|b| b.0 == id).map(|b| b.3)
}

/// The program on `PATH` (or as given, if it has a directory part).
/// Where `agent/installAgent` installs agents (`<data dir>/agents`, an npm prefix), set once
/// by the daemon at startup. Its `node_modules/.bin` is searched before `PATH`.
static AGENTS_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn set_agents_dir(dir: PathBuf) {
    let _ = AGENTS_DIR.set(dir);
}

pub fn agents_dir() -> Option<&'static Path> {
    AGENTS_DIR.get().map(PathBuf::as_path)
}

/// The npm package of a built-in agent (ADR-0016 §2), for `agent/installAgent`.
pub fn builtin_package(id: &str) -> Option<&'static str> {
    match id {
        "claude-code" => Some("@agentclientprotocol/claude-agent-acp"),
        "codex" => Some("@agentclientprotocol/codex-acp"),
        "gemini" => Some("@google/gemini-cli"),
        _ => None,
    }
}

pub fn find_program(program: &str) -> Option<PathBuf> {
    let p = Path::new(program);
    if p.components().count() > 1 {
        return p.is_file().then(|| p.to_path_buf());
    }
    if let Some(bin) = agents_dir().map(|d| d.join("node_modules").join(".bin")) {
        let c = bin.join(program);
        if c.is_file() {
            return Some(c);
        }
    }
    let exts: Vec<String> = if cfg!(windows) {
        vec![".exe".into(), ".cmd".into(), ".bat".into(), String::new()]
    } else {
        vec![String::new()]
    };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .flat_map(|d| exts.iter().map(move |e| d.join(format!("{program}{e}"))))
        .find(|c| c.is_file())
}

#[derive(Debug, Clone, PartialEq)]
pub struct AcpError {
    pub code: i64,
    pub message: String,
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

/// A message from the agent that is not a response.
#[derive(Debug)]
pub enum Incoming {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
}

type Waiters = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, AcpError>>>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// The agent process and its JSON-RPC connection.
pub struct Connection {
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    waiters: Waiters,
    next: AtomicI64,
    incoming: tokio::sync::Mutex<mpsc::Receiver<Incoming>>,
    child: tokio::sync::Mutex<tokio::process::Child>,
    /// The reader saw the end of the agent's output: the process is gone or unusable.
    ended: Arc<std::sync::atomic::AtomicBool>,
}

impl Connection {
    /// Starts `argv` with the daemon's environment minus `withhold` (Axiom's model-key
    /// variables, AUD-010), plus `env`.
    pub fn spawn(
        argv: &[String],
        env: &BTreeMap<String, String>,
        withhold: &[String],
        cwd: &Path,
    ) -> Result<Connection, String> {
        let program = argv.first().ok_or("no command")?;
        let path = find_program(program).ok_or_else(|| format!("`{program}` is not installed"))?;
        let mut cmd = tokio::process::Command::new(path);
        for name in withhold {
            cmd.env_remove(name);
        }
        let mut child = cmd
            .args(&argv[1..])
            .envs(env)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("cannot start `{program}`: {e}"))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("no stdio".into());
        };
        let waiters: Waiters = Arc::default();
        let (tx, rx) = mpsc::channel(INCOMING_QUEUE);
        let w = waiters.clone();
        let ended: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let end = ended.clone();
        tokio::spawn(async move {
            let queue = |m: Incoming| {
                if let Err(mpsc::error::TrySendError::Full(_)) = tx.try_send(m) {
                    tracing::warn!("an external agent sent more than {INCOMING_QUEUE} unhandled messages; one was dropped");
                }
            };
            let mut lines = crate::lines::Lines::new(stdout, crate::lines::MAX_LINE);
            loop {
                let l = match lines.next_line().await {
                    Ok(Some(l)) => l,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!("an external agent's output is unusable: {e}");
                        break;
                    }
                };
                let Ok(v) = serde_json::from_str::<Value>(&l) else {
                    continue;
                };
                match (v.get("method").and_then(Value::as_str), v.get("id")) {
                    (Some(m), Some(id)) => queue(Incoming::Request {
                        id: id.clone(),
                        method: m.into(),
                        params: v.get("params").cloned().unwrap_or(Value::Null),
                    }),
                    (Some(m), None) => queue(Incoming::Notification {
                        method: m.into(),
                        params: v.get("params").cloned().unwrap_or(Value::Null),
                    }),
                    (None, Some(id)) => {
                        let Some(id) = id.as_i64() else { continue };
                        let r = match v.get("error") {
                            Some(e) => Err(AcpError {
                                code: e["code"].as_i64().unwrap_or(-32603),
                                message: e["message"].as_str().unwrap_or("error").into(),
                            }),
                            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        if let Some(s) = lock(&w).remove(&id) {
                            let _ = s.send(r);
                        }
                    }
                    _ => {}
                }
            }
            end.store(true, Ordering::SeqCst);
            for (_, s) in lock(&w).drain() {
                let _ = s.send(Err(AcpError {
                    code: -32603,
                    message: "the agent exited".into(),
                }));
            }
        });
        Ok(Connection {
            stdin: tokio::sync::Mutex::new(stdin),
            waiters,
            next: AtomicI64::new(1),
            incoming: tokio::sync::Mutex::new(rx),
            child: tokio::sync::Mutex::new(child),
            ended,
        })
    }

    /// The agent's process has ended (or its output became unusable): a cached session with
    /// it must be replaced, not reused (AUD-032).
    pub fn is_ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    async fn write(&self, v: Value) -> Result<(), AcpError> {
        let mut s = self.stdin.lock().await;
        let r = async {
            s.write_all(format!("{v}\n").as_bytes()).await?;
            s.flush().await
        }
        .await;
        r.map_err(|e| AcpError {
            code: -32603,
            message: format!("cannot write to the agent: {e}"),
        })
    }

    /// Sends a request; the response is awaited through the returned receiver.
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, AcpError> {
        let rx = self.start_request(method, params).await?;
        rx.await.unwrap_or_else(|_| {
            Err(AcpError {
                code: -32603,
                message: "the agent exited".into(),
            })
        })
    }

    async fn start_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<oneshot::Receiver<Result<Value, AcpError>>, AcpError> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.waiters).insert(id, tx);
        self.write(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        Ok(rx)
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), AcpError> {
        self.write(json!({"jsonrpc":"2.0","method":method,"params":params}))
            .await
    }

    pub async fn respond(&self, id: Value, result: Result<Value, (i64, String)>) {
        let v = match result {
            Ok(r) => json!({"jsonrpc":"2.0","id":id,"result":r}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            }
        };
        let _ = self.write(v).await;
    }

    pub async fn kill(&self) {
        self.ended.store(true, Ordering::SeqCst);
        let _ = self.child.lock().await.kill().await;
    }
}

/// What the agent said it can do.
#[derive(Debug, Clone, Default)]
pub struct AgentCaps {
    pub load_session: bool,
    pub images: bool,
    pub embedded_context: bool,
    /// `name (id)` of each sign-in method, for error messages.
    pub auth: Vec<String>,
}

/// One external agent attached to one Axiom session.
pub struct AcpSession {
    pub conn: Connection,
    pub agent_session: String,
    pub caps: AgentCaps,
}

impl AcpSession {
    /// Stops the agent's process (the session is deleted, or the daemon is stopping).
    pub async fn kill(&self) {
        self.conn.kill().await;
    }
}

/// The MCP server entry offering Axiom's own tools to the agent.
#[derive(Debug, Clone)]
pub struct AxiomMcp {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

fn mcp_servers(m: Option<&AxiomMcp>) -> Value {
    match m {
        Some(m) => json!([{
            "name": "axiom",
            "command": m.command.to_string_lossy(),
            "args": m.args,
            "env": m.env.iter().map(|(n, v)| json!({"name": n, "value": v})).collect::<Vec<_>>(),
        }]),
        None => json!([]),
    }
}

/// Starts the agent and opens (or reloads) its session. `saved` is the agent's session id from
/// an earlier daemon run; `Ok((session, resumed))`.
pub async fn connect(
    config: &ProviderConfig,
    root: &Path,
    saved: Option<&str>,
    mcp: Option<&AxiomMcp>,
    withhold: &[String],
) -> Result<(AcpSession, bool), String> {
    let conn = Connection::spawn(&config.command, &config.env, withhold, root).map_err(|e| {
        match install_hint(&config.id) {
            Some(h) => format!("{} cannot start: {e}. Install it: {h}.", config.name),
            None => format!("{} cannot start: {e}.", config.name),
        }
    })?;
    let init = tokio::time::timeout(
        SETUP_TIMEOUT,
        conn.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {"fs": {"readTextFile": true, "writeTextFile": true}, "terminal": true},
                "clientInfo": {"name": "axiom", "version": env!("CARGO_PKG_VERSION")},
            }),
        ),
    )
    .await
    .map_err(|_| format!("{} did not answer `initialize`.", config.name))?
    .map_err(|e| format!("{} failed to initialise: {e}.", config.name))?;
    if init["protocolVersion"].as_u64() != Some(PROTOCOL_VERSION) {
        conn.kill().await;
        return Err(format!(
            "{} speaks ACP version {}; Axiom speaks {PROTOCOL_VERSION}.",
            config.name, init["protocolVersion"]
        ));
    }
    let a = &init["agentCapabilities"];
    let caps = AgentCaps {
        load_session: a["loadSession"].as_bool().unwrap_or(false),
        images: a["promptCapabilities"]["image"].as_bool().unwrap_or(false),
        embedded_context: a["promptCapabilities"]["embeddedContext"]
            .as_bool()
            .unwrap_or(false),
        auth: init["authMethods"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["name"].as_str().map(String::from))
            .collect(),
    };
    let cwd = root.to_string_lossy().into_owned();
    if let (Some(id), true) = (saved, caps.load_session) {
        let r = tokio::time::timeout(
            SETUP_TIMEOUT,
            conn.request(
                "session/load",
                json!({"sessionId": id, "cwd": cwd, "mcpServers": mcp_servers(mcp)}),
            ),
        )
        .await;
        if let Ok(Ok(_)) = r {
            // The agent replays its history as updates before answering: drop them.
            let mut rx = conn.incoming.lock().await;
            while rx.try_recv().is_ok() {}
            drop(rx);
            return Ok((
                AcpSession {
                    conn,
                    agent_session: id.into(),
                    caps,
                },
                true,
            ));
        }
    }
    let r = tokio::time::timeout(
        SETUP_TIMEOUT,
        conn.request(
            "session/new",
            json!({"cwd": cwd, "mcpServers": mcp_servers(mcp)}),
        ),
    )
    .await
    .map_err(|_| format!("{} did not open a session.", config.name))?
    .map_err(|e| {
        let sign_in = if caps.auth.is_empty() {
            String::new()
        } else {
            format!(" Sign in with its own CLI ({}).", caps.auth.join(", "))
        };
        format!("{} could not open a session: {e}.{sign_in}", config.name)
    })?;
    let id = r["sessionId"]
        .as_str()
        .ok_or_else(|| format!("{} returned no session id.", config.name))?
        .to_string();
    Ok((
        AcpSession {
            conn,
            agent_session: id,
            caps,
        },
        false,
    ))
}

/// Terminal operations after `terminal/create` (the create itself is the gated `acp.terminal`
/// tool). Implemented by the daemon over its executor.
/// `(exitCode, signal)`.
pub type ExitStatus = (Option<i32>, Option<String>);
/// `(output, truncated, exit once it exited)`.
pub type TerminalOutput = (String, bool, Option<ExitStatus>);

pub trait Terminals: Send + Sync {
    fn output<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<TerminalOutput, String>>;
    fn wait<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<ExitStatus, String>>;
    fn kill<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>>;
    fn release<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

/// What a turn needs.
pub struct AcpTurn<'a> {
    pub live: &'a Live,
    pub session: &'a AcpSession,
    /// The provider id (`claude-code`): permission requests are the policy's tool
    /// `<id>.<agent tool name>`.
    pub agent_id: String,
    pub agent_name: String,
    pub root: PathBuf,
    pub registry: &'a Registry,
    pub gate: &'a Gate,
    pub policies: Policies,
    pub ctx: ToolCtx,
    pub shadow: Arc<Shadow>,
    pub audit: PathBuf,
    pub terminals: &'a dyn Terminals,
    pub cancel: tokio_util::sync::CancellationToken,
    pub turn: u32,
}

/// The ACP `ToolKind` as an Axiom effect.
pub fn effect_of(kind: Option<&str>) -> Effect {
    match kind.unwrap_or("other") {
        "read" | "search" | "think" | "switch_mode" => Effect::Read,
        "edit" | "move" => Effect::WriteWorkspace,
        "delete" => Effect::Delete,
        "fetch" => Effect::Network,
        _ => Effect::Execute,
    }
}

/// An absolute path the agent named, as a workspace path (`None` outside the workspace).
pub fn workspace_path(root: &Path, abs: &str) -> Option<String> {
    let p = Path::new(abs);
    let rel = p
        .strip_prefix(root)
        .ok()
        .map(Path::to_path_buf)
        .or_else(|| {
            let canon = std::fs::canonicalize(root).ok()?;
            p.strip_prefix(&canon).ok().map(Path::to_path_buf)
        })?;
    let parts = rel
        .components()
        .map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn content_text(c: &Value) -> Option<String> {
    match c["type"].as_str()? {
        "text" => c["text"].as_str().map(String::from),
        "resource_link" => c["uri"].as_str().map(|u| format!("[{u}]")),
        "resource" => c["resource"]["text"].as_str().map(String::from),
        other => Some(format!("[{other}]")),
    }
}

/// Text of a tool call's `content` (text blocks, diffs, terminals) for the tool row.
fn tool_content(items: &Value, root: &Path) -> String {
    let mut out = Vec::new();
    for it in items.as_array().into_iter().flatten() {
        match it["type"].as_str() {
            Some("content") => {
                if let Some(t) = content_text(&it["content"]) {
                    out.push(t);
                }
            }
            Some("diff") => {
                let path = it["path"].as_str().unwrap_or("");
                let rel = workspace_path(root, path).unwrap_or_else(|| path.to_string());
                let (d, _, _) = crate::tools::patch::diff(
                    &rel,
                    it["oldText"].as_str().unwrap_or(""),
                    it["newText"].as_str().unwrap_or(""),
                );
                out.push(d);
            }
            Some("terminal") => out.push(format!(
                "[terminal {}]",
                it["terminalId"].as_str().unwrap_or("")
            )),
            _ => {}
        }
    }
    out.join("\n")
}

#[derive(Default)]
struct Grants {
    /// Allow rules from approved permission requests, for this turn.
    rules: Vec<PolicyRule>,
    /// The command an approved `execute` permission showed (its `rawInput.command`): it covers
    /// the next terminal only if that terminal runs exactly this command (AUD-040).
    next_terminal: Option<Command>,
}

/// The command a permission request describes.
#[derive(Debug, Clone, PartialEq)]
enum Command {
    /// A command line (run through a shell, or split on spaces).
    Line(String),
    Argv(Vec<String>),
}

impl Command {
    fn of(raw: &Value) -> Option<Command> {
        match &raw["command"] {
            Value::String(s) if !s.trim().is_empty() => Some(Command::Line(s.trim().to_string())),
            Value::Array(a) if !a.is_empty() => a
                .iter()
                .map(|x| x.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()
                .map(Command::Argv),
            _ => None,
        }
    }

    /// Whether a terminal's argv runs this command: the same argv, the line split on spaces,
    /// or the line given to a shell's `-c`.
    fn runs(&self, argv: &[String]) -> bool {
        match self {
            Command::Argv(a) => a == argv,
            Command::Line(l) => {
                let words: Vec<&str> = l.split_whitespace().collect();
                let shell = argv.len() == 3
                    && matches!(argv[1].as_str(), "-c" | "-lc")
                    && argv[2].trim() == l;
                shell || (argv.len() == words.len() && argv.iter().zip(&words).all(|(a, w)| a == w))
            }
        }
    }
}

/// The argv a `terminal/create` asks for (as `acp.terminal` reads it).
fn terminal_argv(input: &Value) -> Vec<String> {
    let mut v: Vec<String> = input["command"]
        .as_str()
        .map(String::from)
        .into_iter()
        .collect();
    for a in input["args"].as_array().into_iter().flatten() {
        v.push(a.as_str().unwrap_or_default().to_string());
    }
    v
}

struct Open {
    title: String,
}

/// Runs one prompt to its end. Errors become events; the turn always ends with `turnEnd`.
pub async fn run_turn(t: AcpTurn<'_>, prompt: Vec<Value>) {
    let reason = drive(&t, prompt).await;
    let _ = t.live.emit(EventBody::TurnEnd { reason });
}

async fn drive(t: &AcpTurn<'_>, prompt: Vec<Value>) -> TurnEndReason {
    let live = t.live;
    let conn = &t.session.conn;
    let mut rx_prompt = match conn
        .start_request(
            "session/prompt",
            json!({"sessionId": t.session.agent_session, "prompt": prompt}),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            error(live, &format!("{} is not answering: {e}.", t.agent_name));
            return TurnEndReason::Error;
        }
    };
    let ckpt = Checkpoint::new(live, &t.shadow, t.turn);
    let mut text = String::new();
    let mut thought = String::new();
    let mut plan: Option<String> = None;
    let mut open: BTreeMap<String, Open> = BTreeMap::new();
    let mut grants = Grants::default();
    let mut cancel_sent: Option<Instant> = None;
    let mut incoming = conn.incoming.lock().await;
    let flush = |text: &mut String, kind: Option<TextKind>| {
        if !text.trim().is_empty() {
            let _ = live.emit(EventBody::AssistantText {
                text: std::mem::take(text),
                kind,
            });
        }
        text.clear();
    };
    let reason = loop {
        // Biased: the reader queues an update before it resolves the prompt's response, so every
        // update the agent sent before answering is handled before the turn ends.
        tokio::select! {
            biased;
            m = incoming.recv() => {
                let Some(m) = m else {
                    error(live, &format!("{} exited.", t.agent_name));
                    break TurnEndReason::Error;
                };
                match m {
                    Incoming::Notification { method, params } if method == "session/update" => {
                        let u = &params["update"];
                        match u["sessionUpdate"].as_str().unwrap_or("") {
                            "agent_message_chunk" => {
                                if !thought.is_empty() { flush(&mut thought, Some(TextKind::Thought)); }
                                if let Some(s) = content_text(&u["content"]) {
                                    text.push_str(&s);
                                    live.emit_ephemeral(EventBody::TextDelta { text: s });
                                }
                            }
                            "agent_thought_chunk" => {
                                if let Some(s) = content_text(&u["content"]) { thought.push_str(&s); }
                            }
                            "plan" => {
                                let lines: Vec<String> = u["entries"].as_array().into_iter().flatten().map(|e| {
                                    let mark = match e["status"].as_str() { Some("completed") => "x", Some("in_progress") => "~", _ => " " };
                                    format!("- [{mark}] {}", e["content"].as_str().unwrap_or(""))
                                }).collect();
                                plan = Some(lines.join("\n"));
                            }
                            "tool_call" => {
                                flush(&mut text, None);
                                flush(&mut thought, Some(TextKind::Thought));
                                let id = u["toolCallId"].as_str().unwrap_or("").to_string();
                                let title = u["title"].as_str().unwrap_or("tool").to_string();
                                let name = u["name"].as_str().map(String::from).unwrap_or_else(|| title.clone());
                                let targets: Vec<String> = u["locations"].as_array().into_iter().flatten()
                                    .filter_map(|l| l["path"].as_str())
                                    .map(|p| workspace_path(&t.root, p).unwrap_or_else(|| p.to_string()))
                                    .collect();
                                let _ = live.emit(EventBody::ToolCall {
                                    call_id: id.clone(),
                                    tool: format!("{}: {name}", t.agent_name),
                                    input: u.get("rawInput").cloned().unwrap_or_else(|| json!({"title": title})),
                                    effect: effect_of(u["kind"].as_str()),
                                    targets,
                                });
                                let status = u["status"].as_str().map(String::from);
                                open.insert(id.clone(), Open { title });
                                if let Some(s) = status { finish_call(live, &mut open, &id, &s, u, &t.root); }
                            }
                            "tool_call_update" => {
                                let id = u["toolCallId"].as_str().unwrap_or("").to_string();
                                if let Some(title) = u["title"].as_str() {
                                    if let Some(o) = open.get_mut(&id) { o.title = title.into(); }
                                }
                                if let Some(s) = u["status"].as_str() { finish_call(live, &mut open, &id, s, u, &t.root); }
                            }
                            _ => {} // commands, modes, usage: not shown in M3
                        }
                    }
                    Incoming::Notification { .. } => {}
                    Incoming::Request { id, method, params } => {
                        let r = handle_request(t, &ckpt, &mut grants, &method, &params).await;
                        conn.respond(id, r).await;
                    }
                }
            }
            r = &mut rx_prompt => {
                break match r {
                    Ok(Ok(v)) => match v["stopReason"].as_str().unwrap_or("end_turn") {
                        "max_tokens" => TurnEndReason::MaxTokens,
                        "max_turn_requests" => TurnEndReason::ToolLimit,
                        "refusal" => TurnEndReason::Refusal,
                        "cancelled" => TurnEndReason::Cancelled,
                        _ => TurnEndReason::EndTurn,
                    },
                    Ok(Err(e)) => {
                        error(live, &format!("{}: {e}.", t.agent_name));
                        TurnEndReason::Error
                    }
                    Err(_) => {
                        error(live, &format!("{} stopped.", t.agent_name));
                        TurnEndReason::Error
                    }
                };
            }
            _ = t.cancel.cancelled(), if cancel_sent.is_none() => {
                cancel_sent = Some(Instant::now());
                let _ = conn.notify("session/cancel", json!({"sessionId": t.session.agent_session})).await;
            }
            _ = tokio::time::sleep(Duration::from_millis(200)), if cancel_sent.is_some_and(|s| s.elapsed() > Duration::from_secs(10)) => {
                // The agent ignored the cancel: stop it.
                conn.kill().await;
                break TurnEndReason::Cancelled;
            }
        }
    };
    drop(incoming);
    flush(&mut thought, Some(TextKind::Thought));
    flush(&mut text, None);
    if let Some(p) = plan {
        let _ = live.emit(EventBody::AssistantText {
            text: p,
            kind: Some(TextKind::Plan),
        });
    }
    for (id, o) in std::mem::take(&mut open) {
        let _ = live.emit(EventBody::ToolResult {
            call_id: id,
            is_error: true,
            summary: format!("{} did not finish", o.title),
            output: json!({}),
            truncated: false,
            rule: None,
        });
    }
    reason
}

fn finish_call(
    live: &Live,
    open: &mut BTreeMap<String, Open>,
    id: &str,
    status: &str,
    u: &Value,
    root: &Path,
) {
    if status != "completed" && status != "failed" {
        return;
    }
    let Some(o) = open.remove(id) else { return };
    let text = tool_content(&u["content"], root);
    let _ = live.emit(EventBody::ToolResult {
        call_id: id.into(),
        is_error: status == "failed",
        summary: o.title,
        output: json!({"text": text, "rawOutput": u.get("rawOutput").cloned().unwrap_or(Value::Null)}),
        truncated: false,
        rule: None,
    });
}

fn error(live: &Live, message: &str) {
    let _ = live.emit(EventBody::Error {
        kind: "model_error".into(),
        model_kind: None,
        message: message.into(),
    });
}

const INVALID: i64 = -32602;
const DENIED: i64 = -32000;

/// One request from the agent.
async fn handle_request(
    t: &AcpTurn<'_>,
    ckpt: &Checkpoint<'_>,
    grants: &mut Grants,
    method: &str,
    p: &Value,
) -> Result<Value, (i64, String)> {
    match method {
        "fs/read_text_file" => {
            let out = tool(
                t,
                ckpt,
                grants,
                "acp.readFile",
                json!({"path": p["path"], "line": p["line"], "limit": p["limit"]}),
            )
            .await?;
            Ok(json!({"content": out["content"]}))
        }
        "fs/write_text_file" => {
            tool(
                t,
                ckpt,
                grants,
                "acp.writeFile",
                json!({"path": p["path"], "content": p["content"]}),
            )
            .await?;
            let _ = t.shadow.record_post(&t.live.id.0).await;
            Ok(json!({}))
        }
        "terminal/create" => {
            let out = tool(
                t,
                ckpt,
                grants,
                "acp.terminal",
                json!({
                    "command": p["command"], "args": p["args"], "env": p["env"], "cwd": p["cwd"],
                    "outputByteLimit": p["outputByteLimit"],
                }),
            )
            .await?;
            Ok(json!({"terminalId": out["terminalId"]}))
        }
        "terminal/output" => {
            let id = p["terminalId"].as_str().unwrap_or("");
            let (output, truncated, exit) =
                t.terminals.output(id).await.map_err(|e| (INVALID, e))?;
            let mut v = json!({"output": output, "truncated": truncated});
            if let Some((code, signal)) = exit {
                v["exitStatus"] = json!({"exitCode": code, "signal": signal});
            }
            Ok(v)
        }
        "terminal/wait_for_exit" => {
            let id = p["terminalId"].as_str().unwrap_or("");
            let (code, signal) = tokio::select! {
                r = t.terminals.wait(id) => r.map_err(|e| (INVALID, e))?,
                _ = t.cancel.cancelled() => {
                    let _ = t.terminals.kill(id).await;
                    t.terminals.wait(id).await.map_err(|e| (INVALID, e))?
                }
            };
            let _ = t.shadow.record_post(&t.live.id.0).await;
            Ok(json!({"exitCode": code, "signal": signal}))
        }
        "terminal/kill" => {
            t.terminals
                .kill(p["terminalId"].as_str().unwrap_or(""))
                .await
                .map_err(|e| (INVALID, e))?;
            Ok(json!({}))
        }
        "terminal/release" => {
            t.terminals
                .release(p["terminalId"].as_str().unwrap_or(""))
                .await
                .map_err(|e| (INVALID, e))?;
            Ok(json!({}))
        }
        "session/request_permission" => Ok(permission(t, grants, p).await),
        other => Err((-32601, format!("Axiom does not offer `{other}`"))),
    }
}

/// A gated call of one of the hidden tools; the structured output on success.
async fn tool(
    t: &AcpTurn<'_>,
    ckpt: &Checkpoint<'_>,
    grants: &mut Grants,
    name: &str,
    input: Value,
) -> Result<Value, (i64, String)> {
    let prepared = t
        .registry
        .prepare(&t.ctx, name, input)
        .await
        .map_err(|e| (INVALID, e.message))?;
    let mut extra = grants.rules.clone();
    let covered = name == "acp.terminal"
        && grants
            .next_terminal
            .take()
            .is_some_and(|c| c.runs(&terminal_argv(&prepared.input)));
    if covered {
        extra.push(PolicyRule {
            effect: Some(Effect::Execute),
            tool: Some("acp.terminal".into()),
            path: None,
            argv_prefix: None,
            decision: Decision::Allow,
        });
    }
    let out = gate::run_prepared_with(
        t.registry,
        t.gate,
        &t.ctx,
        Some(&t.live.id),
        prepared,
        &t.policies,
        ckpt,
        Some(&t.audit),
        Instant::now(),
        &extra,
    )
    .await;
    if out.is_error {
        let code = if out.refusal.is_some() {
            DENIED
        } else {
            INVALID
        };
        return Err((code, out.output.summary));
    }
    Ok(out.output.output)
}

/// Decides the agent's own permission request through the gate.
async fn permission(t: &AcpTurn<'_>, grants: &mut Grants, p: &Value) -> Value {
    let call = &p["toolCall"];
    let options: Vec<(String, String)> = p["options"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|o| {
            Some((
                o["optionId"].as_str()?.to_string(),
                o["kind"].as_str()?.to_string(),
            ))
        })
        .collect();
    let pick = |kinds: &[&str]| {
        kinds.iter().find_map(|k| {
            options
                .iter()
                .find(|(_, ok)| ok == k)
                .map(|(id, _)| id.clone())
        })
    };
    let kind = call["kind"].as_str();
    let effect = effect_of(kind);
    let paths: Vec<axiom_protocol::WorkspacePath> = call["locations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| l["path"].as_str())
        .filter_map(|p| workspace_path(&t.root, p))
        .map(axiom_protocol::WorkspacePath)
        .collect();
    let name = call["name"]
        .as_str()
        .or(call["title"].as_str())
        .unwrap_or("tool");
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let tool_name = format!("{}.{clean}", t.agent_id);
    let preview = {
        let s = tool_content(&call["content"], &t.root);
        (!s.is_empty()).then_some(s)
    };
    let pc = policy::Call {
        tool: &tool_name,
        effect,
        paths: &paths,
        argv: None,
    };
    let mut targets: Vec<String> = paths.iter().map(|p| p.0.clone()).collect();
    if let Some(title) = call["title"].as_str() {
        targets.insert(0, title.to_string());
    }
    let cancelled = json!({"outcome": {"outcome": "cancelled"}});
    match t
        .gate
        .decide_or_ask(
            &t.ctx.principal,
            Some(&t.live.id),
            &pc,
            targets,
            preview,
            &t.policies,
            &t.cancel,
        )
        .await
    {
        Ok((via, _)) => {
            // What the person approved (not what the policy let through unseen) covers the
            // writes to these paths, for an edit, and the next terminal running the command
            // shown, for an execute (AUD-040).
            if matches!(via, Via::Approved(_)) {
                if matches!(effect, Effect::WriteWorkspace | Effect::Delete) {
                    for p in &paths {
                        grants.rules.push(PolicyRule {
                            effect: None,
                            tool: Some("acp.writeFile".into()),
                            path: Some(p.0.clone()),
                            argv_prefix: None,
                            decision: Decision::Allow,
                        });
                    }
                }
                if effect == Effect::Execute {
                    grants.next_terminal = call.get("rawInput").and_then(Command::of);
                }
            }
            let always = matches!(
                via,
                Via::Approved(axiom_protocol::agent::ApprovalDecision::Session)
            );
            let id = if always {
                pick(&["allow_always", "allow_once"])
            } else {
                pick(&["allow_once", "allow_always"])
            };
            match id {
                Some(id) => json!({"outcome": {"outcome": "selected", "optionId": id}}),
                None => cancelled,
            }
        }
        Err(r) if r.kind == axiom_protocol::ErrorKind::Cancelled => cancelled,
        Err(_) => match pick(&["reject_once", "reject_always"]) {
            Some(id) => json!({"outcome": {"outcome": "selected", "optionId": id}}),
            None => cancelled,
        },
    }
}

/// The prompt blocks for a user message.
pub fn prompt_blocks(
    text: &str,
    attached: Option<&str>,
    images: &[axiom_protocol::agent::ImageData],
    caps: &AgentCaps,
) -> Vec<Value> {
    let mut v = vec![json!({"type": "text", "text": text})];
    if let Some(a) = attached {
        v.push(json!({"type": "text", "text": a}));
    }
    for im in images {
        if caps.images {
            v.push(json!({"type": "image", "data": im.data, "mimeType": im.media_type}));
        } else {
            v.push(json!({"type": "text", "text": "[An image was attached, but this agent does not accept images.]"}));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_kinds_map_to_effects() {
        assert_eq!(effect_of(Some("read")), Effect::Read);
        assert_eq!(effect_of(Some("edit")), Effect::WriteWorkspace);
        assert_eq!(effect_of(Some("delete")), Effect::Delete);
        assert_eq!(effect_of(Some("execute")), Effect::Execute);
        assert_eq!(effect_of(Some("fetch")), Effect::Network);
        assert_eq!(effect_of(None), Effect::Execute);
    }

    #[test]
    fn an_approved_command_covers_only_the_same_terminal() {
        let line = Command::of(&json!({"command": "ls -la"})).unwrap();
        let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(line.runs(&argv(&["ls", "-la"])));
        assert!(line.runs(&argv(&["bash", "-c", "ls -la"])));
        assert!(!line.runs(&argv(&["rm", "-rf", "x"])));
        assert!(!line.runs(&argv(&["ls", "-la", "/"])));
        let a = Command::of(&json!({"command": ["bash", "-lc", "ls"]})).unwrap();
        assert!(a.runs(&argv(&["bash", "-lc", "ls"])));
        assert!(!a.runs(&argv(&["bash", "-lc", "ls; rm x"])));
        assert_eq!(Command::of(&json!({"title": "x"})), None);
        assert_eq!(
            terminal_argv(&json!({"command": "ls", "args": ["-l"]})),
            ["ls", "-l"]
        );
    }

    #[test]
    fn acp_paths_inside_and_outside() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        let inside = r.join("src/a.rs");
        assert_eq!(
            workspace_path(r, inside.to_str().unwrap()).as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(workspace_path(r, "/etc/passwd"), None);
        let up = format!("{}/../x", r.display());
        assert_eq!(workspace_path(r, &up), None);
    }
}
