//! Smoke tests against the real `axiomd` binary.

use axiom_protocol::ErrorKind;
#[cfg(unix)]
use axiom_protocol::Message;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const INIT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"t","version":"0"}}}"#;
const SHUTDOWN: &str = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown"}"#;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_axiomd"))
}

#[test]
fn version_flag_prints_version() {
    let out = bin().arg("--version").output().unwrap();
    assert!(out.status.success());
    let s = String::from_utf8(out.stdout).unwrap();
    assert!(s.contains(env!("CARGO_PKG_VERSION")), "{s}");
}

#[test]
fn manual_session_prints_two_responses_and_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = bin()
        .env("AXIOM_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{INIT}").unwrap();
    writeln!(stdin, "{SHUTDOWN}").unwrap();
    stdin.flush().unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    println!("{stdout}");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "{stdout}");

    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["id"], 1);
    assert_eq!(v["result"]["server"]["name"], "axiomd");
    assert_eq!(v["result"]["server"]["version"], env!("CARGO_PKG_VERSION"));
    let methods: BTreeSet<String> = v["result"]["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    let expected: BTreeSet<String> = axiom_protocol::registry::method_names()
        .into_iter()
        .filter(|m| !axiomd::NOT_YET_SERVED.contains(m))
        .map(String::from)
        .collect();
    assert_eq!(methods, expected);

    let v2: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(v2["id"], 2);
    assert_eq!(v2["result"], serde_json::json!({}));
}

#[cfg(unix)]
#[test]
fn stdout_carries_only_valid_protocol_messages_even_with_debug_logging() {
    let dir = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(ws.path().join("a.txt"), "hi").unwrap();
    let mut child = bin()
        .args(["--log-level", "debug"])
        .env("AXIOM_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let open = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"workspace/open","params":{{"path":{}}}}}"#,
        serde_json::to_string(ws.path().to_str().unwrap()).unwrap()
    );
    let task = r#"{"jsonrpc":"2.0","id":4,"method":"task/start","params":{"workspaceId":"w1","command":{"argv":["sh","-c","echo out; echo err 1>&2"]}}}"#;
    for l in [
        INIT,
        &open,
        task,
        "{garbage",
        r#"{"jsonrpc":"2.0","id":5,"method":"settings/get","params":{}}"#,
    ] {
        writeln!(stdin, "{l}").unwrap();
    }
    stdin.flush().unwrap();
    // Let the task finish and notifications flush, then ask to shut down.
    std::thread::sleep(Duration::from_millis(800));
    writeln!(stdin, "{SHUTDOWN}").unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();
    let mut n = 0;
    for line in stdout.lines() {
        serde_json::from_str::<Message>(line)
            .unwrap_or_else(|e| panic!("stdout line is not a protocol message ({e}): {line}"));
        n += 1;
    }
    assert!(
        n >= 6,
        "expected responses and notifications, got {n}:\n{stdout}"
    );
}

#[test]
fn closing_stdin_without_shutdown_exits_zero_within_two_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = bin()
        .env("AXIOM_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(stdin, "{INIT}").unwrap();
    stdin.flush().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    out.read_line(&mut first).unwrap();
    assert!(first.contains("\"id\":1"));
    let t = Instant::now();
    drop(stdin);
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
}

#[test]
fn data_dir_flag_overrides_environment() {
    let env_dir = tempfile::tempdir().unwrap();
    let flag_dir = tempfile::tempdir().unwrap();
    let mut child = bin()
        .arg("--data-dir")
        .arg(flag_dir.path())
        .env("AXIOM_DATA_DIR", env_dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    drop(child.stdin.take());
    assert!(child.wait().unwrap().success());
    assert!(flag_dir.path().join("axiom.db").exists());
    assert!(!env_dir.path().join("axiom.db").exists());
}

#[test]
fn schema_too_new_exits_with_code_two_and_one_line() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("axiom.db");
    {
        // Create a database claiming a future schema, using the store's own file format.
        let s = axiom_store::Store::open(&db).unwrap();
        drop(s);
    }
    // Bump user_version via the sqlite CLI-free route: write the 4-byte header field.
    let mut bytes = std::fs::read(&db).unwrap();
    bytes[60..64].copy_from_slice(&999u32.to_be_bytes());
    std::fs::write(&db, bytes).unwrap();
    for ext in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.path().join(format!("axiom.db{ext}")));
    }
    let out = bin()
        .arg("--data-dir")
        .arg(dir.path())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).unwrap();
    assert_eq!(err.lines().count(), 1, "{err}");
    assert!(err.contains("999"));
    assert!(out.stdout.is_empty());
}

#[test]
fn request_before_initialize_is_rejected_by_the_real_binary() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = bin()
        .env("AXIOM_DATA_DIR", dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":9,"method":"settings/get","params":{{}}}}"#
    )
    .unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let line = String::from_utf8(out.stdout).unwrap();
    let v: serde_json::Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
    assert_eq!(
        v["error"]["data"]["kind"],
        serde_json::to_value(ErrorKind::NotInitialized).unwrap()
    );
}
