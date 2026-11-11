mod common;
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::workspace::{
    Close, GetState, GetStateParams, Open, OpenParams, Recent, SetState, SetStateParams,
    WorkspaceRef,
};
use axiom_protocol::ErrorKind;
use common::*;
use serde_json::json;
use std::collections::BTreeMap;

fn open_params(p: &std::path::Path) -> OpenParams {
    OpenParams {
        path: p.to_str().unwrap().to_string(),
    }
}

#[tokio::test]
async fn open_returns_id_canonical_root_and_name_and_is_idempotent() {
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    let a = d
        .client
        .request::<Open>(open_params(dir.path()))
        .await
        .unwrap();
    let b = d
        .client
        .request::<Open>(open_params(dir.path()))
        .await
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.workspace_id.0, "w1");
    assert_eq!(
        a.root,
        dunce::canonicalize(dir.path()).unwrap().to_str().unwrap()
    );
    assert_eq!(a.name, dir.path().file_name().unwrap().to_str().unwrap());
    let other = tempfile::tempdir().unwrap();
    let c = d
        .client
        .request::<Open>(open_params(other.path()))
        .await
        .unwrap();
    assert_eq!(c.workspace_id.0, "w2");
}

#[tokio::test]
async fn open_errors() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request::<Open>(OpenParams {
            path: "relative/dir".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
    let dir = tempfile::tempdir().unwrap();
    let e = d
        .client
        .request::<Open>(open_params(&dir.path().join("missing")))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    let f = dir.path().join("f");
    std::fs::write(&f, "x").unwrap();
    let e = d.client.request::<Open>(open_params(&f)).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn close_then_use_is_workspace_not_open() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    d.client
        .request::<Close>(WorkspaceRef {
            workspace_id: fx.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        kind(&fx.list("").await.unwrap_err()),
        ErrorKind::WorkspaceNotOpen
    );
    assert_eq!(
        kind(&fx.read("README.md").await.unwrap_err()),
        ErrorKind::WorkspaceNotOpen
    );
    let e = d
        .client
        .request::<Close>(WorkspaceRef {
            workspace_id: fx.id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::WorkspaceNotOpen);
    let e = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: fx.id.clone(),
            keys: vec![],
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::WorkspaceNotOpen);
}

#[tokio::test]
async fn recent_is_newest_first_and_omits_deleted_directories() {
    let d = Daemon::ready().await;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let c = tempfile::tempdir().unwrap();
    for dir in [&a, &b, &c] {
        d.client
            .request::<Open>(open_params(dir.path()))
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let r = d.client.request::<Recent>(Empty {}).await.unwrap();
    let roots: Vec<&str> = r.workspaces.iter().map(|w| w.root.as_str()).collect();
    let canon = |p: &tempfile::TempDir| {
        dunce::canonicalize(p.path())
            .unwrap()
            .to_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(roots, [canon(&c), canon(&b), canon(&a)]);
    assert!(r.workspaces[0].last_opened_ms >= r.workspaces[1].last_opened_ms);
    assert_eq!(
        r.workspaces[0].name,
        c.path().file_name().unwrap().to_str().unwrap()
    );

    let b_root = canon(&b);
    drop(b); // deletes the directory
    let r = d.client.request::<Recent>(Empty {}).await.unwrap();
    assert_eq!(r.workspaces.len(), 2);
    assert!(r.workspaces.iter().all(|w| w.root != b_root));
}

#[tokio::test]
async fn reopening_moves_workspace_to_front_of_recent() {
    let d = Daemon::ready().await;
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    d.client
        .request::<Open>(open_params(a.path()))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    d.client
        .request::<Open>(open_params(b.path()))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    d.client
        .request::<Open>(open_params(a.path()))
        .await
        .unwrap();
    let r = d.client.request::<Recent>(Empty {}).await.unwrap();
    assert_eq!(r.workspaces.len(), 2);
    assert_eq!(
        r.workspaces[0].root,
        dunce::canonicalize(a.path()).unwrap().to_str().unwrap()
    );
}

#[tokio::test]
async fn recent_is_capped_at_twenty() {
    let d = Daemon::ready().await;
    let mut keep = Vec::new();
    for _ in 0..22 {
        let dir = tempfile::tempdir().unwrap();
        d.client
            .request::<Open>(open_params(dir.path()))
            .await
            .unwrap();
        keep.push(dir);
    }
    let r = d.client.request::<Recent>(Empty {}).await.unwrap();
    assert_eq!(r.workspaces.len(), 20);
}

#[tokio::test]
async fn state_set_get_merge_and_delete_by_null() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let set = |values: serde_json::Value| SetStateParams {
        workspace_id: fx.id.clone(),
        values: serde_json::from_value(values).unwrap(),
    };
    d.client
        .request::<SetState>(set(json!({ "tabs": ["a", "b"], "sel": "a" })))
        .await
        .unwrap();
    let all = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: fx.id.clone(),
            keys: vec![],
        })
        .await
        .unwrap();
    assert_eq!(all.values["tabs"], json!(["a", "b"]));
    assert_eq!(all.values.len(), 2);

    let only = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: fx.id.clone(),
            keys: vec!["sel".into(), "nope".into()],
        })
        .await
        .unwrap();
    assert_eq!(only.values.len(), 1, "missing keys are absent");
    assert_eq!(only.values["sel"], json!("a"));

    // Merge: untouched keys survive; null deletes.
    d.client
        .request::<SetState>(set(json!({ "sel": null, "extra": 1 })))
        .await
        .unwrap();
    let all = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: fx.id.clone(),
            keys: vec![],
        })
        .await
        .unwrap();
    let keys: Vec<&String> = all.values.keys().collect();
    assert_eq!(keys, ["extra", "tabs"]);
}

#[tokio::test]
async fn state_is_per_workspace() {
    let d = Daemon::ready().await;
    let a = d.open_fixture().await;
    let b = d.open_fixture().await;
    d.client
        .request::<SetState>(SetStateParams {
            workspace_id: a.id.clone(),
            values: BTreeMap::from([("k".to_string(), json!(1))]),
        })
        .await
        .unwrap();
    let got = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: b.id.clone(),
            keys: vec![],
        })
        .await
        .unwrap();
    assert!(got.values.is_empty());
}

#[tokio::test]
async fn state_persists_across_daemon_restart() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    {
        let d = Daemon::ready_in(data.path()).await;
        let r = d
            .client
            .request::<Open>(open_params(ws.path()))
            .await
            .unwrap();
        d.client
            .request::<SetState>(SetStateParams {
                workspace_id: r.workspace_id,
                values: BTreeMap::from([("tabs".to_string(), json!(["x"]))]),
            })
            .await
            .unwrap();
        d.shutdown().await;
    }
    let d = Daemon::ready_in(data.path()).await;
    let r = d
        .client
        .request::<Open>(open_params(ws.path()))
        .await
        .unwrap();
    assert_eq!(r.workspace_id.0, "w1", "ids are per daemon run");
    let got = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: r.workspace_id,
            keys: vec![],
        })
        .await
        .unwrap();
    assert_eq!(got.values["tabs"], json!(["x"]));
    // Recent also survived.
    let recent = d.client.request::<Recent>(Empty {}).await.unwrap();
    assert_eq!(recent.workspaces.len(), 1);
}

#[tokio::test]
async fn state_value_over_one_mebibyte_is_too_large_and_writes_nothing() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let big = "x".repeat(1024 * 1024 + 1);
    let e = d
        .client
        .request::<SetState>(SetStateParams {
            workspace_id: fx.id.clone(),
            values: BTreeMap::from([
                ("ok".to_string(), json!(1)),
                ("big".to_string(), json!(big)),
            ]),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::TooLarge);
    let got = d
        .client
        .request::<GetState>(GetStateParams {
            workspace_id: fx.id.clone(),
            keys: vec![],
        })
        .await
        .unwrap();
    assert!(got.values.is_empty(), "the batch is atomic");
}

#[tokio::test]
async fn open_records_last_root_setting() {
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    let r = d
        .client
        .request::<Open>(open_params(dir.path()))
        .await
        .unwrap();
    let v = d
        .client
        .request_raw("settings/get", json!({"keys": ["workspace.lastRoot"]}))
        .await
        .unwrap();
    assert_eq!(v["values"]["workspace.lastRoot"], r.root);
}
