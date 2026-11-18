//! One test per numbered behaviour in docs/tasks/M1/M1-RPC-001.md.

use axiom_protocol::lifecycle::{
    CancelParams, Empty, HostOs, InitializeParams, InitializeResult, PeerInfo,
};
use axiom_protocol::{method, notification, ErrorKind, Method, RequestId};
use axiom_rpc::{serve, Client, Router, ServeError, ServerInfo};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{duplex, AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Msg {
    pub text: String,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Ms {
    pub ms: u64,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct N {
    pub n: u32,
}

method!(Echo, "test/echo", Msg => Msg);
method!(Sleep, "test/sleep", Ms => Msg);
method!(Boom, "test/boom", Empty => Empty);
method!(Fail, "test/fail", Empty => Empty);
method!(Stubborn, "test/stubborn", Empty => Msg);
method!(Cleanup, "test/cleanup", Empty => Msg);
method!(Start, "test/start", N => Msg);
method!(Flood, "test/flood", N => Empty);
notification!(Ping, "test/ping", N);

fn info() -> ServerInfo {
    ServerInfo {
        name: "test-server".into(),
        version: "9.9.9".into(),
    }
}

fn client_info() -> InitializeParams {
    InitializeParams {
        protocol_version: "0.1".into(),
        client: PeerInfo {
            name: "t".into(),
            version: "0".into(),
        },
    }
}

fn base_router() -> Router {
    let mut r = Router::new();
    r.method::<Echo, _, _>(|p, _| async move { Ok(p) });
    r.method::<Sleep, _, _>(|p, _| async move {
        tokio::time::sleep(Duration::from_millis(p.ms)).await;
        Ok(Msg {
            text: "slept".into(),
        })
    });
    r.method::<Boom, _, _>(|_, _| async move { panic!("handler exploded") });
    r.method::<Fail, _, _>(|_, _| async move {
        Err(axiom_protocol::RpcError::new(ErrorKind::NotFound, "nope"))
    });
    r
}

struct Harness {
    client: Client,
    server: tokio::task::JoinHandle<Result<(), ServeError>>,
}

fn connect(router: Router) -> Harness {
    let (c2s_r, c2s_w) = duplex(1 << 20);
    let (s2c_r, s2c_w) = duplex(1 << 20);
    let server = tokio::spawn(serve(c2s_r, s2c_w, Arc::new(router), info()));
    let client = Client::new(s2c_r, c2s_w);
    Harness { client, server }
}

async fn connect_init(router: Router) -> Harness {
    let h = connect(router);
    h.client
        .request::<axiom_protocol::lifecycle::Initialize>(client_info())
        .await
        .expect("initialize");
    h
}

fn kind(e: &axiom_protocol::RpcError) -> ErrorKind {
    assert_eq!(e.code, e.data.kind.code());
    e.data.kind
}

// 1 ------------------------------------------------------------------ codec

#[tokio::test]
async fn codec_accepts_crlf_and_ignores_empty_lines() {
    let (mut c_w, s_r) = duplex(1 << 16);
    let (s_w, c_r) = duplex(1 << 16);
    tokio::spawn(serve(s_r, s_w, Arc::new(base_router()), info()));
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"a","version":"b"}}}"#;
    let echo = r#"{"jsonrpc":"2.0","id":2,"method":"test/echo","params":{"text":"hi"}}"#;
    c_w.write_all(format!("\n\r\n{init}\r\n\n{echo}\r\n").as_bytes())
        .await
        .unwrap();
    let mut lines = BufReader::new(c_r).lines();
    let l1 = lines.next_line().await.unwrap().unwrap();
    let l2 = lines.next_line().await.unwrap().unwrap();
    assert!(l1.contains(r#""id":1"#) && l1.contains("result"), "{l1}");
    assert!(l2.contains(r#""id":2"#) && l2.contains("hi"), "{l2}");
    assert!(!l1.contains('\r') && !l2.contains('\r'));
}

#[tokio::test]
async fn codec_line_over_limit_closes_connection() {
    let (mut c_w, s_r) = duplex(1 << 20);
    let (s_w, _c_r) = duplex(1 << 20);
    let server = tokio::spawn(serve(s_r, s_w, Arc::new(base_router()), info()));
    let chunk = vec![b'a'; 1 << 20];
    // 64 MiB + 1 bytes without a newline.
    let writer = tokio::spawn(async move {
        for _ in 0..64 {
            if c_w.write_all(&chunk).await.is_err() {
                return;
            }
        }
        let _ = c_w.write_all(b"a").await;
        let _ = c_w.write_all(b"\n").await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    });
    let res = tokio::time::timeout(Duration::from_secs(60), server)
        .await
        .expect("server closes")
        .unwrap();
    assert!(matches!(res, Err(ServeError::LineTooLong)), "{res:?}");
    writer.abort();
}

// 2 ------------------------------------------------------- malformed input

#[tokio::test]
async fn malformed_lines_yield_null_id_errors_and_connection_stays_up() {
    let h = connect_init(base_router()).await;
    let mut orphans = h.client.orphan_errors();
    h.client.send_line("{not json").await;
    let e = tokio::time::timeout(Duration::from_secs(2), orphans.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kind(&e), ErrorKind::ParseError);
    h.client.send_line(r#"{"jsonrpc":"2.0","id":5}"#).await;
    h.client.send_line(r#"[1,2]"#).await;
    for _ in 0..2 {
        let e = tokio::time::timeout(Duration::from_secs(2), orphans.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kind(&e), ErrorKind::InvalidRequest);
    }
    let ok = h
        .client
        .request::<Echo>(Msg {
            text: "still up".into(),
        })
        .await
        .unwrap();
    assert_eq!(ok.text, "still up");
}

// 3 --------------------------------------------------- initialize gate

#[tokio::test]
async fn requests_before_initialize_fail_and_notifications_are_dropped() {
    let mut r = base_router();
    let seen = Arc::new(AtomicUsize::new(0));
    let s2 = seen.clone();
    r.method::<Start, _, _>(move |_, _| {
        let s = s2.clone();
        async move {
            s.fetch_add(1, Ordering::SeqCst);
            Ok(Msg {
                text: String::new(),
            })
        }
    });
    let h = connect(r);
    let e = h
        .client
        .request::<Echo>(Msg { text: "x".into() })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotInitialized);
    let e = h
        .client
        .request::<axiom_protocol::lifecycle::Shutdown>(Empty {})
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotInitialized);
    // A $/cancel before initialize is dropped without effect and without reply.
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams {
            id: RequestId::Number(1),
        })
        .await;
    h.client
        .request::<axiom_protocol::lifecycle::Initialize>(client_info())
        .await
        .unwrap();
    assert_eq!(seen.load(Ordering::SeqCst), 0);
}

// 4 ------------------------------------------------------------ initialize

#[tokio::test]
async fn initialize_reports_server_methods_and_host() {
    let h = connect(base_router());
    let res: InitializeResult = h
        .client
        .request::<axiom_protocol::lifecycle::Initialize>(client_info())
        .await
        .unwrap();
    assert_eq!(res.protocol_version, "0.1");
    assert_eq!(res.server.name, "test-server");
    assert_eq!(res.server.version, "9.9.9");
    let mut expected: Vec<String> = [
        "initialize",
        "shutdown",
        "test/boom",
        "test/echo",
        "test/fail",
        "test/sleep",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    expected.sort();
    assert_eq!(res.methods, expected);
    assert_eq!(res.methods, base_router().method_names());
    assert_eq!(res.host.arch, std::env::consts::ARCH);
    assert_eq!(
        res.host.path_separator,
        std::path::MAIN_SEPARATOR.to_string()
    );
    let os = match std::env::consts::OS {
        "macos" => HostOs::Macos,
        "windows" => HostOs::Windows,
        _ => HostOs::Linux,
    };
    assert_eq!(res.host.os, os);
}

#[tokio::test]
async fn initialize_negotiates_minor_and_rejects_major_mismatch() {
    let h = connect(base_router());
    let mut p = client_info();
    p.protocol_version = "1.0".into();
    let e = h
        .client
        .request::<axiom_protocol::lifecycle::Initialize>(p)
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Unsupported);
    assert_eq!(
        e.data.details.unwrap(),
        serde_json::json!({ "supported": axiom_protocol::PROTOCOL_VERSION })
    );
    // Still not initialized after a failed attempt; a newer minor negotiates down.
    let mut p = client_info();
    p.protocol_version = "0.7".into();
    let ok = h
        .client
        .request::<axiom_protocol::lifecycle::Initialize>(p)
        .await
        .unwrap();
    // Down to the server's minor (0.2 since M2-ARCH-001).
    assert_eq!(ok.protocol_version, axiom_protocol::PROTOCOL_VERSION);
}

#[tokio::test]
async fn second_initialize_is_invalid_request() {
    let h = connect_init(base_router()).await;
    let e = h
        .client
        .request::<axiom_protocol::lifecycle::Initialize>(client_info())
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidRequest);
}

// 5 -------------------------------------------------- dispatch and params

#[tokio::test]
async fn unknown_method_and_bad_params() {
    let h = connect_init(base_router()).await;
    let e = h
        .client
        .request_raw("nope/nothing", serde_json::json!({}))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::MethodNotFound);
    let e = h
        .client
        .request_raw(Echo::NAME, serde_json::json!({ "text": 5 }))
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
async fn missing_params_are_treated_as_empty_object() {
    let h = connect_init(base_router()).await;
    // `Empty {}` deserialises from `{}`; send the request with no params at all.
    let (c_w, s_r) = duplex(1 << 16);
    let (s_w, c_r) = duplex(1 << 16);
    drop(h);
    let mut c_w = c_w;
    tokio::spawn(serve(s_r, s_w, Arc::new(base_router()), info()));
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"a","version":"b"}}}"#;
    let fail = r#"{"jsonrpc":"2.0","id":2,"method":"test/fail"}"#;
    c_w.write_all(format!("{init}\n{fail}\n").as_bytes())
        .await
        .unwrap();
    let mut lines = BufReader::new(c_r).lines();
    lines.next_line().await.unwrap().unwrap();
    let l2 = lines.next_line().await.unwrap().unwrap();
    // Reached the handler (not_found from the handler), not invalid_params.
    assert!(l2.contains("not_found"), "{l2}");
}

// 6 ---------------------------------------------------------- concurrency

#[tokio::test]
async fn slow_handler_does_not_delay_fast_request() {
    let h = connect_init(base_router()).await;
    let start = Instant::now();
    let slow = h.client.request::<Sleep>(Ms { ms: 200 });
    let fast = async {
        tokio::time::sleep(Duration::from_millis(10)).await;
        let r = h
            .client
            .request::<Echo>(Msg {
                text: "fast".into(),
            })
            .await;
        (r, start.elapsed())
    };
    let ((fast_res, fast_elapsed), slow_res) = tokio::join!(fast, async {
        let r = slow.await;
        (r, start.elapsed())
    });
    assert_eq!(fast_res.unwrap().text, "fast");
    let (slow_res, slow_elapsed) = slow_res;
    assert_eq!(slow_res.unwrap().text, "slept");
    eprintln!("fast: {fast_elapsed:?}, slow: {slow_elapsed:?}");
    assert!(
        fast_elapsed < Duration::from_millis(150),
        "{fast_elapsed:?}"
    );
    assert!(fast_elapsed < slow_elapsed);
}

// 7 ----------------------------------------------------------- cancellation

#[tokio::test]
async fn cancel_in_flight_request_gives_cancelled_error() {
    let h = connect_init(base_router()).await;
    let (id, fut) = h.client.request_with_id::<Sleep>(Ms { ms: 5000 });
    let task = tokio::spawn(fut);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let start = Instant::now();
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams { id })
        .await;
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Cancelled);
    assert!(start.elapsed() < Duration::from_millis(1000));
}

#[tokio::test]
async fn cancel_aborts_handler_that_ignores_token_after_grace() {
    let mut r = base_router();
    r.method::<Stubborn, _, _>(|_, _| async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Ok(Msg {
            text: "late".into(),
        })
    });
    let h = connect_init(r).await;
    let (id, fut) = h.client.request_with_id::<Stubborn>(Empty {});
    let task = tokio::spawn(fut);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let start = Instant::now();
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams { id })
        .await;
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Cancelled);
    let took = start.elapsed();
    assert!(took >= Duration::from_millis(90), "{took:?}");
    assert!(took < Duration::from_millis(1000), "{took:?}");
}

#[tokio::test]
async fn cancelled_error_is_sent_even_if_handler_returns_ok_during_grace() {
    let mut r = base_router();
    let cleaned = Arc::new(AtomicUsize::new(0));
    let c2 = cleaned.clone();
    r.method::<Cleanup, _, _>(move |_, ctx| {
        let c = c2.clone();
        async move {
            ctx.cancel.cancelled().await;
            tokio::time::sleep(Duration::from_millis(20)).await;
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Msg {
                text: "completed anyway".into(),
            })
        }
    });
    let h = connect_init(r).await;
    let (id, fut) = h.client.request_with_id::<Cleanup>(Empty {});
    let task = tokio::spawn(fut);
    tokio::time::sleep(Duration::from_millis(50)).await;
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams { id })
        .await;
    let e = task.await.unwrap().unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Cancelled);
    assert_eq!(cleaned.load(Ordering::SeqCst), 1, "handler got to clean up");
}

#[tokio::test]
async fn cancel_after_completion_and_unknown_ids_are_ignored() {
    let h = connect_init(base_router()).await;
    let (id, fut) = h.client.request_with_id::<Echo>(Msg {
        text: "done".into(),
    });
    assert_eq!(fut.await.unwrap().text, "done");
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams { id })
        .await;
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams {
            id: RequestId::Number(999),
        })
        .await;
    // Connection still healthy.
    assert_eq!(
        h.client
            .request::<Echo>(Msg {
                text: "again".into()
            })
            .await
            .unwrap()
            .text,
        "again"
    );
}

// 8 ------------------------------------------------------------------ panic

#[tokio::test]
async fn handler_panic_becomes_internal_and_connection_survives() {
    let h = connect_init(base_router()).await;
    let e = h.client.request::<Boom>(Empty {}).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Internal);
    assert_eq!(
        h.client
            .request::<Echo>(Msg { text: "ok".into() })
            .await
            .unwrap()
            .text,
        "ok"
    );
}

// 9 ---------------------------------------------------------------- shutdown

#[tokio::test]
async fn shutdown_runs_hook_before_response_and_serve_returns_ok() {
    let mut r = base_router();
    let hooked = Arc::new(AtomicUsize::new(0));
    let h2 = hooked.clone();
    r.on_shutdown(move || {
        let h = h2.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            h.fetch_add(1, Ordering::SeqCst);
        }
    });
    let h = connect_init(r).await;
    h.client
        .request::<axiom_protocol::lifecycle::Shutdown>(Empty {})
        .await
        .unwrap();
    assert_eq!(
        hooked.load(Ordering::SeqCst),
        1,
        "hook ran before the response"
    );
    assert!(h.server.await.unwrap().is_ok());
}

#[tokio::test]
async fn eof_cancels_in_flight_runs_hook_and_returns_ok() {
    let mut r = base_router();
    let hooked = Arc::new(AtomicUsize::new(0));
    let h2 = hooked.clone();
    r.on_shutdown(move || {
        let h = h2.clone();
        async move {
            h.fetch_add(1, Ordering::SeqCst);
        }
    });
    let observed = Arc::new(AtomicUsize::new(0));
    let o2 = observed.clone();
    r.method::<Cleanup, _, _>(move |_, ctx| {
        let o = o2.clone();
        async move {
            ctx.cancel.cancelled().await;
            o.fetch_add(1, Ordering::SeqCst);
            Ok(Msg {
                text: String::new(),
            })
        }
    });
    let (c2s_r, mut c2s_w) = duplex(1 << 16);
    let (s2c_r, s2c_w) = duplex(1 << 16);
    let server = tokio::spawn(serve(c2s_r, s2c_w, Arc::new(r), info()));
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"a","version":"b"}}}"#;
    let req = r#"{"jsonrpc":"2.0","id":2,"method":"test/cleanup","params":{}}"#;
    c2s_w
        .write_all(format!("{init}\n{req}\n").as_bytes())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(c2s_w);
    let _keep = s2c_r;
    assert!(server.await.unwrap().is_ok());
    assert_eq!(hooked.load(Ordering::SeqCst), 1);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        observed.load(Ordering::SeqCst),
        1,
        "in-flight request was cancelled"
    );
}

// 10 ----------------------------------------------------------- after_response

fn ordering_router() -> Router {
    let mut r = base_router();
    r.method::<Start, _, _>(|p, ctx| async move {
        let notifier = ctx.notifier.clone();
        ctx.after_response(async move {
            let _ = notifier.notify::<Ping>(N { n: p.n }).await;
        });
        Ok(Msg {
            text: "started".into(),
        })
    });
    r.method::<Flood, _, _>(|_, ctx| async move {
        ctx.after_response(async { panic!("must never run") });
        Err(axiom_protocol::RpcError::new(ErrorKind::NotFound, "no"))
    });
    r
}

#[tokio::test]
async fn after_response_notifications_follow_the_response_on_the_wire() {
    // Raw wire view: read lines directly so ordering is observed unmediated.
    for i in 0..200u32 {
        let (mut c_w, s_r) = duplex(1 << 16);
        let (s_w, c_r) = duplex(1 << 16);
        tokio::spawn(serve(s_r, s_w, Arc::new(ordering_router()), info()));
        let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"a","version":"b"}}}"#;
        let start =
            format!(r#"{{"jsonrpc":"2.0","id":2,"method":"test/start","params":{{"n":{i}}}}}"#);
        c_w.write_all(format!("{init}\n{start}\n").as_bytes())
            .await
            .unwrap();
        let mut lines = BufReader::new(c_r).lines();
        lines.next_line().await.unwrap().unwrap(); // initialize response
        let first = lines.next_line().await.unwrap().unwrap();
        let second = lines.next_line().await.unwrap().unwrap();
        assert!(
            first.contains(r#""id":2"#) && first.contains("started"),
            "run {i}: {first}"
        );
        assert!(second.contains("test/ping"), "run {i}: {second}");
    }
}

#[tokio::test]
async fn after_response_is_dropped_on_error() {
    let h = connect_init(ordering_router()).await;
    let mut rx = h.client.notifications();
    let e = h.client.request::<Flood>(N { n: 0 }).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    let r = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;
    assert!(r.is_err(), "no notification expected");
    // Server still alive (the panicking future never ran).
    assert!(h
        .client
        .request::<Echo>(Msg { text: "x".into() })
        .await
        .is_ok());
}

#[tokio::test]
async fn after_response_is_dropped_on_cancel() {
    let mut r = base_router();
    r.method::<Cleanup, _, _>(|_, ctx| async move {
        let n = ctx.notifier.clone();
        ctx.after_response(async move {
            let _ = n.notify::<Ping>(N { n: 1 }).await;
        });
        ctx.cancel.cancelled().await;
        Ok(Msg { text: "ok".into() })
    });
    let h = connect_init(r).await;
    let mut rx = h.client.notifications();
    let (id, fut) = h.client.request_with_id::<Cleanup>(Empty {});
    let task = tokio::spawn(fut);
    tokio::time::sleep(Duration::from_millis(30)).await;
    h.client
        .notify::<axiom_protocol::lifecycle::Cancel>(CancelParams { id })
        .await;
    assert_eq!(
        kind(&task.await.unwrap().unwrap_err()),
        ErrorKind::Cancelled
    );
    assert!(tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .is_err());
}

// 11 -------------------------------------------------------------- writer

#[tokio::test]
async fn notify_applies_backpressure_when_peer_does_not_read() {
    let mut r = base_router();
    let sent = Arc::new(AtomicUsize::new(0));
    let sent2 = sent.clone();
    r.method::<Flood, _, _>(move |p, ctx| {
        let sent = sent2.clone();
        async move {
            let notifier = ctx.notifier.clone();
            ctx.after_response(async move {
                for n in 0..p.n {
                    if notifier.notify::<Ping>(N { n }).await.is_err() {
                        return;
                    }
                    sent.fetch_add(1, Ordering::SeqCst);
                }
            });
            Ok(Empty {})
        }
    });
    // Tiny pipe and a peer that does not read: the writer blocks, the
    // 1024-slot queue fills, and the notifier must suspend.
    let (mut c_w, s_r) = duplex(1 << 16);
    let (s_w, c_r) = duplex(256);
    tokio::spawn(serve(s_r, s_w, Arc::new(r), info()));
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"a","version":"b"}}}"#;
    let flood = r#"{"jsonrpc":"2.0","id":2,"method":"test/flood","params":{"n":5000}}"#;
    c_w.write_all(format!("{init}\n{flood}\n").as_bytes())
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    // Not reading yet: producer must be suspended well short of 5000
    // (queue capacity 1024 plus a few messages in flight in the writer/pipe).
    let stalled_at = sent.load(Ordering::SeqCst);
    assert!(
        (1000..1100).contains(&stalled_at),
        "stalled at {stalled_at}"
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(sent.load(Ordering::SeqCst), stalled_at, "still suspended");
    // Now read everything and verify all 5000 arrive.
    let mut lines = BufReader::new(c_r).lines();
    let mut pings = 0u32;
    let mut last = None;
    let deadline = Instant::now() + Duration::from_secs(20);
    while pings < 5000 && Instant::now() < deadline {
        let l = lines.next_line().await.unwrap().unwrap();
        if l.contains("test/ping") {
            let v: serde_json::Value = serde_json::from_str(&l).unwrap();
            let n = v["params"]["n"].as_u64().unwrap() as u32;
            if let Some(p) = last {
                assert_eq!(n, p + 1, "notifications stay in order");
            }
            last = Some(n);
            pings += 1;
        }
    }
    assert_eq!(pings, 5000, "producer resumed once the peer read");
}

#[tokio::test]
async fn notifier_reports_closed_after_connection_ends() {
    let mut r = base_router();
    let (txn, mut rxn) = tokio::sync::mpsc::channel::<axiom_rpc::Notifier>(1);
    r.method::<Start, _, _>(move |_, ctx| {
        let txn = txn.clone();
        async move {
            let _ = txn.send(ctx.notifier.clone()).await;
            Ok(Msg {
                text: String::new(),
            })
        }
    });
    let h = connect_init(r).await;
    h.client.request::<Start>(N { n: 0 }).await.unwrap();
    let notifier = rxn.recv().await.unwrap();
    h.client
        .request::<axiom_protocol::lifecycle::Shutdown>(Empty {})
        .await
        .unwrap();
    h.server.await.unwrap().unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        notifier.notify::<Ping>(N { n: 1 }).await,
        Err(axiom_rpc::Closed)
    );
}

#[test]
#[should_panic(expected = "built in")]
fn registering_builtin_method_panics() {
    let mut r = Router::new();
    r.method::<axiom_protocol::lifecycle::Shutdown, _, _>(|_, _| async { Ok(Empty {}) });
}

#[test]
#[should_panic(expected = "registered twice")]
fn registering_duplicate_method_panics() {
    let mut r = Router::new();
    r.method::<Echo, _, _>(|p, _| async move { Ok(p) });
    r.method::<Echo, _, _>(|p, _| async move { Ok(p) });
}
