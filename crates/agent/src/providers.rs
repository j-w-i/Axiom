//! The configured model providers (settings `agent.providers`, a JSON array of
//! `ProviderConfig`) turned into [`ModelProvider`]s, with their keys from the credential store
//! or the environment. Built providers are cached until their configuration changes.

use crate::credentials::{blocking, CredentialError, Credentials};
use crate::model::anthropic::Anthropic;
use crate::model::openai::OpenAiChat;
use crate::model::profiles::{self, Server};
use crate::model::scripted::ScriptedProvider;
use crate::model::ModelProvider;
use axiom_protocol::agent::{
    ConfigureProviderParams, ProviderConfig, ProviderKind, ProviderStatus,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Id of the test-only scripted provider (offered only when the daemon was given a script).
pub const SCRIPTED_ID: &str = "scripted";
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProvidersError {
    #[error("no provider `{0}` is configured")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Credentials(#[from] CredentialError),
}

type Cached = (ProviderConfig, Arc<dyn ModelProvider>);

pub struct Providers {
    creds: Arc<dyn Credentials>,
    scripted: Option<Arc<ScriptedProvider>>,
    cache: Mutex<HashMap<String, Cached>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn scripted_config() -> ProviderConfig {
    ProviderConfig {
        id: SCRIPTED_ID.into(),
        kind: ProviderKind::Scripted,
        name: "Scripted (test)".into(),
        base_url: None,
        api_key_env: None,
        models: vec![],
        capabilities: None,
        command: vec![],
        env: Default::default(),
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

impl Providers {
    pub fn new(creds: Arc<dyn Credentials>, scripted: Option<Arc<ScriptedProvider>>) -> Self {
        Self {
            creds,
            scripted,
            cache: Mutex::default(),
        }
    }

    /// The scripted provider, when the daemon runs with a test script.
    pub fn scripted(&self) -> Option<&Arc<ScriptedProvider>> {
        self.scripted.as_ref()
    }

    /// `user` plus the scripted provider when enabled.
    pub fn all(&self, user: &[ProviderConfig]) -> Vec<ProviderConfig> {
        let mut v: Vec<ProviderConfig> = user
            .iter()
            .filter(|c| c.kind != ProviderKind::Scripted)
            .cloned()
            .collect();
        // The built-in external agents (ADR-0016), unless the user configured the same id.
        for b in crate::acp::builtin_configs() {
            if !v.iter().any(|c| c.id == b.id) {
                v.push(b);
            }
        }
        if self.scripted.is_some() {
            v.push(scripted_config());
        }
        v
    }

    async fn key(&self, c: &ProviderConfig) -> Result<Option<String>, CredentialError> {
        if let Some(var) = &c.api_key_env {
            return Ok(std::env::var(var).ok().filter(|k| !k.is_empty()));
        }
        let creds = self.creds.clone();
        let id = c.id.clone();
        blocking(move || creds.get(&id)).await
    }

    /// The capability profile of `model` on provider `id` (no network).
    pub fn capabilities(
        &self,
        user: &[ProviderConfig],
        id: &str,
        model: &str,
    ) -> Option<axiom_protocol::agent::Capabilities> {
        let c = self.all(user).into_iter().find(|c| c.id == id)?;
        let base = c.base_url.clone().unwrap_or_else(|| match c.kind {
            ProviderKind::OpenaiChat => crate::model::openai::DEFAULT_BASE_URL.into(),
            _ => String::new(),
        });
        Some(profiles::apply(
            profiles::profile(c.kind, &base, model),
            c.capabilities.as_ref(),
        ))
    }

    /// The configuration of `id`.
    pub fn config(&self, user: &[ProviderConfig], id: &str) -> Option<ProviderConfig> {
        self.all(user).into_iter().find(|c| c.id == id)
    }

    /// The provider for `id`, built (or taken from the cache) with its current key.
    pub async fn get(
        &self,
        user: &[ProviderConfig],
        id: &str,
    ) -> Result<Arc<dyn ModelProvider>, ProvidersError> {
        let config = self
            .all(user)
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| ProvidersError::NotFound(id.into()))?;
        if let Some((c, p)) = lock(&self.cache).get(id) {
            if *c == config {
                return Ok(p.clone());
            }
        }
        let p: Arc<dyn ModelProvider> = match config.kind {
            ProviderKind::Scripted => match &self.scripted {
                Some(s) => s.clone(),
                None => return Err(ProvidersError::NotFound(id.into())),
            },
            // ACP agents run their own loop (M3-ACP-001); they are not model providers.
            ProviderKind::Acp => {
                return Err(ProvidersError::Invalid(format!(
                    "`{id}` is an external agent, not a model API."
                )))
            }
            ProviderKind::Anthropic => Arc::new(
                Anthropic::new(&config, self.key(&config).await?)
                    .map_err(ProvidersError::Invalid)?,
            ),
            ProviderKind::OpenaiChat => Arc::new(
                OpenAiChat::new(&config, self.key(&config).await?)
                    .map_err(ProvidersError::Invalid)?,
            ),
        };
        lock(&self.cache).insert(id.into(), (config, p.clone()));
        Ok(p)
    }

    /// Every provider (or only `only`) with its models, listed from the server where possible.
    pub async fn status(&self, user: &[ProviderConfig], only: Option<&str>) -> Vec<ProviderStatus> {
        let mut out = Vec::new();
        for config in self.all(user) {
            if only.is_some_and(|o| o != config.id) {
                continue;
            }
            let (has_key, key_error) = match config.kind {
                ProviderKind::Scripted => (true, None),
                _ => match self.key(&config).await {
                    Ok(k) => (k.is_some(), None),
                    Err(e) => (false, Some(e.to_string())),
                },
            };
            let needs_key = match config.kind {
                ProviderKind::Anthropic => true,
                ProviderKind::OpenaiChat => {
                    profiles::server_of(
                        config
                            .base_url
                            .as_deref()
                            .unwrap_or(crate::model::openai::DEFAULT_BASE_URL),
                    ) == Server::OpenAi
                }
                ProviderKind::Scripted | ProviderKind::Acp => false,
            };
            let mut status = ProviderStatus {
                config: config.clone(),
                has_key,
                models: vec![],
                error: None,
            };
            if needs_key && !has_key {
                status.error = Some(key_error.unwrap_or_else(|| {
                    "No API key: add one in Settings → Models, or name an environment variable."
                        .into()
                }));
                out.push(status);
                continue;
            }
            if config.kind == ProviderKind::Acp {
                let program = config.command.first().cloned().unwrap_or_default();
                if crate::acp::find_program(&program).is_some() {
                    status.models = vec![axiom_protocol::agent::ModelInfo {
                        id: "default".into(),
                        display_name: config.name.clone(),
                        capabilities: profiles::profile(ProviderKind::Acp, "", "default"),
                    }];
                } else {
                    status.error = Some(match crate::acp::install_hint(&config.id) {
                        Some(h) => format!("Not installed. Install it: {h}."),
                        None => format!("`{program}` is not installed."),
                    });
                }
                out.push(status);
                continue;
            }
            match self.get(user, &config.id).await {
                Ok(p) => match tokio::time::timeout(LIST_TIMEOUT, p.models()).await {
                    Ok(Ok(models)) => status.models = models,
                    Ok(Err(e)) => status.error = Some(sentence(&e.to_string())),
                    Err(_) => status.error = Some("The server did not answer in time.".into()),
                },
                Err(e) => status.error = Some(sentence(&e.to_string())),
            }
            out.push(status);
        }
        out
    }

    /// Applies a configuration change and returns the new `agent.providers` list to store.
    pub async fn configure(
        &self,
        mut user: Vec<ProviderConfig>,
        p: ConfigureProviderParams,
    ) -> Result<Vec<ProviderConfig>, ProvidersError> {
        if !valid_id(&p.id) {
            return Err(ProvidersError::Invalid(
                "A provider id is 1–40 lowercase letters, digits or hyphens.".into(),
            ));
        }
        if p.id == SCRIPTED_ID {
            return Err(ProvidersError::Invalid(format!(
                "`{SCRIPTED_ID}` is reserved."
            )));
        }
        match &p.config {
            Some(c) => {
                if c.id != p.id {
                    return Err(ProvidersError::Invalid(
                        "The configuration's id must equal `id`.".into(),
                    ));
                }
                if c.kind == ProviderKind::Scripted {
                    return Err(ProvidersError::Invalid(
                        "The scripted provider exists only in tests.".into(),
                    ));
                }
                if c.name.trim().is_empty() {
                    return Err(ProvidersError::Invalid("A provider needs a name.".into()));
                }
                if c.kind == ProviderKind::Acp && c.command.is_empty() {
                    return Err(ProvidersError::Invalid(
                        "An external agent needs its command.".into(),
                    ));
                }
                // Validates the base URL the same way the adapter will.
                let check = match c.kind {
                    ProviderKind::Acp => Ok(()),
                    ProviderKind::Anthropic => Anthropic::new(c, None).map(|_| ()),
                    _ => OpenAiChat::new(c, None).map(|_| ()),
                };
                check.map_err(ProvidersError::Invalid)?;
                match user.iter_mut().find(|x| x.id == p.id) {
                    Some(slot) => *slot = c.clone(),
                    None => user.push(c.clone()),
                }
            }
            None => {
                if !user.iter().any(|x| x.id == p.id) {
                    return Err(ProvidersError::NotFound(p.id));
                }
                user.retain(|x| x.id != p.id);
            }
        }
        let creds = self.creds.clone();
        let id = p.id.clone();
        if p.config.is_none() || p.clear_key {
            let (c, i) = (creds.clone(), id.clone());
            blocking(move || c.delete(&i)).await?;
        }
        if let Some(key) = p.api_key.filter(|k| !k.is_empty()) {
            blocking(move || creds.set(&id, &key)).await?;
        }
        lock(&self.cache).remove(&p.id);
        Ok(user)
    }
}

fn sentence(s: &str) -> String {
    let mut t = s.trim().to_string();
    if let Some(first) = t.get(..1) {
        let up = first.to_uppercase();
        t.replace_range(..1, &up);
    }
    if !t.ends_with('.') {
        t.push('.');
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryCredentials;

    fn cfg(id: &str, kind: ProviderKind, url: Option<&str>) -> ProviderConfig {
        ProviderConfig {
            id: id.into(),
            kind,
            name: id.into(),
            base_url: url.map(String::from),
            api_key_env: None,
            models: vec!["m1".into()],
            capabilities: None,
            command: vec![],
            env: Default::default(),
        }
    }

    #[tokio::test]
    async fn configure_stores_the_key_only_in_the_credential_store() {
        let creds = Arc::new(MemoryCredentials::default());
        let ps = Providers::new(creds.clone(), None);
        let list = ps
            .configure(
                vec![],
                ConfigureProviderParams {
                    id: "anth".into(),
                    config: Some(cfg("anth", ProviderKind::Anthropic, None)),
                    api_key: Some("sk-secret".into()),
                    clear_key: false,
                },
            )
            .await
            .unwrap();
        assert!(!serde_json::to_string(&list).unwrap().contains("sk-secret"));
        assert_eq!(creds.get("anth").unwrap().as_deref(), Some("sk-secret"));
        let st = ps.status(&list, None).await;
        assert!(st[0].has_key);
        assert_eq!(st[0].models[0].id, "m1");
        // Removal deletes the key too.
        let list = ps
            .configure(
                list,
                ConfigureProviderParams {
                    id: "anth".into(),
                    config: None,
                    api_key: None,
                    clear_key: false,
                },
            )
            .await
            .unwrap();
        assert!(list.is_empty());
        assert_eq!(creds.get("anth").unwrap(), None);
    }

    #[tokio::test]
    async fn configure_validates() {
        let ps = Providers::new(Arc::new(MemoryCredentials::default()), None);
        let bad = |id: &str, c: ProviderConfig| ConfigureProviderParams {
            id: id.into(),
            config: Some(c),
            api_key: None,
            clear_key: false,
        };
        for p in [
            bad("Bad Id", cfg("Bad Id", ProviderKind::Anthropic, None)),
            bad("a", cfg("b", ProviderKind::Anthropic, None)),
            bad("scripted", cfg("scripted", ProviderKind::OpenaiChat, None)),
            bad("s", cfg("s", ProviderKind::Scripted, None)),
            bad(
                "x",
                cfg("x", ProviderKind::OpenaiChat, Some("http://example.com/v1")),
            ),
        ] {
            assert!(
                matches!(
                    ps.configure(vec![], p).await,
                    Err(ProvidersError::Invalid(_))
                ),
                "accepted"
            );
        }
    }

    #[tokio::test]
    async fn status_without_a_key_says_so_and_local_servers_need_none() {
        let ps = Providers::new(Arc::new(MemoryCredentials::default()), None);
        let list = vec![
            cfg("a", ProviderKind::Anthropic, None),
            cfg(
                "local",
                ProviderKind::OpenaiChat,
                Some("http://127.0.0.1:11434/v1"),
            ),
        ];
        let st = ps.status(&list, None).await;
        assert!(st[0].error.as_deref().unwrap().contains("No API key"));
        assert!(st[1].error.is_none(), "{:?}", st[1].error);
        assert_eq!(st[1].models[0].id, "m1");
    }

    #[tokio::test]
    async fn the_scripted_provider_is_listed_only_when_enabled() {
        let ps = Providers::new(Arc::new(MemoryCredentials::default()), None);
        // Only the built-in external agents (ADR-0016).
        assert!(ps.all(&[]).iter().all(|c| c.kind == ProviderKind::Acp));
        assert!(!ps.all(&[]).iter().any(|c| c.id == SCRIPTED_ID));
        let s = Arc::new(ScriptedProvider::new(
            SCRIPTED_ID,
            serde_json::from_value(serde_json::json!({"turns":[]})).unwrap(),
        ));
        let ps = Providers::new(Arc::new(MemoryCredentials::default()), Some(s));
        assert!(ps.all(&[]).iter().any(|c| c.id == SCRIPTED_ID));
        assert!(ps.get(&[], SCRIPTED_ID).await.is_ok());
    }
}
