//! The `axiom` binary against the real daemon (M3-DAEMON-001).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The daemon binary next to `axiom` in the target dir (built here if a narrow `cargo test -p
/// axiom-cli` has not built it).
fn axiomd() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiom"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join(if cfg!(windows) {
        "axiomd.exe"
    } else {
        "axiomd"
    });
    if !p.exists() {
        let ok = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "axiomd"])
            .status()
            .unwrap()
            .success();
        assert!(ok, "cannot build axiomd");
    }
    p
}

fn axiom(data: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_axiom"));
    c.env("AXIOM_DATA_DIR", data).env("AXIOM_DAEMON", axiomd());
    c
}

#[cfg(unix)]
#[test]
fn cli_task_run_streams_output_and_exits_with_the_code() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let out = axiom(data.path())
        .args(["task", "run", "--workspace"])
        .arg(ws.path())
        .args(["--", "sh", "-c", "echo hello; echo oops >&2; exit 3"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hello\n");
    assert!(String::from_utf8_lossy(&out.stderr).contains("oops"));
    let ok = axiom(data.path())
        .args(["task", "run", "--workspace"])
        .arg(ws.path())
        .args(["--", "true"])
        .status()
        .unwrap();
    assert!(ok.success());
}

#[test]
fn cli_agent_send_streams_a_scripted_answer_without_a_workspace() {
    let data = tempfile::tempdir().unwrap();
    let script = data.path().join("script.json");
    std::fs::write(
        &script,
        r#"{"turns":[{"expect":{"userContains":"hello","noTools":true},"events":[{"type":"text","text":"Hi there, from the script."}]}]}"#,
    )
    .unwrap();
    let out = axiom(data.path())
        .env("AXIOM_TEST_SCRIPTED_MODEL", &script)
        .args(["agent", "send", "--chat", "hello"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("Hi there, from the script."), "{stdout}");
    let list = axiom(data.path())
        .env("AXIOM_TEST_SCRIPTED_MODEL", &script)
        .args(["agent", "sessions", "--chat"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("\thello"),
        "{}",
        String::from_utf8_lossy(&list.stdout)
    );
}

/// With a daemon listening on the data dir (as the desktop app's does), the CLI uses it: the
/// task it starts is the shared daemon's.
#[cfg(unix)]
#[test]
fn cli_uses_a_listening_daemon() {
    use std::io::{BufRead, BufReader, Write};
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let mut d = Command::new(axiomd())
        .env("AXIOM_DATA_DIR", data.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = d.stdin.take().unwrap();
    let mut stdout = BufReader::new(d.stdout.take().unwrap());
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"0.3","client":{{"name":"t","version":"0"}}}}}}"#
    )
    .unwrap();
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    for _ in 0..100 {
        if data.path().join("run/axiomd.token").exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let out = axiom(data.path())
        // No AXIOM_DAEMON fallback should be needed: prove it by pointing it nowhere.
        .env("AXIOM_DAEMON", "/nonexistent/axiomd")
        .args(["task", "run", "--workspace"])
        .arg(ws.path())
        .args(["--", "sh", "-c", "echo shared"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "shared\n");
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"shutdown"}}"#).unwrap();
    let _ = d.wait();
}
