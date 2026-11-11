use crate::codec;
use axiom_protocol::{ErrorKind, Method, Notification, RpcError};
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Capacity of the outgoing queue, in messages.
pub(crate) const QUEUE_CAPACITY: usize = 1024;

/// Name and version reported in the `initialize` result.
#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// Returned by [`Notifier::notify`] once the connection is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("connection closed")]
pub struct Closed;

/// Item on the single outgoing queue feeding the writer task.
pub(crate) enum Out {
    Line(String),
    /// Flush everything queued before this and stop writing.
    Stop,
}

pub(crate) type HandlerFn =
    Arc<dyn Fn(Value, RequestCtx) -> BoxFuture<'static, Result<Value, RpcError>> + Send + Sync>;
pub(crate) type HookFn = Arc<dyn Fn() -> BoxFuture<'static, ()> + Send + Sync>;

/// Method registry. Build once, share via `Arc` across connections.
#[derive(Default)]
pub struct Router {
    pub(crate) handlers: HashMap<&'static str, HandlerFn>,
    pub(crate) hooks: Vec<HookFn>,
}

impl Router {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a handler for method `M`.
    ///
    /// # Panics
    /// If `M::NAME` is already registered or is `initialize`/`shutdown`
    /// (those are built in).
    pub fn method<M, F, Fut>(&mut self, handler: F) -> &mut Self
    where
        M: Method,
        F: Fn(M::Params, RequestCtx) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<M::Result, RpcError>> + Send + 'static,
    {
        assert!(
            M::NAME != "initialize" && M::NAME != "shutdown",
            "`{}` is built in and cannot be registered",
            M::NAME
        );
        assert!(
            !self.handlers.contains_key(M::NAME),
            "method `{}` registered twice",
            M::NAME
        );
        let wrapped: HandlerFn = Arc::new(move |params: Value, ctx: RequestCtx| {
            match serde_json::from_value::<M::Params>(params) {
                Ok(p) => {
                    let fut = handler(p, ctx);
                    async move {
                        let r = fut.await?;
                        serde_json::to_value(r).map_err(|e| {
                            tracing::error!("failed to serialise result of {}: {e}", M::NAME);
                            RpcError::new(ErrorKind::Internal, "Internal error.")
                        })
                    }
                    .boxed()
                }
                Err(e) => std::future::ready(Err(RpcError::new(
                    ErrorKind::InvalidParams,
                    "Invalid parameters.",
                )
                .with_details(serde_json::json!({ "reason": e.to_string() }))))
                .boxed(),
            }
        });
        self.handlers.insert(M::NAME, wrapped);
        self
    }

    /// Called once when `shutdown` is received or the peer disconnects,
    /// before the shutdown response is sent.
    pub fn on_shutdown<F, Fut>(&mut self, hook: F) -> &mut Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.hooks.push(Arc::new(move || hook().boxed()));
        self
    }

    /// Registered methods plus `initialize` and `shutdown`, sorted.
    pub fn method_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.handlers.keys().map(|s| s.to_string()).collect();
        v.push("initialize".into());
        v.push("shutdown".into());
        v.sort();
        v
    }
}

/// Per-request context handed to handlers.
#[derive(Clone)]
pub struct RequestCtx {
    /// Cancelled when the client sends `$/cancel` or disconnects.
    pub cancel: CancellationToken,
    pub notifier: Notifier,
    pub(crate) after: Arc<Mutex<Vec<BoxFuture<'static, ()>>>>,
}

impl RequestCtx {
    pub(crate) fn new(cancel: CancellationToken, notifier: Notifier) -> Self {
        Self {
            cancel,
            notifier,
            after: Arc::default(),
        }
    }

    /// Registers a future that the router spawns only AFTER this request's
    /// response has been placed on the outgoing queue (and only if the
    /// handler returned `Ok` and was not cancelled). The queue is FIFO, so any
    /// notification the future sends is written after the response.
    pub fn after_response<Fut>(&self, fut: Fut)
    where
        Fut: Future<Output = ()> + Send + 'static,
    {
        if let Ok(mut v) = self.after.lock() {
            v.push(fut.boxed());
        }
    }
}

/// Sends notifications on the connection's outgoing queue.
#[derive(Clone)]
pub struct Notifier {
    pub(crate) tx: mpsc::Sender<Out>,
}

impl Notifier {
    /// Queues a notification. Waits while the queue (1024 messages) is full:
    /// this is the backpressure path for task output.
    pub async fn notify<N: Notification>(&self, params: N::Params) -> Result<(), Closed> {
        let value = serde_json::to_value(params).map_err(|e| {
            tracing::error!("failed to serialise {}: {e}", N::NAME);
            Closed
        })?;
        self.tx
            .send(Out::Line(codec::notification_line(N::NAME, value)))
            .await
            .map_err(|_| Closed)
    }
}
