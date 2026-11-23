//! `agent/*` handlers (protocol 0.3, ADR-0015). Thin: read what the service needs from the
//! store, call it, map its error.

use crate::app::App;
use axiom_agent::credentials::CredentialError;
use axiom_agent::gate::Policies;
use axiom_agent::providers::ProvidersError;
use axiom_agent::tools::ToolCtx;
use axiom_protocol::agent::{
    ApproveParams, ConfigureProviderParams, ModelsParams, ModelsResult, Policy, PolicyParams,
    PolicyResult, ProviderConfig, ToolCallParams, ToolCallResult, ToolsResult,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{ErrorKind, RpcError};
use axiom_protocol::{WorkspaceId, WorkspacePath};
use axiom_rpc::{Notifier, RequestCtx};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

pub(crate) const PROVIDERS_KEY: &str = "agent.providers";

pub(crate) async fn provider_configs(app: &App) -> Result<Vec<ProviderConfig>, RpcError> {
    let v = app.store_call(|s| s.setting_get(PROVIDERS_KEY)).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default())
}

pub(crate) fn providers_error(e: ProvidersError) -> RpcError {
    match e {
        ProvidersError::NotFound(id) => RpcError::new(
            ErrorKind::NotFound,
            format!("No provider `{id}` is configured."),
        ),
        ProvidersError::Invalid(m) => RpcError::new(ErrorKind::InvalidParams, m),
        ProvidersError::Credentials(e @ CredentialError::Unavailable(_)) => {
            RpcError::new(ErrorKind::Unsupported, sentence(e.to_string()))
        }
        ProvidersError::Credentials(e @ CredentialError::Failed(_)) => {
            RpcError::new(ErrorKind::Io, sentence(e.to_string()))
        }
    }
}

fn sentence(mut s: String) -> String {
    if let Some(c) = s.get(..1) {
        let up = c.to_uppercase();
        s.replace_range(..1, &up);
    }
    if !s.ends_with('.') {
        s.push('.');
    }
    s
}

pub async fn models(app: Arc<App>, p: ModelsParams) -> Result<ModelsResult, RpcError> {
    let configs = provider_configs(&app).await?;
    Ok(ModelsResult {
        providers: app
            .agent
            .providers
            .status(&configs, p.provider_id.as_deref())
            .await,
    })
}

pub async fn configure_provider(
    app: Arc<App>,
    p: ConfigureProviderParams,
) -> Result<Empty, RpcError> {
    let configs = provider_configs(&app).await?;
    let next = app
        .agent
        .providers
        .configure(configs, p)
        .await
        .map_err(providers_error)?;
    let value = serde_json::to_value(next).map_err(|_| crate::app::internal())?;
    app.store_call(move |s| {
        s.settings_set(&BTreeMap::from([(PROVIDERS_KEY.to_string(), Some(value))]))
    })
    .await?;
    Ok(Empty {})
}

/// The tool registry offered in this workspace (built-in tools; MCP tools join per session).
pub async fn tools(app: Arc<App>, p: WorkspaceRef) -> Result<ToolsResult, RpcError> {
    app.workspaces.get(&p.workspace_id)?;
    Ok(ToolsResult {
        tools: app.agent.tools.infos(),
    })
}

pub(crate) const POLICY_KEY: &str = "agent.policy";

/// The user policy (settings), or the built-in one.
pub(crate) async fn user_policy(app: &App) -> Result<Policy, RpcError> {
    let v = app.store_call(|s| s.setting_get(POLICY_KEY)).await?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_else(axiom_agent::gate::policy::default_policy))
}

/// `[agent.policy]` in the workspace's `.axiom/project.toml` (stricter only), if any.
/// A malformed table is `invalid_params` with the reason, as for the project file's
/// `[environment]`.
pub(crate) async fn workspace_policy(
    app: &App,
    id: &WorkspaceId,
) -> Result<Option<Policy>, RpcError> {
    let ws = app.workspaces.get(id)?;
    let path = WorkspacePath(".axiom/project.toml".into());
    let text = match ws.read_text(&path).await {
        Ok(f) => f.text,
        Err(axiom_workspace::WsError::NotFound(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    parse_workspace_policy(&text)
}

pub(crate) fn parse_workspace_policy(text: &str) -> Result<Option<Policy>, RpcError> {
    axiom_agent::gate::policy::from_project_toml(text).map_err(|reason| {
        RpcError::new(
            ErrorKind::InvalidParams,
            "`.axiom/project.toml` has an invalid `[agent.policy]`.",
        )
        .with_details(serde_json::json!({ "reason": reason }))
    })
}

pub(crate) async fn policies(app: &App, id: &WorkspaceId) -> Result<Policies, RpcError> {
    Ok(Policies {
        user: user_policy(app).await?,
        workspace: workspace_policy(app, id).await?,
        protected: configured_hooks(app.workspaces.get(id)?.as_ref())
            .await
            .into_iter()
            .collect(),
    })
}

/// The workspace's configured git hooks directory (`core.hooksPath` in its `.git/config`) as a
/// workspace path, when it is inside the workspace: writes there always ask, as `.git/hooks`
/// does (review rev3). Read through the workspace (nothing outside it is followed) from the
/// workspace's own `.git/config` only: not an `[include]`d file, not the global or system
/// configuration, and not a `.git` file's repository (a worktree's, a submodule's). A value
/// naming the workspace root itself is not used (the whole workspace is not a hooks folder).
async fn configured_hooks(ws: &axiom_workspace::Workspace) -> Option<WorkspacePath> {
    use std::path::{Component, Path, PathBuf};
    let config = ws
        .read_text(&WorkspacePath(".git/config".into()))
        .await
        .ok()?
        .text;
    let value = axiom_agent::gate::hooks_path(&config)?;
    // git expands a leading `~/`; a relative path is from the top of the work tree.
    let path = match value.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
        None => PathBuf::from(&value),
    };
    let rel = if path.is_absolute() {
        match path.strip_prefix(ws.root()) {
            Ok(r) => r.to_path_buf(),
            Err(_) => std::fs::canonicalize(&path)
                .ok()?
                .strip_prefix(ws.root())
                .ok()?
                .to_path_buf(),
        }
    } else {
        path
    };
    let mut parts = Vec::new();
    for c in Path::new(&rel).components() {
        match c {
            Component::Normal(s) => parts.push(s.to_str()?.to_string()),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    ws.canonical_path(&WorkspacePath(parts.join("/"))).ok()
}

pub(crate) fn audit_path(app: &App, root: &std::path::Path) -> std::path::PathBuf {
    app.env
        .data_dir
        .join("audit")
        .join(format!("{}.jsonl", axiom_agent::workspace_key(root)))
}

pub async fn get_policy(app: Arc<App>, p: PolicyParams) -> Result<PolicyResult, RpcError> {
    Ok(PolicyResult {
        user: user_policy(&app).await?,
        workspace: match &p.workspace_id {
            Some(id) => workspace_policy(&app, id).await?,
            None => None,
        },
    })
}

pub async fn set_policy(app: Arc<App>, p: Policy) -> Result<Empty, RpcError> {
    let v = serde_json::to_value(p).map_err(|_| crate::app::internal())?;
    app.store_call(move |s| s.settings_set(&BTreeMap::from([(POLICY_KEY.to_string(), Some(v))])))
        .await?;
    Ok(Empty {})
}

pub async fn approve(app: Arc<App>, p: ApproveParams) -> Result<Empty, RpcError> {
    app.agent
        .gate
        .approve(&p.request_id, p.decision)
        .await
        .map_err(|_| {
            RpcError::new(
                ErrorKind::NotFound,
                "No approval with that id is pending (it may have been answered already).",
            )
        })?;
    Ok(Empty {})
}

/// The connections that made tool calls from outside a session, numbered.
static CONNECTIONS: Mutex<Vec<(Notifier, u64)>> = Mutex::new(Vec::new());

/// The principal a call from outside a session runs under: the name it declared, made this
/// connection's own (`mcp:<client>#c<n>`). Two clients that both call themselves
/// `claude-code` share no grants, and a connection's grants end with it (AUD-009, AUD-109).
fn connection_principal(app: &App, declared: &str, notifier: &Notifier) -> String {
    let mut c = CONNECTIONS.lock().unwrap_or_else(|p| p.into_inner());
    let closed: Vec<String> = c
        .iter()
        .filter(|(n, _)| n.is_closed())
        .map(|(_, id)| format!("#c{id}"))
        .collect();
    if !closed.is_empty() {
        c.retain(|(n, _)| !n.is_closed());
        app.agent
            .gate
            .retain_grants(|p| !closed.iter().any(|tag| p.ends_with(tag.as_str())));
    }
    let id = match c.iter().find(|(n, _)| n.same_connection(notifier)) {
        Some((_, id)) => *id,
        None => {
            let id = c.iter().map(|(_, id)| id + 1).max().unwrap_or(1);
            c.push((notifier.clone(), id));
            id
        }
    };
    format!("{declared}#c{id}")
}

/// One tool call from outside a session (the MCP front end, the CLI). The principal must say
/// where it comes from (`mcp:<client>` or `cli`), so it can never use a session's grants.
pub async fn tool_call(
    app: Arc<App>,
    p: ToolCallParams,
    ctx: RequestCtx,
) -> Result<ToolCallResult, RpcError> {
    let ok_principal = p.principal == "cli"
        || p.principal
            .strip_prefix("mcp:")
            .is_some_and(|n| !n.is_empty() && n.len() <= 64);
    if !ok_principal {
        return Err(RpcError::new(
            ErrorKind::InvalidParams,
            "The principal must be `cli` or `mcp:<client name>`.",
        ));
    }
    let ws = app.workspaces.get(&p.workspace_id)?;
    // Checkpoints stay under the declared name (one key per client, as before).
    let key: String = format!(
        "ext-{}",
        p.principal
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            })
            .collect::<String>()
    );
    let tctx = ToolCtx {
        workspace_id: p.workspace_id.clone(),
        principal: connection_principal(&app, &p.principal, &ctx.notifier),
        cancel: ctx.cancel.clone(),
    };
    let policies = policies(&app, &p.workspace_id).await?;
    let audit = audit_path(&app, ws.root());
    let shadow = crate::handlers::sessions::shadow(&app, ws.root());
    let hook = ExternalCheckpoint {
        shadow: &shadow,
        key: &key,
        principal: &tctx.principal,
    };
    let out = axiom_agent::gate::call_tool(
        &app.agent.tools,
        &app.agent.gate,
        &tctx,
        None,
        &p.tool,
        p.input,
        &policies,
        &hook,
        Some(&audit),
    )
    .await;
    if out.effect != axiom_protocol::agent::Effect::Read && !out.is_error {
        if let Err(e) = shadow.record_post(&key).await {
            tracing::warn!("cannot record the post-state: {e}");
        }
    }
    if let Some(r) = out.refusal {
        return Err(RpcError::new(r.kind, r.message));
    }
    Ok(ToolCallResult {
        output: out.output.output,
        text: out.output.text,
        is_error: out.is_error,
        truncated: out.output.truncated,
    })
}

/// A checkpoint before every writing call from outside a session (every write is preceded by
/// one, whoever asks; ADR-0015 §5).
struct ExternalCheckpoint<'a> {
    shadow: &'a axiom_agent::checkpoint::Shadow,
    key: &'a str,
    principal: &'a str,
}

impl axiom_agent::gate::BeforeExecute for ExternalCheckpoint<'_> {
    fn before<'b>(
        &'b self,
        p: &'b axiom_agent::tools::Prepared,
    ) -> futures_util::future::BoxFuture<'b, Result<(), axiom_agent::tools::ToolError>> {
        Box::pin(async move {
            if p.effect == axiom_protocol::agent::Effect::Read {
                return Ok(());
            }
            self.shadow
                .create(
                    self.key,
                    &format!("before {} ({})", p.name, self.principal),
                    0,
                )
                .await
                .map(|_| ())
                .map_err(axiom_agent::agent_loop::checkpoint_error)
        })
    }
}

/// `agent/installAgent` (ADR-0020 §6): `npm install --prefix <data dir>/agents <package>`,
/// bounded to ten minutes; the agent's program is then found there.
pub async fn install_agent(
    app: Arc<App>,
    p: axiom_protocol::agent::InstallAgentParams,
) -> Result<axiom_protocol::agent::InstallAgentResult, RpcError> {
    use axiom_agent::acp;
    let package = acp::builtin_package(&p.provider_id).ok_or_else(|| {
        RpcError::new(
            ErrorKind::NotFound,
            format!("`{}` is not an agent Axiom can install.", p.provider_id),
        )
    })?;
    let npm = acp::find_program("npm").ok_or_else(|| {
        RpcError::new(
            ErrorKind::PrerequisiteMissing,
            "Installing an agent needs npm (Node.js). Install Node.js, then try again.",
        )
    })?;
    let prefix = app.env.data_dir.join("agents");
    tokio::fs::create_dir_all(&prefix).await.map_err(|e| {
        RpcError::new(
            ErrorKind::Io,
            format!("Could not create {}: {e}", prefix.display()),
        )
    })?;
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(600),
        tokio::process::Command::new(&npm)
            .args(["install", "--no-fund", "--no-audit", "--prefix"])
            .arg(&prefix)
            .arg(package)
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| RpcError::new(ErrorKind::Timeout, "npm did not finish within 10 minutes."))?
    .map_err(|e| RpcError::new(ErrorKind::SpawnFailed, format!("Could not run npm: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = err.lines().rev().take(8).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(RpcError::new(
            ErrorKind::Io,
            format!("npm could not install {package}:\n{}", tail.join("\n")),
        ));
    }
    let configs = provider_configs(&app).await?;
    let program = app
        .agent
        .providers
        .config(&configs, &p.provider_id)
        .and_then(|c| c.command.first().cloned())
        .and_then(|cmd| acp::find_program(&cmd))
        .ok_or_else(|| {
            RpcError::new(
                ErrorKind::Io,
                format!(
                    "{package} was installed, but its program was not found in {}.",
                    prefix.display()
                ),
            )
        })?;
    Ok(axiom_protocol::agent::InstallAgentResult {
        program: program.to_string_lossy().into_owned(),
    })
}
