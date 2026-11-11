use crate::app::App;
use axiom_exec::{SpawnSpec, TaskEvent};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::task::{
    Exited, ListTasksParams, ListTasksResult, Output, StartParams, StartResult, TaskRef,
};
use axiom_protocol::RpcError;
use axiom_rpc::RequestCtx;
use std::sync::Arc;

pub async fn start(
    app: Arc<App>,
    p: StartParams,
    ctx: RequestCtx,
) -> Result<StartResult, RpcError> {
    let ws = app.workspaces.get(&p.workspace_id)?;
    let cwd = match &p.cwd {
        Some(c) => ws.resolve(c)?,
        None => ws.root().to_path_buf(),
    };
    let spec = SpawnSpec {
        command: p.command,
        cwd,
        env: p.env,
        label: p.label,
    };
    let (task_id, mut rx) = app.executor.start(p.workspace_id, spec).await?;
    // Notifications may only follow the response to this request.
    let notifier = ctx.notifier.clone();
    ctx.after_response(async move {
        while let Some(ev) = rx.recv().await {
            let sent = match ev {
                TaskEvent::Output(o) => notifier.notify::<Output>(o).await,
                TaskEvent::Exited(x) => notifier.notify::<Exited>(x).await,
            };
            if sent.is_err() {
                // Connection gone: keep draining so the task is not stalled.
                while rx.recv().await.is_some() {}
                break;
            }
        }
    });
    Ok(StartResult { task_id })
}

pub async fn cancel(app: Arc<App>, p: TaskRef) -> Result<Empty, RpcError> {
    app.executor.cancel(&p.task_id).await?;
    Ok(Empty {})
}

pub async fn list(app: Arc<App>, p: ListTasksParams) -> Result<ListTasksResult, RpcError> {
    app.workspaces.get(&p.workspace_id)?;
    Ok(ListTasksResult {
        tasks: app.executor.list(&p.workspace_id),
    })
}
