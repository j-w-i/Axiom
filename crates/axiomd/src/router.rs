use crate::app::App;
use crate::handlers::{
    agent, build, env, fs, git, lsp, preview, sessions, settings, task, terminal, ui, workspace,
};
use axiom_protocol::{
    agent as pagent, build as pbuild, env as penv, fs as pfs, settings as pset, task as ptask,
    workspace as pws,
};
use axiom_rpc::Router;
use std::sync::Arc;

/// Registers one handler per protocol method plus the shutdown hook.
pub fn build_router(app: Arc<App>) -> Router {
    let mut r = Router::new();
    if !app
        .lsp_forwarding
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        lsp::spawn_forwarder(&app);
        terminal::spawn_forwarder(&app);
        sessions::spawn_idle_reaper(&app);
    }

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

    wire!(axiom_protocol::preview::Start, preview::start);
    wire!(axiom_protocol::preview::Stop, preview::stop);
    wire!(axiom_protocol::preview::List, preview::list);
    wire!(axiom_protocol::ui::InspectMode, ui::inspect_mode);
    wire!(axiom_protocol::ui::TakeSnapshot, ui::snapshot);
    wire!(axiom_protocol::ui::HitTest, ui::hit_test);
    wire!(axiom_protocol::ui::NodeDetails, ui::node_details);
    wire!(axiom_protocol::ui::Highlight, ui::highlight);
    wire!(axiom_protocol::ui::Verify, ui::verify);
    wire!(pws::Open, workspace::open, ctx);
    wire!(pws::Close, workspace::close, ctx);
    wire!(pws::Recent, workspace::recent);
    wire!(pws::GetState, workspace::get_state);
    wire!(pws::SetState, workspace::set_state);
    wire!(pfs::List, fs::list);
    wire!(axiom_protocol::lsp::Servers, lsp::servers);
    wire!(axiom_protocol::lsp::Request, lsp::request, ctx);
    wire!(axiom_protocol::lsp::Notify, lsp::notify);
    wire!(axiom_protocol::lsp::Restart, lsp::restart);
    wire!(pws::Search, workspace::search, ctx);
    wire!(axiom_protocol::git::Status, git::status);
    wire!(axiom_protocol::git::Show, git::show);
    wire!(axiom_protocol::terminal::Create, terminal::create);
    wire!(axiom_protocol::terminal::Write, terminal::write);
    wire!(axiom_protocol::terminal::Resize, terminal::resize);
    wire!(axiom_protocol::terminal::Close, terminal::close);
    wire!(axiom_protocol::terminal::List, terminal::list);
    wire!(pfs::ReadFile, fs::read_file);
    wire!(pfs::WriteFile, fs::write_file);
    wire!(pfs::Watch, fs::watch, ctx);
    wire!(pfs::CreateFile, fs::create_file);
    wire!(pfs::CreateDirectory, fs::create_directory);
    wire!(pfs::Rename, fs::rename);
    wire!(pfs::Trash, fs::trash);
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
    wire!(pagent::InstallAgent, agent::install_agent);
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
    let a = app.clone();
    r.on_disconnect(move |n| workspace::release_connection(a.clone(), n));
    let a = app;
    r.on_shutdown(move || {
        let a = a.clone();
        async move {
            a.stop_all_watchers();
            git::stop_all_watches(&a);
            // Together: the shell gives the daemon a few seconds before it kills it. Terminals
            // are waited for (their SIGKILL escalation must run before the process exits,
            // AUD-027); external agents and MCP servers are stopped explicitly.
            let terms = a.clone();
            let terminals =
                tokio::task::spawn_blocking(move || terms.terminals.close_all_and_wait());
            let _ = tokio::join!(
                a.lsp.stop_all(),
                a.executor.shutdown(),
                stop_agents(&a),
                terminals
            );
        }
    });
    r
}

/// Ends the external agents (ACP) and MCP servers of every session. Bounded: a map held by a
/// handshake in progress is not waited for long (the processes die with their handles).
async fn stop_agents(a: &App) {
    const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(1);
    if let Ok(mut m) = tokio::time::timeout(LOCK_WAIT, a.agent.acp.lock()).await {
        let sessions: Vec<_> = m.drain().map(|(_, s)| s).collect();
        drop(m);
        for s in sessions {
            let _ = tokio::time::timeout(LOCK_WAIT, s.kill()).await;
        }
    }
    if let Ok(mut m) = tokio::time::timeout(LOCK_WAIT, a.agent.mcp.lock()).await {
        // `kill_on_drop`: dropping the clients ends the servers.
        m.clear();
    }
}

/// Protocol methods registered by a milestone's first task whose handlers later tasks add. Each
/// task that wires one removes it here; the milestone's last backend task leaves this empty
/// (M5-VERIFY-001 did for 0.5). Clients feature-detect from `initialize.methods`, which lists
/// only served methods.
#[doc(hidden)]
pub const NOT_YET_SERVED: &[&str] = &[];

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
