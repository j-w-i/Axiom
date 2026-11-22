//! `ui/*` through the daemon (M5-INSPECT-001): the methods are served, and a preview the daemon
//! does not know is `not_found` (the full path through a real page's adapter is
//! `tests/preview.rs`, M5-PREVIEW-001).

mod common;

use axiom_protocol::inspection::{NodeId, SurfaceId};
use axiom_protocol::lifecycle::Initialize;
use axiom_protocol::preview::PreviewId;
use axiom_protocol::ui::{
    Highlight, HighlightParams, HitTest, HitTestParams, InspectMode, InspectModeParams,
    NodeDetails, NodeParams, SnapshotParams, TakeSnapshot,
};
use axiom_protocol::ErrorKind;
use common::{client_info, kind, Daemon};

#[tokio::test]
async fn ui_methods_are_served_and_an_unknown_preview_is_not_found() {
    let d = Daemon::spawn().await;
    let init = d.client.request::<Initialize>(client_info()).await.unwrap();
    for m in [
        "ui/inspectMode",
        "ui/snapshot",
        "ui/hitTest",
        "ui/nodeDetails",
        "ui/highlight",
    ] {
        assert!(init.methods.iter().any(|x| x == m), "{m} not served");
    }
    let p = PreviewId("nope".into());
    let errs = [
        d.client
            .request::<InspectMode>(InspectModeParams {
                preview_id: p.clone(),
                on: true,
            })
            .await
            .unwrap_err(),
        d.client
            .request::<TakeSnapshot>(SnapshotParams {
                preview_id: p.clone(),
                max_nodes: None,
                include_invisible: false,
            })
            .await
            .unwrap_err(),
        d.client
            .request::<HitTest>(HitTestParams {
                preview_id: p.clone(),
                surface: SurfaceId("s0".into()),
                x: 1.0,
                y: 1.0,
            })
            .await
            .unwrap_err(),
        d.client
            .request::<NodeDetails>(NodeParams {
                preview_id: p.clone(),
                node: NodeId("n".into()),
            })
            .await
            .unwrap_err(),
        d.client
            .request::<Highlight>(HighlightParams {
                preview_id: p,
                node: None,
            })
            .await
            .unwrap_err(),
    ];
    for e in errs {
        assert_eq!(kind(&e), ErrorKind::NotFound, "{e:?}");
    }
}
