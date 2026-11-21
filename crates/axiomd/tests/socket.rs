//! The local socket (M3-DAEMON-001): a second client shares the daemon's workspaces and tasks,
//! needs the token, and its disconnect does not stop the daemon's work. Unix only.
#![cfg(unix)]

mod common;

use axiom_protocol::lifecycle::{Initialize, InitializeParams, PeerInfo};
use axiom_protocol::task::{Command, ListTasks, ListTasksParams, Start, StartParams};
use axiom_protocol::workspace::{Open, OpenParams};
use axiom_protocol::ErrorKind;
use axiom_rpc::Client;
use common::{kind, Daemon};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

fn init(token: Option<String>) -> InitializeParams {
    InitializeParams {
        protocol_version: "0.3".into(),
        client: PeerInfo {
            name: "socket-test".into(),
            version: "0".into(),
        },
        auth_token: token,
        capabilities: None,
    }
}

async fn wait_for(p: &Path) {
    for _ in 0..100 {
        if p.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} never appeared", p.display());
}

async fn socket_client(data: &Path) -> Client {
    let run = data.join("run");
    let stream = tokio::net::UnixStream::connect(run.join("axiomd.sock"))
        .await
        .unwrap();
    let (r, w) = stream.into_split();
    Client::new(r, w)
}

fn token(data: &Path) -> String {
    std::fs::read_to_string(data.join("run/axiomd.token")).unwrap()
}

#[tokio::test]
async fn socket_clients_share_the_daemon_and_need_the_token() {
    let d = Daemon::ready().await;
    let data = d.data_dir().to_path_buf();
    wait_for(&data.join("run/axiomd.token")).await;

    // Permissions: the directory 0700, the socket and token 0600.
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&data.join("run")), 0o700);
    assert_eq!(mode(&data.join("run/axiomd.sock")), 0o600);
    assert_eq!(mode(&data.join("run/axiomd.token")), 0o600);

    // No token, or a wrong one: refused and closed.
    for t in [None, Some("0".repeat(64))] {
        let c = socket_client(&data).await;
        let e = c.request::<Initialize>(init(t)).await.unwrap_err();
        assert_eq!(kind(&e), ErrorKind::PermissionDenied);
        let after = c
            .request::<Initialize>(init(Some(token(&data))))
            .await
            .unwrap_err();
        assert_eq!(after.data.kind, ErrorKind::Io, "the connection is closed");
    }

    // With the token: the same workspace registry and executor as the stdio client.
    let f = d.open_fixture().await;
    let s = socket_client(&data).await;
    s.request::<Initialize>(init(Some(token(&data))))
        .await
        .unwrap();
    let opened = s
        .request::<Open>(OpenParams {
            path: f.root.to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
    assert_eq!(opened.workspace_id, f.id, "one registry: the same id");
    let started = s
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            command: Command::Argv(vec!["sleep".into(), "30".into()]),
            cwd: None,
            env: Default::default(),
            label: None,
        })
        .await
        .unwrap();
    let list = d
        .client
        .request::<ListTasks>(ListTasksParams {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(list.tasks.iter().any(|t| t.task_id == started.task_id));

    // The socket client goes away: the daemon and its tasks keep going.
    drop(s);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let list = d
        .client
        .request::<ListTasks>(ListTasksParams {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    let t = list
        .tasks
        .iter()
        .find(|t| t.task_id == started.task_id)
        .unwrap();
    assert!(t.exit.is_none(), "still running: {t:?}");

    // A second daemon on the same data dir leaves the socket to the first.
    let first_token = token(&data);
    let d2 = Daemon::ready_in(&data).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(token(&data), first_token);
    d2.shutdown().await;
    assert!(
        data.join("run/axiomd.sock").exists(),
        "the second daemon did not remove it"
    );

    // The owner removes its files when it exits.
    d.shutdown().await;
    assert!(!data.join("run/axiomd.sock").exists());
    assert!(!data.join("run/axiomd.token").exists());
}

#[tokio::test]
async fn socket_stale_file_is_replaced() {
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(data.path().join("run")).unwrap();
    // A socket file with no listener (a daemon that was killed).
    let l = std::os::unix::net::UnixListener::bind(data.path().join("run/axiomd.sock")).unwrap();
    drop(l);
    let d = Daemon::ready_in(data.path()).await;
    wait_for(&data.path().join("run/axiomd.token")).await;
    let s = socket_client(data.path()).await;
    s.request::<Initialize>(init(Some(token(data.path()))))
        .await
        .unwrap();
    d.shutdown().await;
}
