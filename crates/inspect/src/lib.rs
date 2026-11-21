//! The inspection service (ADR-0019 §4; `docs/protocols/inspection.md`).
//!
//! The page of a preview runs Axiom's adapter (`inspectors/web/adapter.js`), which dials this
//! service's loopback WebSocket with the preview's one-time token in the URL
//! (`ws://127.0.0.1:<port>/?token=<token>`). The daemon is the JSON-RPC client on that
//! connection: it sends `inspect/hello`, then `inspect/*` requests, and the adapter sends
//! notifications (`inspect/didMutate`, `hovered`, `selected`, `runtimeError`).
//!
//! What the adapter reports is untrusted data: source stamps are resolved to workspace paths
//! (anything outside the workspace is dropped), confidences are capped, sizes are bounded.

mod normalize;
mod session;
mod verify;

pub use normalize::{normalize_node, Normalizer};
pub use session::Session;
pub use verify::{evaluate, find, verify, Observed};

use axiom_protocol::inspection::{
    Capabilities, HighlightParams, HitTestParams, HitTestResult, MutationReason, NodeId, NodeRef,
    SetModeParams, Snapshot, SnapshotParams, UiNode,
};
use axiom_protocol::preview::PreviewId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::broadcast;

/// Timeouts of the adapter protocol (`inspection.md` §Adapter methods).
pub const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
pub const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
/// How far up `ui/nodeDetails` looks for an unstamped element's owner.
const MAX_OWNER_DEPTH: usize = 32;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum InspectError {
    #[error("there is no such preview")]
    NotFound,
    #[error("no page is connected to the preview")]
    AdapterUnavailable,
    #[error("the page did not answer within {0} s")]
    Timeout(u64),
    #[error("the page's adapter answered: {0}")]
    Adapter(String),
    #[error("{0}")]
    NotAtPoint(String),
}

/// What happened in a preview's page, for the daemon to relay to clients. Events are few (one
/// per pointer move at most), so their size does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum InspectEvent {
    Connected {
        preview: PreviewId,
        capabilities: Capabilities,
    },
    Disconnected {
        preview: PreviewId,
    },
    Hovered {
        preview: PreviewId,
        node: Option<UiNode>,
    },
    Selected {
        preview: PreviewId,
        chain: Vec<UiNode>,
    },
    Mutated {
        preview: PreviewId,
        reason: MutationReason,
    },
}

struct Registration {
    preview: PreviewId,
    root: PathBuf,
}

/// The service: one loopback port per daemon, a session per connected preview page.
pub struct Inspect {
    port: u16,
    tokens: Mutex<HashMap<String, Registration>>,
    sessions: Mutex<HashMap<PreviewId, Arc<Session>>>,
    events: broadcast::Sender<InspectEvent>,
}

impl Inspect {
    /// Binds `127.0.0.1:0` and accepts adapters in the background.
    pub async fn start() -> std::io::Result<Arc<Self>> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
        let port = listener.local_addr()?.port();
        let (events, _) = broadcast::channel(256);
        let me = Arc::new(Self {
            port,
            tokens: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            events,
        });
        let weak = Arc::downgrade(&me);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let Some(me) = weak.upgrade() else { return };
                tokio::spawn(async move { session::accept(me, stream).await });
            }
        });
        Ok(me)
    }

    /// The port adapters dial (`ws://127.0.0.1:<port>/?token=…`).
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The endpoint a page's adapter is given.
    pub fn endpoint(&self) -> String {
        format!("ws://127.0.0.1:{}/", self.port)
    }

    /// A one-time token for [preview], whose stamps resolve against [root]. A page that
    /// connects with it becomes the preview's session (replacing an earlier one: a reload).
    pub fn register(&self, preview: PreviewId, root: PathBuf) -> std::io::Result<String> {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).map_err(std::io::Error::other)?;
        let token: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let mut tokens = lock(&self.tokens);
        tokens.retain(|_, r| r.preview != preview);
        tokens.insert(token.clone(), Registration { preview, root });
        Ok(token)
    }

    /// The preview ended: its token is void and its session closed.
    pub fn unregister(&self, preview: &PreviewId) {
        lock(&self.tokens).retain(|_, r| &r.preview != preview);
        if let Some(s) = lock(&self.sessions).remove(preview) {
            s.close();
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<InspectEvent> {
        self.events.subscribe()
    }

    /// The connected page's session.
    pub fn session(&self, preview: &PreviewId) -> Result<Arc<Session>, InspectError> {
        lock(&self.sessions)
            .get(preview)
            .filter(|s| s.is_open())
            .cloned()
            .ok_or(InspectError::AdapterUnavailable)
    }

    pub fn capabilities(&self, preview: &PreviewId) -> Option<Capabilities> {
        self.session(preview).ok().map(|s| s.capabilities().clone())
    }

    pub async fn snapshot(
        &self,
        preview: &PreviewId,
        params: SnapshotParams,
    ) -> Result<Snapshot, InspectError> {
        let s = self.session(preview)?;
        let snap: Snapshot = s
            .request("inspect/snapshot", &params, SNAPSHOT_TIMEOUT)
            .await?;
        let snap = s.normalizer().snapshot(snap);
        s.set_snapshot(snap.clone());
        Ok(snap)
    }

    pub async fn hit_test(
        &self,
        preview: &PreviewId,
        params: HitTestParams,
    ) -> Result<HitTestResult, InspectError> {
        let s = self.session(preview)?;
        let r: HitTestResult = s
            .request("inspect/hitTest", &params, REQUEST_TIMEOUT)
            .await?;
        Ok(s.normalizer().chain(r))
    }

    pub async fn node_details(
        &self,
        preview: &PreviewId,
        node: NodeId,
    ) -> Result<UiNode, InspectError> {
        let s = self.session(preview)?;
        let n: UiNode = s
            .request("inspect/nodeDetails", &NodeRef { node }, REQUEST_TIMEOUT)
            .await?;
        // Without a location of its own, its owner: the nearest ancestor that has one (as a
        // hit-test chain gives it), before any text search.
        let norm = s.normalizer();
        let mut owner = None;
        if norm.own_location_of(n.clone()).is_none() {
            let mut parent = n.parent.clone();
            for _ in 0..MAX_OWNER_DEPTH {
                let Some(p) = parent else { break };
                let Ok(a) = s
                    .request::<_, UiNode>(
                        "inspect/nodeDetails",
                        &NodeRef { node: p },
                        REQUEST_TIMEOUT,
                    )
                    .await
                else {
                    break;
                };
                parent = a.parent.clone();
                if let Some(l) = norm.own_location_of(a) {
                    owner = Some(l);
                    break;
                }
            }
        }
        Ok(norm.node(n, owner))
    }

    pub async fn highlight(
        &self,
        preview: &PreviewId,
        node: Option<NodeId>,
    ) -> Result<(), InspectError> {
        let s = self.session(preview)?;
        let _: serde_json::Value = s
            .request(
                "inspect/highlight",
                &HighlightParams { node },
                REQUEST_TIMEOUT,
            )
            .await?;
        Ok(())
    }

    pub async fn set_mode(&self, preview: &PreviewId, on: bool) -> Result<(), InspectError> {
        let s = self.session(preview)?;
        let _: serde_json::Value = s
            .request("inspect/setMode", &SetModeParams { on }, REQUEST_TIMEOUT)
            .await?;
        Ok(())
    }

    fn take_token(&self, token: &str) -> Option<Registration> {
        // One-time: a page that reloads gets a new token from the dev server only through a new
        // config, so the token stays valid for reconnects of the same preview (a reload) but
        // belongs to one preview only.
        lock(&self.tokens).get(token).map(|r| Registration {
            preview: r.preview.clone(),
            root: r.root.clone(),
        })
    }

    fn adopt(&self, preview: PreviewId, s: Arc<Session>) {
        if let Some(old) = lock(&self.sessions).insert(preview, s) {
            old.close();
        }
    }

    fn drop_session(&self, preview: &PreviewId, s: &Arc<Session>) {
        let mut sessions = lock(&self.sessions);
        if sessions.get(preview).is_some_and(|x| Arc::ptr_eq(x, s)) {
            sessions.remove(preview);
            let _ = self.events.send(InspectEvent::Disconnected {
                preview: preview.clone(),
            });
        }
    }

    fn emit(&self, e: InspectEvent) {
        let _ = self.events.send(e);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}
