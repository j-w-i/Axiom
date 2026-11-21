//! `agent/*` handlers (protocol 0.3, ADR-0015). Thin: read what the service needs from the
//! store, call it, map its error.

use crate::app::App;
use axiom_agent::credentials::CredentialError;
use axiom_agent::providers::ProvidersError;
use axiom_protocol::agent::{
    ConfigureProviderParams, ModelsParams, ModelsResult, ProviderConfig, ToolsResult,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{ErrorKind, RpcError};
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
