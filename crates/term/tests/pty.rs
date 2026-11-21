//! Real PTYs (M4-TERM-001): an interactive shell, resize, the environment, exit codes, a
//! character split across writes, close ending the process group, and bounded memory.
#![cfg(unix)]

use axiom_protocol::WorkspaceId;
use axiom_term::{Spec, TermEvent, Terminals, BACKLOG_BYTES};
use std::collections::BTreeMap;
use std::time::Duration;

fn spec(dir: &std::path::Path, argv: &[&str]) -> Spec {
    let mut env: BTreeMap<String, String> = std::env::vars().collect();
    env.insert("AXIOM_TEST_MARK".into(), "from-the-project".into());
    env.insert("PS1".into(), "$ ".into());
    Spec {
        workspace_id: WorkspaceId("w1".into()),
        argv: Some(argv.iter().map(|s| s.to_string()).collect()),
        cwd: dir.to_path_buf(),
        env,
        cols: 80,
        rows: 24,
    }
}

/// Output until `want` appears (or the terminal exits), and the exit code if it did.
async fn until(
    rx: &mut tokio::sync::broadcast::Receiver<TermEvent>,
    want: &str,
) -> (String, Option<Option<i32>>) {
    let mut out = String::new();
    let r = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            match rx.recv().await {
                Ok(TermEvent::Output { data, .. }) => {
                    out.push_str(&data);
                    if !want.is_empty() && out.contains(want) {
                        return None;
                    }
                }
                Ok(TermEvent::Exited { code, .. }) => return Some(code),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(_) => return Some(None),
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {want:?}; got {out:?}"));
    (out, r)
}

#[tokio::test]
async fn an_interactive_shell_echoes_resizes_sees_the_environment_and_exits() {
    let dir = tempfile::tempdir().unwrap();
    let terms = Terminals::default();
    let mut rx = terms.subscribe();
    let t = terms.create(spec(dir.path(), &["sh", "-i"])).unwrap();
    assert!(t.info().running && t.pid.is_some());
    assert_eq!(t.title, "sh");
    t.write("echo $((6*7))\r").unwrap();
    until(&mut rx, "42").await;
    t.write("echo mark=$AXIOM_TEST_MARK term=$TERM\r").unwrap();
    until(&mut rx, "mark=from-the-project term=xterm-256color").await;
    t.write("pwd\r").unwrap();
    let canon = std::fs::canonicalize(dir.path()).unwrap();
    until(&mut rx, canon.file_name().unwrap().to_str().unwrap()).await;
    t.resize(123, 40).unwrap();
    t.write("stty size\r").unwrap();
    until(&mut rx, "40 123").await;
    // A 4-byte character written in two halves arrives whole.
    let face = "😀".as_bytes();
    t.write("printf '").unwrap();
    {
        // Through the shell's printf: the bytes as octal escapes, the first two then the rest.
        let esc = |b: &[u8]| b.iter().map(|x| format!("\\{:03o}", x)).collect::<String>();
        t.write(&format!(
            "{}' ; printf '{}\\n'\r",
            esc(&face[..2]),
            esc(&face[2..])
        ))
        .unwrap();
    }
    let (got, _) = until(&mut rx, "😀").await;
    assert!(!got.contains('\u{FFFD}'), "{got:?}");
    t.write("exit 7\r").unwrap();
    let (_, code) = until(&mut rx, "").await;
    assert_eq!(code, Some(Some(7)));
    assert!(!t.info().running);
    assert_eq!(t.info().exit_code, Some(7));
    let listed = t.listed();
    assert!(listed.backlog.contains("42") && listed.seq > 0);
}

#[tokio::test]
async fn closing_ends_the_whole_process_group() {
    let dir = tempfile::tempdir().unwrap();
    let terms = Terminals::default();
    let mut rx = terms.subscribe();
    let t = terms
        .create(spec(
            dir.path(),
            &["sh", "-c", "sleep 1000 & echo child=$!; wait"],
        ))
        .unwrap();
    let (out, _) = until(&mut rx, "\n").await;
    let child: i32 = out
        .split("child=")
        .nth(1)
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or_else(|| panic!("{out:?}"));
    terms.close(&t.id).unwrap();
    let (_, code) = until(&mut rx, "never").await;
    assert!(code.is_some(), "exited");
    // The background `sleep` (same process group) is gone too.
    let alive = |pid: i32| nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok();
    let end = std::time::Instant::now() + Duration::from_secs(5);
    while alive(child) && std::time::Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(!alive(child), "sleep {child} survived");
    assert!(terms.get(&t.id).is_err());
}

#[tokio::test]
async fn ten_megabytes_of_output_keep_the_backlog_bounded() {
    let dir = tempfile::tempdir().unwrap();
    let terms = Terminals::default();
    let mut rx = terms.subscribe();
    let t = terms
        .create(spec(
            dir.path(),
            &[
                "sh",
                "-c",
                "head -c 10485760 /dev/zero | tr '\\0' 'x'; echo; echo done",
            ],
        ))
        .unwrap();
    let mut events = 0u64;
    let code = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            match rx.recv().await {
                Ok(TermEvent::Output { .. }) => events += 1,
                Ok(TermEvent::Exited { code, .. }) => return code,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => events += n,
                Err(_) => return None,
            }
        }
    })
    .await
    .expect("finished");
    assert_eq!(code, Some(0));
    let l = t.listed();
    // Full (so the 10 MB went through) and no larger than the bound.
    assert!(
        l.backlog.len() <= BACKLOG_BYTES && l.backlog.len() > BACKLOG_BYTES - 64,
        "{}",
        l.backlog.len()
    );
    assert!(l.backlog.matches('x').count() > BACKLOG_BYTES - 64);
    assert!(l.backlog.trim_end().ends_with("done"));
    // Coalesced: far fewer events than 64 KiB reads would give at 1 per read without it is not
    // asserted (timing); every event was counted.
    assert!(events > 0 && l.seq >= 1);
}
