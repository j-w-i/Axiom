mod common;
use axiom_protocol::settings::{Get, GetParams, GetResult, Set, SetParams};
use axiom_protocol::ErrorKind;
use common::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn set(values: Value) -> SetParams {
    SetParams {
        values: serde_json::from_value(values).unwrap(),
    }
}

async fn get(d: &Daemon, keys: &[&str]) -> BTreeMap<String, Value> {
    d.client
        .request::<Get>(GetParams {
            keys: keys.iter().map(|k| k.to_string()).collect(),
        })
        .await
        .unwrap()
        .values
}

#[tokio::test]
async fn defaults_for_all_known_keys() {
    let d = Daemon::ready().await;
    let v = get(&d, &[]).await;
    assert_eq!(v["appearance.theme"], json!("system"));
    assert_eq!(v["editor.fontSize"], json!(13));
    assert_eq!(v["editor.fontFamily"], json!(""));
    assert_eq!(v["editor.wordWrap"], json!(false));
    assert_eq!(v["workspace.reopenLast"], json!(true));
    assert_eq!(v["workspace.lastRoot"], Value::Null);
    // M3-MODEL-002.
    assert_eq!(v["agent.providers"], json!([]));
    // M3-GATE-001: the built-in policy.
    assert_eq!(v["agent.policy"]["default"], json!("ask"));
    // M3-MCP-001.
    assert_eq!(v["agent.mcpServers"], json!([]));
    // M4-LSP-001.
    assert_eq!(v["lsp.servers"], json!({}));
    // M5.5-ARCH-001: servers are installed only when asked, unless this is on.
    assert_eq!(v["lsp.autoInstall"], json!(false));
    assert_eq!(v["editor.formatOnSave"], json!(false));
    assert_eq!(v.len(), 12);
}

#[tokio::test]
async fn get_with_keys_returns_only_those() {
    let d = Daemon::ready().await;
    let v = get(&d, &["editor.fontSize", "no.such.key"]).await;
    assert_eq!(v.len(), 1);
    assert_eq!(v["editor.fontSize"], json!(13));
}

#[tokio::test]
async fn set_then_get_round_trip() {
    let d = Daemon::ready().await;
    d.client
        .request::<Set>(set(json!({"appearance.theme": "dark", "editor.fontSize": 18, "editor.wordWrap": true, "editor.fontFamily": "Fira Code"})))
        .await
        .unwrap();
    let v = get(&d, &[]).await;
    assert_eq!(v["appearance.theme"], json!("dark"));
    assert_eq!(v["editor.fontSize"], json!(18));
    assert_eq!(v["editor.wordWrap"], json!(true));
    assert_eq!(v["editor.fontFamily"], json!("Fira Code"));
    assert_eq!(
        v["workspace.reopenLast"],
        json!(true),
        "untouched keys keep defaults"
    );
}

#[tokio::test]
async fn known_keys_are_type_and_range_validated_with_key_in_details() {
    let d = Daemon::ready().await;
    for (key, bad) in [
        ("appearance.theme", json!("blue")),
        ("appearance.theme", json!(1)),
        ("editor.fontSize", json!("big")),
        ("editor.fontSize", json!(7)),
        ("editor.fontSize", json!(41)),
        ("editor.fontSize", json!(13.5)),
        ("editor.wordWrap", json!("yes")),
        ("editor.fontFamily", json!(3)),
        ("workspace.reopenLast", json!(0)),
        ("workspace.lastRoot", json!(5)),
    ] {
        let e = d
            .client
            .request::<Set>(set(json!({ key: bad.clone() })))
            .await
            .unwrap_err();
        assert_eq!(kind(&e), ErrorKind::InvalidParams, "{key}={bad}");
        assert_eq!(e.data.details.unwrap()["key"], key);
    }
    // Boundaries are accepted.
    d.client
        .request::<Set>(set(json!({"editor.fontSize": 8})))
        .await
        .unwrap();
    d.client
        .request::<Set>(set(json!({"editor.fontSize": 40})))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_rejected_batch_writes_nothing() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request::<Set>(set(
            json!({"editor.wordWrap": true, "editor.fontSize": "nope"}),
        ))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
    assert_eq!(
        get(&d, &["editor.wordWrap"]).await["editor.wordWrap"],
        json!(false)
    );
}

#[tokio::test]
async fn null_resets_a_key_to_its_default() {
    let d = Daemon::ready().await;
    d.client
        .request::<Set>(set(json!({"editor.fontSize": 20})))
        .await
        .unwrap();
    d.client
        .request::<Set>(set(json!({"editor.fontSize": null})))
        .await
        .unwrap();
    assert_eq!(
        get(&d, &["editor.fontSize"]).await["editor.fontSize"],
        json!(13)
    );
}

#[tokio::test]
async fn did_change_is_emitted_with_effective_values_after_the_response() {
    let d = Daemon::ready().await;
    d.client
        .request::<Set>(set(
            json!({"appearance.theme": "light", "editor.fontSize": 21}),
        ))
        .await
        .unwrap();
    let p: GetResult =
        serde_json::from_value(d.next_notification("settings/didChange", WAIT).await).unwrap();
    assert_eq!(p.values.len(), 2);
    assert_eq!(p.values["appearance.theme"], json!("light"));
    assert_eq!(p.values["editor.fontSize"], json!(21));
    // A reset reports the default, not null.
    d.client
        .request::<Set>(set(json!({"editor.fontSize": null})))
        .await
        .unwrap();
    let p: GetResult =
        serde_json::from_value(d.next_notification("settings/didChange", WAIT).await).unwrap();
    assert_eq!(p.values["editor.fontSize"], json!(13));
}

#[tokio::test]
async fn did_change_is_not_sent_for_a_rejected_set() {
    let d = Daemon::ready().await;
    let _ = d
        .client
        .request::<Set>(set(json!({"editor.fontSize": 1})))
        .await
        .unwrap_err();
    assert!(
        d.no_notification("settings/didChange", std::time::Duration::from_millis(300))
            .await
    );
}

#[tokio::test]
async fn unknown_keys_round_trip_unchanged() {
    let d = Daemon::ready().await;
    let weird = json!({"nested": {"a": [1, 2, {"b": null}]}, "s": "ü"});
    d.client
        .request::<Set>(set(json!({"x.custom": weird.clone(), "x.num": 5})))
        .await
        .unwrap();
    let v = get(&d, &["x.custom", "x.num"]).await;
    assert_eq!(v["x.custom"], weird);
    assert_eq!(v["x.num"], json!(5));
    assert_eq!(
        get(&d, &[]).await["x.custom"],
        weird,
        "all-keys read includes stored unknown keys"
    );
    d.client
        .request::<Set>(set(json!({"x.custom": null})))
        .await
        .unwrap();
    assert!(!get(&d, &[]).await.contains_key("x.custom"));
}

#[tokio::test]
async fn settings_persist_across_restart() {
    let data = tempfile::tempdir().unwrap();
    {
        let d = Daemon::ready_in(data.path()).await;
        d.client
            .request::<Set>(set(json!({"appearance.theme": "dark", "x.custom": [1]})))
            .await
            .unwrap();
        d.shutdown().await;
    }
    let d = Daemon::ready_in(data.path()).await;
    let v = get(&d, &[]).await;
    assert_eq!(v["appearance.theme"], json!("dark"));
    assert_eq!(v["x.custom"], json!([1]));
    assert_eq!(v["editor.fontSize"], json!(13));
}

#[tokio::test]
async fn too_large_unknown_value_is_too_large() {
    let d = Daemon::ready().await;
    let big = "x".repeat(1024 * 1024 + 1);
    let e = d
        .client
        .request::<Set>(set(json!({"x.big": big})))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::TooLarge);
}
