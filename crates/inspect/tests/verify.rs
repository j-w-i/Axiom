//! Verification (M5-VERIFY-001): each check kind and the verdict rule on fixed observations, and
//! through a fake adapter whose page changes: a true geometry check is `verified`, a false one
//! `failed`, a node that cannot be found again `inconclusive`.

use axiom_inspect::{evaluate, find, verify, Inspect, Observed};
use axiom_protocol::inspection::{CheckKind, Snapshot, UiNode, Verdict};
use axiom_protocol::preview::PreviewId;
use axiom_protocol::ui::{Assertion, Comparison, Edge, EdgeRef, NodeTarget, VerifyParams};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

fn node(
    id: &str,
    parent: Option<&str>,
    key: Option<&str>,
    typ: &str,
    text: Option<&str>,
    x: f64,
) -> UiNode {
    serde_json::from_value(json!({
        "id": id, "surface": "page", "typeName": typ, "kind": "element",
        "parent": parent, "text": text, "stableKey": key,
        "visibility": {"visible": true},
        "bounds": {"x": x, "y": 0.0, "width": 100.0, "height": 20.0}
    }))
    .unwrap()
}

fn snap(nodes: Vec<UiNode>) -> Snapshot {
    Snapshot {
        captured_ms: 1,
        surfaces: vec![],
        nodes,
        truncated: false,
    }
}

fn key(k: &str) -> NodeTarget {
    NodeTarget {
        stable_key: Some(k.into()),
        node: None,
    }
}

fn geometry_eq(node: &str, other: &str, offset: f64) -> Assertion {
    Assertion::Geometry {
        node: key(node),
        edge: Edge::Left,
        op: Comparison::Eq,
        value: None,
        other: Some(EdgeRef {
            node: key(other),
            edge: Edge::Left,
        }),
        offset,
    }
}

fn observed(after: Snapshot) -> Observed {
    Observed {
        before: None,
        after,
        properties: HashMap::new(),
        runtime_errors: vec![],
        build_errors: Some(vec![]),
        unsettled: false,
    }
}

fn page(card_x: f64, button_x: f64) -> Snapshot {
    snap(vec![
        node("n1", None, Some("App.jsx:9:7#0"), "section", None, card_x),
        node(
            "n2",
            Some("n1"),
            Some("App.jsx:11:9#0"),
            "button",
            Some("Pay now"),
            button_x,
        ),
    ])
}

#[test]
fn a_true_geometry_check_verifies_and_a_false_one_fails() {
    let a = [geometry_eq("App.jsx:11:9#0", "App.jsx:9:7#0", 16.0)];
    let ok = evaluate(
        &a,
        Some(&key("App.jsx:11:9#0")),
        &observed(page(10.0, 26.0)),
    );
    assert_eq!(ok.verdict, Verdict::Verified, "{ok:#?}");
    let g = ok
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::Geometry)
        .unwrap();
    assert!(g.passed);
    assert_eq!(g.actual, Some(json!(26.0)));
    // Within the 0.5 px tolerance, still equal.
    assert_eq!(
        evaluate(&a, None, &observed(page(10.0, 26.4))).verdict,
        Verdict::Verified
    );
    let bad = evaluate(&a, None, &observed(page(10.0, 30.0)));
    assert_eq!(bad.verdict, Verdict::Failed);
    assert!(bad
        .checks
        .iter()
        .any(|c| c.kind == CheckKind::Geometry && !c.passed && !c.skipped));
}

#[test]
fn comparisons_and_fixed_values() {
    let mk = |op, value| Assertion::Geometry {
        node: key("App.jsx:11:9#0"),
        edge: Edge::Width,
        op,
        value: Some(value),
        other: None,
        offset: 0.0,
    };
    let o = observed(page(0.0, 0.0)); // width 100
    for (op, v, want) in [
        (Comparison::Eq, 100.0, Verdict::Verified),
        (Comparison::Lt, 120.0, Verdict::Verified),
        (Comparison::Lt, 100.0, Verdict::Failed),
        (Comparison::Le, 100.0, Verdict::Verified),
        (Comparison::Gt, 99.0, Verdict::Verified),
        (Comparison::Ge, 101.0, Verdict::Failed),
    ] {
        assert_eq!(evaluate(&[mk(op, v)], None, &o).verdict, want, "{op:?} {v}");
    }
}

#[test]
fn property_checks_compare_numbers_and_text() {
    let mut o = observed(page(0.0, 0.0));
    let mut props = BTreeMap::new();
    props.insert("css.padding-left".to_string(), json!("16px"));
    props.insert("css.display".to_string(), json!("flex"));
    o.properties
        .insert(axiom_protocol::inspection::NodeId("n2".into()), props);
    let p = |name: &str, v: Value| Assertion::Property {
        node: key("App.jsx:11:9#0"),
        name: name.into(),
        equals: v,
    };
    assert_eq!(
        evaluate(&[p("css.padding-left", json!(16))], None, &o).verdict,
        Verdict::Verified
    );
    assert_eq!(
        evaluate(&[p("css.padding-left", json!("16px"))], None, &o).verdict,
        Verdict::Verified
    );
    assert_eq!(
        evaluate(&[p("css.display", json!("flex"))], None, &o).verdict,
        Verdict::Verified
    );
    assert_eq!(
        evaluate(&[p("css.display", json!("grid"))], None, &o).verdict,
        Verdict::Failed
    );
    assert_eq!(
        evaluate(&[p("css.gap", json!("8px"))], None, &o).verdict,
        Verdict::Failed
    );
}

#[test]
fn a_node_that_cannot_be_found_again_is_inconclusive_never_verified() {
    let a = [geometry_eq("App.jsx:11:9#0", "App.jsx:9:7#0", 16.0)];
    // The button's key moved (an edit above it) and nothing else identifies it uniquely.
    let after = snap(vec![
        node("n1", None, Some("App.jsx:9:7#0"), "section", None, 10.0),
        node(
            "n7",
            Some("n1"),
            Some("App.jsx:12:9#0"),
            "button",
            None,
            26.0,
        ),
        node(
            "n8",
            Some("n1"),
            Some("App.jsx:13:9#0"),
            "button",
            None,
            26.0,
        ),
    ]);
    let r = evaluate(&a, Some(&key("App.jsx:11:9#0")), &observed(after));
    assert_eq!(r.verdict, Verdict::Inconclusive, "{r:#?}");
    let re = r
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::NodeReidentified)
        .unwrap();
    assert!(!re.passed);
}

#[test]
fn re_identification_falls_back_to_type_text_and_path_when_unique() {
    let before = page(10.0, 26.0);
    // The key changed (lines moved), but one button under a section still says "Pay now".
    let after = snap(vec![
        node("m1", None, Some("App.jsx:10:7#0"), "section", None, 10.0),
        node(
            "m2",
            Some("m1"),
            Some("App.jsx:12:9#0"),
            "button",
            Some("Pay now"),
            26.0,
        ),
    ]);
    let found = find(&key("App.jsx:11:9#0"), Some(&before), &after).unwrap();
    assert_eq!(found.id.0, "m2");
    // Two such buttons: no guess.
    let two = snap(vec![
        node("m1", None, None, "section", None, 10.0),
        node(
            "m2",
            Some("m1"),
            Some("A#0"),
            "button",
            Some("Pay now"),
            26.0,
        ),
        node(
            "m3",
            Some("m1"),
            Some("B#0"),
            "button",
            Some("Pay now"),
            40.0,
        ),
    ]);
    assert!(find(&key("App.jsx:11:9#0"), Some(&before), &two).is_none());
}

#[test]
fn build_and_runtime_errors_fail_pixels_are_skipped_and_nothing_stated_is_inconclusive() {
    let a = [geometry_eq("App.jsx:11:9#0", "App.jsx:9:7#0", 16.0)];
    let mut o = observed(page(10.0, 26.0));
    o.runtime_errors = vec!["TypeError: x is undefined".into()];
    let r = evaluate(&a, None, &o);
    assert_eq!(r.verdict, Verdict::Failed);
    let mut o = observed(page(10.0, 26.0));
    o.build_errors = Some(vec!["[vite] Internal server error".into()]);
    assert_eq!(evaluate(&a, None, &o).verdict, Verdict::Failed);
    let r = evaluate(&a, None, &observed(page(10.0, 26.0)));
    for k in [
        CheckKind::PixelsChangedInRegion,
        CheckKind::PixelsStableElsewhere,
    ] {
        let c = r.checks.iter().find(|c| c.kind == k).unwrap();
        assert!(c.skipped && !c.passed);
    }
    // No stated check: never verified.
    assert_eq!(
        evaluate(&[], None, &observed(page(10.0, 26.0))).verdict,
        Verdict::Inconclusive
    );
    // A page that did not settle: inconclusive.
    let mut o = observed(page(10.0, 26.0));
    o.unsettled = true;
    assert_eq!(evaluate(&a, None, &o).verdict, Verdict::Inconclusive);
}

// ---- through a fake adapter -------------------------------------------------------------------

/// A page whose button's left edge the test sets.
async fn fake_page(
    port: u16,
    token: &str,
    button_x: Arc<Mutex<f64>>,
) -> tokio::sync::mpsc::UnboundedSender<Value> {
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/?token={token}"))
        .await
        .unwrap();
    let (mut sink, mut source) = ws.split();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    tokio::spawn(async move {
        while let Some(v) = rx.recv().await {
            if sink.send(Message::text(v.to_string())).await.is_err() {
                break;
            }
        }
    });
    let out = tx.clone();
    tokio::spawn(async move {
        while let Some(Ok(Message::Text(t))) = source.next().await {
            let v: Value = serde_json::from_str(&t).unwrap();
            let Some(m) = v["method"].as_str() else {
                continue;
            };
            let x = *button_x.lock().unwrap();
            let result = match m {
                "inspect/hello" => json!({
                    "framework": "dom", "adapter": {"name": "fake", "version": "0"}, "tree": true,
                    "hitTest": true, "properties": true, "accessibility": false, "source": "lineColumn",
                    "highlight": true, "screenshot": false, "mutations": true
                }),
                "inspect/snapshot" => serde_json::to_value(page(10.0, x)).unwrap(),
                _ => json!({}),
            };
            let _ = out.send(json!({"jsonrpc": "2.0", "id": v["id"], "result": result}));
        }
    });
    tx
}

#[tokio::test]
async fn verify_through_a_page_after_a_hot_reload() {
    let root = tempfile::tempdir().unwrap();
    let inspect = Inspect::start().await.unwrap();
    let preview = PreviewId("p1".into());
    let token = inspect
        .register(preview.clone(), root.path().to_path_buf())
        .unwrap();
    let x = Arc::new(Mutex::new(10.0));
    let page_tx = fake_page(inspect.port(), &token, x.clone()).await;
    for _ in 0..100 {
        if inspect.session(&preview).is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let params = |offset| VerifyParams {
        preview_id: preview.clone(),
        assertions: vec![geometry_eq("App.jsx:11:9#0", "App.jsx:9:7#0", offset)],
        target: Some(key("App.jsx:11:9#0")),
        timeout_ms: Some(2000),
        since_ms: None,
    };
    // The patch: the button moves to 26 (16 px after the card), the page reloads.
    *x.lock().unwrap() = 26.0;
    page_tx
        .send(json!({"jsonrpc": "2.0", "method": "inspect/didMutate", "params": {"reason": "hotReload"}}))
        .unwrap();
    let (r, _) = verify(&inspect, &preview, &params(16.0), |_| Some(vec![]))
        .await
        .unwrap();
    assert_eq!(r.verdict, Verdict::Verified, "{r:#?}");
    // A wrong claim about the same page fails.
    let (r, _) = verify(&inspect, &preview, &params(24.0), |_| Some(vec![]))
        .await
        .unwrap();
    assert_eq!(r.verdict, Verdict::Failed);
    // A runtime error after the last verify fails the next one.
    page_tx
        .send(json!({"jsonrpc": "2.0", "method": "inspect/runtimeError", "params": {"message": "boom", "atMs": 0}}))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (r, _) = verify(&inspect, &preview, &params(16.0), |_| Some(vec![]))
        .await
        .unwrap();
    assert_eq!(r.verdict, Verdict::Failed);
    let rc = r
        .checks
        .iter()
        .find(|c| c.kind == CheckKind::RuntimeClean)
        .unwrap();
    assert!(rc.summary.contains("boom"));
}
