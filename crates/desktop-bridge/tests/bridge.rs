//! Behaviour of the bridge against the real `axiomd` binary (no webview).
//! Requires the daemon binary: `scripts/check.sh` and CI run
//! `cargo build -p axiomd` first. The path comes from `AXIOMD_UNDER_TEST` or
//! defaults to `<repo>/target/debug/axiomd`; the test fails (never skips) when
//! it is missing.

use axiom_desktop_bridge::{BridgeConfig, BridgeEvent, DaemonBridge, DaemonState};
use axiom_protocol::ErrorKind;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn daemon_path() -> PathBuf {
    if let Some(p) = std::env::var_os("AXIOMD_UNDER_TEST") {
        return PathBuf::from(p);
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.join("target/debug")
        .join(format!("axiomd{}", std::env::consts::EXE_SUFFIX))
}

struct Harness {
    bridge: DaemonBridge,
    events: Arc<Mutex<Vec<BridgeEvent>>>,
    _data: tempfile::TempDir,
}

fn harness_with(program: Option<PathBuf>, extra: impl FnOnce(&mut BridgeConfig)) -> Harness {
    let data = tempfile::tempdir().expect("tempdir");
    let mut config = BridgeConfig::new(program);
    config.args = vec![
        "--data-dir".into(),
        data.path().to_string_lossy().into_owned(),
    ];
    extra(&mut config);
    let events: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
    let sink_events = events.clone();
    let bridge = DaemonBridge::new(
        config,
        Arc::new(move |e| sink_events.lock().unwrap().push(e)),
    );
    Harness {
        bridge,
        events,
        _data: data,
    }
}

fn harness() -> Harness {
    let p = daemon_path();
    assert!(
        p.is_file(),
        "axiomd not built at {} (cargo build -p axiomd)",
        p.display()
    );
    harness_with(Some(p), |_| {})
}

async fn wait_until<F: Fn(&[BridgeEvent]) -> bool>(h: &Harness, within: Duration, f: F) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if f(&h.events.lock().unwrap()) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    false
}

fn response(events: &[BridgeEvent], key: u64) -> Option<(Option<Value>, Option<ErrorKind>)> {
    events.iter().find_map(|e| match e {
        BridgeEvent::Response {
            key: k,
            result,
            error,
        } if *k == key => Some((result.clone(), error.as_ref().map(|e| e.data.kind))),
        _ => None,
    })
}

fn response_count(events: &[BridgeEvent], key: u64) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, BridgeEvent::Response { key: k, .. } if *k == key))
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn connects_and_reports_ready() {
    let h = harness();
    let state = h.bridge.start().await;
    match state {
        DaemonState::Ready {
            server_version,
            protocol_version,
            methods,
        } => {
            assert!(!server_version.is_empty());
            assert_eq!(protocol_version, "0.1");
            assert!(methods.iter().any(|m| m == "fs/readFile"));
        }
        other => panic!("not ready: {other:?}"),
    }
    h.bridge.shutdown().await;
    assert_eq!(h.bridge.state().1, DaemonState::Stopped);
}

/// D-6: the daemon's stderr is forwarded as log events (the host prints them
/// with a `[axiomd]` prefix), not swallowed. A file where the database should
/// be makes axiomd write one deterministic line and exit.
#[tokio::test(flavor = "multi_thread")]
async fn daemon_stderr_is_forwarded_as_log_events() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    let not_a_dir = file.path().to_path_buf();
    let h = harness_with(Some(daemon_path()), |c| {
        c.args = vec![
            "--data-dir".into(),
            not_a_dir.to_string_lossy().into_owned(),
        ];
    });
    // The daemon exits at once, so start() may return before the exit is
    // observed: wait for the signal instead of racing it.
    let _ = h.bridge.start().await;
    let forwarded = wait_until(&h, Duration::from_secs(5), |events| {
        events
            .iter()
            .any(|e| matches!(e, BridgeEvent::Log { line } if line.starts_with("axiomd:")))
    })
    .await;
    assert!(
        forwarded,
        "no `axiomd:` stderr line among {:?}",
        h.events.lock().unwrap()
    );
    let lines: Vec<String> = h
        .events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            BridgeEvent::Log { line } => Some(line.clone()),
            _ => None,
        })
        .collect();
    // The same line is what a failure message quotes back to the user.
    assert!(
        lines.iter().any(|l| l.contains("cannot open the database")),
        "{lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_program_fails_clearly() {
    let h = harness_with(Some(PathBuf::from("/nonexistent/axiomd")), |_| {});
    let state = h.bridge.start().await;
    let DaemonState::Failed { message } = state else {
        panic!("expected failure")
    };
    assert!(message.contains("Could not start the backend"), "{message}");

    let h = harness_with(None, |_| {});
    assert!(matches!(h.bridge.start().await, DaemonState::Failed { .. }));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_requests_each_answered_once() {
    let h = harness();
    h.bridge.start().await;
    for key in 1..=200u64 {
        h.bridge
            .request_forward(key, "settings/get", json!({}), None);
    }
    assert!(
        wait_until(&h, Duration::from_secs(10), |ev| {
            (1..=200).all(|k| response(ev, k).is_some())
        })
        .await
    );
    let ev = h.events.lock().unwrap().clone();
    for key in 1..=200u64 {
        assert_eq!(response_count(&ev, key), 1, "key {key}");
        assert!(response(&ev, key).unwrap().0.is_some());
    }
    h.bridge.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_carry_their_kind() {
    let h = harness();
    h.bridge.start().await;
    let err = h
        .bridge
        .request(
            "fs/list",
            json!({ "workspaceId": "nope", "path": "" }),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(err.data.kind, ErrorKind::WorkspaceNotOpen);
    h.bridge.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn task_start_response_precedes_its_output_in_event_order() {
    let h = harness();
    h.bridge.start().await;
    let dir = tempfile::tempdir().unwrap();
    let ws = h
        .bridge
        .request(
            "workspace/open",
            json!({ "path": dir.path().to_string_lossy() }),
            None,
        )
        .await
        .unwrap();
    let id = ws["workspaceId"].as_str().unwrap().to_string();
    let shell = if cfg!(windows) {
        "echo hi & echo err 1>&2 & exit 3"
    } else {
        "echo hi; echo err >&2; exit 3"
    };
    h.bridge.request_forward(
        7,
        "task/start",
        json!({ "workspaceId": id, "command": { "shell": shell } }),
        None,
    );
    assert!(
        wait_until(&h, Duration::from_secs(10), |ev| ev.iter().any(
            |e| matches!(e, BridgeEvent::Notification { method, .. } if method == "task/exited")
        ))
        .await
    );
    let ev = h.events.lock().unwrap().clone();
    let resp_at = ev
        .iter()
        .position(|e| matches!(e, BridgeEvent::Response { key: 7, .. }))
        .unwrap();
    let first_out = ev
        .iter()
        .position(
            |e| matches!(e, BridgeEvent::Notification { method, .. } if method == "task/output"),
        )
        .unwrap();
    assert!(resp_at < first_out, "response must precede output");
    let exited = ev
        .iter()
        .find_map(|e| match e {
            BridgeEvent::Notification { method, params } if method == "task/exited" => {
                Some(params.clone())
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(exited["outcome"], "failed");
    assert_eq!(exited["exitCode"], 3);
    h.bridge.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_of_a_forwarded_request_still_answers_once() {
    let h = harness();
    h.bridge.start().await;
    // Cancelling a fast request: either the normal answer or `cancelled`, exactly once.
    h.bridge
        .request_forward(1, "workspace/recent", json!({}), None);
    h.bridge.cancel_forward(1);
    assert!(wait_until(&h, Duration::from_secs(5), |ev| response(ev, 1).is_some()).await);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let ev = h.events.lock().unwrap().clone();
    assert_eq!(response_count(&ev, 1), 1);
    let (result, kind) = response(&ev, 1).unwrap();
    assert!(result.is_some() || kind == Some(ErrorKind::Cancelled));
    h.bridge.shutdown().await;
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn timeout_answers_with_timeout_and_sends_cancel() {
    // A suspended daemon never answers: the bridge must time out, not hang.
    let h = harness_with(Some(daemon_path()), |c| {
        c.default_timeout = Duration::from_millis(300)
    });
    h.bridge.start().await;
    let pid = h.bridge.daemon_pid().unwrap();
    std::process::Command::new("kill")
        .args(["-STOP", &pid.to_string()])
        .status()
        .unwrap();
    h.bridge
        .request_forward(1, "workspace/recent", json!({}), None);
    assert!(wait_until(&h, Duration::from_secs(3), |ev| response(ev, 1).is_some()).await);
    assert_eq!(
        response(&h.events.lock().unwrap(), 1).unwrap().1,
        Some(ErrorKind::Timeout)
    );
    std::process::Command::new("kill")
        .args(["-CONT", &pid.to_string()])
        .status()
        .unwrap();
    h.bridge.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn crash_fails_pending_and_restart_recovers() {
    let h = harness();
    h.bridge.start().await;
    let pid = h.bridge.daemon_pid().unwrap();
    // Kill the daemon from outside, like a crash.
    #[cfg(unix)]
    std::process::Command::new("kill")
        .args(["-KILL", &pid.to_string()])
        .status()
        .unwrap();
    #[cfg(windows)]
    std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()
        .unwrap();
    assert!(
        wait_until(&h, Duration::from_secs(5), |_| matches!(
            h.bridge.state().1,
            DaemonState::Failed { .. }
        ))
        .await,
        "state after crash: {:?}",
        h.bridge.state()
    );
    let DaemonState::Failed { message } = h.bridge.state().1 else {
        unreachable!()
    };
    assert!(message.contains("backend"), "{message}");
    // A request now is answered at once with "Backend stopped".
    h.bridge
        .request_forward(9, "workspace/recent", json!({}), None);
    assert_eq!(
        response(&h.events.lock().unwrap(), 9).unwrap().1,
        Some(ErrorKind::Internal)
    );
    // Restart.
    let state = h.bridge.restart().await;
    assert!(matches!(state, DaemonState::Ready { .. }), "{state:?}");
    assert_ne!(h.bridge.daemon_pid(), Some(pid));
    assert!(h
        .bridge
        .request("workspace/recent", json!({}), None)
        .await
        .is_ok());
    h.bridge.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_is_bounded_and_the_process_exits() {
    let h = harness();
    h.bridge.start().await;
    let started = Instant::now();
    h.bridge.shutdown().await;
    assert!(started.elapsed() < Duration::from_secs(6));
    assert_eq!(h.bridge.state().1, DaemonState::Stopped);
    // No "failed" was reported for a deliberate shutdown.
    assert!(!h.events.lock().unwrap().iter().any(|e| matches!(
        e,
        BridgeEvent::State {
            state: DaemonState::Failed { .. },
            ..
        }
    )));
}

#[tokio::test(flavor = "multi_thread")]
async fn ui_callable_excludes_lifecycle() {
    assert!(axiom_desktop_bridge::ui_callable("fs/readFile"));
    assert!(!axiom_desktop_bridge::ui_callable("initialize"));
    assert!(!axiom_desktop_bridge::ui_callable("shutdown"));
    assert!(!axiom_desktop_bridge::ui_callable("evil/method"));
}

/// Regression: Tauri commands call `request_forward` from the main thread,
/// outside any Tokio context. That must not panic ("no reactor running").
#[test]
fn request_forward_from_a_thread_without_a_runtime() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let data = tempfile::tempdir().unwrap();
    let mut config = BridgeConfig::new(Some(daemon_path()));
    config.args = vec![
        "--data-dir".into(),
        data.path().to_string_lossy().into_owned(),
    ];
    let events: Arc<Mutex<Vec<BridgeEvent>>> = Arc::default();
    let sink = events.clone();
    let bridge = DaemonBridge::with_runtime(
        rt.handle().clone(),
        config,
        Arc::new(move |e| sink.lock().unwrap().push(e)),
    );
    rt.block_on(bridge.start());
    let b = bridge.clone();
    std::thread::spawn(move || b.request_forward(1, "workspace/recent", json!({}), None))
        .join()
        .expect("request_forward panicked outside a runtime");
    let deadline = Instant::now() + Duration::from_secs(5);
    while response(&events.lock().unwrap(), 1).is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(response(&events.lock().unwrap(), 1).unwrap().0.is_some());
    rt.block_on(bridge.shutdown());
}

#[test]
fn events_serialize_with_camel_case_fields() {
    let v = serde_json::to_value(BridgeEvent::State {
        generation: 1,
        state: DaemonState::Ready {
            server_version: "0.1.0".into(),
            protocol_version: "0.1".into(),
            methods: vec![],
        },
    })
    .unwrap();
    assert_eq!(v["type"], "state");
    assert_eq!(v["state"]["state"], "ready");
    assert_eq!(v["state"]["serverVersion"], "0.1.0");
    assert_eq!(v["state"]["protocolVersion"], "0.1");
}
