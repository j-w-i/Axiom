//! A language server does not outlive a daemon that is killed (AUD-027): its process group is
//! registered with the daemon's exit watchdog when it starts.
#![cfg(unix)]

mod common;

use axiom_protocol::lsp::{Request, RequestParams};
use common::Daemon;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fake_lsp() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .to_path_buf();
    // Built once per test process (the tests run in parallel): a build per call could relink
    // the binary while another test's server is (re)starting, which then finds no fake-lsp.
    static BUILT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| {
        assert!(std::process::Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "fake-lsp"])
            .status()
            .unwrap()
            .success());
    });
    dir.join("fake-lsp")
}

fn alive(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

#[tokio::test]
async fn a_killed_daemon_takes_its_language_servers_with_it() {
    let data = tempfile::tempdir().unwrap();
    let pid_file = data.path().join("server.pid");
    let pdir = data.path().join("providers").join("fakelang");
    std::fs::create_dir_all(&pdir).unwrap();
    // The server writes its pid and runs fake-lsp; when fake-lsp ends (its input closes with
    // the daemon) the same process sleeps on, holding the output pipe: only the watchdog's
    // group kill ends it.
    let script = format!(
        "echo $$ > '{}'; '{}'; exec sleep 60",
        pid_file.display(),
        fake_lsp().display()
    );
    std::fs::write(
        pdir.join("provider.toml"),
        format!(
            r#"schema = "axiom.provider/1"
id = "fakelang"
name = "Fake language"
description = "Test provider with a language server."
version = "0.1.0"

[[lsp]]
id = "fake"
languages = ["fakelang"]
command = ["sh", "-c", {script:?}]
"#
        ),
    )
    .unwrap();
    let mut d = Daemon::ready_in(data.path()).await;
    let f = d.open_fixture().await;
    let r = d
        .client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "fake/echo".into(),
            params: json!({"n": 1}),
        })
        .await
        .unwrap();
    assert_eq!(r.result, json!({"n": 1}));
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(alive(pid));

    d.child.start_kill().unwrap();
    let _ = d.child.wait().await;
    let end = tokio::time::Instant::now() + Duration::from_secs(3);
    while alive(pid) && tokio::time::Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!alive(pid), "the language server {pid} outlived the daemon");
}
