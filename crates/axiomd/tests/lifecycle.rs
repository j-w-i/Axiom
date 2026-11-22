mod common;
use axiom_protocol::lifecycle::{Empty, Initialize, InitializeParams, PeerInfo, Shutdown};
use axiom_protocol::ErrorKind;
use common::*;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

#[tokio::test]
async fn request_before_initialize_is_not_initialized() {
    let d = Daemon::spawn().await;
    let e = d
        .client
        .request_raw("settings/get", serde_json::json!({}))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotInitialized);
    // ... and so is shutdown.
    let e = d.client.request::<Shutdown>(Empty {}).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotInitialized);
    // Still usable afterwards.
    d.initialize().await;
}

#[tokio::test]
async fn notifications_before_initialize_are_dropped() {
    let mut s = RawSession::spawn().await;
    // A cancel for an id that will later be in flight must have no effect.
    s.send(r#"{"jsonrpc":"2.0","method":"$/cancel","params":{"id":1}}"#)
        .await;
    s.initialize().await;
    s.send(r#"{"jsonrpc":"2.0","id":7,"method":"settings/get","params":{}}"#)
        .await;
    let v = s.next().await;
    assert_eq!(v["id"], 7);
    assert!(v["result"].is_object());
}

#[tokio::test]
async fn major_version_mismatch_is_unsupported_with_supported_detail() {
    let d = Daemon::spawn().await;
    let mut p = client_info();
    p.protocol_version = "1.0".into();
    let e = d.client.request::<Initialize>(p).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Unsupported);
    assert_eq!(
        e.data.details.unwrap()["supported"],
        axiom_protocol::PROTOCOL_VERSION
    );
}

#[tokio::test]
async fn negotiated_version_is_min_of_minors() {
    let d = Daemon::spawn().await;
    let r = d
        .client
        .request::<Initialize>(InitializeParams {
            protocol_version: "0.9".into(),
            client: PeerInfo {
                name: "t".into(),
                version: "0".into(),
            },
            auth_token: None,
            capabilities: None,
        })
        .await
        .unwrap();
    // The server's minor (0.2 since M2-ARCH-001) is below the client's 0.9.
    assert_eq!(r.protocol_version, axiom_protocol::PROTOCOL_VERSION);
}

#[tokio::test]
async fn second_initialize_is_invalid_request() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request::<Initialize>(client_info())
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidRequest);
}

#[tokio::test]
async fn initialize_lists_every_method_and_host_info() {
    let d = Daemon::spawn().await;
    let r = d.initialize().await;
    let got: BTreeSet<&str> = r.methods.iter().map(String::as_str).collect();
    let mut want: BTreeSet<&str> = axiom_protocol::registry::method_names()
        .into_iter()
        .filter(|m| !axiomd::NOT_YET_SERVED.contains(m))
        .collect();
    want.insert("initialize");
    want.insert("shutdown");
    assert_eq!(got, want);
    assert_eq!(r.server.name, "axiomd");
    assert_eq!(r.server.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(r.host.arch, std::env::consts::ARCH);
    assert_eq!(r.host.path_separator, std::path::MAIN_SEPARATOR.to_string());
}

#[tokio::test]
async fn shutdown_responds_then_process_exits_zero() {
    let d = Daemon::ready().await;
    let status = d.shutdown().await;
    assert_eq!(status.code(), Some(0));
}

#[tokio::test]
async fn eof_without_shutdown_exits_zero_within_two_seconds() {
    let mut s = RawSession::spawn().await;
    s.initialize().await;
    let t = Instant::now();
    s.close_stdin();
    let status = tokio::time::timeout(Duration::from_secs(2), s.child.wait())
        .await
        .expect("daemon must exit within 2 s of EOF")
        .unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[tokio::test]
async fn eof_kills_running_tasks() {
    use axiom_protocol::task::{Command, StartParams};
    let dir = tempfile::tempdir().unwrap();
    let mut s = RawSession::spawn().await;
    s.initialize().await;
    s.send(&format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"workspace/open","params":{{"path":{}}}}}"#,
        serde_json::to_string(dir.path().to_str().unwrap()).unwrap()
    ))
    .await;
    s.next().await;
    let start = StartParams {
        workspace_id: axiom_protocol::WorkspaceId("w1".into()),
        command: Command::Argv(vec!["sh".into(), "-c".into(), "echo $$; sleep 60".into()]),
        cwd: None,
        env: Default::default(),
        label: None,
    };
    let req = serde_json::json!({"jsonrpc":"2.0","id":2,"method":"task/start","params":start});
    s.send(&req.to_string()).await;
    let _resp = s.next().await;
    let out = s.next().await; // first task/output carries the shell pid
    let pid: i32 = out["params"]["data"]
        .as_str()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    s.close_stdin();
    tokio::time::timeout(Duration::from_secs(5), s.child.wait())
        .await
        .unwrap()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        // /proc is Linux specific; elsewhere use kill(0).
        let alive = std::path::Path::new(&format!("/proc/{pid}")).exists()
            && std::fs::read_to_string(format!("/proc/{pid}/stat"))
                .map(|s| !s.contains(") Z"))
                .unwrap_or(false);
        if !alive {
            break;
        }
        assert!(
            Instant::now() < end,
            "task process {pid} survived the daemon"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// Moved from `crates/desktop-bridge/tests/bridge.rs` when that crate was removed (AUD-102): the
// daemon-side lifecycle behaviours its client tests pinned and nothing here covered.

/// A daemon that cannot start explains itself in one `axiomd:` line on stderr (what a client
/// quotes back to the person) and exits non-zero, without touching stdout.
#[tokio::test]
async fn a_data_dir_that_is_a_file_fails_with_one_stderr_line() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let out = tokio::time::timeout(
        Duration::from_secs(10),
        axiomd()
            .arg("--data-dir")
            .arg(file.path())
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .expect("the daemon exits")
    .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "stdout is the protocol channel");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.lines()
            .any(|l| l.starts_with("axiomd:") && l.contains("cannot open the database")),
        "{err}"
    );
}

/// A daemon killed from outside (a crash) leaves its data dir usable: the next daemon starts on
/// it, answers, and takes over the local socket.
#[tokio::test]
async fn a_killed_daemon_can_be_restarted_on_the_same_data_dir() {
    let data = tempfile::tempdir().unwrap();
    let mut d = Daemon::ready_in(data.path()).await;
    let token = data.path().join("run/axiomd.token");
    for _ in 0..100 {
        if token.exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(token.exists());
    d.child.start_kill().unwrap();
    let _ = d.child.wait().await;
    let first = std::fs::read_to_string(&token).unwrap_or_default();
    let d2 = Daemon::ready_in(data.path()).await;
    d2.client
        .request::<axiom_protocol::workspace::Recent>(Empty {})
        .await
        .unwrap();
    let mut replaced = false;
    for _ in 0..100 {
        if std::fs::read_to_string(&token).is_ok_and(|t| !t.is_empty() && t != first) {
            replaced = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(replaced, "the new daemon did not take over the socket");
    assert_eq!(d2.shutdown().await.code(), Some(0));
}
