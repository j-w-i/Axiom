use axiom_exec::Executor;
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId};
use axiom_store::{Store, StoreError};
use axiom_workspace::Workspaces;
use std::collections::HashMap;
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
}

impl App {
    pub fn new(store: Store) -> Arc<App> {
        Arc::new(App {
            store: Arc::new(store),
            workspaces: Workspaces::new(),
            executor: Executor::new(),
            watchers: Mutex::default(),
        })
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
