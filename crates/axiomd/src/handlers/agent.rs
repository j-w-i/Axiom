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
use axiom_rpc::RequestCtx;
use std::collections::BTreeMap;
use std::sync::Arc;

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
    })
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
    let tctx = ToolCtx {
        workspace_id: p.workspace_id.clone(),
        principal: p.principal,
        cancel: ctx.cancel.clone(),
    };
    let policies = policies(&app, &p.workspace_id).await?;
    let audit = audit_path(&app, ws.root());
    let out = axiom_agent::gate::call_tool(
        &app.agent.tools,
        &app.agent.gate,
        &tctx,
        None,
        &p.tool,
        p.input,
        &policies,
        &axiom_agent::gate::NoHook,
        Some(&audit),
    )
    .await;
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
