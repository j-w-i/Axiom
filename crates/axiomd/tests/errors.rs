mod common;
use axiom_protocol::ErrorKind;
use common::*;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn parse_error_has_null_id_and_connection_stays_up() {
    let mut s = RawSession::spawn().await;
    s.initialize().await;
    s.send("{not json").await;
    let v = s.next().await;
    assert!(v["id"].is_null());
    assert_eq!(v["error"]["data"]["kind"], "parse_error");
    assert_eq!(v["error"]["code"], -32700);
    s.send(r#"{"jsonrpc":"2.0","id":3,"method":"settings/get","params":{}}"#)
        .await;
    assert_eq!(s.next().await["id"], 3);
}

#[tokio::test]
async fn invalid_request_shapes_are_rejected_and_connection_stays_up() {
    let mut s = RawSession::spawn().await;
    s.initialize().await;
    for bad in [
        r#"[1,2,3]"#,
        r#"{"jsonrpc":"2.0","id":4}"#,
        r#"{"jsonrpc":"1.0","id":4,"method":"x"}"#,
        r#""str""#,
    ] {
        s.send(bad).await;
        let v = s.next().await;
        assert_eq!(v["error"]["data"]["kind"], "invalid_request", "{bad}");
        assert_eq!(v["error"]["code"], -32600);
    }
    s.send(r#"{"jsonrpc":"2.0","id":5,"method":"settings/get","params":{}}"#)
        .await;
    assert_eq!(s.next().await["id"], 5);
}

#[tokio::test]
async fn blank_lines_and_crlf_are_accepted() {
    let mut s = RawSession::spawn().await;
    s.send("").await;
    s.send("\r").await;
    s.initialize().await;
    s.send("{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"settings/get\",\"params\":{}}\r")
        .await;
    assert_eq!(s.next().await["id"], 8);
}

#[tokio::test]
async fn unknown_method_is_method_not_found() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request_raw("nope/nothing", json!({}))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::MethodNotFound);
    assert_eq!(e.code, -32601);
}

#[tokio::test]
async fn wrong_param_type_is_invalid_params_with_reason() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request_raw("workspace/open", json!({ "path": 42 }))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
    let reason = e.data.details.unwrap()["reason"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!reason.is_empty());
}

#[tokio::test]
async fn missing_required_field_is_invalid_params() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request_raw("workspace/open", json!({}))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
    let e = d
        .client
        .request_raw("fs/readFile", json!({ "workspaceId": "w1" }))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn unknown_fields_in_params_are_ignored() {
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    let r = d
        .client
        .request_raw(
            "workspace/open",
            json!({ "path": dir.path().to_str().unwrap(), "futureField": [1, 2, 3] }),
        )
        .await
        .unwrap();
    assert_eq!(r["workspaceId"], "w1");
}

#[tokio::test]
async fn unknown_enum_value_is_invalid_params() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let e = d
        .client
        .request_raw(
            "task/start",
            json!({ "workspaceId": fx.id.0, "command": { "bogus": ["x"] } }),
        )
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn missing_params_object_is_treated_as_empty() {
    let mut s = RawSession::spawn().await;
    s.initialize().await;
    s.send(r#"{"jsonrpc":"2.0","id":2,"method":"settings/get"}"#)
        .await;
    let v = s.next().await;
    assert!(v["result"]["values"].is_object());
}

#[tokio::test]
async fn every_error_carries_kind_and_matching_code() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let errors = [
        d.client.request_raw("x/y", json!({})).await.unwrap_err(),
        d.client
            .request_raw("workspace/open", json!({"path": 1}))
            .await
            .unwrap_err(),
        d.client
            .request_raw("workspace/open", json!({"path": "relative"}))
            .await
            .unwrap_err(),
        d.client
            .request_raw("fs/list", json!({"workspaceId":"w99","path":""}))
            .await
            .unwrap_err(),
        fx.read("missing.txt").await.unwrap_err(),
        fx.read("data/invalid-utf8.bin").await.unwrap_err(),
        fx.list("README.md").await.unwrap_err(),
    ];
    let kinds: Vec<ErrorKind> = errors.iter().map(kind).collect();
    assert_eq!(
        kinds,
        [
            ErrorKind::MethodNotFound,
            ErrorKind::InvalidParams,
            ErrorKind::InvalidParams,
            ErrorKind::WorkspaceNotOpen,
            ErrorKind::NotFound,
            ErrorKind::UnsupportedEncoding,
            ErrorKind::InvalidParams,
        ]
    );
    // retryable defaults to false for all of these.
    assert!(errors.iter().all(|e| !e.data.retryable));
    let _ = Duration::ZERO;
}
