//! `ui/*` (protocol 0.5, ADR-0019 §3; M5-INSPECT-001): the running UI of a preview, through
//! `axiom-inspect`'s session with the page's adapter, normalised; and the relay of the page's
//! selection, hover and mutations to the workspace's clients.

use crate::app::App;
use axiom_inspect::{InspectError, InspectEvent};
use axiom_protocol::inspection::{self, HitTestResult, Snapshot, UiNode};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::preview::{PreviewId, PreviewInfo, PreviewState, State};
use axiom_protocol::ui::{
    DidMutate, DidMutateParams, HighlightParams, HitTestParams, Hovered, HoveredParams,
    InspectModeParams, NodeParams, Selected, SelectedParams, SnapshotParams,
};
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId};
use std::sync::Arc;

pub fn map_err(e: InspectError) -> RpcError {
    let kind = match &e {
        InspectError::NotFound | InspectError::NotAtPoint(_) => ErrorKind::NotFound,
        InspectError::AdapterUnavailable => ErrorKind::AdapterUnavailable,
        InspectError::Timeout(_) => ErrorKind::Timeout,
        InspectError::Adapter(_) => ErrorKind::AdapterUnavailable,
    };
    let message = match &e {
        InspectError::AdapterUnavailable => {
            "No page is connected to the preview: open its URL in a browser.".to_string()
        }
        other => {
            let s = other.to_string();
            let mut c = s.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str() + ".")
                .unwrap_or_default()
        }
    };
    RpcError::new(kind, message)
}

/// The preview's inspection service, or `not_found` for an unknown preview.
fn service(app: &App, preview: &PreviewId) -> Result<Arc<axiom_inspect::Inspect>, RpcError> {
    if !app.previews.contains(preview) {
        return Err(RpcError::new(
            ErrorKind::NotFound,
            "There is no such preview.",
        ));
    }
    app.previews
        .inspect()
        .ok_or_else(|| map_err(InspectError::AdapterUnavailable))
}

pub async fn inspect_mode(app: Arc<App>, p: InspectModeParams) -> Result<Empty, RpcError> {
    service(&app, &p.preview_id)?
        .set_mode(&p.preview_id, p.on)
        .await
        .map_err(map_err)?;
    Ok(Empty {})
}

pub async fn snapshot(app: Arc<App>, p: SnapshotParams) -> Result<Snapshot, RpcError> {
    service(&app, &p.preview_id)?
        .snapshot(
            &p.preview_id,
            inspection::SnapshotParams {
                root: None,
                max_nodes: p.max_nodes,
                include_invisible: p.include_invisible,
            },
        )
        .await
        .map_err(map_err)
}

pub async fn hit_test(app: Arc<App>, p: HitTestParams) -> Result<HitTestResult, RpcError> {
    let r = service(&app, &p.preview_id)?
        .hit_test(
            &p.preview_id,
            inspection::HitTestParams {
                surface: p.surface,
                x: p.x,
                y: p.y,
            },
        )
        .await
        .map_err(map_err)?;
    if r.chain.is_empty() {
        return Err(RpcError::new(
            ErrorKind::NotFound,
            "Nothing is at that point.",
        ));
    }
    Ok(r)
}

pub async fn node_details(app: Arc<App>, p: NodeParams) -> Result<UiNode, RpcError> {
    service(&app, &p.preview_id)?
        .node_details(&p.preview_id, p.node)
        .await
        .map_err(map_err)
}

pub async fn highlight(app: Arc<App>, p: HighlightParams) -> Result<Empty, RpcError> {
    service(&app, &p.preview_id)?
        .highlight(&p.preview_id, p.node)
        .await
        .map_err(map_err)?;
    Ok(Empty {})
}

/// Relays the pages' events to the clients of each preview's workspace, for the life of the
/// inspection service.
pub fn spawn_forwarder(app: &Arc<App>, inspect: &axiom_inspect::Inspect) {
    let mut rx = inspect.subscribe();
    let weak = Arc::downgrade(app);
    tokio::spawn(async move {
        loop {
            let ev = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            };
            let Some(app) = weak.upgrade() else { return };
            match ev {
                InspectEvent::Connected {
                    preview,
                    capabilities,
                } => {
                    if let Some(info) = app
                        .previews
                        .update(&preview, |i| i.adapter = Some(capabilities))
                    {
                        notify_state(&app, info).await;
                    }
                }
                InspectEvent::Disconnected { preview } => {
                    if let Some(info) = app.previews.update(&preview, |i| i.adapter = None) {
                        notify_state(&app, info).await;
                    }
                }
                InspectEvent::Hovered { preview, node } => {
                    let Some(ws) = app.previews.workspace(&preview) else {
                        continue;
                    };
                    let n = HoveredParams {
                        preview_id: preview,
                        node,
                    };
                    for c in app.clients_of(&ws) {
                        let _ = c.notify::<Hovered>(n.clone()).await;
                    }
                }
                InspectEvent::Selected { preview, chain } => {
                    let Some(ws) = app.previews.workspace(&preview) else {
                        continue;
                    };
                    let n = SelectedParams {
                        preview_id: preview,
                        chain,
                    };
                    for c in app.clients_of(&ws) {
                        let _ = c.notify::<Selected>(n.clone()).await;
                    }
                }
                InspectEvent::Mutated { preview, reason } => {
                    let Some(ws) = app.previews.workspace(&preview) else {
                        continue;
                    };
                    let n = DidMutateParams {
                        preview_id: preview,
                        reason,
                    };
                    for c in app.clients_of(&ws) {
                        let _ = c.notify::<DidMutate>(n.clone()).await;
                    }
                }
            }
        }
    });
}

/// `preview/state` to the preview's workspace's clients.
pub async fn notify_state(app: &App, info: PreviewInfo) {
    for c in app.clients_of(&info.workspace_id) {
        let _ = c.notify::<State>(info.clone()).await;
    }
}

/// The daemon's previews (M5-PREVIEW-001 starts them) and the inspection service, which starts
/// with the first preview.
#[derive(Default)]
pub struct Previews {
    inner: std::sync::Mutex<std::collections::HashMap<PreviewId, PreviewInfo>>,
    inspect: tokio::sync::OnceCell<Arc<axiom_inspect::Inspect>>,
}

impl Previews {
    /// The inspection service, starting it (and its relay) on first use.
    pub async fn service(
        self: &Previews,
        app: &Arc<App>,
    ) -> std::io::Result<Arc<axiom_inspect::Inspect>> {
        self.inspect
            .get_or_try_init(|| async {
                let i = axiom_inspect::Inspect::start().await?;
                spawn_forwarder(app, &i);
                Ok(i)
            })
            .await
            .cloned()
    }

    pub fn inspect(&self) -> Option<Arc<axiom_inspect::Inspect>> {
        self.inspect.get().cloned()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<PreviewId, PreviewInfo>> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn contains(&self, id: &PreviewId) -> bool {
        self.lock().contains_key(id)
    }

    pub fn get(&self, id: &PreviewId) -> Option<PreviewInfo> {
        self.lock().get(id).cloned()
    }

    pub fn insert(&self, info: PreviewInfo) {
        self.lock().insert(info.preview_id.clone(), info);
    }

    pub fn remove(&self, id: &PreviewId) -> Option<PreviewInfo> {
        self.lock().remove(id)
    }

    pub fn workspace(&self, id: &PreviewId) -> Option<WorkspaceId> {
        self.lock().get(id).map(|i| i.workspace_id.clone())
    }

    /// Changes a preview's info; the new info (for `preview/state`).
    pub fn update(&self, id: &PreviewId, f: impl FnOnce(&mut PreviewInfo)) -> Option<PreviewInfo> {
        let mut m = self.lock();
        let i = m.get_mut(id)?;
        f(i);
        Some(i.clone())
    }

    pub fn of_workspace(&self, ws: &WorkspaceId) -> Vec<PreviewInfo> {
        let mut v: Vec<_> = self
            .lock()
            .values()
            .filter(|i| &i.workspace_id == ws && i.state != PreviewState::Stopped)
            .cloned()
            .collect();
        v.sort_by(|a, b| a.preview_id.0.cmp(&b.preview_id.0));
        v
    }
}
