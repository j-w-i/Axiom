//! Contract tests for the protocol 0.5 additions (ADR-0019, M5-ARCH-001) and the inspection
//! adapter protocol frozen for M5. Same rule as `contract.rs`: a failure means the wire changed;
//! follow `governance.md`, do not edit the JSON.

use axiom_protocol::envelope::{ErrorKind, RpcError};
use axiom_protocol::inspection::{self, Check, CheckKind};
use axiom_protocol::method::{Method, Notification};
use axiom_protocol::{preview, registry, ui, PROTOCOL_VERSION};
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
fn protocol_version_is_0_5_or_later() {
    let (major, minor) = PROTOCOL_VERSION.split_once('.').expect("MAJOR.MINOR");
    assert_eq!(major, "0");
    assert!(minor.parse::<u32>().unwrap() >= 5, "{PROTOCOL_VERSION}");
}

#[test]
fn m5_methods_and_notifications_are_registered() {
    let names = registry::method_names();
    for (name, wire) in [
        (preview::Start::NAME, "preview/start"),
        (preview::Stop::NAME, "preview/stop"),
        (preview::List::NAME, "preview/list"),
        (ui::InspectMode::NAME, "ui/inspectMode"),
        (ui::TakeSnapshot::NAME, "ui/snapshot"),
        (ui::HitTest::NAME, "ui/hitTest"),
        (ui::NodeDetails::NAME, "ui/nodeDetails"),
        (ui::Highlight::NAME, "ui/highlight"),
        (ui::Verify::NAME, "ui/verify"),
    ] {
        assert_eq!(name, wire);
        assert!(names.contains(&name), "{name} not registered");
    }
    let mut g = schemars::SchemaGenerator::default();
    let notes: Vec<_> = registry::notifications(&mut g)
        .into_iter()
        .map(|n| n.name)
        .collect();
    for (name, wire) in [
        (preview::State::NAME, "preview/state"),
        (ui::Selected::NAME, "ui/selected"),
        (ui::Hovered::NAME, "ui/hovered"),
        (ui::DidMutate::NAME, "ui/didMutate"),
    ] {
        assert_eq!(name, wire);
        assert!(notes.contains(&name), "{name} not registered");
    }
    // The adapter's own methods are not client methods.
    assert!(!names.contains(&"inspect/hello"));
}

#[test]
fn adapter_protocol_additions() {
    assert_eq!(inspection::SetMode::NAME, "inspect/setMode");
    assert_eq!(inspection::AdapterHovered::NAME, "inspect/hovered");
    assert_eq!(inspection::AdapterSelected::NAME, "inspect/selected");
    assert_eq!(inspection::RuntimeError::NAME, "inspect/runtimeError");
    round_trip::<inspection::SetModeParams>(json!({ "on": true }));
    round_trip::<inspection::RuntimeErrorParams>(json!({ "message": "boom", "atMs": 5 }));
    let doc = registry::inspection_document();
    for m in ["inspect/hello", "inspect/snapshot", "inspect/setMode"] {
        assert!(doc["methods"][m].is_object(), "{m}");
    }
    for n in [
        "inspect/didMutate",
        "inspect/hovered",
        "inspect/selected",
        "inspect/runtimeError",
    ] {
        assert!(doc["notifications"][n].is_object(), "{n}");
    }
}

#[test]
fn preview_shapes() {
    round_trip::<preview::StartParams>(json!({ "workspaceId": "w1" }));
    round_trip::<preview::StartParams>(json!({ "workspaceId": "w1", "argv": ["npx", "vite"] }));
    let info: preview::PreviewInfo = round_trip(json!({
        "previewId": "p1", "workspaceId": "w1", "state": "ready",
        "url": "http://127.0.0.1:5173/", "taskId": "t1"
    }));
    assert_eq!(info.state, preview::PreviewState::Ready);
    round_trip::<preview::PreviewInfo>(json!({
        "previewId": "p1", "workspaceId": "w1", "state": "failed",
        "message": "The dev server exited (code 1).", "output": "Error: no vite"
    }));
    round_trip::<preview::PreviewRef>(json!({ "previewId": "p1" }));
}

#[test]
fn ui_shapes() {
    round_trip::<ui::InspectModeParams>(json!({ "previewId": "p1", "on": true }));
    round_trip::<ui::SnapshotParams>(json!({ "previewId": "p1", "includeInvisible": false }));
    round_trip::<ui::HitTestParams>(
        json!({ "previewId": "p1", "surface": "s0", "x": 10.0, "y": 4.5 }),
    );
    let v: ui::VerifyParams = round_trip(json!({
        "previewId": "p1",
        "target": { "stableKey": "src/App.jsx:12:7#0" },
        "assertions": [
            { "kind": "geometry", "node": { "stableKey": "src/App.jsx:12:7#0" }, "edge": "left",
              "op": "eq", "other": { "node": { "stableKey": "src/App.jsx:8:5#0" }, "edge": "left" },
              "offset": 16.0 },
            { "kind": "property", "node": { "stableKey": "src/App.jsx:12:7#0" },
              "name": "css.padding-left", "equals": "16px" }
        ],
        "timeoutMs": 5000
    }));
    assert!(matches!(
        v.assertions[0],
        ui::Assertion::Geometry {
            edge: ui::Edge::Left,
            op: ui::Comparison::Eq,
            ..
        }
    ));
    round_trip::<ui::DidMutateParams>(json!({ "previewId": "p1", "reason": "hotReload" }));
    round_trip::<ui::HoveredParams>(json!({ "previewId": "p1" }));
}

#[test]
fn a_skipped_check_says_so_and_a_normal_one_omits_it() {
    let c: Check = round_trip(json!({
        "kind": "pixelsChangedInRegion", "passed": false, "skipped": true,
        "summary": "no capture source in M5"
    }));
    assert_eq!(c.kind, CheckKind::PixelsChangedInRegion);
    round_trip::<Check>(json!({ "kind": "geometry", "passed": true, "summary": "left = 24" }));
}

#[test]
fn m5_error_codes() {
    for (kind, code, name) in [
        (ErrorKind::PreviewFailed, -32029, "preview_failed"),
        (ErrorKind::AdapterUnavailable, -32030, "adapter_unavailable"),
    ] {
        assert_eq!(kind.code(), code);
        let v = serde_json::to_value(RpcError::new(kind, "x")).unwrap();
        assert_eq!(v["code"], code);
        assert_eq!(v["data"]["kind"], name);
        let back: RpcError = serde_json::from_value(v).unwrap();
        assert_eq!(back.data.kind, kind);
    }
}
