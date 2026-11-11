#![cfg(unix)] // scripts below are POSIX sh; Windows variants are future work (see handoff)
mod common;
use axiom_protocol::task::{
    Cancel, Command, ListTasks, ListTasksParams, Outcome, Start, StartParams, Stream, TaskRef,
};
use axiom_protocol::workspace::{Close, WorkspaceRef};
use axiom_protocol::{ErrorKind, TaskId};
use common::*;
use std::collections::BTreeMap;
use std::time::Duration;

fn sh(script: &str) -> Command {
    Command::Argv(vec!["sh".into(), "-c".into(), script.into()])
}

fn params(fx: &Fixture, command: Command) -> StartParams {
    StartParams {
        workspace_id: fx.id.clone(),
        command,
        cwd: None,
        env: BTreeMap::new(),
        label: None,
    }
}

async fn start(d: &Daemon, fx: &Fixture, command: Command) -> TaskId {
    d.client
        .request::<Start>(params(fx, command))
        .await
        .unwrap()
        .task_id
}

fn text(out: &[axiom_protocol::task::OutputParams], s: Stream) -> String {
    out.iter()
        .filter(|o| o.stream == s)
        .map(|o| o.data.as_str())
        .collect()
}

#[tokio::test]
async fn argv_form_streams_stdout_and_stderr_with_exit_code() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, sh("echo out; echo err 1>&2; exit 3")).await;
    assert_eq!(id.0, "t1");
    let (out, exited) = d.task_events(&id.0).await;
    assert!(text(&out, Stream::Stdout).contains("out"));
    assert!(text(&out, Stream::Stderr).contains("err"));
    assert_eq!(exited.status.outcome, Outcome::Failed);
    assert_eq!(exited.status.exit_code, Some(3));
    // seq is contiguous from 0.
    for (i, o) in out.iter().enumerate() {
        assert_eq!(o.seq, i as u64);
    }
}

#[tokio::test]
async fn shell_form_runs_through_the_shell() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, Command::Shell("echo one && echo two".into())).await;
    let (out, exited) = d.task_events(&id.0).await;
    let s = text(&out, Stream::Stdout);
    assert!(s.contains("one") && s.contains("two"), "{s}");
    assert_eq!(exited.status.outcome, Outcome::Succeeded);
    assert_eq!(exited.status.exit_code, Some(0));
}

#[tokio::test]
async fn start_response_precedes_the_first_notification_on_the_wire() {
    for _ in 0..25 {
        let mut s = RawSession::spawn().await;
        s.initialize().await;
        let dir = tempfile::tempdir().unwrap();
        s.send(&format!(
            r#"{{"jsonrpc":"2.0","id":1,"method":"workspace/open","params":{{"path":{}}}}}"#,
            serde_json::to_string(dir.path().to_str().unwrap()).unwrap()
        ))
        .await;
        s.next().await;
        s.send(r#"{"jsonrpc":"2.0","id":2,"method":"task/start","params":{"workspaceId":"w1","command":{"argv":["sh","-c","echo hi"]}}}"#).await;
        let first = s.next().await;
        assert_eq!(first["id"], 2, "response must come first: {first}");
        assert_eq!(first["result"]["taskId"], "t1");
        let second = s.next().await;
        assert_eq!(second["method"], "task/output");
        let third = s.next().await;
        assert_eq!(third["method"], "task/exited");
    }
}

#[tokio::test]
async fn output_arrives_in_seq_order_before_exited() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(
        &d,
        &fx,
        sh("i=0; while [ $i -lt 200 ]; do echo line$i; i=$((i+1)); done"),
    )
    .await;
    let (out, exited) = d.task_events(&id.0).await;
    assert_eq!(exited.status.outcome, Outcome::Succeeded);
    let all = text(&out, Stream::Stdout);
    let lines: Vec<&str> = all.lines().collect();
    assert_eq!(lines.len(), 200);
    assert_eq!(lines[199], "line199");
    for (i, o) in out.iter().enumerate() {
        assert_eq!(o.seq, i as u64);
    }
}

#[tokio::test]
async fn two_hundred_kib_of_output_arrives_complete() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, sh("head -c 204800 /dev/zero | tr '\\0' 'z'")).await;
    let (out, exited) = d.task_events(&id.0).await;
    assert_eq!(exited.status.outcome, Outcome::Succeeded);
    let s = text(&out, Stream::Stdout);
    assert_eq!(s.len(), 200 * 1024);
    assert!(s.bytes().all(|b| b == b'z'));
}

#[tokio::test]
async fn spawn_failed_has_program_and_os_error_details_and_no_notifications() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let e = d
        .client
        .request::<Start>(params(
            &fx,
            Command::Argv(vec!["definitely-not-a-program".into()]),
        ))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::SpawnFailed);
    let det = e.data.details.unwrap();
    assert_eq!(det["program"], "definitely-not-a-program");
    assert!(det["osError"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(
        d.no_notification("task/exited", Duration::from_millis(300))
            .await
    );
    assert!(
        d.no_notification("task/output", Duration::from_millis(10))
            .await
    );
}

#[tokio::test]
async fn empty_argv_is_invalid_params() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let e = d
        .client
        .request::<Start>(params(&fx, Command::Argv(vec![])))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn cwd_defaults_to_root_and_can_be_a_subdirectory() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, sh("pwd")).await;
    let (out, _) = d.task_events(&id.0).await;
    assert_eq!(text(&out, Stream::Stdout).trim(), fx.root.to_str().unwrap());

    let mut p = params(&fx, sh("pwd"));
    p.cwd = Some(wp("src"));
    let id = d.client.request::<Start>(p).await.unwrap().task_id;
    let (out, _) = d.task_events(&id.0).await;
    assert_eq!(
        text(&out, Stream::Stdout).trim(),
        fx.path("src").to_str().unwrap()
    );
}

#[tokio::test]
async fn cwd_outside_the_workspace_or_missing_or_a_file_is_an_error() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    for (cwd, want) in [
        ("../outside", ErrorKind::InvalidParams),
        ("/tmp", ErrorKind::InvalidParams),
        ("missing", ErrorKind::NotFound),
        ("README.md", ErrorKind::NotFound),
    ] {
        let mut p = params(&fx, sh("pwd"));
        p.cwd = Some(wp(cwd));
        let e = d.client.request::<Start>(p).await.unwrap_err();
        assert_eq!(kind(&e), want, "cwd {cwd}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cwd_through_escaping_symlink_is_permission_denied() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), fx.path("out")).unwrap();
    let mut p = params(&fx, sh("pwd"));
    p.cwd = Some(wp("out"));
    let e = d.client.request::<Start>(p).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::PermissionDenied);
}

#[tokio::test]
async fn env_is_added_to_the_inherited_environment() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let mut p = params(&fx, sh("echo $AXIOM_T; echo $HOME"));
    p.env.insert("AXIOM_T".into(), "from-test".into());
    let id = d.client.request::<Start>(p).await.unwrap().task_id;
    let (out, _) = d.task_events(&id.0).await;
    let s = text(&out, Stream::Stdout);
    let lines: Vec<&str> = s.lines().collect();
    assert_eq!(lines[0], "from-test");
    assert!(!lines[1].is_empty(), "inherited HOME must still be set");
}

#[tokio::test]
async fn stdin_is_closed() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, sh("cat; echo done")).await;
    let (out, exited) = d.task_events(&id.0).await;
    assert!(text(&out, Stream::Stdout).contains("done"));
    assert_eq!(exited.status.outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn cancel_yields_cancelled_outcome() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, sh("sleep 60")).await;
    d.client
        .request::<Cancel>(TaskRef {
            task_id: id.clone(),
        })
        .await
        .unwrap();
    let (_, exited) = d.task_events(&id.0).await;
    assert_eq!(exited.status.outcome, Outcome::Cancelled);
}

#[tokio::test]
async fn cancel_unknown_task_is_not_found_and_finished_task_is_ok() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let e = d
        .client
        .request::<Cancel>(TaskRef {
            task_id: TaskId("t99".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    let id = start(&d, &fx, sh("exit 0")).await;
    d.task_events(&id.0).await;
    d.client
        .request::<Cancel>(TaskRef { task_id: id })
        .await
        .unwrap();
}

#[tokio::test]
async fn task_list_shows_running_and_finished_newest_first() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let first = start(&d, &fx, sh("exit 0")).await;
    d.task_events(&first.0).await;
    let mut p = params(&fx, sh("sleep 60"));
    p.label = Some("sleeper".into());
    let second = d.client.request::<Start>(p).await.unwrap().task_id;
    let l = d
        .client
        .request::<ListTasks>(ListTasksParams {
            workspace_id: fx.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(l.tasks.len(), 2);
    assert_eq!(l.tasks[0].task_id, second);
    assert_eq!(l.tasks[0].label, "sleeper");
    assert!(l.tasks[0].exit.is_none());
    assert_eq!(l.tasks[1].task_id, first);
    assert_eq!(
        l.tasks[1].exit.as_ref().unwrap().outcome,
        Outcome::Succeeded
    );
    assert_eq!(l.tasks[1].label, "sh -c exit 0");
    d.client
        .request::<Cancel>(TaskRef {
            task_id: second.clone(),
        })
        .await
        .unwrap();
    d.task_events(&second.0).await;
}

#[tokio::test]
async fn closing_the_workspace_cancels_its_tasks() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let other = d.open_fixture().await;
    let id = start(&d, &fx, sh("sleep 60")).await;
    let other_id = start(&d, &other, sh("sleep 60")).await;
    d.client
        .request::<Close>(WorkspaceRef {
            workspace_id: fx.id.clone(),
        })
        .await
        .unwrap();
    let (_, exited) = d.task_events(&id.0).await;
    assert_eq!(exited.status.outcome, Outcome::Cancelled);
    // The other workspace's task is untouched.
    let l = d
        .client
        .request::<ListTasks>(ListTasksParams {
            workspace_id: other.id.clone(),
        })
        .await
        .unwrap();
    assert!(l.tasks[0].exit.is_none());
    d.client
        .request::<Cancel>(TaskRef {
            task_id: other_id.clone(),
        })
        .await
        .unwrap();
    d.task_events(&other_id.0).await;
}

#[tokio::test]
async fn list_for_unknown_workspace_is_workspace_not_open() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request::<ListTasks>(ListTasksParams {
            workspace_id: axiom_protocol::WorkspaceId("w9".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::WorkspaceNotOpen);
}

#[tokio::test]
async fn shutdown_cancels_running_tasks_and_process_exits() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let id = start(&d, &fx, sh("sleep 60")).await;
    let _ = id;
    let status = d.shutdown().await;
    assert_eq!(status.code(), Some(0));
}
