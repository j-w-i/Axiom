use crate::codec::{self, ReadLine};
use crate::router::{Notifier, Out, RequestCtx, Router, ServerInfo, QUEUE_CAPACITY};
use axiom_protocol::envelope::Request;
use axiom_protocol::lifecycle::{
    Empty, HostInfo, HostOs, InitializeParams, InitializeResult, PeerInfo,
};
use axiom_protocol::{lifecycle, PROTOCOL_VERSION};
use axiom_protocol::{ErrorKind, Message, Method, Notification, RequestId, RpcError};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// How long a cancelled handler may take to clean up before it is aborted.
const CANCEL_GRACE: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum ServeError {
    #[error("a line exceeded the 64 MiB limit")]
    LineTooLong,
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),
}

struct InFlight {
    cancel: CancellationToken,
    signal: oneshot::Sender<()>,
}

type InFlightMap = Arc<Mutex<HashMap<RequestId, InFlight>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // A poisoned lock only means another task panicked; the data is still
    // consistent for our simple insert/remove usage.
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// How one connection is served (protocol 0.3, ADR-0015 §6).
#[derive(Debug, Clone)]
pub struct ServeOptions {
    /// `initialize` must carry this `authToken`, or the connection is refused and closed.
    pub token: Option<String>,
    /// Run the router's shutdown hooks when this connection ends. True for the daemon's own
    /// (stdio) client; false for extra connections, whose end must not stop the daemon's work.
    pub run_hooks: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            token: None,
            run_hooks: true,
        }
    }
}

/// Serves one connection until `shutdown` completes or the reader hits EOF.
pub async fn serve<R, W>(
    reader: R,
    writer: W,
    router: Arc<Router>,
    info: ServerInfo,
) -> Result<(), ServeError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    serve_with(reader, writer, router, info, ServeOptions::default()).await
}

/// [`serve`] with options.
pub async fn serve_with<R, W>(
    reader: R,
    writer: W,
    router: Arc<Router>,
    info: ServerInfo,
    opts: ServeOptions,
) -> Result<(), ServeError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, rx) = mpsc::channel::<Out>(QUEUE_CAPACITY);
    let writer_task = tokio::spawn(write_loop(writer, rx));
    let notifier = Notifier { tx: tx.clone() };
    let in_flight: InFlightMap = Arc::default();

    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    let mut initialized = false;
    let mut result: Result<(), ServeError> = Ok(());
    let mut shutdown_requested: Option<RequestId> = None;

    loop {
        let line = match codec::read_line(&mut reader, &mut buf).await {
            Ok(ReadLine::Line) => &buf,
            Ok(ReadLine::Eof) => break,
            Ok(ReadLine::TooLong) => {
                result = Err(ServeError::LineTooLong);
                break;
            }
            Err(e) => {
                result = Err(ServeError::Io(e));
                break;
            }
        };
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let msg = match codec::parse_message(line) {
            Ok(m) => m,
            Err((id, e)) => {
                let _ = tx.send(Out::Line(codec::response_err(id, e))).await;
                continue;
            }
        };
        match msg {
            Message::Response(_) => {}
            Message::Notification(n) => {
                if !initialized {
                    continue;
                }
                if n.method == lifecycle::Cancel::NAME {
                    handle_cancel(n.params, &in_flight);
                }
            }
            Message::Request(req) => {
                let Request {
                    id, method, params, ..
                } = req;
                if method == lifecycle::Shutdown::NAME {
                    if !initialized {
                        send_err(&tx, id, not_initialized()).await;
                        continue;
                    }
                    shutdown_requested = Some(id);
                    break;
                }
                if method == lifecycle::Initialize::NAME {
                    let outcome = if initialized {
                        Err(RpcError::new(
                            ErrorKind::InvalidRequest,
                            "Already initialized.",
                        ))
                    } else {
                        initialize(params, &router, &info)
                    };
                    let outcome = outcome.and_then(|(v, params)| match &opts.token {
                        Some(t) if params.auth_token.as_deref() != Some(t.as_str()) => {
                            Err(RpcError::new(
                                ErrorKind::PermissionDenied,
                                "This connection needs the daemon's token.",
                            ))
                        }
                        _ => Ok((v, params)),
                    });
                    if let (Err(_), Some(_)) = (&outcome, &opts.token) {
                        // Refused: answer once and close.
                        if let Err(e) = outcome {
                            send_err(&tx, id, e).await;
                        }
                        break;
                    }
                    match outcome {
                        Ok((v, params)) => {
                            initialized = true;
                            for hook in &router.init_hooks {
                                hook(&params, notifier.clone());
                            }
                            let _ = tx.send(Out::Line(codec::response_ok(id, v))).await;
                        }
                        Err(e) => send_err(&tx, id, e).await,
                    }
                    continue;
                }
                if !initialized {
                    send_err(&tx, id, not_initialized()).await;
                    continue;
                }
                dispatch(id, &method, params, &router, &in_flight, &notifier, &tx).await;
            }
        }
    }

    // Peer gone, fatal error, or shutdown: stop all in-flight work, run hooks.
    cancel_all(&in_flight);
    if opts.run_hooks {
        for hook in &router.hooks {
            hook().await;
        }
    }
    if let Some(id) = shutdown_requested {
        let v = serde_json::to_value(Empty {}).unwrap_or(Value::Null);
        let _ = tx.send(Out::Line(codec::response_ok(id, v))).await;
    }
    let _ = tx.send(Out::Stop).await;
    drop(tx);
    let _ = writer_task.await;
    for hook in &router.disconnect_hooks {
        hook(notifier.clone()).await;
    }
    result
}

fn not_initialized() -> RpcError {
    RpcError::new(
        ErrorKind::NotInitialized,
        "The connection is not initialized.",
    )
}

async fn send_err(tx: &mpsc::Sender<Out>, id: RequestId, e: RpcError) {
    let _ = tx.send(Out::Line(codec::response_err(Some(id), e))).await;
}

async fn write_loop<W: AsyncWrite + Unpin>(mut writer: W, mut rx: mpsc::Receiver<Out>) {
    while let Some(item) = rx.recv().await {
        match item {
            Out::Line(s) => {
                if writer.write_all(s.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                    break;
                }
            }
            Out::Stop => break,
        }
    }
    let _ = writer.shutdown().await;
}

fn parse_version(s: &str) -> Option<(u32, u32)> {
    let (a, b) = s.split_once('.')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}

fn initialize(
    params: Option<Value>,
    router: &Router,
    info: &ServerInfo,
) -> Result<(Value, InitializeParams), RpcError> {
    let invalid = |reason: String| {
        RpcError::new(ErrorKind::InvalidParams, "Invalid parameters.")
            .with_details(serde_json::json!({ "reason": reason }))
    };
    let p: InitializeParams =
        serde_json::from_value(params_or_empty(params)).map_err(|e| invalid(e.to_string()))?;
    let (cmaj, cmin) = parse_version(&p.protocol_version)
        .ok_or_else(|| invalid("protocolVersion must be MAJOR.MINOR".into()))?;
    let (smaj, smin) = parse_version(PROTOCOL_VERSION).unwrap_or((0, 1));
    if cmaj != smaj {
        return Err(RpcError::new(
            ErrorKind::Unsupported,
            "The protocol version is not supported.",
        )
        .with_details(serde_json::json!({ "supported": PROTOCOL_VERSION })));
    }
    let os = match std::env::consts::OS {
        "macos" => HostOs::Macos,
        "windows" => HostOs::Windows,
        _ => HostOs::Linux,
    };
    let result = InitializeResult {
        protocol_version: format!("{smaj}.{}", cmin.min(smin)),
        server: PeerInfo {
            name: info.name.clone(),
            version: info.version.clone(),
        },
        methods: router.method_names(),
        host: HostInfo {
            os,
            arch: std::env::consts::ARCH.to_string(),
            path_separator: std::path::MAIN_SEPARATOR.to_string(),
        },
    };
    serde_json::to_value(result)
        .map(|v| (v, p))
        .map_err(|_| RpcError::new(ErrorKind::Internal, "Internal error."))
}

fn params_or_empty(params: Option<Value>) -> Value {
    match params {
        None | Some(Value::Null) => Value::Object(Default::default()),
        Some(v) => v,
    }
}

fn handle_cancel(params: Option<Value>, in_flight: &InFlightMap) {
    let Some(id) = params
        .and_then(|v| serde_json::from_value::<lifecycle::CancelParams>(v).ok())
        .map(|p| p.id)
    else {
        return;
    };
    // Removing the entry is the moment the cancel is "processed": a response
    // queued before this point wins, otherwise the supervisor answers with
    // `cancelled`.
    if let Some(entry) = lock(in_flight).remove(&id) {
        entry.cancel.cancel();
        let _ = entry.signal.send(());
    }
}

fn cancel_all(in_flight: &InFlightMap) {
    let entries: Vec<InFlight> = lock(in_flight).drain().map(|(_, v)| v).collect();
    for e in entries {
        e.cancel.cancel();
        let _ = e.signal.send(());
    }
}

async fn dispatch(
    id: RequestId,
    method: &str,
    params: Option<Value>,
    router: &Arc<Router>,
    in_flight: &InFlightMap,
    notifier: &Notifier,
    tx: &mpsc::Sender<Out>,
) {
    let Some(handler) = router.handlers.get(method).cloned() else {
        send_err(
            tx,
            id,
            RpcError::new(ErrorKind::MethodNotFound, "Unknown method."),
        )
        .await;
        return;
    };
    let cancel = CancellationToken::new();
    let (signal, cancelled_rx) = oneshot::channel();
    let duplicate = {
        let mut map = lock(in_flight);
        if map.contains_key(&id) {
            true
        } else {
            map.insert(
                id.clone(),
                InFlight {
                    cancel: cancel.clone(),
                    signal,
                },
            );
            false
        }
    };
    if duplicate {
        send_err(
            tx,
            id,
            RpcError::new(ErrorKind::InvalidRequest, "Request id already in flight."),
        )
        .await;
        return;
    }
    let ctx = RequestCtx::new(cancel, notifier.clone());
    let after = ctx.after.clone();
    let discarded = ctx.discarded.clone();
    // The handler succeeded but its answer was replaced by `cancelled`: undo (AUD-098).
    let run_discarded = move || {
        for f in std::mem::take(&mut *lock(&discarded)) {
            tokio::spawn(f);
        }
    };
    let fut = handler(params_or_empty(params), ctx);
    let in_flight = in_flight.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        let mut handle = tokio::spawn(fut);
        let cancelled_error = || RpcError::new(ErrorKind::Cancelled, "The request was cancelled.");
        tokio::select! {
            joined = &mut handle => {
                let outcome = match joined {
                    Ok(r) => r,
                    Err(e) => {
                        if e.is_panic() {
                            tracing::error!("handler for request {id:?} panicked: {e}");
                        }
                        Err(RpcError::new(ErrorKind::Internal, "Internal error."))
                    }
                };
                // Reserve first so that "remove from map" and "queue the
                // response" are not separated by an await point.
                let Ok(permit) = tx.reserve().await else { return };
                let still_ours = lock(&in_flight).remove(&id).is_some();
                if !still_ours {
                    permit.send(Out::Line(codec::response_err(Some(id), cancelled_error())));
                    if outcome.is_ok() {
                        run_discarded();
                    }
                    return;
                }
                match outcome {
                    Ok(v) => {
                        permit.send(Out::Line(codec::response_ok(id, v)));
                        let futs = std::mem::take(&mut *lock(&after));
                        for f in futs {
                            tokio::spawn(f);
                        }
                    }
                    Err(e) => permit.send(Out::Line(codec::response_err(Some(id), e))),
                }
            }
            _ = cancelled_rx => {
                match tokio::time::timeout(CANCEL_GRACE, &mut handle).await {
                    Ok(Ok(Ok(_))) => run_discarded(),
                    Ok(_) => {}
                    Err(_) => handle.abort(),
                }
                let _ = tx
                    .send(Out::Line(codec::response_err(Some(id), cancelled_error())))
                    .await;
            }
        }
    });
}
