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
    /// Forwarder tasks for `fs/watch`: one per workspace and connection that enabled it (each
    /// connection gets the events, AUD-028). Aborting a task drops the `WatchHandle` it owns.
    pub watchers: Mutex<HashMap<WorkspaceId, Vec<(Notifier, AbortHandle)>>>,
    pub env: EnvConfig,
    pub agent: AgentServices,
    /// The newest build's diagnostics per workspace (`build.diagnostics` tool).
    pub last_diagnostics: Mutex<HashMap<WorkspaceId, (TaskId, Vec<Diagnostic>)>>,
    /// Terminals ACP agents created (`terminal/create`).
    pub acp_terminals: Mutex<HashMap<String, Arc<crate::acp::Terminal>>>,
    /// Connections that opened each workspace (one entry per connection), for daemon-initiated
    /// notifications. They also hold the workspace open: it is torn down when the last one
    /// closes it or disconnects (AUD-029). Closed connections are pruned on every read (AUD-030).
    pub clients: Mutex<HashMap<WorkspaceId, Vec<Notifier>>>,
    /// Held by `workspace/open` and by a close's "last holder? then tear down": an open during a
    /// teardown waits for it and opens the folder afresh, rather than getting the id that is
    /// being closed (AUD-029, review B5).
    pub workspace_lifecycle: tokio::sync::Mutex<()>,
    /// Connections that let a relay time out on their full queue (see `notify_clients`).
    lagging: Mutex<Vec<Notifier>>,
    /// Language servers (M4, ADR-0018 §2).
    pub lsp: axiom_lsp::Manager,
    /// Files outside each workspace that one of its language servers returned in a location:
    /// readable (only) through `fs/readFile` with a `file://` path (ADR-0018 §2).
    pub external_reads: Mutex<HashMap<WorkspaceId, std::collections::HashSet<PathBuf>>>,
    /// `git/didChange` watches, one per workspace that asked for its status (M4-GIT-001).
    pub git_watches: Mutex<HashMap<WorkspaceId, AbortHandle>>,
    /// Workspaces whose change watch fell back to polling (`GitStatus.polling`, AUD-012).
    pub git_polling: Mutex<std::collections::HashSet<WorkspaceId>>,
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
    /// When each session last used its external agent or MCP servers: idle ones are stopped
    /// (AUD-032, `handlers::sessions::spawn_idle_reaper`).
    pub last_used: Mutex<HashMap<axiom_protocol::agent::SessionId, std::time::Instant>>,
}

/// How long a connection whose queue is full may hold up a daemon-wide relay (AUD-030).
pub const SLOW_CLIENT: std::time::Duration = std::time::Duration::from_millis(500);

/// How long an approval prompt waits for room on a connection whose queue is full. Much longer
/// than [`SLOW_CLIENT`]: a prompt is for the person, and a connection that is busy but reading
/// (the shell during an output flood) catches up well within it, where giving up would answer
/// "no approver" (review R4). The other approvers are not held up: each send is separate, and
/// an answer that arrives meanwhile is kept (the gate registers the request first).
pub const APPROVAL_DELIVERY: std::time::Duration = std::time::Duration::from_secs(10);

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
            // To every approving connection at once, none waited on past APPROVAL_DELIVERY: one
            // that stopped reading does not hold the prompt back from the others (review B6b,
            // R4). One that is slow but open stays an approver; only closed ones are dropped.
            let sends = self.all().into_iter().map(|n| async move {
                let r = tokio::time::timeout(
                    APPROVAL_DELIVERY,
                    n.notify::<axiom_protocol::agent::ApprovalRequested>(req.clone()),
                )
                .await;
                (matches!(r, Ok(Ok(()))), n)
            });
            let results = futures_util::future::join_all(sends).await;
            let sent = results.iter().filter(|(ok, _)| *ok).count();
            *self.0.lock().unwrap_or_else(|p| p.into_inner()) = results
                .into_iter()
                .filter(|(ok, n)| *ok || !n.is_closed())
                .map(|(_, n)| n)
                .collect();
            sent
        })
    }
    fn resolved<'a>(
        &'a self,
        res: &'a axiom_protocol::agent::ApprovalResolution,
    ) -> futures_util::future::BoxFuture<'a, ()> {
        Box::pin(async move {
            let sends = self.all().into_iter().map(|n| async move {
                let _ = tokio::time::timeout(
                    APPROVAL_DELIVERY,
                    n.notify::<axiom_protocol::agent::ApprovalResolved>(res.clone()),
                )
                .await;
            });
            futures_util::future::join_all(sends).await;
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
        last_used: Mutex::default(),
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
            workspace_lifecycle: tokio::sync::Mutex::default(),
            lagging: Mutex::default(),
            lsp: axiom_lsp::Manager::default(),
            external_reads: Mutex::default(),
            git_watches: Mutex::default(),
            git_polling: Mutex::default(),
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

    /// Records that the connection of `n` has `ws` open (once per connection, AUD-030).
    pub fn add_client(&self, ws: &WorkspaceId, n: Notifier) {
        let mut m = self.clients.lock().unwrap_or_else(|p| p.into_inner());
        let v = m.entry(ws.clone()).or_default();
        v.retain(|c| !c.is_closed());
        if !v.iter().any(|c| c.same_connection(&n)) {
            v.push(n);
        }
    }

    /// The live connections that opened `ws` (closed ones are pruned here).
    pub fn clients_of(&self, ws: &WorkspaceId) -> Vec<Notifier> {
        let mut m = self.clients.lock().unwrap_or_else(|p| p.into_inner());
        match m.get_mut(ws) {
            Some(v) => {
                v.retain(|c| !c.is_closed());
                v.clone()
            }
            None => Vec::new(),
        }
    }

    /// Removes the connection of `n` from `ws`'s holders; true if no live connection holds the
    /// workspace any more (the caller tears it down).
    pub fn release_client(&self, ws: &WorkspaceId, n: &Notifier) -> bool {
        let mut m = self.clients.lock().unwrap_or_else(|p| p.into_inner());
        let Some(v) = m.get_mut(ws) else {
            return true;
        };
        v.retain(|c| !c.is_closed() && !c.same_connection(n));
        v.is_empty()
    }

    /// The workspaces the connection of `n` holds open.
    pub fn workspaces_of(&self, n: &Notifier) -> Vec<WorkspaceId> {
        self.clients
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .filter(|(_, v)| v.iter().any(|c| c.same_connection(n)))
            .map(|(w, _)| w.clone())
            .collect()
    }

    /// Sends a notification to every live connection of `ws` at once, without letting a slow
    /// one hold up the others (AUD-030): a connection whose queue is full gets
    /// [`SLOW_CLIENT`] to make room, then misses this notification. Returns how many
    /// connections got it.
    pub async fn notify_clients<N>(&self, ws: &WorkspaceId, params: N::Params) -> usize
    where
        N: axiom_protocol::Notification,
        N::Params: Clone,
    {
        let sends = self.clients_of(ws).into_iter().map(|c| {
            let p = params.clone();
            async move {
                match c.try_notify::<N>(p.clone()) {
                    Ok(true) => {
                        self.set_lagging(&c, false);
                        true
                    }
                    Err(_) => false,
                    // Already known to be stuck: no more waiting on it until it reads again.
                    Ok(false) if self.is_lagging(&c) => false,
                    Ok(false) => match tokio::time::timeout(SLOW_CLIENT, c.notify::<N>(p)).await {
                        Ok(r) => r.is_ok(),
                        Err(_) => {
                            tracing::warn!("a connection is not reading; it misses notifications");
                            self.set_lagging(&c, true);
                            false
                        }
                    },
                }
            }
        });
        futures_util::future::join_all(sends)
            .await
            .into_iter()
            .filter(|sent| *sent)
            .count()
    }

    fn is_lagging(&self, n: &Notifier) -> bool {
        self.lagging
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .any(|c| c.same_connection(n))
    }

    fn set_lagging(&self, n: &Notifier, on: bool) {
        let mut v = self.lagging.lock().unwrap_or_else(|p| p.into_inner());
        v.retain(|c| !c.is_closed() && !c.same_connection(n));
        if on {
            v.push(n.clone());
        }
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

    /// Stops every connection's `fs/watch` of the workspace.
    pub fn stop_watcher(&self, id: &WorkspaceId) {
        let handles = self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(id)
            .unwrap_or_default();
        for (_, h) in handles {
            h.abort();
        }
    }

    /// Stops the `fs/watch` of one connection (`n`) on the workspace.
    pub fn stop_watcher_of(&self, id: &WorkspaceId, n: &Notifier) {
        let mut m = self.watchers.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(v) = m.get_mut(id) {
            v.retain(|(c, h)| {
                let mine = c.same_connection(n);
                if mine {
                    h.abort();
                }
                !mine
            });
        }
    }

    pub fn stop_all_watchers(&self) {
        let all: Vec<AbortHandle> = self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .drain()
            .flat_map(|(_, v)| v.into_iter().map(|(_, h)| h))
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

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_protocol::workspace::WorkspaceRef;
    use std::time::Duration;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.6","client":{"name":"t","version":"0"}}}"#;

    /// A served connection: its notifier, and the client's end of the stream.
    async fn connection(
        router: Arc<axiom_rpc::Router>,
        seen: Arc<Mutex<Vec<Notifier>>>,
    ) -> (Notifier, BufReader<tokio::io::DuplexStream>) {
        let (mut cw, sr) = tokio::io::duplex(4096);
        let (sw, cr) = tokio::io::duplex(4096);
        let info = axiom_rpc::ServerInfo {
            name: "t".into(),
            version: "0".into(),
        };
        tokio::spawn(axiom_rpc::serve(sr, sw, router, info));
        cw.write_all(format!("{INIT}\n").as_bytes()).await.unwrap();
        let mut r = BufReader::new(cr);
        let mut line = String::new();
        r.read_line(&mut line).await.unwrap();
        // Keep the writer open (EOF would end the connection).
        std::mem::forget(cw);
        let n = seen.lock().unwrap().pop().unwrap();
        (n, r)
    }

    /// AUD-030: one connection that stops reading does not hold up a daemon-wide relay: the
    /// other connection gets every notification, promptly.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_stalled_connection_does_not_block_the_others() {
        let app = App::new(Store::open_in_memory().unwrap());
        let seen: Arc<Mutex<Vec<Notifier>>> = Arc::default();
        let mut r = axiom_rpc::Router::new();
        let s2 = seen.clone();
        r.on_initialize(move |_, n| s2.lock().unwrap().push(n));
        let router = Arc::new(r);
        let ws = WorkspaceId("w1".into());
        let (stalled, _keep) = connection(router.clone(), seen.clone()).await;
        let (live, reader) = connection(router.clone(), seen.clone()).await;
        app.add_client(&ws, stalled);
        app.add_client(&ws, live.clone());
        // Adding the same connection again keeps one entry.
        app.add_client(&ws, live);
        assert_eq!(app.clients_of(&ws).len(), 2);
        const N: usize = 3000;
        let reading = tokio::spawn(async move {
            let mut reader = reader;
            let mut got = 0;
            let mut line = String::new();
            while got < N {
                line.clear();
                if reader.read_line(&mut line).await.unwrap() == 0 {
                    break;
                }
                got += 1;
            }
            got
        });
        let t = std::time::Instant::now();
        let sending = async {
            for _ in 0..N {
                app.notify_clients::<axiom_protocol::git::DidChange>(
                    &ws,
                    WorkspaceRef {
                        workspace_id: ws.clone(),
                    },
                )
                .await;
            }
        };
        tokio::time::timeout(Duration::from_secs(20), sending)
            .await
            .expect("the relay was held up by the stalled connection");
        let got = tokio::time::timeout(Duration::from_secs(10), reading)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got, N);
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    }
}
