//! Contract tests for the wire format. These pin the JSON shape: if one of
//! them fails, the protocol changed and `docs/protocols/ipc.md` plus every
//! client must be reviewed. Do not "fix" a failure by editing the expected
//! JSON without following the change rules in `docs/architecture/governance.md`.

use axiom_protocol::envelope::{ErrorKind, Message, RpcError};
use axiom_protocol::method::Method;
use axiom_protocol::{fs, lifecycle, registry, task, workspace, WorkspacePath};
use serde_json::{json, Value};

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    v: Value,
) -> T {
    let t: T = serde_json::from_value(v.clone()).expect("deserializes");
    assert_eq!(
        serde_json::to_value(&t).unwrap(),
        v,
        "re-serializes identically"
    );
    t
}

#[test]
fn method_names_are_unique_and_well_formed() {
    let names = registry::method_names();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate method name");
    for n in names {
        assert!(
            n == "initialize"
                || n == "shutdown"
                || n.split_once('/')
                    .is_some_and(|(d, v)| !d.is_empty() && !v.is_empty()),
            "{n}"
        );
    }
}

#[test]
fn envelope_distinguishes_message_kinds() {
    let req: Message =
        serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"fs/list","params":{}}))
            .unwrap();
    assert!(matches!(req, Message::Request(_)));
    let note: Message =
        serde_json::from_value(json!({"jsonrpc":"2.0","method":"task/output","params":{}}))
            .unwrap();
    assert!(matches!(note, Message::Notification(_)));
    let res: Message =
        serde_json::from_value(json!({"jsonrpc":"2.0","id":"a","result":{}})).unwrap();
    assert!(matches!(res, Message::Response(_)));
    assert!(
        serde_json::from_value::<Message>(json!({"jsonrpc":"1.0","id":1,"method":"x"})).is_err()
    );
    assert!(
        serde_json::from_value::<Message>(json!({"jsonrpc":"2.0","id":1})).is_err(),
        "neither result nor error"
    );
    assert!(serde_json::from_value::<Message>(
        json!({"jsonrpc":"2.0","id":1,"result":{},"error":{}})
    )
    .is_err());
    assert!(
        serde_json::from_value::<Message>(json!([1, 2])).is_err(),
        "batches are not supported"
    );
}

#[test]
fn error_shape_and_codes() {
    let e = RpcError::new(ErrorKind::Conflict, "File changed on disk.")
        .with_details(json!({"currentVersion": "ab"}));
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!({"code": -32013, "message": "File changed on disk.",
               "data": {"kind": "conflict", "retryable": false, "details": {"currentVersion": "ab"}}})
    );
    // Codes are unique.
    let kinds = [
        ErrorKind::ParseError,
        ErrorKind::InvalidRequest,
        ErrorKind::MethodNotFound,
        ErrorKind::InvalidParams,
        ErrorKind::Internal,
        ErrorKind::NotInitialized,
        ErrorKind::Cancelled,
        ErrorKind::Timeout,
        ErrorKind::Unsupported,
        ErrorKind::NotFound,
        ErrorKind::AlreadyExists,
        ErrorKind::PermissionDenied,
        ErrorKind::Conflict,
        ErrorKind::Io,
        ErrorKind::UnsupportedEncoding,
        ErrorKind::TooLarge,
        ErrorKind::WorkspaceNotOpen,
        ErrorKind::SpawnFailed,
        ErrorKind::Unknown,
    ];
    let mut codes: Vec<i32> = kinds.iter().map(|k| k.code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), kinds.len());
}

#[test]
fn unknown_error_kind_from_newer_peer_is_tolerated() {
    let k: ErrorKind = serde_json::from_value(json!("quota_exceeded")).unwrap();
    assert_eq!(k, ErrorKind::Unknown);
}

#[test]
fn initialize_wire_shape() {
    assert_eq!(lifecycle::Initialize::NAME, "initialize");
    round_trip::<lifecycle::InitializeParams>(
        json!({"protocolVersion":"0.1","client":{"name":"axiom-desktop","version":"0.1.0"}}),
    );
    round_trip::<lifecycle::InitializeResult>(json!({
        "protocolVersion":"0.1","server":{"name":"axiomd","version":"0.1.0"},
        "methods":["initialize"],"host":{"os":"linux","arch":"x86_64","pathSeparator":"/"}}));
}

#[test]
fn workspace_and_fs_wire_shape() {
    round_trip::<workspace::OpenResult>(
        json!({"workspaceId":"w1","root":"/home/u/proj","name":"proj"}),
    );
    round_trip::<fs::ListResult>(json!({"entries":[
        {"name":"src","kind":"directory","ignored":false},
        {"name":"main.rs","kind":"file","size":120,"ignored":false}]}));
    round_trip::<fs::ReadFileResult>(json!({"text":"a\r\nb","version":"9f86d0"}));
    let w: fs::WriteFileParams =
        serde_json::from_value(json!({"workspaceId":"w1","path":"src/main.rs","text":"x"}))
            .unwrap();
    assert!(
        w.expected_version.is_none() && !w.overwrite,
        "optional fields default"
    );
    round_trip::<fs::DidChangeParams>(
        json!({"workspaceId":"w1","changes":[{"path":"a.txt","kind":"modified"}]}),
    );
}

#[test]
fn task_wire_shape() {
    round_trip::<task::StartParams>(
        json!({"workspaceId":"w1","command":{"argv":["cargo","build"]},"env":{}}),
    );
    round_trip::<task::StartParams>(
        json!({"workspaceId":"w1","command":{"shell":"make -j4"},"cwd":"sub","env":{"A":"1"},"label":"Build"}),
    );
    round_trip::<task::OutputParams>(
        json!({"taskId":"t1","seq":0,"stream":"stderr","data":"warning"}),
    );
    // ExitStatus is flattened into task/exited.
    round_trip::<task::ExitedParams>(
        json!({"taskId":"t1","outcome":"failed","exitCode":3,"durationMs":12}),
    );
    round_trip::<task::ExitedParams>(
        json!({"taskId":"t1","outcome":"cancelled","signal":15,"durationMs":12}),
    );
}

#[test]
fn workspace_path_lexical_rules() {
    for ok in ["", "a", "a/b.txt", "a b/ç.rs", ".gitignore", "a/.hidden/b"] {
        assert!(
            WorkspacePath(ok.into()).is_lexically_valid(),
            "{ok:?} should be valid"
        );
    }
    for bad in [
        "/a", "a/", "a//b", "../a", "a/../b", "./a", "a\\b", "a/./b", "a\0b",
    ] {
        assert!(
            !WorkspacePath(bad.into()).is_lexically_valid(),
            "{bad:?} should be invalid"
        );
    }
}

#[test]
fn committed_schemas_are_current() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/");
    for (rel, doc) in [
        ("ipc/protocol.schema.json", registry::protocol_document()),
        (
            "provider/manifest.schema.json",
            registry::provider_manifest_document(),
        ),
        (
            "inspection/inspection.schema.json",
            registry::inspection_document(),
        ),
    ] {
        let text = std::fs::read_to_string(format!("{dir}{rel}"))
            .unwrap_or_else(|_| panic!("schemas/{rel} exists"));
        let committed: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            committed, doc,
            "schemas/{rel} is stale; run: cargo run -p axiom-protocol --example export_schema"
        );
    }
}
