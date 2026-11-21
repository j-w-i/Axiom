use crate::app::App;
use crate::handlers::{agent, build, env, fs, sessions, settings, task, workspace};
use axiom_protocol::{
    agent as pagent, build as pbuild, env as penv, fs as pfs, settings as pset, task as ptask,
    workspace as pws,
};
use axiom_rpc::Router;
use std::sync::Arc;

/// Registers one handler per protocol method plus the shutdown hook.
pub fn build_router(app: Arc<App>) -> Router {
    let mut r = Router::new();

    macro_rules! wire {
        // handler takes (app, params)
        ($m:ty, $f:path) => {{
            let a = app.clone();
            r.method::<$m, _, _>(move |p, _ctx| $f(a.clone(), p));
        }};
        // handler takes (app, params, ctx)
        ($m:ty, $f:path, ctx) => {{
            let a = app.clone();
            r.method::<$m, _, _>(move |p, ctx| $f(a.clone(), p, ctx));
        }};
    }

    wire!(pws::Open, workspace::open, ctx);
    wire!(pws::Close, workspace::close);
    wire!(pws::Recent, workspace::recent);
    wire!(pws::GetState, workspace::get_state);
    wire!(pws::SetState, workspace::set_state);
    wire!(pfs::List, fs::list);
    wire!(pfs::ReadFile, fs::read_file);
    wire!(pfs::WriteFile, fs::write_file);
    wire!(pfs::Watch, fs::watch, ctx);
    wire!(ptask::Start, task::start, ctx);
    wire!(ptask::Cancel, task::cancel);
    wire!(ptask::ListTasks, task::list);
    wire!(pset::Get, settings::get);
    wire!(pset::Set, settings::set, ctx);
    wire!(penv::Providers, env::providers);
    wire!(penv::Detect, env::detect_tools);
    wire!(penv::Trust, env::trust);
    wire!(penv::Versions, env::versions);
    wire!(penv::PlanInstall, env::plan_install);
    wire!(penv::Install, env::install, ctx);
    wire!(penv::Uninstall, env::uninstall);
    wire!(penv::Resolve, env::resolve);
    wire!(pbuild::Tasks, build::tasks);
    wire!(pbuild::Run, build::run, ctx);
    wire!(pagent::Models, agent::models);
    wire!(pagent::ConfigureProvider, agent::configure_provider);
    wire!(pagent::Tools, agent::tools);
    wire!(pagent::GetPolicy, agent::get_policy);
    wire!(pagent::SetPolicy, agent::set_policy);
    wire!(pagent::Approve, agent::approve);
    wire!(pagent::ToolCall, agent::tool_call, ctx);
    wire!(pagent::SessionCreate, sessions::create, ctx);
    wire!(pagent::SessionList, sessions::list);
    wire!(pagent::SessionOpen, sessions::open, ctx);
    wire!(pagent::SessionRename, sessions::rename);
    wire!(pagent::SessionDelete, sessions::delete);
    wire!(pagent::Image, sessions::image);
    wire!(pagent::Send, sessions::send, ctx);
    wire!(pagent::Cancel, sessions::cancel);
    wire!(pagent::Checkpoints, sessions::checkpoints);
    wire!(pagent::Restore, sessions::restore);

    let a = app.clone();
    r.on_initialize(move |params, notifier| {
        if params.capabilities.as_ref().is_some_and(|c| c.approvals) {
            a.agent.approvers.add(notifier);
        }
    });
    let a = app;
    r.on_shutdown(move || {
        let a = a.clone();
        async move {
            a.stop_all_watchers();
            a.executor.shutdown().await;
        }
    });
    r
}

/// Protocol 0.4 methods registered by M4-ARCH-001 whose handlers later M4 tasks add. Each task
/// that wires one removes it here; the last M4 backend task leaves this empty. Clients
/// feature-detect from `initialize.methods` (ADR-0018 §1), which lists only served methods.
#[doc(hidden)]
pub const NOT_YET_SERVED: &[&str] = &[
    "lsp/servers",
    "lsp/request",
    "lsp/notify",
    "lsp/restart",
    "terminal/create",
    "terminal/write",
    "terminal/resize",
    "terminal/close",
    "terminal/list",
    "git/status",
    "git/show",
    "workspace/search",
];

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_store::Store;
    use std::collections::BTreeSet;

    /// A protocol method can never be left unwired (beyond the explicit `NOT_YET_SERVED`).
    #[tokio::test]
    async fn router_serves_exactly_the_registry_methods() {
        let app = App::new(Store::open_in_memory().unwrap());
        let router = build_router(app);
        let served: BTreeSet<String> = router.method_names().into_iter().collect();
        let expected: BTreeSet<String> = axiom_protocol::registry::method_names()
            .into_iter()
            .filter(|m| !NOT_YET_SERVED.contains(m))
            .map(String::from)
            .collect();
        assert_eq!(served, expected);
        for m in NOT_YET_SERVED {
            assert!(
                axiom_protocol::registry::method_names().contains(m),
                "{m} is not a protocol method"
            );
        }
    }
}
