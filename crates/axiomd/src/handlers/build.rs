use crate::app::App;
use crate::handlers::env::{project_file, registry, workspace};
use axiom_build::{BuildEvent, Counts};
use axiom_protocol::build::{Diagnostics, RunParams, RunResult, TasksResult};
use axiom_protocol::env::Environment;
use axiom_protocol::task::{Exited, Output};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::RpcError;
use axiom_rpc::RequestCtx;
use axiom_store::BuildRun;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn tasks(app: Arc<App>, p: WorkspaceRef) -> Result<TasksResult, RpcError> {
    let ws = workspace(&app, &p.workspace_id).await?;
    let root = ws.0.clone();
    let r = registry(&app, Some(ws)).await?;
    Ok(TasksResult {
        tasks: axiom_build::tasks(&r, &root),
    })
}

/// Starts a provider task inside the project's environment (`.axiom/project.toml`, if any).
/// Output and exit are the `task/*` notifications; parsed diagnostics follow as
/// `build/diagnostics`, the complete batch before `task/exited`.
pub async fn run(app: Arc<App>, p: RunParams, ctx: RequestCtx) -> Result<RunResult, RpcError> {
    let ws = workspace(&app, &p.workspace_id).await?;
    let root = ws.0.clone();
    let r = registry(&app, Some(ws)).await?;
    let environment = match project_file(&app, &p.workspace_id).await? {
        Some(selection) => {
            let path = std::env::var_os("PATH");
            axiom_env::project::resolve(
                &r,
                &selection,
                &app.env.data_dir.join("toolchains"),
                path.as_deref(),
            )
            .await?
            .0
        }
        None => Environment::default(),
    };
    let prepared = axiom_build::prepare(
        &r,
        &p.provider_id,
        &p.task,
        &root,
        &environment,
        std::env::var_os("PATH"),
    )?;
    let started_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let (task_id, rx) = app
        .executor
        .start(p.workspace_id.clone(), prepared.spec)
        .await?;
    let mut events = axiom_build::drive(
        task_id.clone(),
        rx,
        &prepared.diagnostics,
        vec![root.clone()],
    );
    let notifier = ctx.notifier.clone();
    let history_root = root.to_string_lossy().into_owned();
    let (provider, task) = (p.provider_id, p.task);
    // Notifications may only follow the response to this request.
    ctx.after_response(async move {
        let mut counts = Counts::default();
        let mut connected = true;
        while let Some(ev) = events.recv().await {
            match ev {
                BuildEvent::Output(o) if connected => {
                    connected = notifier.notify::<Output>(o).await.is_ok();
                }
                BuildEvent::Diagnostics(d) => {
                    counts.add(&d.diagnostics);
                    if connected {
                        connected = notifier.notify::<Diagnostics>(d).await.is_ok();
                    }
                }
                BuildEvent::Exited(x) => {
                    let run = BuildRun {
                        provider: provider.clone(),
                        task: task.clone(),
                        started_ms,
                        duration_ms: x.status.duration_ms,
                        outcome: serde_json::to_value(x.status.outcome)
                            .ok()
                            .and_then(|v| v.as_str().map(String::from))
                            .unwrap_or_default(),
                        errors: counts.errors,
                        warnings: counts.warnings,
                    };
                    if connected {
                        let _ = notifier.notify::<Exited>(x).await;
                    }
                    let store = app.store.clone();
                    let root = history_root.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        if let Err(e) = store.build_history_add(&root, &run) {
                            tracing::warn!("cannot record the build: {e}");
                        }
                    })
                    .await;
                }
                BuildEvent::Output(_) => {}
            }
        }
    });
    Ok(RunResult { task_id })
}
