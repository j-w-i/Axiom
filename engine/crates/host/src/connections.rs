//! Accounts, keys and servers: any number for each provider. What they are is in a file; their
//! secrets are in a [`Vault`] and nowhere else (SI-3).

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use axiom_model::{Client, Credential, Endpoint, Options, Profile, Secret};
use axiom_proto::{Connection, ErrorCode, WireError, limits};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::paths::{private_dir, private_file};

/// Cleared when it is dropped.
pub type Text = Zeroizing<String>;

/// Where secrets are.
pub trait Vault: Send + Sync {
    fn put(&self, id: &str, secret: &Text) -> Result<(), String>;
    fn get(&self, id: &str) -> Result<Option<Text>, String>;
    fn remove(&self, id: &str) -> Result<(), String>;
    /// Whether a secret is still there after the host has stopped.
    fn lasts(&self) -> bool {
        false
    }
}

/// Keeps secrets as long as the host runs.
#[derive(Default)]
pub struct MemoryVault(Mutex<HashMap<String, Text>>);

impl MemoryVault {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Text>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
impl Vault for MemoryVault {
    fn put(&self, id: &str, secret: &Text) -> Result<(), String> {
        self.lock().insert(id.into(), secret.clone());
        Ok(())
    }
    fn get(&self, id: &str) -> Result<Option<Text>, String> {
        Ok(self.lock().get(id).cloned())
    }
    fn remove(&self, id: &str) -> Result<(), String> {
        self.lock().remove(id);
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kept {
    id: String,
    name: String,
    provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    secret: bool,
}

fn secret_of(t: &Text) -> Secret {
    Secret::new(t.to_string())
}

/// Most connections.
const MOST: usize = 200;
const NAME: usize = 200;
const SECRET: usize = 16 * 1024;

/// A program's own accounts, as providers.
const PROGRAMS: &[(&str, &str, &str, &str)] =
    &[("claude-code", "claude", "CLAUDE_CONFIG_DIR", "claude auth login")];

/// What an account of the harness is called as a provider.
pub fn provider_of(harness: &str) -> Option<&'static str> {
    PROGRAMS.iter().find(|p| p.1 == harness).map(|p| p.0)
}

pub struct Connections {
    /// Where programs keep accounts beside their first: a folder for each. What is in them is the
    /// programs' own; nothing here reads it.
    accounts: PathBuf,
    file: PathBuf,
    vault: Arc<dyn Vault>,
    all: Mutex<Vec<Kept>>,
}

fn bad(words: impl Into<String>) -> WireError {
    WireError::new(ErrorCode::BadRequest, words)
}
fn internal(words: impl std::fmt::Display) -> WireError {
    WireError::new(ErrorCode::Internal, words.to_string())
}

/// What is known of a provider. `None`: not one Axiom may use (decision 0007).
fn profile(provider: &str, url: Option<&str>) -> Option<Profile> {
    match provider {
        "anthropic" => Some(Profile::anthropic()),
        "openai" => Some(Profile::openai()),
        "gemini" => Some(Profile::gemini()),
        "openrouter" => Some(Profile::openrouter()),
        "lm-studio" => Some(Profile::lm_studio()),
        "openai-compatible" => Some(Profile::openai_compatible("openai-compatible", url?)),
        _ => None,
    }
}

/// Those that are always at their own address and answer no one without a key.
fn keyed(provider: &str) -> bool {
    matches!(provider, "anthropic" | "openai" | "gemini" | "openrouter")
}

impl Connections {
    /// As they were kept. A file that cannot be read is an error: nothing is written over it.
    pub fn open(file: &Path, vault: Arc<dyn Vault>) -> io::Result<Self> {
        let all = match std::fs::read_to_string(file) {
            Ok(t) => serde_json::from_str(&t)
                .map_err(|e| io::Error::other(format!("{}: {e}", file.display())))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        let accounts = file.with_file_name("accounts");
        Ok(Self { accounts, file: file.into(), vault, all: Mutex::new(all) })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Kept>> {
        self.all.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, all: &[Kept]) -> Result<(), WireError> {
        let text = serde_json::to_string_pretty(all).map_err(internal)?;
        private_file(&self.file, &text).map_err(internal)
    }

    pub fn list(&self) -> Vec<Connection> {
        let shown = |k: &Kept| Connection {
            id: k.id.clone(),
            name: k.name.clone(),
            provider: k.provider.clone(),
            url: k.url.clone(),
            secret: k.secret,
            lasts: k.secret && self.vault.lasts(),
            sign_in: self.sign_in(k),
        };
        self.lock().iter().map(shown).collect()
    }

    fn folder(&self, id: &str) -> PathBuf {
        self.accounts.join(id)
    }

    /// For the person's own terminal: nothing here signs in, or sees what that gives.
    fn sign_in(&self, k: &Kept) -> Option<String> {
        let p = PROGRAMS.iter().find(|p| p.0 == k.provider)?;
        let dir = self.folder(&k.id).display().to_string();
        Some(if cfg!(windows) {
            format!("$env:{}='{}'; {}", p.2, dir.replace('\'', "''"), p.3)
        } else {
            format!("{}='{}' {}", p.2, dir.replace('\'', "'\\''"), p.3)
        })
    }

    /// Where a program keeps an account of this harness.
    pub fn account(&self, id: &str, harness: &str) -> Result<PathBuf, WireError> {
        let k = self.lock().iter().find(|k| k.id == id).cloned();
        let k = k.ok_or_else(|| WireError::new(ErrorCode::NotFound, "no such account"))?;
        if provider_of(harness) != Some(k.provider.as_str()) {
            return Err(bad("this account is not one of that program"));
        }
        let dir = self.folder(id);
        private_dir(&self.accounts).map_err(internal)?;
        private_dir(&dir).map_err(internal)?;
        Ok(dir)
    }

    pub fn add(
        &self,
        name: String,
        provider: String,
        url: Option<String>,
        secret: Option<String>,
    ) -> Result<String, WireError> {
        if name.trim().is_empty() || limits::text_len(&name) > NAME {
            return Err(bad("a connection's name has between 1 and 200 bytes"));
        }
        // Cleared when dropped, also if it is refused.
        let secret = secret.map(Zeroizing::new).map(|s| {
            if s.len() > SECRET || s.trim().is_empty() {
                return Err(bad("a secret has between 1 and 16384 bytes"));
            }
            Ok(s)
        });
        let secret = secret.transpose()?;
        if PROGRAMS.iter().any(|p| p.0 == provider) {
            if url.is_some() || secret.is_some() {
                return Err(bad(
                    "the program signs in by itself: it is given no address and no key",
                ));
            }
            return self.keep(name, provider, None, None);
        }
        let p = profile(&provider, url.as_deref())
            .ok_or_else(|| bad("the provider is not one that can be added, or needs an address"))?;
        if keyed(&provider) && (url.is_some() || secret.is_none()) {
            return Err(bad("a connection to this provider has a key and no address"));
        }
        let address =
            url.clone().or_else(|| p.base_url.clone()).ok_or_else(|| bad("needs an address"))?;
        // Understood before it is kept: an address the secret could not be sent to is refused here.
        let endpoint = Endpoint::parse(&address).map_err(|e| bad(e.to_string()))?;
        if let Some(s) = &secret {
            Credential::new(endpoint.origin(), &p.auth, secret_of(s))
                .map_err(|e| bad(e.to_string()))?;
        }
        self.keep(name, provider, url, secret)
    }

    fn keep(
        &self,
        name: String,
        provider: String,
        url: Option<String>,
        secret: Option<Text>,
    ) -> Result<String, WireError> {
        let program = PROGRAMS.iter().any(|p| p.0 == provider);
        let mut all = self.lock();
        if all.len() >= MOST {
            return Err(bad("there are as many connections as there may be"));
        }
        let id = axiom_proto::RequestId::new().0.replacen("q_", "c_", 1);
        if let Some(s) = &secret {
            self.vault.put(&id, s).map_err(internal)?;
        }
        if program {
            // There when it is signed in to.
            private_dir(&self.accounts).map_err(internal)?;
            private_dir(&self.folder(&id)).map_err(internal)?;
        }
        let mut next = all.clone();
        next.push(Kept { id: id.clone(), name, provider, url, secret: secret.is_some() });
        if let Err(e) = self.write(&next) {
            let _ = self.vault.remove(&id);
            return Err(e);
        }
        *all = next;
        Ok(id)
    }

    /// The secret goes first: one without a connection would stay for good.
    pub fn remove(&self, id: &str) -> Result<(), WireError> {
        let mut all = self.lock();
        let Some(at) = all.iter().position(|k| k.id == id) else {
            return Err(WireError::new(ErrorCode::NotFound, "no such connection"));
        };
        if all[at].secret {
            self.vault.remove(id).map_err(internal)?;
        }
        let mut next = all.clone();
        next.remove(at);
        self.write(&next)?;
        *all = next;
        Ok(())
    }

    /// A client for it.
    pub fn client(&self, id: &str) -> Result<Client, WireError> {
        let k = self.lock().iter().find(|k| k.id == id).cloned();
        let k = k.ok_or_else(|| WireError::new(ErrorCode::NotFound, "no such connection"))?;
        let p = profile(&k.provider, k.url.as_deref())
            .ok_or_else(|| bad("the provider is not known"))?;
        let address =
            k.url.or_else(|| p.base_url.clone()).ok_or_else(|| bad("needs an address"))?;
        let endpoint = Endpoint::parse(&address).map_err(|e| bad(e.to_string()))?;
        let credential = if k.secret {
            let s = self.vault.get(&k.id).map_err(internal)?.ok_or_else(|| {
                bad("the secret of this connection is not in the system's store any more; add it again")
            })?;
            Some(
                Credential::new(endpoint.origin(), &p.auth, secret_of(&s))
                    .map_err(|e| bad(e.to_string()))?,
            )
        } else {
            None
        };
        Client::new(p, endpoint, credential, Options::default()).map_err(|e| bad(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct T {
        d: tempfile::TempDir,
        vault: Arc<MemoryVault>,
    }
    impl T {
        fn new() -> Self {
            Self { d: tempfile::tempdir().unwrap(), vault: Arc::default() }
        }
        fn file(&self) -> PathBuf {
            self.d.path().join("connections.json")
        }
        fn open(&self) -> Connections {
            Connections::open(&self.file(), self.vault.clone()).unwrap()
        }
    }
    const CANARY: &str = "sk-canary-81723";

    #[test]
    fn several_for_one_provider_and_no_secret_in_the_file() {
        let t = T::new();
        let c = t.open();
        let a = c.add("Mine".into(), "anthropic".into(), None, Some(CANARY.into())).unwrap();
        let b = c.add("Work".into(), "anthropic".into(), None, Some("other".into())).unwrap();
        let l = c.add("Here".into(), "lm-studio".into(), None, None).unwrap();
        assert!(a != b && c.list().len() == 3);
        let text = std::fs::read_to_string(t.file()).unwrap();
        assert!(!text.contains(CANARY), "{text}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(t.file()).unwrap().permissions().mode() & 0o077, 0);
        }
        let again = t.open();
        assert_eq!(again.list(), c.list());
        assert!(again.client(&a).is_ok() && again.client(&l).is_ok());
        again.remove(&a).unwrap();
        assert!(t.vault.get(&a).unwrap().is_none() && t.vault.get(&b).unwrap().is_some());
        assert!(again.client(&a).is_err() && again.remove(&a).is_err());
        assert_eq!(t.open().list().len(), 2);
        // Its secret is gone from the store.
        t.vault.remove(&b).unwrap();
        assert!(again.client(&b).unwrap_err().message.contains("add it again"));
    }

    #[test]
    fn a_key_goes_to_its_providers_own_address() {
        let t = T::new();
        let c = t.open();
        for (provider, address) in [
            ("anthropic", "https://api.anthropic.com/v1"),
            ("openai", "https://api.openai.com/v1"),
            ("gemini", "https://generativelanguage.googleapis.com/v1beta"),
            ("openrouter", "https://openrouter.ai/api/v1"),
        ] {
            let id = c.add("Mine".into(), provider.into(), None, Some(CANARY.into())).unwrap();
            let client = t.open().client(&id).unwrap();
            assert_eq!(client.profile().provider, provider);
            assert_eq!(client.profile().base_url.as_deref(), Some(address));
            let listed = c.list().into_iter().find(|k| k.id == id).unwrap();
            assert_eq!(
                (listed.provider.as_str(), listed.url, listed.secret),
                (provider, None, true)
            );
        }
        assert!(!std::fs::read_to_string(t.file()).unwrap().contains(CANARY));
    }

    #[test]
    fn what_cannot_be_used_is_not_kept() {
        let t = T::new();
        let c = t.open();
        for (name, provider, url, secret) in [
            ("", "lm-studio", None, None),
            ("x", "nobody", None, None),
            ("x", "anthropic", None, None),
            ("x", "anthropic", Some("https://evil.example/v1"), Some(CANARY)),
            ("x", "openai-compatible", None, None),
            ("x", "openai-compatible", Some("http://far.example/v1"), Some(CANARY)),
            ("x", "openai-compatible", Some("ftp://far.example"), None),
            ("x", "openai-compatible", Some("https://u:p@far.example"), None),
            ("x", "anthropic", None, Some(" ")),
            ("x", "anthropic", None, Some("a\nb")),
            ("x", "openai", None, None),
            ("x", "openai", Some("https://api.openai.com/v1"), Some(CANARY)),
            ("x", "gemini", None, None),
            ("x", "gemini", Some("https://evil.example/v1"), Some(CANARY)),
            ("x", "openrouter", None, None),
            ("x", "openrouter", Some("https://evil.example/v1"), Some(CANARY)),
            ("x", "OpenAI", None, Some(CANARY)),
        ] {
            let e =
                c.add(name.into(), provider.into(), url.map(Into::into), secret.map(Into::into));
            let e = e.expect_err(&format!("{name} {provider} {url:?}"));
            assert!(!format!("{e:?}").contains(CANARY));
        }
        assert!(c.list().is_empty() && t.vault.lock().is_empty() && !t.file().exists());
        std::fs::write(t.file(), "{").unwrap();
        assert!(Connections::open(&t.file(), t.vault.clone()).is_err());
    }

    /// A store that takes nothing: the connection is not made.
    #[test]
    fn a_store_that_fails_is_an_error() {
        struct No;
        impl Vault for No {
            fn put(&self, _: &str, _: &Text) -> Result<(), String> {
                Err("locked".into())
            }
            fn get(&self, _: &str) -> Result<Option<Text>, String> {
                Err("locked".into())
            }
            fn remove(&self, _: &str) -> Result<(), String> {
                Err("locked".into())
            }
        }
        let t = T::new();
        let c = Connections::open(&t.file(), Arc::new(No)).unwrap();
        let e = c.add("x".into(), "anthropic".into(), None, Some(CANARY.into())).unwrap_err();
        assert!(e.message.contains("locked") && c.list().is_empty());
    }

    /// One place says how, another where: they are of one mind.
    #[test]
    fn programs_with_accounts_are_those_that_document_it() {
        for k in crate::delegated::known() {
            let here = PROGRAMS.iter().find(|p| p.1 == k.id).map(|p| p.2);
            assert_eq!(here, k.harness.accounts(), "{}", k.id);
        }
    }
}
