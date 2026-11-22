//! The inspection service against a fake adapter (a WebSocket client in the test), as the
//! page's adapter behaves (M5-INSPECT-001): the token gate, hello, snapshot, hit test and details
//! round trips, source resolution (exact stamps, owners below 0.6, text search, nothing outside
//! the workspace), notifications, and timeouts marking the session degraded.

use axiom_inspect::{Inspect, InspectError, InspectEvent};
use axiom_protocol::inspection::{
    HitTestParams, MappingMethod, NodeId, SnapshotParams, SourceRelation, SurfaceId,
};
use axiom_protocol::preview::PreviewId;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

fn caps() -> Value {
    json!({
        "framework": "react", "adapter": {"name": "fake", "version": "0"},
        "tree": true, "hitTest": true, "properties": true, "accessibility": false,
        "source": "lineColumn", "highlight": true, "screenshot": false, "mutations": true
    })
}

fn node(id: &str, parent: Option<&str>, stamp: Option<&str>, text: Option<&str>) -> Value {
    let mut n = json!({
        "id": id, "surface": "s0", "typeName": "div", "kind": "element",
        "visibility": {"visible": true},
        "bounds": {"x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0}
    });
    if let Some(p) = parent {
        n["parent"] = json!(p);
    }
    if let Some(t) = text {
        n["text"] = json!(t);
    }
    if let Some(s) = stamp {
        let (file, rest) = s.split_once(':').unwrap();
        let (line, col) = rest.split_once(':').unwrap();
        n["sources"] = json!([{
            "location": {"file": file, "line": line.parse::<u32>().unwrap(), "column": col.parse::<u32>().unwrap()},
            "relation": "instantiation", "method": "buildInstrumentation", "confidence": 1.0
        }]);
    }
    n
}

/// The nodes the fake page has: a stamped button, an unstamped child (a library's span), a
/// stamp outside the workspace, and an unstamped root with text found in the source.
fn page_nodes() -> Vec<Value> {
    vec![
        node("root", None, None, Some("Pay now please")),
        node("btn", Some("root"), Some("src/App.jsx:12:7"), None),
        node("lib", Some("btn"), None, None),
        node("evil", Some("root"), Some("../../etc/passwd:1:1"), None),
        node("abs", Some("root"), Some("/etc/hostname:1:1"), None),
    ]
}

/// A fake adapter: answers hello and the inspect methods; `silent` methods are never answered.
async fn adapter(
    port: u16,
    token: &str,
    silent: &'static [&'static str],
) -> tokio::sync::mpsc::UnboundedSender<Value> {
    let url = format!("ws://127.0.0.1:{port}/?token={token}");
    let (ws, _) = tokio_tungstenite::connect_async(url)
        .await
        .expect("connects");
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
            let Some(method) = v["method"].as_str() else {
                continue;
            };
            if silent.contains(&method) {
                continue;
            }
            let result = match method {
                "inspect/hello" => caps(),
                "inspect/snapshot" => json!({
                    "capturedMs": 1, "surfaces": [], "nodes": page_nodes(), "truncated": false
                }),
                "inspect/hitTest" => json!({"chain": [
                    page_nodes()[2].clone(), page_nodes()[1].clone(), page_nodes()[0].clone()
                ]}),
                "inspect/nodeDetails" => {
                    let id = v["params"]["node"].as_str().unwrap_or("root").to_string();
                    let mut n = page_nodes()
                        .into_iter()
                        .find(|n| n["id"] == id.as_str())
                        .unwrap_or_else(|| page_nodes()[0].clone());
                    n["properties"] = json!({"css.padding-left": "16px"});
                    n
                }
                _ => json!({}),
            };
            let _ = out.send(json!({"jsonrpc": "2.0", "id": v["id"], "result": result}));
        }
    });
    tx
}

async fn setup() -> (Arc<Inspect>, PreviewId, String, tempfile::TempDir) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("src")).unwrap();
    std::fs::write(
        root.path().join("src/App.jsx"),
        "export function App() {\n  return (\n    <main>\n      <p>Pay now please</p>\n",
    )
    .unwrap();
    let inspect = Inspect::start().await.unwrap();
    let preview = PreviewId("p1".into());
    let token = inspect
        .register(preview.clone(), root.path().to_path_buf())
        .unwrap();
    (inspect, preview, token, root)
}

async fn connected(inspect: &Inspect, preview: &PreviewId) {
    for _ in 0..100 {
        if inspect.session(preview).is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the adapter never became the session");
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (inspect, preview, _token, _root) = setup().await;
    let url = format!("ws://127.0.0.1:{}/?token=nope", inspect.port());
    let err = tokio_tungstenite::connect_async(url).await.unwrap_err();
    assert!(err.to_string().contains("401"), "{err}");
    let url = format!("ws://127.0.0.1:{}/", inspect.port());
    assert!(tokio_tungstenite::connect_async(url).await.is_err());
    assert_eq!(
        inspect.session(&preview).err(),
        Some(InspectError::AdapterUnavailable)
    );
}

#[tokio::test]
async fn snapshot_resolves_stamps_owners_and_drops_paths_outside() {
    let (inspect, preview, token, _root) = setup().await;
    let mut events = inspect.subscribe();
    let _a = adapter(inspect.port(), &token, &[]).await;
    connected(&inspect, &preview).await;
    let ev = events.recv().await.unwrap();
    assert!(matches!(ev, InspectEvent::Connected { .. }));
    assert!(inspect.capabilities(&preview).unwrap().tree);

    let snap = inspect
        .snapshot(
            &preview,
            SnapshotParams {
                root: None,
                max_nodes: None,
                include_invisible: false,
            },
        )
        .await
        .unwrap();
    let by = |id: &str| {
        snap.nodes
            .iter()
            .find(|n| n.id == NodeId(id.into()))
            .unwrap()
    };
    // The stamped element: exact file, line, column, confidence 1.
    let btn = &by("btn").sources[0];
    assert_eq!(btn.location.file.0, "src/App.jsx");
    assert_eq!((btn.location.line, btn.location.column), (12, Some(7)));
    assert_eq!(btn.method, MappingMethod::BuildInstrumentation);
    assert_eq!(btn.confidence, 1.0);
    // The unstamped child: its nearest stamped ancestor as the owner, below 0.6.
    let lib = &by("lib").sources;
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0].relation, SourceRelation::Owner);
    assert_eq!(lib[0].location.file.0, "src/App.jsx");
    assert!(lib[0].confidence < 0.6);
    // Outside the workspace: dropped, relative or absolute.
    assert!(by("evil").sources.is_empty());
    assert!(by("abs").sources.is_empty());
    assert!(inspect
        .session(&preview)
        .unwrap()
        .latest_snapshot()
        .is_some());
}

#[tokio::test]
async fn hit_test_and_details_round_trip_with_a_text_search_fallback() {
    let (inspect, preview, token, _root) = setup().await;
    let _a = adapter(inspect.port(), &token, &[]).await;
    connected(&inspect, &preview).await;
    let r = inspect
        .hit_test(
            &preview,
            HitTestParams {
                surface: SurfaceId("s0".into()),
                x: 5.0,
                y: 5.0,
            },
        )
        .await
        .unwrap();
    assert_eq!(r.chain.len(), 3);
    assert_eq!(r.chain[0].sources[0].relation, SourceRelation::Owner);
    assert_eq!(r.chain[1].sources[0].location.line, 12);

    // The root has no stamp, no stamped ancestor, but text that is in the source.
    let d = inspect
        .node_details(&preview, NodeId("root".into()))
        .await
        .unwrap();
    assert_eq!(d.properties["css.padding-left"], json!("16px"));
    let c = &d.sources[0];
    assert_eq!(c.method, MappingMethod::Heuristic);
    assert!(c.confidence <= 0.4);
    assert_eq!(
        (
            c.location.file.0.as_str(),
            c.location.line,
            c.location.column
        ),
        ("src/App.jsx", 4, Some(10))
    );

    // An unstamped element's details name its nearest stamped ancestor as the owner, as the
    // hit test does (not a text search).
    let d = inspect
        .node_details(&preview, NodeId("lib".into()))
        .await
        .unwrap();
    assert_eq!(d.sources.len(), 1, "{:?}", d.sources);
    assert_eq!(d.sources[0].relation, SourceRelation::Owner);
    assert_eq!(d.sources[0].location.line, 12);
    assert!(d.sources[0].confidence < 0.6);
}

#[tokio::test]
async fn notifications_become_events_and_mutations_end_quiescence() {
    let (inspect, preview, token, _root) = setup().await;
    let mut events = inspect.subscribe();
    let a = adapter(inspect.port(), &token, &[]).await;
    connected(&inspect, &preview).await;
    let _ = events.recv().await; // connected
    a.send(
        json!({"jsonrpc": "2.0", "method": "inspect/selected", "params": {"chain": [
            page_nodes()[2].clone(), page_nodes()[1].clone()
        ]}}),
    )
    .unwrap();
    match events.recv().await.unwrap() {
        InspectEvent::Selected { chain, .. } => {
            assert_eq!(chain.len(), 2);
            assert_eq!(chain[0].sources[0].relation, SourceRelation::Owner);
        }
        e => panic!("{e:?}"),
    }
    a.send(
        json!({"jsonrpc": "2.0", "method": "inspect/didMutate", "params": {"reason": "hotReload"}}),
    )
    .unwrap();
    assert!(matches!(
        events.recv().await.unwrap(),
        InspectEvent::Mutated { .. }
    ));
    let s = inspect.session(&preview).unwrap();
    assert_eq!(s.reloads_since(s.connected_ms()), 1);
    let t = std::time::Instant::now();
    assert!(
        s.quiescence(Duration::from_millis(150), Duration::from_secs(2))
            .await
    );
    assert!(t.elapsed() < Duration::from_millis(400));
    a.send(json!({"jsonrpc": "2.0", "method": "inspect/runtimeError", "params": {"message": "boom", "atMs": 1}}))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(s.errors_since(s.connected_ms()), vec!["boom".to_string()]);
}

#[tokio::test]
async fn a_silent_adapter_times_out_and_the_session_is_degraded() {
    let (inspect, preview, token, _root) = setup().await;
    let _a = adapter(inspect.port(), &token, &["inspect/highlight"]).await;
    connected(&inspect, &preview).await;
    let err = inspect.highlight(&preview, None).await.unwrap_err();
    assert_eq!(err, InspectError::Timeout(3));
    assert!(inspect.session(&preview).unwrap().is_degraded());
    // An answer clears it.
    inspect.set_mode(&preview, true).await.unwrap();
    assert!(!inspect.session(&preview).unwrap().is_degraded());
}

#[tokio::test]
async fn unregistering_ends_the_session_and_the_token() {
    let (inspect, preview, token, _root) = setup().await;
    let _a = adapter(inspect.port(), &token, &[]).await;
    connected(&inspect, &preview).await;
    inspect.unregister(&preview);
    assert_eq!(
        inspect.session(&preview).err(),
        Some(InspectError::AdapterUnavailable)
    );
    let url = format!("ws://127.0.0.1:{}/?token={token}", inspect.port());
    assert!(tokio_tungstenite::connect_async(url).await.is_err());
}
