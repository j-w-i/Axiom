//! API keys (ADR-0015 §3): the OS credential store through `keyring` (Secret Service on Linux,
//! Keychain on macOS, Credential Manager on Windows), under service `axiom`, account
//! `<provider id>` (shown as `axiom/<provider id>`). A key never reaches settings, SQLite, the
//! journal, logs or fixtures.
//!
//! The calls block (D-Bus, Keychain); callers use [`blocking`], which runs them on a plain
//! thread (the Secret Service backend drives its own executor and must not run on a Tokio
//! thread).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    /// No credential store is reachable (no Secret Service on this desktop, locked keychain).
    #[error("the system credential store is not available ({0}); name an environment variable for the key instead")]
    Unavailable(String),
    #[error("the credential store failed: {0}")]
    Failed(String),
}

pub trait Credentials: Send + Sync {
    fn get(&self, provider_id: &str) -> Result<Option<String>, CredentialError>;
    fn set(&self, provider_id: &str, secret: &str) -> Result<(), CredentialError>;
    /// Deleting a key that does not exist is not an error.
    fn delete(&self, provider_id: &str) -> Result<(), CredentialError>;
}

pub const SERVICE: &str = "axiom";

/// The platform store.
#[derive(Debug, Default)]
pub struct KeyringCredentials;

fn map(e: keyring::Error) -> CredentialError {
    match e {
        keyring::Error::NoStorageAccess(inner) => CredentialError::Unavailable(inner.to_string()),
        keyring::Error::PlatformFailure(inner) => CredentialError::Unavailable(inner.to_string()),
        other => CredentialError::Failed(other.to_string()),
    }
}

impl Credentials for KeyringCredentials {
    fn get(&self, provider_id: &str) -> Result<Option<String>, CredentialError> {
        let entry = keyring::Entry::new(SERVICE, provider_id).map_err(map)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map(e)),
        }
    }

    fn set(&self, provider_id: &str, secret: &str) -> Result<(), CredentialError> {
        keyring::Entry::new(SERVICE, provider_id)
            .and_then(|e| e.set_password(secret))
            .map_err(map)
    }

    fn delete(&self, provider_id: &str) -> Result<(), CredentialError> {
        let entry = keyring::Entry::new(SERVICE, provider_id).map_err(map)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map(e)),
        }
    }
}

/// In memory (unit tests).
#[derive(Debug, Default)]
pub struct MemoryCredentials(Mutex<BTreeMap<String, String>>);

impl Credentials for MemoryCredentials {
    fn get(&self, id: &str) -> Result<Option<String>, CredentialError> {
        Ok(lock(&self.0).get(id).cloned())
    }
    fn set(&self, id: &str, s: &str) -> Result<(), CredentialError> {
        lock(&self.0).insert(id.into(), s.into());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), CredentialError> {
        lock(&self.0).remove(id);
        Ok(())
    }
}

/// TEST ONLY: a JSON file standing in for the OS store, so integration tests of the daemon
/// binary never touch the developer's keychain. Selected by `AXIOM_TEST_CREDENTIALS_FILE`.
#[derive(Debug)]
pub struct FileCredentials(pub PathBuf);

impl FileCredentials {
    fn load(&self) -> BTreeMap<String, String> {
        std::fs::read_to_string(&self.0)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    /// Written readable by the owner only (`0600` on Unix): it holds keys in plain text
    /// (AUD-109).
    fn save(&self, m: &BTreeMap<String, String>) -> Result<(), CredentialError> {
        use std::io::Write;
        let text = serde_json::to_string(m).map_err(|e| CredentialError::Failed(e.to_string()))?;
        let failed = |e: std::io::Error| CredentialError::Failed(e.to_string());
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        let mut f = o.open(&self.0).map_err(failed)?;
        // A file that existed with wider permissions is narrowed too.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(failed)?;
        }
        f.write_all(text.as_bytes()).map_err(failed)
    }
}

impl Credentials for FileCredentials {
    fn get(&self, id: &str) -> Result<Option<String>, CredentialError> {
        Ok(self.load().get(id).cloned())
    }
    fn set(&self, id: &str, s: &str) -> Result<(), CredentialError> {
        let mut m = self.load();
        m.insert(id.into(), s.into());
        self.save(&m)
    }
    fn delete(&self, id: &str) -> Result<(), CredentialError> {
        let mut m = self.load();
        m.remove(id);
        self.save(&m)
    }
}

pub const TEST_FILE_ENV: &str = "AXIOM_TEST_CREDENTIALS_FILE";

/// The store the daemon uses: the test file when `AXIOM_TEST_CREDENTIALS_FILE` is set, else
/// the platform store.
pub fn platform_or_test() -> Box<dyn Credentials> {
    match std::env::var_os(TEST_FILE_ENV) {
        Some(p) => {
            tracing::warn!("{TEST_FILE_ENV} is set: API keys are kept in a plain file");
            Box::new(FileCredentials(PathBuf::from(p)))
        }
        None => Box::new(KeyringCredentials),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Runs a blocking credential call on its own thread and awaits it.
pub async fn blocking<T, F>(f: F) -> Result<T, CredentialError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CredentialError> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("axiom-credentials".into())
        .spawn(move || {
            let _ = tx.send(f());
        })
        .map_err(|e| CredentialError::Failed(e.to_string()))?;
    rx.await
        .map_err(|_| CredentialError::Failed("the credential call panicked".into()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let s = FileCredentials(dir.path().join("c.json"));
        assert_eq!(s.get("a").unwrap(), None);
        s.set("a", "k1").unwrap();
        assert_eq!(s.get("a").unwrap().as_deref(), Some("k1"));
        s.delete("a").unwrap();
        s.delete("a").unwrap();
        assert_eq!(s.get("a").unwrap(), None);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("c.json"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    /// Exercises the real platform store; run with `--features keyring-live` on a desktop
    /// session (M3-MODEL-002 acceptance 3).
    #[cfg(feature = "keyring-live")]
    #[tokio::test]
    async fn keyring_live_round_trip() {
        let id = format!("axiom-test-{}", std::process::id());
        let id2 = id.clone();
        blocking(move || KeyringCredentials.set(&id2, "secret-value"))
            .await
            .unwrap();
        let id2 = id.clone();
        let got = blocking(move || KeyringCredentials.get(&id2))
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("secret-value"));
        let id2 = id.clone();
        blocking(move || KeyringCredentials.delete(&id2))
            .await
            .unwrap();
        let got = blocking(move || KeyringCredentials.get(&id)).await.unwrap();
        assert_eq!(got, None);
    }
}
