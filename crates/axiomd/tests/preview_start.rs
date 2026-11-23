//! `preview/start` without a browser or Vite (AUD-034): a fake dev server that prints its
//! address after a while. A cancelled start still becomes `ready` (the preview's own task moves
//! it, not the request), and two starts at once make one preview.
#![cfg(unix)]

mod common;

use axiom_protocol::lifecycle::CancelParams;
use axiom_protocol::preview::{List, PreviewRef, PreviewState, Start, StartParams, Stop};
use axiom_protocol::workspace::WorkspaceRef;
use common::{kind, Daemon};
use std::time::Duration;

fn fake_server(delay: &str) -> Vec<String> {
    vec![
        "sh".into(),
        "-c".into(),
        format!("sleep {delay}; echo '  Local:   http://127.0.0.1:5999/'; exec sleep 60"),
    ]
}

/// A fake dev server that prints its address only once `go` exists, so a test decides when it
/// becomes ready instead of racing a delay.
fn gated_server(go: &std::path::Path) -> Vec<String> {
    vec![
        "sh".into(),
        "-c".into(),
        format!(
            "while [ ! -e \"{}\" ]; do sleep 0.05; done; \
             echo '  Local:   http://127.0.0.1:5999/'; exec sleep 60",
            go.display()
        ),
    ]
}

#[tokio::test]
async fn a_cancelled_start_still_becomes_ready() {
    let d = Daemon::ready().await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let gate = tempfile::tempdir().unwrap();
    let go = gate.path().join("go");
    let (id, fut) = d.client.request_with_id::<Start>(StartParams {
        workspace_id: f.id.clone(),
        argv: Some(gated_server(&go)),
    });
    let req = tokio::spawn(fut);
    // Cancel once the preview exists and is still starting (its server waits for `go`).
    let mut starting = false;
    for _ in 0..100 {
        let l = d
            .client
            .request::<List>(WorkspaceRef {
                workspace_id: f.id.clone(),
            })
            .await
            .unwrap();
        if l.previews.iter().any(|p| p.state == PreviewState::Starting) {
            starting = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(starting, "the preview never appeared");
    d.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams { id })
        .await;
    let e = req.await.unwrap().unwrap_err();
    assert_eq!(kind(&e), axiom_protocol::ErrorKind::Cancelled);
    // The server prints its address after the request is gone: the preview is ready.
    std::fs::write(&go, "").unwrap();
    let mut ready = None;
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let l = d
            .client
            .request::<List>(WorkspaceRef {
                workspace_id: f.id.clone(),
            })
            .await
            .unwrap();
        if let Some(p) = l
            .previews
            .into_iter()
            .find(|p| p.state == PreviewState::Ready)
        {
            ready = Some(p);
            break;
        }
    }
    let p = ready.expect("the preview stayed starting");
    assert_eq!(p.url.as_deref(), Some("http://127.0.0.1:5999/"));
    // A later start returns it, and it can be stopped.
    let again = d
        .client
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            argv: Some(fake_server("0")),
        })
        .await
        .unwrap();
    assert_eq!(again.preview_id, p.preview_id);
    d.client
        .request::<Stop>(PreviewRef {
            preview_id: p.preview_id.clone(),
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn two_starts_at_once_make_one_preview() {
    let d = Daemon::ready().await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let start = || {
        d.client.request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            argv: Some(fake_server("0.3")),
        })
    };
    let (a, b) = tokio::join!(start(), start());
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.preview_id, b.preview_id);
    let l = d
        .client
        .request::<List>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(l.previews.len(), 1, "{:?}", l.previews);
    d.client
        .request::<Stop>(PreviewRef {
            preview_id: a.preview_id,
        })
        .await
        .unwrap();
}
