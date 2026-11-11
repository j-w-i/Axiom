//! Tests for DRAFT contracts (provider manifest, inspection model). They keep
//! the drafts honest: the reference manifest must parse, and the inspection
//! model must be able to express what experiments E2 and E3 actually observed.

use axiom_protocol::inspection::{MappingMethod, SourcePrecision, UiNode, VerificationReport};
use axiom_protocol::provider::{DiagnosticsFormat, InstallMethod, InstallScope, ProviderManifest};
use serde_json::json;

#[test]
fn reference_rust_manifest_parses() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../providers/rust/provider.toml"
    ))
    .unwrap();
    let m: ProviderManifest = toml::from_str(&text).expect("manifest matches the contract");
    assert_eq!(m.schema, "axiom.provider/1");
    assert_eq!(m.id, "rust");
    assert!(m
        .tools
        .iter()
        .any(|t| t.id == "cargo" && t.required && t.version_args == ["--version"]));
    assert!(matches!(
        &m.install[0],
        InstallMethod::Delegate {
            scope: InstallScope::Managed,
            ..
        }
    ));
    assert_eq!(m.tasks[0].diagnostics, DiagnosticsFormat::CargoJson);
}

#[test]
fn manifest_rejects_unknown_fields() {
    let bad = r#"schema="axiom.provider/1"
id="x"
name="x"
description="x"
version="0.1.0"
post_install_script="curl | sh""#;
    assert!(toml::from_str::<ProviderManifest>(bad).is_err());
}

/// The node E3 resolved: second "Transfer" button of a repeated React component.
#[test]
fn inspection_model_expresses_e3_react_result() {
    let node: UiNode = serde_json::from_value(json!({
        "id": "dom:5", "surface": "page:1", "parent": "dom:4", "children": [],
        "typeName": "button", "kind": "element", "text": "Transfer",
        "bounds": {"x": 32.0, "y": 251.8, "width": 77.1, "height": 31.5},
        "visibility": {"visible": true, "clipped": false, "occluded": false},
        "sources": [
            {"location": {"file": "src/components/BalanceCard.jsx", "line": 6, "column": 7},
             "relation": "instantiation", "method": "buildInstrumentation", "confidence": 1.0, "generated": false},
            {"location": {"file": "src/App.jsx", "line": 13, "column": 9},
             "relation": "owner", "method": "buildInstrumentation", "confidence": 1.0, "generated": false}
        ],
        "stableKey": "src/components/BalanceCard.jsx:6:7#1",
        "instance": {"index": 1, "count": 2}
    }))
    .unwrap();
    assert_eq!(node.instance.unwrap().count, 2);
    assert!(node
        .sources
        .iter()
        .all(|s| s.confidence <= s.method.max_confidence()));
}

/// The node E2 resolved: a QML Rectangle with objectName, located via QQmlData.
#[test]
fn inspection_model_expresses_e2_qml_result() {
    let node: UiNode = serde_json::from_value(json!({
        "id": "qml:0x55d0", "surface": "window:1", "children": ["qml:0x55f8"],
        "typeName": "QQuickRectangle", "kind": "element", "name": "transferButton",
        "bounds": {"x": 32.0, "y": 192.0, "width": 80.0, "height": 24.0},
        "visibility": {"visible": true},
        "sources": [{"location": {"file": "BalanceCard.qml", "line": 10, "column": 5},
                     "relation": "instantiation", "method": "runtimeMetadata", "confidence": 1.0}]
    }))
    .unwrap();
    assert!(
        node.visibility.occluded.is_none(),
        "QML probe did not determine occlusion"
    );
    assert!(node.properties.is_empty());
}

#[test]
fn confidence_ceilings_are_ordered() {
    use MappingMethod::*;
    let c = [
        BuildInstrumentation,
        RuntimeMetadata,
        SourceMap,
        Heuristic,
        VisualInference,
    ]
    .map(MappingMethod::max_confidence);
    assert!(c.windows(2).all(|w| w[0] >= w[1]));
    assert!(SourcePrecision::LineColumn > SourcePrecision::File);
}

#[test]
fn verification_report_wire_shape() {
    let r: VerificationReport = serde_json::from_value(json!({
        "verdict": "verified",
        "checks": [
            {"kind": "buildClean", "passed": true, "summary": "No new error diagnostics."},
            {"kind": "geometry", "passed": true, "summary": "Button left edge moved +24 px.", "expected": 24, "actual": 24}
        ],
        "changedRegion": {"x": 32.0, "y": 133.0, "width": 101.0, "height": 150.5},
        "changedFraction": 0.00196
    }))
    .unwrap();
    assert_eq!(r.checks.len(), 2);
}
