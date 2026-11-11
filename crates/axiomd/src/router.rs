use crate::app::App;
use crate::handlers::{fs, settings, task, workspace};
use axiom_protocol::{fs as pfs, settings as pset, task as ptask, workspace as pws};
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

    wire!(pws::Open, workspace::open);
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

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_store::Store;
    use std::collections::BTreeSet;

    /// A protocol method can never be left unwired.
    #[tokio::test]
    async fn router_serves_exactly_the_registry_methods() {
        let app = App::new(Store::open_in_memory().unwrap());
        let router = build_router(app);
        let served: BTreeSet<String> = router.method_names().into_iter().collect();
        let expected: BTreeSet<String> = axiom_protocol::registry::method_names()
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(served, expected);
    }
}
