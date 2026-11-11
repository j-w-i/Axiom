mod common;
use axiom_protocol::fs::{DidChangeParams, Watch, WatchParams};
use axiom_protocol::workspace::{Close, WorkspaceRef};
use common::*;
use std::time::Duration;

async fn set_watch(d: &Daemon, fx: &Fixture, enabled: bool) {
    d.client
        .request::<Watch>(WatchParams {
            workspace_id: fx.id.clone(),
            enabled,
        })
        .await
        .unwrap();
}

async fn next_change(d: &Daemon) -> DidChangeParams {
    serde_json::from_value(d.next_notification("fs/didChange", WAIT).await).unwrap()
}

/// Waits until a batch mentions `path`; returns its kind.
async fn wait_for_path(d: &Daemon, path: &str) -> String {
    loop {
        let p = next_change(d).await;
        if let Some(c) = p.changes.iter().find(|c| c.path.0 == path) {
            return serde_json::to_value(c.kind)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string();
        }
    }
}

#[tokio::test]
async fn no_notifications_before_fs_watch() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    std::fs::write(fx.path("unwatched.txt"), "x").unwrap();
    assert!(
        d.no_notification("fs/didChange", Duration::from_millis(600))
            .await
    );
}

#[tokio::test]
async fn change_after_enabling_is_reported_with_workspace_id() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    set_watch(&d, &fx, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(fx.path("external.txt"), "x").unwrap();
    loop {
        let p = next_change(&d).await;
        assert_eq!(p.workspace_id, fx.id);
        if let Some(c) = p.changes.iter().find(|c| c.path.0 == "external.txt") {
            assert_eq!(serde_json::to_value(c.kind).unwrap(), "created");
            break;
        }
    }
}

#[tokio::test]
async fn own_write_file_is_reported() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    set_watch(&d, &fx, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    fx.write("README.md", "rewritten", None, true)
        .await
        .unwrap();
    let kind = wait_for_path(&d, "README.md").await;
    assert!(kind == "modified" || kind == "created", "{kind}");
}

#[tokio::test]
async fn disable_stops_notifications_and_enable_is_idempotent() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    set_watch(&d, &fx, true).await;
    set_watch(&d, &fx, true).await; // idempotent
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(fx.path("a.txt"), "1").unwrap();
    wait_for_path(&d, "a.txt").await;
    // Drain anything still in flight, then disable.
    tokio::time::sleep(Duration::from_millis(300)).await;
    set_watch(&d, &fx, false).await;
    set_watch(&d, &fx, false).await; // idempotent
    while !d
        .no_notification("fs/didChange", Duration::from_millis(300))
        .await
    {}
    std::fs::write(fx.path("b.txt"), "2").unwrap();
    assert!(
        d.no_notification("fs/didChange", Duration::from_millis(600))
            .await
    );
    // Re-enabling works.
    set_watch(&d, &fx, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(fx.path("c.txt"), "3").unwrap();
    wait_for_path(&d, "c.txt").await;
}

#[tokio::test]
async fn single_enable_delivers_each_change_once_per_batch_not_duplicated() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    set_watch(&d, &fx, true).await;
    set_watch(&d, &fx, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(fx.path("once.txt"), "1").unwrap();
    let mut seen = 0;
    let end = tokio::time::Instant::now() + Duration::from_millis(1500);
    while tokio::time::Instant::now() < end {
        let left = end - tokio::time::Instant::now();
        let Ok(p) = tokio::time::timeout(left, async {
            serde_json::from_value::<DidChangeParams>(
                d.next_notification("fs/didChange", Duration::from_secs(2))
                    .await,
            )
            .unwrap()
        })
        .await
        else {
            break;
        };
        seen += p.changes.iter().filter(|c| c.path.0 == "once.txt").count();
    }
    assert_eq!(
        seen, 1,
        "a second fs/watch must not create a second watcher"
    );
}

#[tokio::test]
async fn git_and_ignored_directories_are_not_reported() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    set_watch(&d, &fx, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(fx.path(".git/HEAD"), "ref").unwrap();
    std::fs::write(fx.path("target/new.o"), "o").unwrap();
    assert!(
        d.no_notification("fs/didChange", Duration::from_millis(600))
            .await
    );
    // An individually ignored *file* in a non-ignored directory is reported.
    std::fs::write(fx.path("fresh.log"), "x").unwrap();
    wait_for_path(&d, "fresh.log").await;
}

#[tokio::test]
async fn closing_the_workspace_stops_its_watcher() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    set_watch(&d, &fx, true).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    d.client
        .request::<Close>(WorkspaceRef {
            workspace_id: fx.id.clone(),
        })
        .await
        .unwrap();
    while !d
        .no_notification("fs/didChange", Duration::from_millis(300))
        .await
    {}
    std::fs::write(fx.path("after-close.txt"), "x").unwrap();
    assert!(
        d.no_notification("fs/didChange", Duration::from_millis(600))
            .await
    );
}

#[tokio::test]
async fn watch_unknown_workspace_is_workspace_not_open() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request_raw(
            "fs/watch",
            serde_json::json!({"workspaceId": "w42", "enabled": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(kind(&e), axiom_protocol::ErrorKind::WorkspaceNotOpen);
}
