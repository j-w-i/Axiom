//! The language-server supervisor (`axiom-lsp`) against tests/helpers/fake-lsp (M4-LSP-001).
//! Here rather than in `crates/lsp` because the helper binary is found next to `axiomd`.
// Unix only for now: Windows support is deferred.
#![cfg(unix)]

use axiom_lsp::{Definition, Event, LspError, Manager};
use axiom_protocol::lsp::ServerState;
use axiom_protocol::WorkspaceId;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

fn fake_lsp() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join(if cfg!(windows) {
        "fake-lsp.exe"
    } else {
        "fake-lsp"
    });
    // Always built: a no-op when up to date, and a binary left from before a change to the
    // helper would not have it.
    assert!(std::process::Command::new(env!("CARGO"))
        .args(["build", "-q", "-p", "fake-lsp"])
        .status()
        .unwrap()
        .success());
    p
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    log: PathBuf,
    manager: Arc<Manager>,
    ws: WorkspaceId,
}

fn launch(root: &Path, log: &Path, extra: &[(&str, String)], settings: Value) -> axiom_lsp::Launch {
    let mut env = BTreeMap::new();
    let exe = fake_lsp();
    env.insert(
        "PATH".into(),
        exe.parent().unwrap().to_string_lossy().into_owned(),
    );
    env.insert("FAKE_LSP_LOG".into(), log.to_string_lossy().into_owned());
    for (k, v) in extra {
        env.insert((*k).into(), v.clone());
    }
    axiom_lsp::Launch {
        root: root.to_path_buf(),
        env,
        settings,
        initialization_options: None,
        blocked: None,
        install_provider: None,
        restriction: Default::default(),
    }
}

fn def(command: &str) -> Definition {
    Definition {
        id: "fake".into(),
        provider_id: "test".into(),
        languages: vec!["rust".into()],
        command: vec![command.into()],
        initialization_options: None,
        install_hint: Some("Install the fake server.".into()),
        requires: vec![],
    }
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("w");
    std::fs::create_dir(&root).unwrap();
    let log = dir.path().join("log.txt");
    Fixture {
        _dir: dir,
        root,
        log,
        manager: Arc::new(Manager::default()),
        ws: WorkspaceId("w1".into()),
    }
}

fn uri(root: &Path, name: &str) -> String {
    axiom_lsp::file_uri(&root.join(name))
}

async fn next_event(
    rx: &mut tokio::sync::broadcast::Receiver<Event>,
    pred: impl Fn(&Event) -> bool,
) -> Event {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let e = rx.recv().await.unwrap();
            if pred(&e) {
                return e;
            }
        }
    })
    .await
    .expect("the event arrived")
}

fn open(uri: &str, text: &str, version: i64) -> Value {
    json!({"textDocument":{"uri":uri,"languageId":"rust","version":version,"text":text}})
}

#[tokio::test]
async fn requests_from_many_clients_share_one_server() {
    let f = fixture();
    let s = f.manager.server(
        &f.ws,
        def("fake-lsp"),
        launch(&f.root, &f.log, &[], json!({})),
    );
    assert_eq!(s.state(), ServerState::Idle);
    let none = CancellationToken::new();
    // Twenty concurrent requests (two clients' worth), each with its own answer.
    let mut tasks = Vec::new();
    for i in 0..20 {
        let s = s.clone();
        let none = none.clone();
        tasks.push(tokio::spawn(async move {
            s.request("fake/echo", json!({"n": i}), &none).await
        }));
    }
    for (i, t) in tasks.into_iter().enumerate() {
        assert_eq!(t.await.unwrap().unwrap(), json!({"n": i}));
    }
    assert_eq!(s.state(), ServerState::Ready);
    assert_eq!(s.info().capabilities.unwrap()["hoverProvider"], true);
    // A server error keeps its code and data.
    match s.request("fake/fail", json!({}), &none).await {
        Err(LspError::Server {
            code,
            message,
            data,
        }) => {
            assert_eq!((code, message.as_str()), (-32001, "it failed"));
            assert_eq!(data, Some(json!({"why":"asked"})));
        }
        other => panic!("{other:?}"),
    }
    s.stop().await;
    assert_eq!(s.state(), ServerState::Stopped);
    let log = std::fs::read_to_string(&f.log).unwrap();
    assert_eq!(log.matches("initialize\n").count(), 1, "{log}");
    assert!(log.contains("shutdown\nexit\n"), "{log}");
}

#[tokio::test]
async fn documents_diagnostics_and_server_requests() {
    let f = fixture();
    let mut rx = f.manager.subscribe();
    let s = f.manager.server(
        &f.ws,
        def("fake-lsp"),
        launch(
            &f.root,
            &f.log,
            &[],
            json!({"fake": {"answer": 42}, "other": true}),
        ),
    );
    let a = uri(&f.root, "a.rs");
    // didOpen on an idle server starts it; the document reaches it once it is ready.
    s.notify(
        "textDocument/didOpen",
        open(&a, "fn a() {}\nlet ERROR = 1;\n", 1),
    )
    .await
    .unwrap();
    let e = next_event(&mut rx, |e| matches!(e, Event::Notification { method, .. } if method == "textDocument/publishDiagnostics")).await;
    let Event::Notification {
        params,
        workspace_id,
        server_id,
        ..
    } = e
    else {
        unreachable!()
    };
    assert_eq!(
        (workspace_id.0.as_str(), server_id.as_str()),
        ("w1", "fake")
    );
    assert_eq!(params["diagnostics"][0]["range"]["start"]["line"], 1);
    assert_eq!(s.diagnostics()[&a][0]["message"], "an ERROR here");
    // A full change clears it; the mirror follows.
    s.notify(
        "textDocument/didChange",
        json!({"textDocument":{"uri":a,"version":2},"contentChanges":[{"text":"fn a() {}\n"}]}),
    )
    .await
    .unwrap();
    next_event(
        &mut rx,
        |e| matches!(e, Event::Notification { params, .. } if params["version"] == 2),
    )
    .await;
    assert!(s.diagnostics().is_empty());
    assert_eq!(s.document(&a).unwrap().text, "fn a() {}\n");
    let none = CancellationToken::new();
    let hover = s
        .request(
            "textDocument/hover",
            json!({"textDocument":{"uri":a},"position":{"line":0,"character":0}}),
            &none,
        )
        .await
        .unwrap();
    assert_eq!(hover["contents"]["value"], "version 2");
    // The server asks the client for configuration: answered from the settings.
    let cfg = s.request("fake/config", json!({}), &none).await.unwrap();
    assert_eq!(cfg, json!([42, {"fake": {"answer": 42}, "other": true}]));
    // applyEdit is refused.
    let applied = s.request("fake/apply", json!({}), &none).await.unwrap();
    assert_eq!(applied["applied"], false);
    s.stop().await;
}

#[tokio::test]
async fn cancelling_a_request_cancels_it_on_the_server() {
    let f = fixture();
    let s = f.manager.server(
        &f.ws,
        def("fake-lsp"),
        launch(&f.root, &f.log, &[], json!({})),
    );
    let a = uri(&f.root, "slow.rs");
    s.notify("textDocument/didOpen", open(&a, "SLOW\n", 1))
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        c2.cancel();
    });
    let r = s
        .request(
            "textDocument/hover",
            json!({"textDocument":{"uri":a},"position":{"line":0,"character":0}}),
            &cancel,
        )
        .await;
    assert!(matches!(r, Err(LspError::Cancelled)), "{r:?}");
    // The server got $/cancelRequest and is still usable.
    let none = CancellationToken::new();
    assert_eq!(
        s.request("fake/echo", json!(1), &none).await.unwrap(),
        json!(1)
    );
    assert!(std::fs::read_to_string(&f.log)
        .unwrap()
        .contains("$/cancelRequest"));
    s.stop().await;
}

#[tokio::test]
async fn a_crash_restarts_and_reopens_documents_until_the_limit() {
    let f = fixture();
    let s = f.manager.server(
        &f.ws,
        def("fake-lsp"),
        launch(&f.root, &f.log, &[], json!({})),
    );
    let a = uri(&f.root, "a.rs");
    s.notify("textDocument/didOpen", open(&a, "fn a() {}\n", 1))
        .await
        .unwrap();
    let none = CancellationToken::new();
    for crash in 1..=3 {
        let r = s.request("fake/crash", json!({}), &none).await;
        assert!(matches!(r, Err(LspError::Exited(_))), "{r:?}");
        if crash < 3 {
            // Restarted, with the document open again.
            let hover = s
                .request(
                    "textDocument/hover",
                    json!({"textDocument":{"uri":a},"position":{"line":0,"character":0}}),
                    &none,
                )
                .await
                .unwrap();
            assert_eq!(hover["contents"]["value"], "version 1");
        }
    }
    // The third crash within 3 minutes leaves it failed.
    tokio::time::timeout(Duration::from_secs(10), async {
        while s.state() != ServerState::Failed {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("failed");
    assert!(s.info().detail.unwrap().contains("3 times"));
    let r = s.request("fake/echo", json!(1), &none).await;
    assert!(
        matches!(r, Err(LspError::Unavailable(ServerState::Failed, _))),
        "{r:?}"
    );
    let log = std::fs::read_to_string(&f.log).unwrap();
    assert_eq!(log.matches("initialize\n").count(), 3, "{log}");
    assert_eq!(log.matches("textDocument/didOpen\n").count(), 3, "{log}");
    // Restart takes it out of failed.
    s.restart().await;
    assert_eq!(
        s.request("fake/echo", json!(2), &none).await.unwrap(),
        json!(2)
    );
    s.stop().await;
}

#[tokio::test]
async fn a_missing_command_is_missing_with_its_hint() {
    let f = fixture();
    let mut rx = f.manager.subscribe();
    let s = f.manager.server(
        &f.ws,
        def("no-such-server"),
        launch(&f.root, &f.log, &[], json!({})),
    );
    let none = CancellationToken::new();
    let r = s.request("fake/echo", json!(1), &none).await;
    assert!(
        matches!(r, Err(LspError::Unavailable(ServerState::Missing, _))),
        "{r:?}"
    );
    let info = s.info();
    assert_eq!(info.state, ServerState::Missing);
    assert_eq!(
        info.install_hint.as_deref(),
        Some("Install the fake server.")
    );
    next_event(
        &mut rx,
        |e| matches!(e, Event::State { info, .. } if info.state == ServerState::Missing),
    )
    .await;
}

#[tokio::test]
async fn closing_the_workspace_stops_its_servers() {
    let f = fixture();
    let s = f.manager.server(
        &f.ws,
        def("fake-lsp"),
        launch(&f.root, &f.log, &[], json!({})),
    );
    let none = CancellationToken::new();
    s.request("fake/echo", json!(1), &none).await.unwrap();
    f.manager.close_workspace(&f.ws).await;
    assert_eq!(s.state(), ServerState::Stopped);
    assert!(f.manager.of_workspace(&f.ws).is_empty());
}

fn def_as(id: &str) -> Definition {
    Definition {
        id: id.into(),
        ..def(&fake_lsp().to_string_lossy())
    }
}

/// In a folder the person has not trusted, a built-in server that would run the project's
/// code starts with those features off and says so (AUD-008): here jdtls's Gradle and Maven
/// imports, in the settings it is answered with and its initialization options.
#[tokio::test]
async fn an_untrusted_folder_runs_a_restricted_server_with_its_features_off() {
    let f = fixture();
    let d = def_as("jdtls");
    let l = axiom_lsp::restrict(
        &d,
        launch(&f.root, &f.log, &[], json!({"fake": {"answer": 42}})),
    );
    let s = f.manager.server(&f.ws, d, l);
    let none = CancellationToken::new();
    let cfg = s.request("fake/config", json!({}), &none).await.unwrap();
    assert_eq!(cfg[0], 42, "the person's settings stay: {cfg}");
    assert_eq!(
        cfg[1]["java"]["import"]["gradle"]["enabled"], false,
        "{cfg}"
    );
    assert_eq!(cfg[1]["java"]["import"]["maven"]["enabled"], false, "{cfg}");
    assert_eq!(s.state(), ServerState::Ready);
    assert!(s.info().detail.unwrap().starts_with("Restricted"));
    s.stop().await;
}

/// A built-in server no setting keeps from running the project's code is not started in an
/// untrusted folder: it is `stopped`, every request says why at once (not after the 60 s start
/// timeout), and nothing runs; trusting the folder (a new launch) makes it idle and usable.
#[tokio::test]
async fn an_untrusted_folder_does_not_start_a_refused_server() {
    let f = fixture();
    let d = def_as("rust-analyzer");
    let l = axiom_lsp::restrict(&d, launch(&f.root, &f.log, &[], json!({})));
    let s = f.manager.server(&f.ws, d.clone(), l);
    let none = CancellationToken::new();
    let started = std::time::Instant::now();
    match s.request("fake/echo", json!({"n": 1}), &none).await {
        Err(LspError::Unavailable(ServerState::Stopped, Some(detail))) => {
            assert!(detail.starts_with(axiom_lsp::TRUST_TO_ENABLE), "{detail}")
        }
        other => panic!("{other:?}"),
    }
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(s
        .notify(
            "textDocument/didOpen",
            open(&uri(&f.root, "a.rs"), "x\n", 1)
        )
        .await
        .is_err());
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!f.log.exists(), "the server ran");
    let info = s.info();
    assert_eq!(info.state, ServerState::Stopped);
    assert!(info.detail.unwrap().starts_with(axiom_lsp::TRUST_TO_ENABLE));
    // Trusted: as configured.
    s.set_launch(launch(&f.root, &f.log, &[], json!({})));
    assert_eq!(s.state(), ServerState::Idle);
    assert_eq!(
        s.request("fake/echo", json!({"n": 2}), &none)
            .await
            .unwrap(),
        json!({"n": 2})
    );
    s.stop().await;
}
