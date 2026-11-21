use axiom_env::{Builtin, Http, HttpConfig, Installer};
use axiom_exec::Executor;
use axiom_protocol::build::BuildDiagnostics;
use axiom_protocol::diagnostics::Diagnostic;
use axiom_protocol::{ErrorKind, RpcError, TaskId, WorkspaceId};
use axiom_rpc::Notifier;
use axiom_store::{Store, StoreError};
use axiom_workspace::Workspaces;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::task::AbortHandle;

/// Everything the handlers share.
pub struct App {
    pub store: Arc<Store>,
    pub workspaces: Workspaces,
    pub executor: Executor,
    /// Forwarder tasks for `fs/watch`, one per workspace. Aborting a task
    /// drops the `WatchHandle` it owns.
    pub watchers: Mutex<HashMap<WorkspaceId, AbortHandle>>,
    pub env: EnvConfig,
    pub agent: AgentServices,
    /// The newest build's diagnostics per workspace (`build.diagnostics` tool).
    pub last_diagnostics: Mutex<HashMap<WorkspaceId, (TaskId, Vec<Diagnostic>)>>,
    /// Terminals ACP agents created (`terminal/create`).
    pub acp_terminals: Mutex<HashMap<String, Arc<crate::acp::Terminal>>>,
    /// Connections that opened each workspace, for daemon-initiated notifications
    /// (`agent/reveal`). Closed connections are dropped on the next send.
    pub clients: Mutex<HashMap<WorkspaceId, Vec<Notifier>>>,
    /// Language servers (M4, ADR-0018 §2).
    pub lsp: axiom_lsp::Manager,
    /// Files outside each workspace that one of its language servers returned in a location:
    /// readable (only) through `fs/readFile` with a `file://` path (ADR-0018 §2).
    pub external_reads: Mutex<HashMap<WorkspaceId, std::collections::HashSet<PathBuf>>>,
    /// `git/didChange` watches, one per workspace that asked for its status (M4-GIT-001).
    pub git_watches: Mutex<HashMap<WorkspaceId, AbortHandle>>,
    /// Integrated terminals (M4, ADR-0018 §4).
    pub terminals: axiom_term::Terminals,
    /// The language-server event forwarder runs (started with the first router).
    pub lsp_forwarding: std::sync::atomic::AtomicBool,
    /// Previews and their pages' inspection (M5, ADR-0019).
    pub previews: crate::handlers::ui::Previews,
}

/// The agent runtime's daemon-wide state (M3).
pub struct AgentServices {
    pub providers: axiom_agent::providers::Providers,
    /// The one permission gate (ADR-0015 §8).
    pub gate: axiom_agent::gate::Gate,
    /// Connections that declared `capabilities.approvals`.
    pub approvers: Arc<ConnApprovers>,
    /// Open sessions (journals under `<data dir>/sessions/`).
    pub sessions: axiom_agent::live::Sessions,
    /// One shadow repository per workspace root (checkpoints).
    pub shadows: Mutex<HashMap<PathBuf, Arc<axiom_agent::checkpoint::Shadow>>>,
    /// Running MCP servers per session, with their tools (started on the session's first turn).
    pub mcp: tokio::sync::Mutex<HashMap<axiom_protocol::agent::SessionId, Vec<RunningMcp>>>,
    /// External agents attached to sessions (ADR-0016), started on the first turn.
    pub acp: tokio::sync::Mutex<
        HashMap<axiom_protocol::agent::SessionId, Arc<axiom_agent::acp::AcpSession>>,
    >,
    /// Which connection is attached to which session (one forwarder each).
    pub attached: Mutex<Vec<(axiom_protocol::agent::SessionId, Notifier)>>,
    /// Built-in tools (ADR-0015 §2), backed by this daemon's services.
    pub tools: axiom_agent::tools::Registry,
}

/// TEST ONLY: a script file for the scripted model provider (ADR-0015 §4). When set, the daemon
/// offers the provider `scripted` that replays it.
pub const SCRIPTED_MODEL_ENV: &str = "AXIOM_TEST_SCRIPTED_MODEL";

/// A started MCP server and the tools it listed.
pub type RunningMcp = (
    Arc<axiom_agent::mcp_client::McpClient>,
    Vec<axiom_agent::mcp_client::RemoteTool>,
);

/// The connections that can answer approvals; a closed one is dropped on the next send.
#[derive(Default)]
pub struct ConnApprovers(Mutex<Vec<Notifier>>);

impl ConnApprovers {
    pub fn add(&self, n: Notifier) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).push(n);
    }
    fn all(&self) -> Vec<Notifier> {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

impl axiom_agent::gate::Approvers for ConnApprovers {
    fn request<'a>(
        &'a self,
        req: &'a axiom_protocol::agent::ApprovalRequest,
    ) -> futures_util::future::BoxFuture<'a, usize> {
        Box::pin(async move {
            let mut sent = 0;
            let mut live = Vec::new();
            for n in self.all() {
                if n.notify::<axiom_protocol::agent::ApprovalRequested>(req.clone())
                    .await
                    .is_ok()
                {
                    sent += 1;
                    live.push(n);
                }
            }
            *self.0.lock().unwrap_or_else(|p| p.into_inner()) = live;
            sent
        })
    }
    fn resolved<'a>(
        &'a self,
        res: &'a axiom_protocol::agent::ApprovalResolution,
    ) -> futures_util::future::BoxFuture<'a, ()> {
        Box::pin(async move {
            for n in self.all() {
                let _ = n
                    .notify::<axiom_protocol::agent::ApprovalResolved>(res.clone())
                    .await;
            }
        })
    }
}

fn agent_services(app: std::sync::Weak<App>, data_dir: &std::path::Path) -> AgentServices {
    let approvers = Arc::new(ConnApprovers::default());
    let scripted = std::env::var_os(SCRIPTED_MODEL_ENV).and_then(|p| {
        match axiom_agent::model::scripted::ScriptedProvider::from_file(
            axiom_agent::providers::SCRIPTED_ID,
            std::path::Path::new(&p),
        ) {
            Ok(s) => {
                tracing::warn!("{SCRIPTED_MODEL_ENV} is set: the scripted test model is offered");
                Some(Arc::new(s))
            }
            Err(e) => {
                tracing::error!("{SCRIPTED_MODEL_ENV}: {e}");
                None
            }
        }
    });
    AgentServices {
        providers: axiom_agent::providers::Providers::new(
            Arc::from(axiom_agent::credentials::platform_or_test()),
            scripted,
        ),
        tools: crate::tools::registry(app),
        gate: axiom_agent::gate::Gate::new(approvers.clone()),
        approvers,
        sessions: axiom_agent::live::Sessions::new(data_dir.join("sessions")),
        shadows: Mutex::default(),
        mcp: Default::default(),
        acp: Default::default(),
        attached: Mutex::default(),
    }
}

/// Where the environment services find their files.
pub struct EnvConfig {
    /// The daemon's data directory: `providers/` (user providers) and `toolchains/` (managed
    /// installs) live here, next to the database.
    pub data_dir: PathBuf,
    pub builtin: Builtin,
    pub installer: Arc<Installer>,
}

/// TEST ONLY: set to `1` to let installs use plain HTTP to loopback hosts (a fixture server).
/// Nothing else changes: every other URL must still be HTTPS.
pub const INSECURE_LOOPBACK_ENV: &str = "AXIOM_TEST_INSECURE_LOOPBACK_HTTP";

impl App {
    /// Data directory: the database's (`AXIOM_DATA_DIR` or the platform default).
    pub fn new(store: Store) -> Arc<App> {
        let data_dir = axiom_store::default_db_path()
            .parent()
            .map(PathBuf::from)
            .unwrap_or_default();
        Self::with_data_dir(store, data_dir)
    }

    pub fn with_data_dir(store: Store, data_dir: PathBuf) -> Arc<App> {
        axiom_agent::acp::set_agents_dir(data_dir.join("agents"));
        let allow_http = std::env::var(INSECURE_LOOPBACK_ENV).is_ok_and(|v| v == "1");
        if allow_http {
            tracing::warn!("{INSECURE_LOOPBACK_ENV}=1: plain HTTP to loopback hosts is allowed");
        }
        let http = match Http::new(HttpConfig {
            allow_http,
            ..HttpConfig::default()
        }) {
            Ok(h) => h,
            Err(e) => {
                // Building a client only fails if the TLS backend cannot initialise.
                tracing::error!("cannot initialise HTTPS: {e}");
                std::process::exit(1);
            }
        };
        let installer = Arc::new(Installer::new(data_dir.clone(), http));
        let sessions_dir = data_dir.clone();
        Arc::new_cyclic(|weak| App {
            store: Arc::new(store),
            workspaces: Workspaces::new(),
            executor: Executor::new(),
            watchers: Mutex::default(),
            env: EnvConfig {
                data_dir,
                builtin: Builtin::Embedded,
                installer,
            },
            agent: agent_services(weak.clone(), &sessions_dir),
            last_diagnostics: Mutex::default(),
            acp_terminals: Mutex::default(),
            clients: Mutex::default(),
            lsp: axiom_lsp::Manager::default(),
            external_reads: Mutex::default(),
            git_watches: Mutex::default(),
            terminals: axiom_term::Terminals::default(),
            lsp_forwarding: std::sync::atomic::AtomicBool::new(false),
            previews: crate::handlers::ui::Previews::default(),
        })
    }

    pub fn record_diagnostics(&self, ws: &WorkspaceId, d: &BuildDiagnostics) {
        let mut m = self
            .last_diagnostics
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let entry = m
            .entry(ws.clone())
            .or_insert_with(|| (d.task_id.clone(), Vec::new()));
        if entry.0 != d.task_id {
            *entry = (d.task_id.clone(), Vec::new());
        }
        entry.1.extend(d.diagnostics.iter().cloned());
    }

    pub fn add_client(&self, ws: &WorkspaceId, n: Notifier) {
        self.clients
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .entry(ws.clone())
            .or_default()
            .push(n);
    }

    /// The connections that opened `ws`.
    pub fn clients_of(&self, ws: &WorkspaceId) -> Vec<Notifier> {
        self.clients
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(ws)
            .cloned()
            .unwrap_or_default()
    }

    /// Records the `file://` URIs outside `root` that a language server returned, so the editor
    /// may open them read-only.
    pub fn allow_external_reads(&self, ws: &WorkspaceId, root: &std::path::Path, uris: &[String]) {
        let mut m = self
            .external_reads
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let set = m.entry(ws.clone()).or_default();
        for u in uris {
            let Some(path) = url::Url::parse(u).ok().and_then(|u| u.to_file_path().ok()) else {
                continue;
            };
            if !path.starts_with(root) && set.len() < 10_000 {
                set.insert(path);
            }
        }
    }

    /// Whether `path` (absolute) was returned by one of the workspace's language servers.
    pub fn external_read_allowed(&self, ws: &WorkspaceId, path: &std::path::Path) -> bool {
        self.external_reads
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(ws)
            .is_some_and(|s| s.contains(path))
    }

    pub fn stop_watcher(&self, id: &WorkspaceId) {
        let handle = self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id);
        if let Some(h) = handle {
            h.abort();
        }
    }

    pub fn stop_all_watchers(&self) {
        let all: Vec<AbortHandle> = self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain()
            .map(|(_, h)| h)
            .collect();
        for h in all {
            h.abort();
        }
    }

    /// Runs a blocking `Store` call off the async threads.
    pub async fn store_call<T, F>(&self, f: F) -> Result<T, RpcError>
    where
        T: Send + 'static,
        F: FnOnce(&Store) -> Result<T, StoreError> + Send + 'static,
    {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || f(&store))
            .await
            .map_err(|e| {
                tracing::error!("store task failed: {e}");
                internal()
            })?
            .map_err(store_error)
    }
}

pub(crate) fn internal() -> RpcError {
    RpcError::new(ErrorKind::Internal, "Internal error.")
}

pub(crate) fn store_error(e: StoreError) -> RpcError {
    match e {
        StoreError::TooLarge { key, bytes, limit } => RpcError::new(
            ErrorKind::TooLarge,
            format!("The value for `{key}` is too large."),
        )
        .with_details(serde_json::json!({ "key": key, "bytes": bytes, "limit": limit })),
        other => {
            tracing::error!("store error: {other}");
            internal()
        }
    }
}
