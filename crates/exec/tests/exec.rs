//! Tests for axiom-exec. Unix commands use `sh`; Windows equivalents use `cmd /C`.

use axiom_exec::{Executor, SpawnSpec, TaskEvent};
use axiom_protocol::task::{Command, ExitedParams, Outcome, Stream};
use axiom_protocol::{ErrorKind, RpcError, TaskId, WorkspaceId};
use std::collections::BTreeMap;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;
use tokio::sync::mpsc::Receiver;

fn ws() -> WorkspaceId {
    WorkspaceId("w1".into())
}

fn spec(command: Command) -> SpawnSpec {
    SpawnSpec {
        command,
        cwd: std::env::temp_dir(),
        env: BTreeMap::new(),
        label: None,
    }
}

/// A script in the host's dialect: `sh -c` on Unix, `cmd /C` on Windows.
fn host(unix: &str, windows: &str) -> SpawnSpec {
    if cfg!(windows) {
        sh(windows)
    } else {
        sh(unix)
    }
}

fn sh(script: &str) -> SpawnSpec {
    #[cfg(unix)]
    return spec(Command::Argv(vec!["sh".into(), "-c".into(), script.into()]));
    #[cfg(windows)]
    return spec(Command::Argv(vec![
        "cmd".into(),
        "/C".into(),
        script.into(),
    ]));
}

struct Collected {
    events: Vec<TaskEvent>,
}

impl Collected {
    fn stream(&self, which: Stream) -> String {
        self.events
            .iter()
            .filter_map(|e| match e {
                TaskEvent::Output(o) if o.stream == which => Some(o.data.as_str()),
                _ => None,
            })
            .collect()
    }
    fn exited(&self) -> &ExitedParams {
        match self.events.last() {
            Some(TaskEvent::Exited(x)) => x,
            other => panic!("last event is not Exited: {other:?}"),
        }
    }
}

async fn collect(mut rx: Receiver<TaskEvent>) -> Collected {
    let mut events = Vec::new();
    let deadline = Duration::from_secs(60);
    loop {
        match tokio::time::timeout(deadline, rx.recv()).await {
            Ok(Some(e)) => {
                let done = matches!(e, TaskEvent::Exited(_));
                events.push(e);
                if done {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => panic!("timed out waiting for task events"),
        }
    }
    // Nothing may follow Exited and the channel must close.
    assert!(
        tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .is_none(),
        "events after Exited"
    );
    Collected { events }
}

async fn run(s: SpawnSpec) -> Collected {
    let ex = Executor::new();
    let (_, rx) = ex.start(ws(), s).await.unwrap();
    collect(rx).await
}

fn assert_contiguous_seq(c: &Collected, id: Option<&TaskId>) {
    let mut expect = 0;
    for e in &c.events {
        if let TaskEvent::Output(o) = e {
            assert_eq!(o.seq, expect);
            if let Some(id) = id {
                assert_eq!(&o.task_id, id);
            }
            expect += 1;
        }
    }
}

#[cfg(unix)]
fn pid_alive(pid: i32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // Signal 0 only checks existence. A zombie still counts as existing, but
    // grandchildren are re-parented and reaped by init/subreaper.
    kill(Pid::from_raw(pid), None).is_ok()
}

#[cfg(unix)]
async fn wait_until_dead(pid: i32) -> bool {
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if !pid_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test]
async fn captures_and_tags_stdout_and_stderr_and_exit_code() {
    let c = run(host(
        "echo out; echo err 1>&2; exit 3",
        "echo out& echo err 1>&2& exit 3",
    ))
    .await;
    assert!(c.stream(Stream::Stdout).contains("out"));
    assert!(c.stream(Stream::Stderr).contains("err"));
    assert!(!c.stream(Stream::Stdout).contains("err"));
    let x = c.exited();
    assert_eq!(x.status.outcome, Outcome::Failed);
    assert_eq!(x.status.exit_code, Some(3));
    assert_eq!(x.status.signal, None);
}

#[tokio::test]
async fn success_outcome_and_duration() {
    let c = run(sh("exit 0")).await;
    let x = c.exited();
    assert_eq!(x.status.outcome, Outcome::Succeeded);
    assert_eq!(x.status.exit_code, Some(0));
    assert!(x.status.duration_ms < 5000);
}

#[tokio::test]
async fn seq_is_contiguous_from_zero_and_exited_is_last() {
    let c = run(host(
        "for i in 1 2 3 4 5 6 7 8 9 10; do echo o$i; echo e$i 1>&2; done",
        "for /L %i in (1,1,10) do @(echo o%i& echo e%i 1>&2)",
    ))
    .await;
    assert!(c.events.len() >= 3);
    assert_contiguous_seq(&c, None);
    assert!(matches!(c.events.last(), Some(TaskEvent::Exited(_))));
    assert_eq!(
        c.events
            .iter()
            .filter(|e| matches!(e, TaskEvent::Exited(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn missing_program_is_spawn_failed_naming_program() {
    let ex = Executor::new();
    let err = ex
        .start(
            ws(),
            spec(Command::Argv(vec!["definitely-not-a-program-xyz".into()])),
        )
        .await
        .unwrap_err();
    match &err {
        axiom_exec::ExecError::SpawnFailed { program, .. } => {
            assert_eq!(program, "definitely-not-a-program-xyz")
        }
        e => panic!("{e:?}"),
    }
    let r: RpcError = err.into();
    assert_eq!(r.data.kind, ErrorKind::SpawnFailed);
    let d = r.data.details.unwrap();
    assert_eq!(d["program"], "definitely-not-a-program-xyz");
    assert!(d["osError"].as_str().is_some_and(|s| !s.is_empty()));
    // A failed start leaves no task behind.
    assert!(ex.list(&ws()).is_empty());
}

#[tokio::test]
async fn empty_command_and_missing_cwd() {
    let ex = Executor::new();
    let e = ex
        .start(ws(), spec(Command::Argv(vec![])))
        .await
        .unwrap_err();
    assert!(matches!(e, axiom_exec::ExecError::EmptyCommand));
    assert_eq!(RpcError::from(e).data.kind, ErrorKind::InvalidParams);

    let mut s = sh("exit 0");
    s.cwd = std::env::temp_dir().join("axiom-exec-no-such-dir-xyz");
    let e = ex.start(ws(), s).await.unwrap_err();
    assert!(matches!(e, axiom_exec::ExecError::CwdNotFound));
    assert_eq!(RpcError::from(e).data.kind, ErrorKind::NotFound);

    let e = ex.cancel(&TaskId("t999".into())).await.unwrap_err();
    assert!(matches!(e, axiom_exec::ExecError::NotFound));
    assert_eq!(RpcError::from(e).data.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn cwd_and_env_are_applied() {
    let dir = std::env::temp_dir().canonicalize().unwrap();
    // Windows canonical paths carry a `\\?\` prefix that `cd` never prints.
    let shown = dir
        .to_str()
        .unwrap()
        .trim_start_matches(r"\\?\")
        .to_string();
    let mut s = sh(if cfg!(windows) {
        "cd & echo %AXIOM_TEST_VAR%"
    } else {
        "pwd; echo $AXIOM_TEST_VAR"
    });
    s.cwd = dir.clone();
    s.env.insert("AXIOM_TEST_VAR".into(), "hello-env".into());
    let c = run(s).await;
    let out = c.stream(Stream::Stdout);
    assert!(out.contains("hello-env"), "{out}");
    assert!(out.contains(&shown), "{out}");
}

/// M2-ENV-005: a project environment reaches a task as a `PATH` in the env overlay. The
/// overlay's `PATH` decides which program an argv names (the child's PATH, not the daemon's).
#[tokio::test]
async fn env_overlay_path_selects_the_program() {
    let dir = std::env::temp_dir().join(format!("axiom-exec-overlay-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let program = if cfg!(windows) {
        "axiomoverlaydemo.bat"
    } else {
        "axiomoverlaydemo"
    };
    let file = dir.join(program);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(&file, "#!/bin/sh\necho from-overlay\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(windows)]
    std::fs::write(&file, "@echo from-overlay\r\n").unwrap();

    // Not on the daemon's PATH: without the overlay the program is not found.
    assert!(std::process::Command::new(program).output().is_err());

    let mut dirs = vec![dir.clone()];
    dirs.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let mut s = spec(Command::Argv(vec![program.into()]));
    s.env.insert(
        "PATH".into(),
        std::env::join_paths(dirs)
            .unwrap()
            .to_string_lossy()
            .into_owned(),
    );
    let c = run(s).await;
    assert_eq!(c.stream(Stream::Stdout).trim(), "from-overlay");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn shell_command_runs_through_the_shell() {
    let c = run(spec(Command::Shell("echo one && echo two".into()))).await;
    let out = c.stream(Stream::Stdout);
    assert!(out.contains("one") && out.contains("two"), "{out}");
    assert_eq!(c.exited().status.outcome, Outcome::Succeeded);
}

#[cfg(unix)]
#[tokio::test]
async fn one_mebibyte_of_output_arrives_complete_and_in_order() {
    // 1 MiB of a repeating, position-dependent pattern.
    let c = run(sh(
        "head -c 1048576 /dev/zero | tr '\\0' 'a'; head -c 1048576 /dev/zero | tr '\\0' 'b' 1>&2",
    ))
    .await;
    let out = c.stream(Stream::Stdout);
    let err = c.stream(Stream::Stderr);
    assert_eq!(out.len(), 1024 * 1024);
    assert!(out.bytes().all(|b| b == b'a'));
    assert_eq!(err.len(), 1024 * 1024);
    assert!(err.bytes().all(|b| b == b'b'));
    assert_contiguous_seq(&c, None);
    for e in &c.events {
        if let TaskEvent::Output(o) = e {
            assert!(o.data.len() <= 8 * 1024);
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn ordered_numbered_lines_survive_backpressure() {
    // Far more chunks than the 256-slot channel; the consumer is slow at first.
    let ex = Executor::new();
    let (_, mut rx) = ex
        .start(
            ws(),
            sh("i=0; while [ $i -lt 3000 ]; do echo line$i; i=$((i+1)); done"),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let mut text = String::new();
    let mut last_seq = None;
    while let Some(e) = rx.recv().await {
        match e {
            TaskEvent::Output(o) => {
                if let Some(p) = last_seq {
                    assert_eq!(o.seq, p + 1);
                }
                last_seq = Some(o.seq);
                text.push_str(&o.data);
            }
            TaskEvent::Exited(x) => {
                assert_eq!(x.status.outcome, Outcome::Succeeded);
                break;
            }
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3000);
    for (i, l) in lines.iter().enumerate() {
        assert_eq!(*l, format!("line{i}"));
    }
}

#[cfg(unix)]
#[tokio::test]
async fn multibyte_character_split_across_8k_boundary_arrives_intact() {
    // 8191 ASCII bytes then "é" (2 bytes: straddles a read of 8192), then more.
    let script = "head -c 8191 /dev/zero | tr '\\0' 'x'; printf '\\303\\251'; printf 'tail'";
    let c = run(sh(script)).await;
    let out = c.stream(Stream::Stdout);
    let expected = format!("{}étail", "x".repeat(8191));
    assert_eq!(out, expected);
    assert!(!out.contains('\u{FFFD}'));
}

#[cfg(unix)]
#[tokio::test]
async fn invalid_utf8_is_replaced_not_dropped() {
    let c = run(sh("printf 'a\\377b'")).await;
    assert_eq!(c.stream(Stream::Stdout), "a\u{FFFD}b");
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_sleep_is_cancelled_within_one_second() {
    let ex = Executor::new();
    let (id, rx) = ex.start(ws(), sh("sleep 60")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let t = Instant::now();
    ex.cancel(&id).await.unwrap();
    let c = collect(rx).await;
    assert!(t.elapsed() < Duration::from_secs(1), "{:?}", t.elapsed());
    let x = c.exited();
    assert_eq!(x.status.outcome, Outcome::Cancelled);
    assert_eq!(x.status.signal, Some(15));
    assert_eq!(x.status.exit_code, None);
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_kills_grandchildren() {
    let ex = Executor::new();
    let (id, mut rx) = ex
        .start(ws(), sh("sleep 60 & echo $!; wait"))
        .await
        .unwrap();
    // First output is the grandchild's pid.
    let pid: i32 = match tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
    {
        TaskEvent::Output(o) => o.data.trim().parse().unwrap(),
        e => panic!("{e:?}"),
    };
    assert!(pid_alive(pid));
    ex.cancel(&id).await.unwrap();
    let c = collect(rx).await;
    assert_eq!(c.exited().status.outcome, Outcome::Cancelled);
    assert!(
        wait_until_dead(pid).await,
        "grandchild {pid} survived cancel"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn sigterm_ignoring_process_is_killed_after_grace() {
    let ex = Executor::new();
    let (id, mut rx) = ex
        .start(
            ws(),
            sh("trap '' TERM; echo ready; sleep 60; echo not-reached"),
        )
        .await
        .unwrap();
    // Wait until the trap is installed.
    match tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
    {
        TaskEvent::Output(o) => assert!(o.data.contains("ready")),
        e => panic!("{e:?}"),
    }
    let t = Instant::now();
    ex.cancel(&id).await.unwrap();
    let c = collect(rx).await;
    let took = t.elapsed();
    assert!(
        took >= Duration::from_millis(2800),
        "killed too early: {took:?}"
    );
    assert!(took < Duration::from_secs(6), "{took:?}");
    let x = c.exited();
    assert_eq!(x.status.outcome, Outcome::Cancelled);
    assert_eq!(x.status.signal, Some(9));
}

#[cfg(unix)]
#[tokio::test]
async fn cancelling_a_finished_task_is_ok_and_changes_nothing() {
    let ex = Executor::new();
    let (id, rx) = ex.start(ws(), sh("exit 0")).await.unwrap();
    let c = collect(rx).await;
    assert_eq!(c.exited().status.outcome, Outcome::Succeeded);
    ex.cancel(&id).await.unwrap();
    let l = ex.list(&ws());
    assert_eq!(l[0].exit.as_ref().unwrap().outcome, Outcome::Succeeded);
}

#[cfg(unix)]
#[tokio::test]
async fn cancel_all_cancels_only_that_workspace_and_waits() {
    let ex = Executor::new();
    let w2 = WorkspaceId("w2".into());
    let (_, rx1) = ex.start(ws(), sh("sleep 60")).await.unwrap();
    let (_, rx1b) = ex.start(ws(), sh("sleep 60")).await.unwrap();
    let (id2, rx2) = ex.start(w2.clone(), sh("sleep 60")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let drain1 = tokio::spawn(collect(rx1));
    let drain1b = tokio::spawn(collect(rx1b));
    ex.cancel_all(&ws()).await;
    for i in ex.list(&ws()) {
        assert_eq!(i.exit.unwrap().outcome, Outcome::Cancelled);
    }
    assert_eq!(
        drain1.await.unwrap().exited().status.outcome,
        Outcome::Cancelled
    );
    assert_eq!(
        drain1b.await.unwrap().exited().status.outcome,
        Outcome::Cancelled
    );
    assert!(ex.list(&w2)[0].exit.is_none(), "other workspace untouched");
    ex.cancel(&id2).await.unwrap();
    assert_eq!(
        collect(rx2).await.exited().status.outcome,
        Outcome::Cancelled
    );
}

#[cfg(unix)]
#[tokio::test]
async fn shutdown_cancels_everything() {
    let ex = Executor::new();
    let (_, rx_a) = ex
        .start(WorkspaceId("a".into()), sh("sleep 60"))
        .await
        .unwrap();
    let (_, rx_b) = ex
        .start(WorkspaceId("b".into()), sh("sleep 60"))
        .await
        .unwrap();
    let a = tokio::spawn(collect(rx_a));
    let b = tokio::spawn(collect(rx_b));
    tokio::time::sleep(Duration::from_millis(100)).await;
    ex.shutdown().await;
    assert_eq!(a.await.unwrap().exited().status.outcome, Outcome::Cancelled);
    assert_eq!(b.await.unwrap().exited().status.outcome, Outcome::Cancelled);
}

#[tokio::test]
async fn list_shows_running_then_finished_newest_first_with_ids() {
    let ex = Executor::new();
    let (id1, rx1) = ex.start(ws(), sh("exit 0")).await.unwrap();
    assert_eq!(id1.0, "t1");
    collect(rx1).await;
    #[cfg(unix)]
    let long = sh("sleep 60");
    #[cfg(windows)]
    let long = sh("ping -n 60 127.0.0.1 > nul");
    let mut long = long;
    long.label = Some("long one".into());
    let (id2, rx2) = ex.start(ws(), long).await.unwrap();
    assert_eq!(id2.0, "t2");
    let l = ex.list(&ws());
    assert_eq!(l.len(), 2);
    assert_eq!(l[0].task_id, id2);
    assert_eq!(l[0].label, "long one");
    assert!(l[0].exit.is_none(), "running");
    assert_eq!(l[1].task_id, id1);
    assert!(l[1].exit.is_some(), "finished");
    assert!(l[1].started_ms > 1_600_000_000_000);
    assert!(ex.list(&WorkspaceId("other".into())).is_empty());
    ex.cancel(&id2).await.unwrap();
    collect(rx2).await;
}

#[tokio::test]
async fn label_defaults_to_command_line() {
    let ex = Executor::new();
    let (_, rx) = ex
        .start(ws(), spec(Command::Shell("echo hi".into())))
        .await
        .unwrap();
    collect(rx).await;
    assert_eq!(ex.list(&ws())[0].label, "echo hi");
}

#[tokio::test]
async fn finished_history_is_capped_at_fifty_per_workspace() {
    let ex = Executor::new();
    let other = WorkspaceId("other".into());
    let (_, rx) = ex.start(other.clone(), sh("exit 0")).await.unwrap();
    collect(rx).await;
    for _ in 0..60 {
        let (_, rx) = ex.start(ws(), sh("exit 0")).await.unwrap();
        collect(rx).await;
    }
    let l = ex.list(&ws());
    assert_eq!(l.len(), 50);
    assert_eq!(l[0].task_id.0, "t61", "newest first");
    assert_eq!(l[49].task_id.0, "t12");
    assert_eq!(ex.list(&other).len(), 1, "history is per workspace");
}

#[cfg(unix)]
#[tokio::test]
async fn running_tasks_are_never_pruned() {
    let ex = Executor::new();
    let (id, rx) = ex.start(ws(), sh("sleep 60")).await.unwrap();
    for _ in 0..55 {
        let (_, rx) = ex.start(ws(), sh("exit 0")).await.unwrap();
        collect(rx).await;
    }
    let l = ex.list(&ws());
    assert_eq!(l.len(), 51, "50 finished + 1 running");
    assert!(l.iter().any(|i| i.task_id == id && i.exit.is_none()));
    ex.cancel(&id).await.unwrap();
    collect(rx).await;
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_the_executor_kills_running_tasks() {
    let pid;
    {
        let ex = Executor::new();
        let (_, mut rx) = ex
            .start(ws(), sh("sleep 60 & echo $!; wait"))
            .await
            .unwrap();
        pid = match tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap()
        {
            TaskEvent::Output(o) => o.data.trim().parse::<i32>().unwrap(),
            e => panic!("{e:?}"),
        };
        assert!(pid_alive(pid));
    }
    assert!(
        wait_until_dead(pid).await,
        "descendant survived executor drop"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_the_receiver_does_not_hang_the_task() {
    let ex = Executor::new();
    let (id, rx) = ex
        .start(ws(), sh("head -c 2097152 /dev/zero | tr '\\0' 'a'"))
        .await
        .unwrap();
    drop(rx);
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let l = ex.list(&ws());
        if l.iter().any(|i| i.task_id == id && i.exit.is_some()) {
            break;
        }
        assert!(
            Instant::now() < end,
            "task never finished without a receiver"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// AUD-007: a background descendant that inherited stdout keeps the pipe open after the task's
/// own process has exited. The task still ends (within the drain window) with the shell's exit
/// status, and the descendant, still holding the task's output, is ended with it.
#[cfg(unix)]
#[tokio::test]
async fn exit_with_a_background_pipe_holder_still_reports_exited() {
    let ex = Executor::new();
    let t = Instant::now();
    let (_, mut rx) = ex
        .start(ws(), sh("sleep 30 & echo $!; exit 0"))
        .await
        .unwrap();
    let pid: i32 = match tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .unwrap()
        .unwrap()
    {
        TaskEvent::Output(o) => o.data.trim().parse().unwrap(),
        e => panic!("{e:?}"),
    };
    let c = collect(rx).await;
    assert!(t.elapsed() < Duration::from_secs(4), "{:?}", t.elapsed());
    assert_eq!(c.exited().status.outcome, Outcome::Succeeded);
    assert_eq!(c.exited().status.exit_code, Some(0));
    assert!(wait_until_dead(pid).await, "the pipe holder {pid} survived");
    // `list` agrees.
    assert!(ex.list(&ws())[0].exit.is_some());
}

/// AUD-007: a descendant that left the task's process group (setsid) and holds the pipe cannot
/// be killed through the group; the task ends anyway once the drain window is over, and a
/// cancel during the drain is not ignored.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_pipe_holder_outside_the_group_does_not_hang_the_task_or_cancel() {
    let ex = Executor::new();
    let (id, mut rx) = ex
        .start(
            ws(),
            sh("setsid sh -c 'echo $$; exec sleep 30' & sleep 0.2; echo started; exit 0"),
        )
        .await
        .unwrap();
    let mut text = String::new();
    while !text.contains("started") {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv())
            .await
            .unwrap()
            .unwrap()
        {
            TaskEvent::Output(o) => text.push_str(&o.data),
            e => panic!("{e:?}"),
        }
    }
    let holder: i32 = text.lines().next().unwrap().trim().parse().unwrap();
    // The shell has exited (or is about to); cancel while the pipe is still held.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t = Instant::now();
    ex.cancel(&id).await.unwrap();
    let c = collect(rx).await;
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    assert!(matches!(
        c.exited().status.outcome,
        Outcome::Cancelled | Outcome::Succeeded
    ));
    // Not ours any more (another session): end it here.
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(holder),
        nix::sys::signal::Signal::SIGKILL,
    );
}

/// AUD-007: `cancel_all` (what `workspace/close` and shutdown wait on) returns within a bound
/// when a task's process has exited and a descendant holds its output.
#[cfg(unix)]
#[tokio::test]
async fn cancel_all_returns_with_a_pipe_holder() {
    let ex = Executor::new();
    let (_, rx) = ex.start(ws(), sh("sleep 30 & exit 0")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let t = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), ex.cancel_all(&ws()))
        .await
        .expect("cancel_all must not hang");
    assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    drop(rx);
}

/// AUD-099: a closed workspace's finished tasks are forgotten; running ones are kept.
#[cfg(unix)]
#[tokio::test]
async fn forgetting_a_workspace_drops_its_history() {
    let ex = Executor::new();
    let (_, rx) = ex.start(ws(), sh("exit 0")).await.unwrap();
    collect(rx).await;
    let w2 = WorkspaceId("w2".into());
    let (_, rx2) = ex.start(w2.clone(), sh("exit 0")).await.unwrap();
    collect(rx2).await;
    ex.forget_workspace(&ws());
    assert!(ex.list(&ws()).is_empty());
    assert_eq!(ex.list(&w2).len(), 1);
}
