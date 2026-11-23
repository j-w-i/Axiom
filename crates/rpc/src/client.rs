use crate::codec::{self, ReadLine};
use axiom_protocol::envelope::{JsonRpcVersion, Request};
use axiom_protocol::{ErrorKind, Message, Method, Notification, RequestId, RpcError};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{broadcast, mpsc, oneshot};

type Pending = Arc<Mutex<HashMap<RequestId, oneshot::Sender<Result<Value, RpcError>>>>>;

struct Inner {
    tx: mpsc::Sender<String>,
    pending: Pending,
    next_id: AtomicI64,
    notifications: broadcast::Sender<(String, Value)>,
    orphans: broadcast::Sender<RpcError>,
    /// Set when the reader ends: later requests fail at once instead of waiting forever.
    closed: Arc<std::sync::atomic::AtomicBool>,
}

/// Typed client over the NDJSON framing. Cheap to clone.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

fn closed() -> RpcError {
    RpcError::new(ErrorKind::Io, "The connection is closed.")
}

impl Client {
    /// Spawns the read and write loops on the current Tokio runtime.
    pub fn new<R, W>(reader: R, writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<String>(1024);
        let pending: Pending = Arc::default();
        let (notifications, _) = broadcast::channel(4096);
        let (orphans, _) = broadcast::channel(64);
        let closed_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_closed = closed_flag.clone();
        let inner = Arc::new(Inner {
            tx,
            pending: pending.clone(),
            next_id: AtomicI64::new(1),
            notifications: notifications.clone(),
            orphans: orphans.clone(),
            closed: closed_flag,
        });

        tokio::spawn(async move {
            let mut writer = writer;
            while let Some(line) = rx.recv().await {
                if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err()
                {
                    break;
                }
            }
            let _ = writer.shutdown().await;
        });

        tokio::spawn(async move {
            let mut reader = BufReader::new(reader);
            let mut buf = Vec::new();
            while let Ok(ReadLine::Line) = codec::read_line(&mut reader, &mut buf).await {
                if buf.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                let Ok(msg) = codec::parse_message(&buf) else {
                    continue;
                };
                match msg {
                    Message::Notification(n) => {
                        let _ = notifications.send((n.method, n.params.unwrap_or(Value::Null)));
                    }
                    Message::Response(r) => {
                        let outcome = match (r.result, r.error) {
                            (_, Some(e)) => Err(e),
                            (Some(v), None) => Ok(v),
                            (None, None) => Ok(Value::Null),
                        };
                        match r.id {
                            Some(id) => {
                                let sender = pending
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner())
                                    .remove(&id);
                                if let Some(s) = sender {
                                    let _ = s.send(outcome);
                                }
                            }
                            None => {
                                if let Err(e) = outcome {
                                    let _ = orphans.send(e);
                                }
                            }
                        }
                    }
                    Message::Request(_) => {}
                }
            }
            // Connection ended: fail every outstanding request (and every later one).
            reader_closed.store(true, Ordering::SeqCst);
            let drained: Vec<_> = pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .drain()
                .collect();
            for (_, s) in drained {
                let _ = s.send(Err(closed()));
            }
        });

        Self { inner }
    }

    pub async fn request<M: Method>(&self, params: M::Params) -> Result<M::Result, RpcError> {
        self.request_with_id::<M>(params).1.await
    }

    pub async fn request_raw(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        let (_, fut) = self.send_request(method, Some(params));
        fut.await
    }

    /// Registers the request immediately and returns its id, so a test can
    /// `$/cancel` it; the request is written when the future is first polled.
    pub fn request_with_id<M: Method>(
        &self,
        params: M::Params,
    ) -> (RequestId, impl Future<Output = Result<M::Result, RpcError>>) {
        let value = serde_json::to_value(params);
        let (id, fut) = match value {
            Ok(v) => {
                let (id, fut) = self.send_request(M::NAME, Some(v));
                (id, Some(fut))
            }
            Err(_) => (RequestId::Number(0), None),
        };
        (id, async move {
            let fut = fut.ok_or_else(|| RpcError::new(ErrorKind::InvalidParams, "Bad params."))?;
            let v = fut.await?;
            serde_json::from_value::<M::Result>(v).map_err(|e| {
                RpcError::new(ErrorKind::Internal, "Malformed result.")
                    .with_details(serde_json::json!({ "reason": e.to_string() }))
            })
        })
    }

    fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> (RequestId, impl Future<Output = Result<Value, RpcError>>) {
        let id = RequestId::Number(self.inner.next_id.fetch_add(1, Ordering::Relaxed));
        let (otx, orx) = oneshot::channel();
        self.inner
            .pending
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(id.clone(), otx);
        let already_closed = self.inner.closed.load(Ordering::SeqCst);
        if already_closed {
            self.inner
                .pending
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&id);
        }
        let line = codec::encode(&Request {
            jsonrpc: JsonRpcVersion::V2,
            id: id.clone(),
            method: method.to_string(),
            params,
        });
        let inner = self.inner.clone();
        let too_long = line.len() - 1 > codec::MAX_LINE;
        (id.clone(), async move {
            if too_long {
                // The peer would close the connection on it (AUD-100): refuse here.
                inner
                    .pending
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(&id);
                return Err(RpcError::new(
                    ErrorKind::TooLarge,
                    "The request is larger than the protocol's 64 MiB line limit.",
                ));
            }
            if already_closed || inner.tx.send(line).await.is_err() {
                return Err(closed());
            }
            orx.await.unwrap_or_else(|_| Err(closed()))
        })
    }

    pub async fn notify<N: Notification>(&self, params: N::Params) {
        if let Some(line) = serde_json::to_value(params)
            .ok()
            .and_then(|v| codec::notification_line(N::NAME, v))
        {
            let _ = self.inner.tx.send(line).await;
        }
    }

    /// Writes `raw` followed by `\n`, unchecked. For malformed-input tests.
    pub async fn send_line(&self, raw: &str) {
        let _ = self.inner.tx.send(format!("{raw}\n")).await;
    }

    /// Stream of server notifications as (method, params).
    pub fn notifications(&self) -> broadcast::Receiver<(String, Value)> {
        self.inner.notifications.subscribe()
    }

    /// Errors the server sent with `id: null` (parse / invalid request).
    pub fn orphan_errors(&self) -> broadcast::Receiver<RpcError> {
        self.inner.orphans.subscribe()
    }
}
