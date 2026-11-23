//! One connected page: the WebSocket, the JSON-RPC client side (the daemon asks, the adapter
//! answers), and what the page told us (capabilities, the latest snapshot, mutations, errors).

use crate::normalize::Normalizer;
use crate::{lock, Inspect, InspectError, InspectEvent, HELLO_TIMEOUT};
use axiom_protocol::inspection::{
    AdapterHoveredParams, Capabilities, DidMutateParams, HelloParams, HitTestResult,
    RuntimeErrorParams, Snapshot,
};
use axiom_protocol::preview::PreviewId;
use axiom_protocol::PROTOCOL_VERSION;
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot, Notify};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::Message;

/// The largest message a page may send (a 5000-node snapshot is well under this).
const MAX_MESSAGE: usize = 16 * 1024 * 1024;

type Pending = Mutex<HashMap<u64, oneshot::Sender<Result<Value, InspectError>>>>;

pub struct Session {
    pub preview: PreviewId,
    out: mpsc::UnboundedSender<Message>,
    pending: Pending,
    next: AtomicU64,
    open: AtomicBool,
    capabilities: Mutex<Option<Capabilities>>,
    normalizer: Arc<Normalizer>,
    snapshot: Mutex<Option<Snapshot>>,
    /// When the page last changed (for quiescence), and why.
    last_mutation: Mutex<Option<Instant>>,
    mutated: Notify,
    /// Runtime errors with the adapter's time (ms since the epoch).
    errors: Mutex<Vec<(u64, String)>>,
    /// Hot reloads seen (ms since the epoch, the daemon's clock).
    reloads: Mutex<Vec<u64>>,
    connected_ms: u64,
    /// The last `ui/verify` (ms since the epoch): the next one counts errors from here.
    last_verify: AtomicU64,
    degraded: AtomicBool,
}

impl Session {
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::SeqCst)
    }

    /// The adapter stopped answering in time: the Inspector says its data may be stale.
    pub fn is_degraded(&self) -> bool {
        self.degraded.load(Ordering::SeqCst)
    }

    pub fn close(&self) {
        self.open.store(false, Ordering::SeqCst);
        let _ = self.out.send(Message::Close(None));
        for (_, tx) in lock(&self.pending).drain() {
            let _ = tx.send(Err(InspectError::AdapterUnavailable));
        }
    }

    pub fn capabilities(&self) -> Capabilities {
        lock(&self.capabilities)
            .clone()
            .expect("a session is adopted only after hello")
    }

    pub fn normalizer(&self) -> &Arc<Normalizer> {
        &self.normalizer
    }

    pub fn latest_snapshot(&self) -> Option<Snapshot> {
        lock(&self.snapshot).clone()
    }

    pub(crate) fn set_snapshot(&self, s: Snapshot) {
        *lock(&self.snapshot) = Some(s);
    }

    pub fn connected_ms(&self) -> u64 {
        self.connected_ms
    }

    /// The previous `ui/verify`, or the connection.
    pub fn last_verify_ms(&self) -> u64 {
        match self.last_verify.load(Ordering::SeqCst) {
            0 => self.connected_ms,
            t => t,
        }
    }

    pub(crate) fn mark_verified(&self, ms: u64) {
        self.last_verify.store(ms, Ordering::SeqCst);
    }

    /// Runtime errors reported at or after [since_ms].
    pub fn errors_since(&self, since_ms: u64) -> Vec<String> {
        lock(&self.errors)
            .iter()
            .filter(|(t, _)| *t >= since_ms)
            .map(|(_, m)| m.clone())
            .collect()
    }

    /// Hot reloads seen at or after [since_ms].
    pub fn reloads_since(&self, since_ms: u64) -> usize {
        lock(&self.reloads)
            .iter()
            .filter(|t| **t >= since_ms)
            .count()
    }

    /// Waits, at most [limit], for a hot reload at or after [since_ms]. True when one came.
    pub async fn hot_reload_since(&self, since_ms: u64, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        loop {
            // Registered before the check, so a reload between the two is not missed.
            let notified = self.mutated.notified();
            if self.reloads_since(since_ms) > 0 {
                return true;
            }
            let now = Instant::now();
            if now >= end {
                return false;
            }
            let _ = tokio::time::timeout(end - now, notified).await;
        }
    }

    /// Waits until the page has not changed for [quiet], at most [limit]. True when it settled.
    pub async fn quiescence(&self, quiet: Duration, limit: Duration) -> bool {
        let end = Instant::now() + limit;
        loop {
            let last = *lock(&self.last_mutation);
            let now = Instant::now();
            let since = last
                .map(|t| now.saturating_duration_since(t))
                .unwrap_or(quiet);
            if since >= quiet {
                return true;
            }
            if now >= end {
                return false;
            }
            let wait = (quiet - since).min(end - now);
            let _ = tokio::time::timeout(wait, self.mutated.notified()).await;
        }
    }

    /// Sends a request to the page's adapter and waits for its answer.
    pub async fn request<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
        timeout: Duration,
    ) -> Result<R, InspectError> {
        if !self.is_open() {
            return Err(InspectError::AdapterUnavailable);
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if self.out.send(Message::text(msg.to_string())).is_err() {
            lock(&self.pending).remove(&id);
            return Err(InspectError::AdapterUnavailable);
        }
        let v = match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(r)) => r?,
            Ok(Err(_)) => return Err(InspectError::AdapterUnavailable),
            Err(_) => {
                lock(&self.pending).remove(&id);
                self.degraded.store(true, Ordering::SeqCst);
                let _ = self.out.send(Message::text(
                    json!({"jsonrpc": "2.0", "method": "$/cancel", "params": {"id": id}})
                        .to_string(),
                ));
                return Err(InspectError::Timeout(timeout.as_secs()));
            }
        };
        self.degraded.store(false, Ordering::SeqCst);
        serde_json::from_value(v)
            .map_err(|e| InspectError::Adapter(format!("an answer of the wrong shape: {e}")))
    }

    fn answer(&self, id: u64, r: Result<Value, InspectError>) {
        if let Some(tx) = lock(&self.pending).remove(&id) {
            let _ = tx.send(r);
        }
    }

    fn mutation(&self) {
        *lock(&self.last_mutation) = Some(Instant::now());
        self.mutated.notify_waiters();
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A page may connect from the preview's dev server only: no `Origin` (a non-browser client)
/// or an `http(s)` origin on a loopback host, where the generated config serves the preview.
/// A website the person has open elsewhere cannot connect (AUD-043).
pub fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(o) = origin else { return true };
    let Some(rest) = o
        .strip_prefix("http://")
        .or_else(|| o.strip_prefix("https://"))
    else {
        return false;
    };
    let host = if let Some(v6) = rest.strip_prefix('[') {
        match v6.split_once(']') {
            Some((h, tail)) if tail.is_empty() || tail.starts_with(':') => h,
            _ => return false,
        }
    } else {
        rest.split(':').next().unwrap_or("")
    };
    let port_ok = |r: &str| {
        r.rsplit_once(':')
            .is_none_or(|(_, p)| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1") && !rest.contains('/') && port_ok(rest)
}

/// How long a new connection waits for the preview's current page to go (a reload closes it)
/// before it is refused (AUD-043: a second connection no longer replaces a live one).
const HANDOVER: Duration = Duration::from_secs(3);

/// One connection: the token in the URL decides the preview; then hello; then the message loop.
pub(crate) async fn accept(inspect: Arc<Inspect>, stream: TcpStream) {
    let mut token = None;
    // The library's callback type returns its error response by value.
    #[allow(clippy::result_large_err)]
    let check = |req: &Request, resp: Response| -> Result<Response, ErrorResponse> {
        let origin = req
            .headers()
            .get("origin")
            .map(|v| v.to_str().unwrap_or("invalid"));
        if !origin_allowed(origin) {
            let mut e = ErrorResponse::new(Some("origin not allowed".into()));
            *e.status_mut() = StatusCode::FORBIDDEN;
            return Err(e);
        }
        let t = req.uri().query().and_then(|q| {
            q.split('&')
                .find_map(|kv| kv.strip_prefix("token=").map(str::to_string))
        });
        match t
            .as_deref()
            .and_then(|t| inspect.take_token(t).map(|r| (t, r)))
        {
            Some((t, _)) => {
                token = Some(t.to_string());
                Ok(resp)
            }
            None => {
                let mut e = ErrorResponse::new(Some("unknown token".into()));
                *e.status_mut() = StatusCode::UNAUTHORIZED;
                Err(e)
            }
        }
    };
    let config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE))
        .max_frame_size(Some(MAX_MESSAGE));
    let Ok(ws) = tokio_tungstenite::accept_hdr_async_with_config(stream, check, Some(config)).await
    else {
        return;
    };
    let Some(reg) = token.as_deref().and_then(|t| inspect.take_token(t)) else {
        return;
    };
    let (mut sink, mut source) = ws.split();
    let (out, mut out_rx) = mpsc::unbounded_channel::<Message>();
    tokio::spawn(async move {
        while let Some(m) = out_rx.recv().await {
            let close = matches!(m, Message::Close(_));
            if sink.send(m).await.is_err() || close {
                break;
            }
        }
        let _ = sink.close().await;
    });
    let session = Arc::new(Session {
        preview: reg.preview.clone(),
        out,
        pending: Mutex::new(HashMap::new()),
        next: AtomicU64::new(1),
        open: AtomicBool::new(true),
        capabilities: Mutex::new(None),
        normalizer: Arc::new(Normalizer::new(reg.root)),
        snapshot: Mutex::new(None),
        last_mutation: Mutex::new(None),
        mutated: Notify::new(),
        errors: Mutex::new(Vec::new()),
        reloads: Mutex::new(Vec::new()),
        connected_ms: now_ms(),
        last_verify: AtomicU64::new(0),
        degraded: AtomicBool::new(false),
    });

    // Messages are read concurrently with hello (its answer arrives through the loop).
    let reader = {
        let session = session.clone();
        let inspect = inspect.clone();
        tokio::spawn(async move {
            while let Some(Ok(m)) = source.next().await {
                let text = match m {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                handle(&inspect, &session, v);
            }
            session.close();
            inspect.drop_session(&session.preview, &session);
        })
    };

    let hello = HelloParams {
        protocol_version: PROTOCOL_VERSION.into(),
        token: token.unwrap_or_default(),
    };
    match session
        .request::<_, Capabilities>("inspect/hello", &hello, HELLO_TIMEOUT)
        .await
    {
        Ok(caps) => {
            *lock(&session.capabilities) = Some(caps.clone());
            if !inspect
                .adopt(reg.preview.clone(), session.clone(), HANDOVER)
                .await
            {
                tracing::warn!(
                    "a second page connected to a preview whose page is still connected; refused"
                );
                session.close();
                reader.abort();
                return;
            }
            inspect.emit(InspectEvent::Connected {
                preview: reg.preview,
                capabilities: caps,
            });
        }
        Err(e) => {
            tracing::warn!("a preview page's adapter failed hello: {e}");
            session.close();
            reader.abort();
        }
    }
}

fn handle(inspect: &Arc<Inspect>, s: &Session, v: Value) {
    // An answer to one of our requests.
    if v.get("method").is_none() {
        let Some(id) = v.get("id").and_then(Value::as_u64) else {
            return;
        };
        let r = match v.get("error") {
            Some(e) => Err(InspectError::Adapter(
                e.get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("error")
                    .to_string(),
            )),
            None => Ok(v.get("result").cloned().unwrap_or(Value::Null)),
        };
        s.answer(id, r);
        return;
    }
    let params = v.get("params").cloned().unwrap_or(Value::Null);
    let preview = s.preview.clone();
    match v["method"].as_str().unwrap_or_default() {
        "inspect/didMutate" => {
            let Ok(p) = serde_json::from_value::<DidMutateParams>(params) else {
                return;
            };
            s.mutation();
            // Files may have changed: text-search results are stale.
            s.normalizer.clear_cache();
            if p.reason == axiom_protocol::inspection::MutationReason::HotReload {
                lock(&s.reloads).push(now_ms());
            }
            inspect.emit(InspectEvent::Mutated {
                preview,
                reason: p.reason,
            });
        }
        "inspect/hovered" => {
            let Ok(p) = serde_json::from_value::<AdapterHoveredParams>(params) else {
                return;
            };
            // Hover comes at pointer-move rate: its own sources and owners only, never the
            // workspace text search (AUD-042); a click (selected) searches.
            let node = p.node.map(|n| s.normalizer.node_without_search(n, None));
            inspect.emit(InspectEvent::Hovered { preview, node });
        }
        "inspect/selected" => {
            let Ok(r) = serde_json::from_value::<HitTestResult>(params) else {
                return;
            };
            // The text search reads files: on the blocking pool, not this reader's worker.
            let norm = s.normalizer.clone();
            let inspect = inspect.clone();
            tokio::spawn(async move {
                if let Ok(r) = tokio::task::spawn_blocking(move || norm.chain(r)).await {
                    inspect.emit(InspectEvent::Selected {
                        preview,
                        chain: r.chain,
                    });
                }
            });
        }
        "inspect/runtimeError" => {
            let Ok(p) = serde_json::from_value::<RuntimeErrorParams>(params) else {
                return;
            };
            let mut errors = lock(&s.errors);
            if errors.len() < 200 {
                let msg: String = p.message.chars().take(2000).collect();
                // The daemon's clock: the page's may differ.
                errors.push((now_ms(), msg));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::origin_allowed;

    #[test]
    fn only_loopback_origins_may_connect() {
        for ok in [
            None,
            Some("http://127.0.0.1:5173"),
            Some("http://localhost:3000"),
            Some("https://localhost"),
            Some("http://[::1]:8080"),
        ] {
            assert!(origin_allowed(ok), "{ok:?}");
        }
        for bad in [
            "https://evil.example",
            "http://127.0.0.1.evil.example",
            "http://localhost.evil.example:80",
            "http://evil.example/127.0.0.1",
            "null",
            "file://",
            "http://127.0.0.1:80/x",
            "http://[::1]evil",
            "http://localhost:",
        ] {
            assert!(!origin_allowed(Some(bad)), "{bad}");
        }
    }
}
