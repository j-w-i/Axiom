//! Agent sessions (`agent/session*`, `agent/send`, `agent/cancel`, `agent/checkpoints`,
//! `agent/restore`, `agent/image`; M3-LOOP-001). The journal, the loop and checkpoints are in
//! `axiom-agent`; these handlers find the session, check it belongs to the workspace named,
//! keep the store's index row, and attach the calling connection to the session's events.

use crate::app::{internal, App};
use crate::handlers::agent::{audit_path, policies, provider_configs};
use axiom_agent::agent_loop::{self, ToolEnv, Turn, UserInput};
use axiom_agent::checkpoint::{CheckpointError, Shadow};
use axiom_agent::live::{now_ms, Live};
use axiom_agent::session::JournalError;
use axiom_agent::tools::{Tool, ToolCtx};
use axiom_protocol::agent::{
    CheckpointsResult, Event, EventBody, ImageData, ImageParams, ProviderKind, RestoreParams,
    RestoreResult, SendParams, SessionCreateParams, SessionId, SessionInfo, SessionListParams,
    SessionListResult, SessionOpenResult, SessionRef, SessionRenameParams,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId, WorkspacePath};
use axiom_rpc::{Notifier, RequestCtx};
use axiom_store::SessionRow;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const DEFAULT_TITLE: &str = "New conversation";
const MAX_ATTACHED_BYTES: usize = 256 * 1024;

fn journal_error(e: JournalError) -> RpcError {
    match e {
        JournalError::Image(m) => RpcError::new(
            ErrorKind::InvalidParams,
            format!("The image is invalid: {m}."),
        ),
        other => {
            tracing::error!("session journal: {other}");
            RpcError::new(
                ErrorKind::Io,
                "The session journal could not be read or written.",
            )
        }
    }
}

fn not_found() -> RpcError {
    RpcError::new(ErrorKind::NotFound, "There is no such session.")
}

fn new_id() -> SessionId {
    use std::time::{SystemTime, UNIX_EPOCH};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = N.fetch_add(1, Ordering::Relaxed);
    let key = axiom_agent::workspace_key(std::path::Path::new(&format!(
        "{nanos}-{n}-{}",
        std::process::id()
    )));
    SessionId(format!("s{key}"))
}

/// The workspace root for an optional workspace id (`None` = a plain conversation).
fn root_of(app: &App, ws: Option<&WorkspaceId>) -> Result<Option<PathBuf>, RpcError> {
    match ws {
        Some(id) => Ok(Some(app.workspaces.get(id)?.root().to_path_buf())),
        None => Ok(None),
    }
}

fn root_key(root: &Option<PathBuf>) -> Option<String> {
    root.as_ref().map(|r| r.to_string_lossy().into_owned())
}

/// The session's row, if it belongs to that workspace (or to none, as asked).
async fn row(
    app: &App,
    ws: Option<&WorkspaceId>,
    id: &SessionId,
) -> Result<(SessionRow, Option<PathBuf>), RpcError> {
    let root = root_of(app, ws)?;
    let key = id.0.clone();
    let row = app
        .store_call(move |s| s.session_get(&key))
        .await?
        .ok_or_else(not_found)?;
    if row.root != root_key(&root) {
        return Err(not_found());
    }
    Ok((row, root))
}

fn info(app: &App, row: &SessionRow) -> SessionInfo {
    let id = SessionId(row.id.clone());
    let running = app.agent.sessions.get(&id).is_some_and(|l| l.running());
    SessionInfo {
        session_id: id.clone(),
        title: row.title.clone(),
        provider_id: row.provider.clone(),
        model: row.model.clone(),
        created_ms: row.created_ms,
        updated_ms: row.updated_ms,
        running,
        pending_approvals: app.agent.gate.pending(Some(&id)).len() as u32,
    }
}

fn open_live(app: &App, id: &SessionId, root: &Option<PathBuf>) -> Result<Arc<Live>, RpcError> {
    app.agent
        .sessions
        .open(id, root.as_deref())
        .map_err(journal_error)
}

/// Forwards the session's events to this connection from `after_seq` on (once per connection).
/// The connection counts as attached only once the request has succeeded and its forwarder
/// runs, and no longer once the forwarder ends: a failed or cancelled request leaves nothing
/// behind that would stop a later one from attaching (AUD-033).
fn attach(app: &Arc<App>, live: &Arc<Live>, notifier: Notifier, after_seq: u64, ctx: &RequestCtx) {
    // Subscribed now, so nothing emitted before the response is missed.
    let mut rx = live.subscribe();
    let app = app.clone();
    let id = live.id.clone();
    ctx.after_response(async move {
        {
            let mut a = app.agent.attached.lock().unwrap_or_else(|p| p.into_inner());
            a.retain(|(_, n)| !n.is_closed());
            if a.iter()
                .any(|(s, n)| *s == id && n.same_connection(&notifier))
            {
                return;
            }
            a.push((id.clone(), notifier.clone()));
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let ephemeral = matches!(ev.event, EventBody::TextDelta { .. });
                    if !ephemeral && ev.seq <= after_seq {
                        continue;
                    }
                    if notifier.notify::<Event>(ev).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("a client fell {n} session events behind");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
        app.agent
            .attached
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|(s, n)| !(*s == id && n.same_connection(&notifier)));
    });
}

/// Ends a turn that was begun but never handed to its task (an error or a cancelled request
/// after `Live::begin`), so the session is not left "running" (AUD-011).
struct TurnGuard(Option<Arc<Live>>);

impl TurnGuard {
    fn new(live: &Arc<Live>) -> Self {
        Self(Some(live.clone()))
    }
    /// The turn's task owns ending it now.
    fn disarm(mut self) {
        self.0 = None;
    }
}

impl Drop for TurnGuard {
    fn drop(&mut self) {
        if let Some(l) = self.0.take() {
            l.end();
        }
    }
}

/// The principal a session's tool calls and grants are under.
fn principal_of(id: &SessionId) -> String {
    format!("session:{}", id.0)
}

/// The variables model providers read their keys from (`apiKeyEnv`): an external agent or MCP
/// server Axiom starts does not get them (AUD-010).
fn key_vars(app: &App, configs: &[axiom_protocol::agent::ProviderConfig]) -> Vec<String> {
    app.agent
        .providers
        .all(configs)
        .into_iter()
        .filter_map(|c| c.api_key_env)
        .collect()
}

pub async fn create(
    app: Arc<App>,
    p: SessionCreateParams,
    ctx: RequestCtx,
) -> Result<SessionInfo, RpcError> {
    let root = root_of(&app, p.workspace_id.as_ref())?;
    let configs = provider_configs(&app).await?;
    app.agent
        .providers
        .config(&configs, &p.provider_id)
        .ok_or_else(|| {
            RpcError::new(
                ErrorKind::NotFound,
                format!("No provider `{}` is configured.", p.provider_id),
            )
        })?;
    if p.model.trim().is_empty() {
        return Err(RpcError::new(
            ErrorKind::InvalidParams,
            "A model is required.",
        ));
    }
    let now = now_ms();
    let row = SessionRow {
        id: new_id().0,
        root: root_key(&root),
        title: p
            .title
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_TITLE.into()),
        provider: p.provider_id,
        model: p.model,
        created_ms: now,
        updated_ms: now,
    };
    let r2 = row.clone();
    app.store_call(move |s| s.session_put(&r2)).await?;
    let id = SessionId(row.id.clone());
    let live = open_live(&app, &id, &root)?;
    attach(&app, &live, ctx.notifier.clone(), 0, &ctx);
    Ok(info(&app, &row))
}

pub async fn list(app: Arc<App>, p: SessionListParams) -> Result<SessionListResult, RpcError> {
    let root = root_key(&root_of(&app, p.workspace_id.as_ref())?);
    let rows = app
        .store_call(move |s| s.session_list(root.as_deref()))
        .await?;
    Ok(SessionListResult {
        sessions: rows.iter().map(|r| info(&app, r)).collect(),
    })
}

pub async fn open(
    app: Arc<App>,
    p: SessionRef,
    ctx: RequestCtx,
) -> Result<SessionOpenResult, RpcError> {
    let (row, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    let live = open_live(&app, &p.session_id, &root)?;
    let events = live.events();
    let last = events.last().map_or(0, |e| e.seq);
    attach(&app, &live, ctx.notifier.clone(), last, &ctx);
    Ok(SessionOpenResult {
        session: info(&app, &row),
        events,
        pending: app.agent.gate.pending(Some(&p.session_id)),
    })
}

pub async fn rename(app: Arc<App>, p: SessionRenameParams) -> Result<SessionInfo, RpcError> {
    let title = p.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 120 {
        return Err(RpcError::new(
            ErrorKind::InvalidParams,
            "A title is 1–120 characters.",
        ));
    }
    let (mut row, _) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    row.title = title;
    let r2 = row.clone();
    app.store_call(move |s| s.session_put(&r2)).await?;
    Ok(info(&app, &row))
}

pub async fn delete(app: Arc<App>, p: SessionRef) -> Result<Empty, RpcError> {
    let (_, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    app.agent
        .sessions
        .delete(&p.session_id, root.as_deref())
        .map_err(journal_error)?;
    // Stops its MCP servers and external agent (their processes are killed when dropped).
    app.agent.mcp.lock().await.remove(&p.session_id);
    let acp = app.agent.acp.lock().await.remove(&p.session_id);
    if let Some(a) = acp {
        a.conn.kill().await;
    }
    app.agent.gate.clear_grants(&principal_of(&p.session_id));
    crate::acp::drop_session(&app, &principal_of(&p.session_id)).await;
    let id = p.session_id.0.clone();
    app.store_call(move |s| s.session_delete(&id)).await?;
    Ok(Empty {})
}

pub async fn image(app: Arc<App>, p: ImageParams) -> Result<ImageData, RpcError> {
    let (_, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    let live = open_live(&app, &p.session_id, &root)?;
    let found = live.events().into_iter().find_map(|e| match e.event {
        EventBody::User { images, .. } => images.into_iter().find(|i| i.sha256 == p.sha256),
        _ => None,
    });
    let r = found.ok_or_else(|| {
        RpcError::new(
            ErrorKind::NotFound,
            "There is no such image in this session.",
        )
    })?;
    live.journal
        .get_image(&r)
        .map_err(|_| RpcError::new(ErrorKind::NotFound, "The image is gone."))
}

pub(crate) fn shadow(app: &App, root: &std::path::Path) -> Arc<Shadow> {
    let mut m = app.agent.shadows.lock().unwrap_or_else(|p| p.into_inner());
    m.entry(root.to_path_buf())
        .or_insert_with(|| Arc::new(Shadow::new(&app.env.data_dir, root)))
        .clone()
}

/// The attached files' contents, read through the workspace (contained, size-capped).
async fn attached_text(
    app: &App,
    ws: &WorkspaceId,
    p: &SendParams,
) -> Result<Option<String>, RpcError> {
    if p.attachments.is_empty() {
        return Ok(None);
    }
    let w = app.workspaces.get(ws)?;
    let mut out = String::new();
    for a in &p.attachments {
        let f = w.read_text(&a.path).await?;
        let lines: Vec<&str> = f.text.lines().collect();
        let (from, to) = a.lines.unwrap_or((1, lines.len() as u32));
        let from = from.max(1) as usize;
        let to = (to as usize).min(lines.len());
        let body = if from <= to {
            lines[from - 1..to].join("\n")
        } else {
            String::new()
        };
        out.push_str(&format!(
            "Attached: {} (lines {from}–{to})\n```\n{body}\n```\n\n",
            a.path.0
        ));
        if out.len() > MAX_ATTACHED_BYTES {
            return Err(RpcError::new(
                ErrorKind::TooLarge,
                "The attachments are larger than 256 KiB together.",
            ));
        }
    }
    Ok(Some(out))
}

async fn agents_md(app: &App, ws: &WorkspaceId) -> Option<String> {
    let w = app.workspaces.get(ws).ok()?;
    w.read_text(&WorkspacePath("AGENTS.md".into()))
        .await
        .ok()
        .map(|f| f.text)
}

fn title_from(text: &str) -> String {
    let first = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    let mut t: String = first.chars().take(60).collect();
    if first.chars().count() > 60 {
        t.push('…');
    }
    if t.is_empty() {
        DEFAULT_TITLE.into()
    } else {
        t
    }
}

pub async fn send(app: Arc<App>, p: SendParams, ctx: RequestCtx) -> Result<Empty, RpcError> {
    if p.text.trim().is_empty() && p.images.is_empty() {
        return Err(RpcError::new(
            ErrorKind::InvalidParams,
            "The message is empty.",
        ));
    }
    let (mut row, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    let live = open_live(&app, &p.session_id, &root)?;
    attach(&app, &live, ctx.notifier.clone(), live.last_seq(), &ctx);
    if let Some(n) = p.from_turn {
        if n == 0 || n > live.turn() {
            return Err(RpcError::new(
                ErrorKind::InvalidParams,
                format!("There is no turn {n} to replace."),
            ));
        }
    }
    let configs = provider_configs(&app).await?;
    let config = app
        .agent
        .providers
        .config(&configs, &row.provider)
        .ok_or_else(|| {
            RpcError::new(
                ErrorKind::NotFound,
                format!("No provider `{}` is configured.", row.provider),
            )
        })?;
    if config.kind == ProviderKind::Acp {
        let key_vars = key_vars(&app, &configs);
        return send_acp(app, p, row, root, live, config, key_vars).await;
    }
    let provider = app
        .agent
        .providers
        .get(&configs, &row.provider)
        .await
        .map_err(crate::handlers::agent::providers_error)?;
    let capabilities = app
        .agent
        .providers
        .capabilities(&configs, &row.provider, &row.model)
        .unwrap_or_default();
    let attached = match &p.workspace_id {
        Some(ws) => attached_text(&app, ws, &p).await?,
        None => None,
    };
    let mut images = Vec::new();
    for im in &p.images {
        images.push(live.journal.put_image(im).map_err(journal_error)?);
    }
    // Everything that can fail on the workspace's content (a malformed `[agent.policy]`) is
    // read before the turn begins (AUD-011).
    let tools = match &p.workspace_id {
        Some(ws) => {
            let root = root.clone().ok_or_else(internal)?;
            Some((ws.clone(), root, policies(&app, ws).await?))
        }
        None => None,
    };
    let system = match &p.workspace_id {
        Some(ws) => {
            let name = app.workspaces.get(ws)?.name().to_string();
            let md = agents_md(&app, ws).await;
            agent_loop::system_prompt(Some((&name, md.as_deref())))
        }
        None => agent_loop::system_prompt(None),
    };
    let Some(cancel) = live.begin() else {
        return Err(RpcError::new(
            ErrorKind::Conflict,
            "A turn is already running in this session.",
        ));
    };
    let guard = TurnGuard::new(&live);
    let input = UserInput {
        text: p.text.clone(),
        attachments: p.attachments.clone(),
        attached_text: attached,
        images,
        from_turn: p.from_turn,
    };
    let turn = agent_loop::begin(&live, &input).map_err(journal_error)?;
    if p.from_turn.is_some() {
        // Rewound: what the person allowed for the session no longer holds.
        app.agent.gate.clear_grants(&principal_of(&live.id));
    }
    if turn == 1 && row.title == DEFAULT_TITLE {
        row.title = title_from(&p.text);
    }
    row.updated_ms = now_ms();
    let r2 = row.clone();
    app.store_call(move |s| s.session_put(&r2)).await?;
    let key_vars = key_vars(&app, &configs);
    guard.disarm();
    let a = app.clone();
    let model = row.model.clone();
    tokio::spawn(async move {
        // Built-in tools plus the session's MCP servers' (started on first use).
        let registry = match &tools {
            Some((_, root, _)) => session_registry(&a, &live, root, &key_vars).await,
            None => a.agent.tools.clone(),
        };
        let env = tools.map(|(ws, root, policies)| ToolEnv {
            registry: &registry,
            gate: &a.agent.gate,
            policies,
            ctx: ToolCtx {
                workspace_id: ws,
                principal: principal_of(&live.id),
                cancel: cancel.clone(),
            },
            shadow: shadow(&a, &root),
            audit: audit_path(&a, &root),
        });
        let t = Turn {
            live: &live,
            provider,
            model,
            capabilities,
            system,
            tools: env,
            cancel,
        };
        agent_loop::run(t, turn).await;
        live.end();
    });
    Ok(Empty {})
}

/// Axiom's own MCP server for an external agent: the `axiom` CLI next to this daemon, if
/// present, serving this workspace through this daemon's socket.
fn axiom_mcp(app: &App, root: &std::path::Path) -> Option<axiom_agent::acp::AxiomMcp> {
    let exe = std::env::current_exe().ok()?;
    let cli = exe
        .parent()?
        .join(if cfg!(windows) { "axiom.exe" } else { "axiom" });
    cli.is_file().then(|| axiom_agent::acp::AxiomMcp {
        command: cli,
        args: vec![
            "mcp".into(),
            "--workspace".into(),
            root.to_string_lossy().into_owned(),
        ],
        env: vec![(
            "AXIOM_DATA_DIR".into(),
            app.env.data_dir.to_string_lossy().into_owned(),
        )],
    })
}

/// A turn of an external agent (ADR-0016).
async fn send_acp(
    app: Arc<App>,
    p: SendParams,
    mut row: SessionRow,
    root: Option<PathBuf>,
    live: Arc<Live>,
    config: axiom_protocol::agent::ProviderConfig,
    key_vars: Vec<String>,
) -> Result<Empty, RpcError> {
    // A conversation without a folder (Chat's general scope, ADR-0020 §6) runs the agent in a
    // scratch folder of its own under the data directory, opened here as a workspace, so its
    // files, terminals, approvals and checkpoints work as in a folder.
    let (ws, root) = match (p.workspace_id.clone(), root) {
        (Some(ws), Some(root)) => (ws, root),
        _ => scratch_workspace(&app, &live.id).await?,
    };
    let attached = attached_text(&app, &ws, &p).await?;
    let mut images = Vec::new();
    for im in &p.images {
        images.push(live.journal.put_image(im).map_err(journal_error)?);
    }
    if p.from_turn.is_some() {
        return Err(RpcError::new(
            ErrorKind::Unsupported,
            "An external agent keeps its own history: editing an earlier message is not possible. Send a new message instead.",
        ));
    }
    // Read before the turn begins: a malformed workspace policy is an error, not a stuck
    // session (AUD-011).
    let policies = policies(&app, &ws).await?;
    let Some(cancel) = live.begin() else {
        return Err(RpcError::new(
            ErrorKind::Conflict,
            "A turn is already running in this session.",
        ));
    };
    let guard = TurnGuard::new(&live);
    let input = UserInput {
        text: p.text.clone(),
        attachments: p.attachments.clone(),
        attached_text: attached.clone(),
        images,
        from_turn: None,
    };
    let turn = agent_loop::begin(&live, &input).map_err(journal_error)?;
    if turn == 1 && row.title == DEFAULT_TITLE {
        row.title = title_from(&p.text);
    }
    row.updated_ms = now_ms();
    let r2 = row.clone();
    app.store_call(move |s| s.session_put(&r2)).await?;
    guard.disarm();
    let a = app.clone();
    tokio::spawn(async move {
        let session = acp_session(&a, &live, &config, &root, &key_vars).await;
        match session {
            Ok(session) => {
                let images: Vec<ImageData> = p.images.clone();
                let prompt = axiom_agent::acp::prompt_blocks(
                    &p.text,
                    attached.as_deref(),
                    &images,
                    &session.caps,
                );
                let terminals = crate::acp::Terminals(a.clone());
                let t = axiom_agent::acp::AcpTurn {
                    live: &live,
                    session: &session,
                    agent_id: config.id.clone(),
                    agent_name: config.name.clone(),
                    root: root.clone(),
                    registry: &a.agent.tools,
                    gate: &a.agent.gate,
                    policies,
                    ctx: ToolCtx {
                        workspace_id: ws,
                        principal: principal_of(&live.id),
                        cancel: cancel.clone(),
                    },
                    shadow: shadow(&a, &root),
                    audit: audit_path(&a, &root),
                    terminals: &terminals,
                    cancel,
                    turn,
                };
                axiom_agent::acp::run_turn(t, prompt).await;
            }
            Err(message) => {
                let _ = live.emit(EventBody::Error {
                    kind: "model_error".into(),
                    model_kind: None,
                    message,
                });
                let _ = live.emit(EventBody::TurnEnd {
                    reason: axiom_protocol::agent::TurnEndReason::Error,
                });
            }
        }
        live.end();
    });
    Ok(Empty {})
}

/// The session's external agent: the running one, else a new one (also when the cached one has
/// exited or was killed: AUD-032). The daemon-wide map is locked only to look and to insert,
/// never while the agent starts, so one slow agent does not hold up other sessions.
async fn acp_session(
    a: &Arc<App>,
    live: &Arc<Live>,
    config: &axiom_protocol::agent::ProviderConfig,
    root: &std::path::Path,
    key_vars: &[String],
) -> Result<Arc<axiom_agent::acp::AcpSession>, String> {
    touch(a, &live.id);
    {
        let mut m = a.agent.acp.lock().await;
        match m.get(&live.id) {
            Some(s) if !s.conn.is_ended() => return Ok(s.clone()),
            Some(_) => {
                m.remove(&live.id);
            }
            None => {}
        }
    }
    let saved_path = live.journal.path().with_extension("acp");
    let saved = tokio::fs::read_to_string(&saved_path).await.ok();
    let (s, resumed) = axiom_agent::acp::connect(
        config,
        root,
        saved.as_deref().map(str::trim),
        axiom_mcp(a, root).as_ref(),
        key_vars,
    )
    .await?;
    if saved.is_some() && !resumed {
        let _ = live.emit(EventBody::Error {
            kind: "notice".into(),
            model_kind: None,
            message: format!("{} cannot resume its earlier conversation, so it starts fresh; the history above is kept.", config.name),
        });
    }
    let _ = tokio::fs::write(&saved_path, &s.agent_session).await;
    let s = Arc::new(s);
    let mut m = a.agent.acp.lock().await;
    // Deleted while it started: it goes with the session.
    if a.agent.sessions.get(&live.id).is_none() {
        drop(m);
        s.conn.kill().await;
        return Err(format!(
            "{} stopped: the conversation was deleted.",
            config.name
        ));
    }
    m.insert(live.id.clone(), s.clone());
    Ok(s)
}

/// A session used its external agent or MCP servers now.
fn touch(app: &App, id: &SessionId) {
    app.agent
        .last_used
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .insert(id.clone(), std::time::Instant::now());
}

/// How long a session's external agent and MCP servers may sit unused before they are stopped.
pub const IDLE: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// Stops what idle sessions keep running (AUD-032): their MCP servers (started again on the next
/// turn: they keep no conversation), and their external agent when it can resume the
/// conversation (`loadSession`); one that cannot is kept, or the conversation would be lost.
pub fn spawn_idle_reaper(app: &Arc<App>) {
    let weak = Arc::downgrade(app);
    // TEST ONLY: `AXIOM_TEST_AGENT_IDLE_MS` shortens the idle time (and the check's interval).
    let idle = std::env::var("AXIOM_TEST_AGENT_IDLE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or(IDLE);
    let every = (idle / 2).min(std::time::Duration::from_secs(60));
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            let Some(app) = weak.upgrade() else { return };
            reap_idle(&app, idle).await;
        }
    });
}

/// One pass of [`spawn_idle_reaper`]: sessions unused for `idle` and not mid-turn.
pub async fn reap_idle(app: &App, idle: std::time::Duration) {
    let now = std::time::Instant::now();
    let stale: Vec<SessionId> = app
        .agent
        .last_used
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .iter()
        .filter(|(_, t)| now.duration_since(**t) >= idle)
        .map(|(id, _)| id.clone())
        .collect();
    for id in stale {
        if app.agent.sessions.get(&id).is_some_and(|l| l.running()) {
            continue;
        }
        // Dropping a running MCP client stops its server.
        app.agent.mcp.lock().await.remove(&id);
        let acp = {
            let mut m = app.agent.acp.lock().await;
            match m.get(&id) {
                Some(s) if s.caps.load_session => m.remove(&id),
                _ => None,
            }
        };
        if let Some(s) = acp {
            s.conn.kill().await;
        }
        let keep = app.agent.acp.lock().await.contains_key(&id);
        if !keep {
            app.agent
                .last_used
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
        }
    }
}

/// The scratch folder of a conversation without a folder: `<data dir>/scratch/<session>`,
/// created on first use and opened as a workspace (the same id while the daemon runs).
async fn scratch_workspace(app: &App, id: &SessionId) -> Result<(WorkspaceId, PathBuf), RpcError> {
    let dir = app.env.data_dir.join("scratch").join(&id.0);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        RpcError::new(
            ErrorKind::Io,
            format!("Could not create the conversation's folder: {e}"),
        )
    })?;
    let opened = app.workspaces.open(&dir.to_string_lossy()).await?;
    Ok((opened.workspace_id, PathBuf::from(opened.root)))
}

const MCP_KEY: &str = "agent.mcpServers";

/// The built-in registry plus the tools of every configured MCP server, which are started once
/// per session. A server that fails to start is reported as an error event; the turn goes on.
async fn session_registry(
    app: &App,
    live: &Live,
    root: &std::path::Path,
    key_vars: &[String],
) -> axiom_agent::tools::Registry {
    touch(app, &live.id);
    let mut reg = app.agent.tools.clone();
    let configs: Vec<axiom_agent::mcp_client::ServerConfig> = app
        .store_call(|s| s.setting_get(MCP_KEY))
        .await
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    if configs.is_empty() {
        return reg;
    }
    // Which servers to start (none running, or the running one exited: AUD-032), under a short
    // lock; they start without it, so a slow server holds up no other session.
    let missing: Vec<&axiom_agent::mcp_client::ServerConfig> = {
        let mut running = app.agent.mcp.lock().await;
        let entry = running.entry(live.id.clone()).or_default();
        entry.retain(|(c, _)| !c.is_ended());
        configs
            .iter()
            .filter(|cfg| !entry.iter().any(|(c, _)| c.name == cfg.name))
            .collect()
    };
    let mut started = Vec::new();
    for cfg in missing {
        match axiom_agent::mcp_client::McpClient::start(cfg, root, key_vars).await {
            Ok(s) => started.push(s),
            Err(e) => {
                let _ = live.emit(EventBody::Error {
                    kind: "io".into(),
                    model_kind: None,
                    message: format!("{e}; its tools are not available."),
                });
            }
        }
    }
    let mut running = app.agent.mcp.lock().await;
    if app.agent.sessions.get(&live.id).is_none() {
        // Deleted meanwhile: the servers just started are dropped (and killed) here.
        running.remove(&live.id);
        return reg;
    }
    let entry = running.entry(live.id.clone()).or_default();
    for s in started {
        if !entry.iter().any(|(c, _)| c.name == s.0.name) {
            entry.push(s);
        }
    }
    for (client, tools) in entry.iter() {
        for t in tools {
            let tool = axiom_agent::mcp_client::McpTool::new(client.clone(), t.clone());
            if reg.get(&tool.spec().name).is_none() {
                reg.register(Arc::new(tool));
            }
        }
    }
    reg
}

pub async fn cancel(app: Arc<App>, p: SessionRef) -> Result<Empty, RpcError> {
    let (_, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    open_live(&app, &p.session_id, &root)?.cancel();
    Ok(Empty {})
}

fn checkpoint_rpc(e: CheckpointError) -> RpcError {
    match e {
        CheckpointError::GitMissing => RpcError::new(
            ErrorKind::PrerequisiteMissing,
            "Checkpoints need git, which is not installed.",
        )
        .with_details(serde_json::json!({ "missing": ["git"] })),
        CheckpointError::NotFound(n) => RpcError::new(
            ErrorKind::NotFound,
            format!("There is no checkpoint {n} in this session."),
        ),
        CheckpointError::Conflicts(c) => RpcError::new(
            ErrorKind::CheckpointConflict,
            format!(
                "{} files changed since the agent last wrote them; nothing was restored.",
                c.len()
            ),
        )
        .with_details(serde_json::json!({ "conflicts": c })),
        other => {
            tracing::error!("checkpoint: {other}");
            RpcError::new(ErrorKind::Io, "The checkpoint operation failed.")
        }
    }
}

pub async fn checkpoints(app: Arc<App>, p: SessionRef) -> Result<CheckpointsResult, RpcError> {
    let (_, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    let Some(root) = root else {
        return Ok(CheckpointsResult {
            checkpoints: vec![],
        });
    };
    Ok(CheckpointsResult {
        checkpoints: shadow(&app, &root)
            .list(&p.session_id.0)
            .await
            .map_err(checkpoint_rpc)?,
    })
}

pub async fn restore(app: Arc<App>, p: RestoreParams) -> Result<RestoreResult, RpcError> {
    let (_, root) = row(&app, p.workspace_id.as_ref(), &p.session_id).await?;
    let root = root.ok_or_else(|| {
        RpcError::new(
            ErrorKind::NotFound,
            "A conversation without a workspace has no checkpoints.",
        )
    })?;
    let live = open_live(&app, &p.session_id, &Some(root.clone()))?;
    if live.running() {
        return Err(RpcError::new(
            ErrorKind::Conflict,
            "Stop the running turn before restoring.",
        ));
    }
    let r = shadow(&app, &root)
        .restore(&p.session_id.0, p.checkpoint, live.turn(), p.skip_conflicts)
        .await
        .map_err(checkpoint_rpc)?;
    let _ = live.emit(EventBody::Restored { result: r.clone() });
    // The files are back to an earlier state: what was allowed for the session no longer
    // holds (AUD-009).
    app.agent.gate.clear_grants(&principal_of(&p.session_id));
    Ok(r)
}
