mod common;
use axiom_protocol::lifecycle::{Cancel, CancelParams};
use axiom_protocol::RequestId;
use common::*;
use serde_json::json;

#[tokio::test]
async fn cancel_of_unknown_id_is_ignored_and_connection_still_works() {
    let d = Daemon::ready().await;
    d.client
        .notify::<Cancel>(CancelParams {
            id: RequestId::Number(424242),
        })
        .await;
    d.client
        .notify::<Cancel>(CancelParams {
            id: RequestId::String("nope".into()),
        })
        .await;
    let v = d
        .client
        .request_raw("settings/get", json!({}))
        .await
        .unwrap();
    assert!(v["values"].is_object());
}

#[tokio::test]
async fn cancel_of_a_finished_request_is_ignored() {
    let d = Daemon::ready().await;
    let (id, fut) = d.client.request_with_id::<axiom_protocol::settings::Get>(
        axiom_protocol::settings::GetParams { keys: vec![] },
    );
    fut.await.unwrap();
    d.client.notify::<Cancel>(CancelParams { id }).await;
    assert!(d
        .client
        .request_raw("settings/get", json!({}))
        .await
        .is_ok());
}

#[tokio::test]
async fn cancel_with_malformed_params_is_ignored() {
    let mut s = RawSession::spawn().await;
    s.initialize().await;
    s.send(r#"{"jsonrpc":"2.0","method":"$/cancel","params":{"id":{"x":1}}}"#)
        .await;
    s.send(r#"{"jsonrpc":"2.0","method":"$/cancel"}"#).await;
    s.send(r#"{"jsonrpc":"2.0","id":9,"method":"settings/get","params":{}}"#)
        .await;
    assert_eq!(s.next().await["id"], 9);
}
