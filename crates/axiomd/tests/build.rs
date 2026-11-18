//! `build/tasks` and `build/run` against the real daemon with the real `cargo` (M2-BUILD-001).

mod common;
use axiom_protocol::build::{BuildDiagnostics, Run, RunParams, Tasks};
use axiom_protocol::diagnostics::Severity;
use axiom_protocol::provider::TaskKind;
use axiom_protocol::task::ExitedParams;
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::ErrorKind;
use common::*;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

/// A crate whose `src/main.rs` line 2 has a type error (E0308 at column 18).
fn broken_crate() -> TempDir {
    let d = TempDir::new().unwrap();
    std::fs::write(
        d.path().join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::create_dir(d.path().join("src")).unwrap();
    std::fs::write(
        d.path().join("src/main.rs"),
        "fn main() {\n    let x: i32 = \"text\";\n    println!(\"{x}\");\n}\n",
    )
    .unwrap();
    d
}

/// The daemon with `$CARGO`'s directory first on PATH.
async fn daemon() -> (Daemon, TempDir) {
    let data = TempDir::new().unwrap();
    let cargo_dir = PathBuf::from(env!("CARGO")).parent().unwrap().to_path_buf();
    let path = std::env::join_paths(std::iter::once(cargo_dir).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("PATH", path);
    })
    .await;
    d.initialize().await;
    (d, data)
}

#[tokio::test]
async fn tasks_lists_the_rust_tasks_of_a_cargo_workspace() {
    let (d, _data) = daemon().await;
    let ws = d.open_dir(broken_crate()).await;
    let r = d
        .client
        .request::<Tasks>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap();
    let rust: Vec<_> = r
        .tasks
        .iter()
        .filter(|t| t.provider_id == "rust")
        .map(|t| (t.id.as_str(), t.kind))
        .collect();
    assert_eq!(
        rust,
        [
            ("build", TaskKind::Build),
            ("test", TaskKind::Test),
            ("run", TaskKind::Run)
        ]
    );
    // A folder no provider applies to has no tasks.
    let empty = d.open_dir(TempDir::new().unwrap()).await;
    let r = d
        .client
        .request::<Tasks>(WorkspaceRef {
            workspace_id: empty.id.clone(),
        })
        .await
        .unwrap();
    assert!(r.tasks.is_empty());
}

#[tokio::test]
async fn cargo_build_type_error_yields_e0308_before_exit() {
    let (d, _data) = daemon().await;
    let ws = d.open_dir(broken_crate()).await;
    let mut rx = d.client.notifications();
    let r = d
        .client
        .request::<Run>(RunParams {
            workspace_id: ws.id.clone(),
            provider_id: "rust".into(),
            task: "build".into(),
        })
        .await
        .unwrap();
    let mut batches: Vec<BuildDiagnostics> = Vec::new();
    let mut output = 0;
    let exited: ExitedParams = loop {
        let (method, params) = tokio::time::timeout(Duration::from_secs(240), rx.recv())
            .await
            .expect("build finished in time")
            .unwrap();
        match method.as_str() {
            "task/output" if params["taskId"] == r.task_id.0 => output += 1,
            "build/diagnostics" => {
                let b: BuildDiagnostics = serde_json::from_value(params).unwrap();
                assert_eq!(b.task_id, r.task_id);
                batches.push(b);
            }
            "task/exited" if params["taskId"] == r.task_id.0 => {
                break serde_json::from_value(params).unwrap()
            }
            _ => {}
        }
    };
    assert!(output > 0, "cargo's output still flows as task/output");
    assert_eq!(exited.status.exit_code, Some(101));
    // The complete batch came last, before task/exited; batches hold only new diagnostics.
    assert!(batches.last().unwrap().complete);
    assert!(batches[..batches.len() - 1].iter().all(|b| !b.complete));
    let all: Vec<_> = batches.into_iter().flat_map(|b| b.diagnostics).collect();
    let errors: Vec<_> = all
        .iter()
        .filter(|x| x.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1, "{all:#?}");
    let e = errors[0];
    assert_eq!(e.code.as_deref(), Some("E0308"));
    let l = e.location.as_ref().unwrap();
    assert_eq!(
        (l.file.0.as_str(), l.line, l.column),
        ("src/main.rs", 2, Some(18))
    );
}

#[tokio::test]
async fn unknown_task_and_missing_environment() {
    let (d, _data) = daemon().await;
    let dir = broken_crate();
    std::fs::create_dir(dir.path().join(".axiom")).unwrap();
    std::fs::write(
        dir.path().join(".axiom/project.toml"),
        "[environment]\nrust = \"1.0.0\"\n",
    )
    .unwrap();
    let ws = d.open_dir(dir).await;
    let run = |task: &str| RunParams {
        workspace_id: ws.id.clone(),
        provider_id: "rust".into(),
        task: task.into(),
    };
    // The selected toolchain is not installed: the task does not start.
    let e = d.client.request::<Run>(run("build")).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::PrerequisiteMissing);
    std::fs::remove_file(ws.root.join(".axiom/project.toml")).unwrap();
    let e = d.client.request::<Run>(run("deploy")).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    let e = d
        .client
        .request::<Run>(RunParams {
            workspace_id: ws.id.clone(),
            provider_id: "nope".into(),
            task: "build".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
}
