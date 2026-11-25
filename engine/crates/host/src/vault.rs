//! The system's own store for secrets. It is never allowed to ask anybody anything: what it would
//! have to ask for is an error, in words.
//!
//! macOS: the login keychain. An item is readable by the program that wrote it; a build that is not
//! signed is another program each time it is made, and is then refused. Windows: the Credential
//! Manager. Linux: none yet, so secrets last as long as the host.

use std::sync::Arc;

use crate::{
    connections::{MemoryVault, Vault},
    paths::Paths,
};

#[cfg(any(target_os = "macos", windows))]
const SERVICE: &str = "dev.axiom.connection";

/// For the platform's own data directory, the system's store. For any other (a test's, a trial's)
/// nothing that outlives the host: the system's store belongs to the person, not to a directory.
pub fn of(paths: &Paths) -> Arc<dyn Vault> {
    if !paths.usual() {
        return Arc::new(MemoryVault::default());
    }
    #[cfg(target_os = "macos")]
    return Arc::new(macos::Keychain::login());
    #[cfg(windows)]
    return Arc::new(windows::Manager);
    #[cfg(not(any(target_os = "macos", windows)))]
    Arc::new(MemoryVault::default())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;

    use security_framework::os::macos::keychain::SecKeychain;
    use zeroize::Zeroizing;

    use super::SERVICE;
    use crate::connections::{Text, Vault};

    /// The keychain at a path, or the login one.
    pub struct Keychain(Option<PathBuf>);

    const NOT_FOUND: i32 = -25300;

    impl Keychain {
        pub fn login() -> Self {
            Self(None)
        }
        #[cfg(test)]
        pub fn at(p: PathBuf) -> Self {
            Self(Some(p))
        }

        fn with<T>(&self, f: impl FnOnce(&SecKeychain) -> Result<T, String>) -> Result<T, String> {
            let words = |e: security_framework::base::Error| format!("the keychain: {e}");
            let _silent = SecKeychain::disable_user_interaction().map_err(words)?;
            let k = match &self.0 {
                // For a file that is not there the system takes the login keychain instead.
                Some(p) if !p.exists() => return Err("the keychain is not there".into()),
                Some(p) => SecKeychain::open(p),
                None => SecKeychain::default(),
            };
            f(&k.map_err(words)?)
        }
    }

    impl Vault for Keychain {
        fn put(&self, id: &str, secret: &Text) -> Result<(), String> {
            self.with(|k| {
                k.set_generic_password(SERVICE, id, secret.as_bytes())
                    .map_err(|e| format!("the keychain does not take it: {e}"))
            })
        }
        fn get(&self, id: &str) -> Result<Option<Text>, String> {
            self.with(|k| match k.find_generic_password(SERVICE, id) {
                Ok((p, _)) => {
                    let t = std::str::from_utf8(&p)
                        .map_err(|_| "the keychain holds no text".to_owned())?;
                    Ok(Some(Zeroizing::new(t.to_owned())))
                }
                Err(e) if e.code() == NOT_FOUND => Ok(None),
                Err(e) => Err(format!("the keychain does not give it: {e}")),
            })
        }
        fn remove(&self, id: &str) -> Result<(), String> {
            self.with(|k| match k.find_generic_password(SERVICE, id) {
                Ok((_, item)) => {
                    item.delete();
                    Ok(())
                }
                Err(e) if e.code() == NOT_FOUND => Ok(()),
                Err(e) => Err(format!("the keychain does not let it go: {e}")),
            })
        }
        fn lasts(&self) -> bool {
            true
        }
    }

    #[cfg(test)]
    mod tests {
        use security_framework::os::macos::keychain::CreateOptions;

        use super::*;

        /// In a keychain of the test's own, which no search finds. The login one is not touched.
        #[test]
        fn a_secret_is_kept_replaced_and_let_go() {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("test.keychain");
            CreateOptions::new().password("x").create(&p).unwrap();
            let k = Keychain::at(p.clone());
            let text = |s: &str| Zeroizing::new(s.to_owned());
            assert_eq!(k.get("c_1").unwrap(), None);
            k.put("c_1", &text("one")).unwrap();
            k.put("c_2", &text("two")).unwrap();
            k.put("c_1", &text("other")).unwrap();
            let again = Keychain::at(p);
            assert_eq!(again.get("c_1").unwrap(), Some(text("other")));
            again.remove("c_1").unwrap();
            again.remove("c_1").unwrap();
            assert_eq!(again.get("c_1").unwrap(), None);
            assert_eq!(again.get("c_2").unwrap(), Some(text("two")));
            // Nothing is written for this: the system would put it into the login keychain.
            let none = Keychain::at(d.path().join("none.keychain"));
            assert!(none.get("c_1").is_err() && none.remove("c_1").is_err());
        }
    }
}

#[cfg(windows)]
mod windows {
    use keyring_core::{Entry, Error, api::CredentialStoreApi};
    use zeroize::Zeroizing;

    use super::SERVICE;
    use crate::connections::{Text, Vault};

    pub struct Manager;

    impl Manager {
        fn entry(&self, id: &str) -> Result<Entry, String> {
            let store = windows_native_keyring_store::Store::new().map_err(words)?;
            store.build(SERVICE, id, None).map_err(words)
        }
    }

    fn words(e: Error) -> String {
        format!("the Credential Manager: {e}")
    }

    impl Vault for Manager {
        fn put(&self, id: &str, secret: &Text) -> Result<(), String> {
            self.entry(id)?.set_password(secret).map_err(words)
        }
        fn get(&self, id: &str) -> Result<Option<Text>, String> {
            match self.entry(id)?.get_password() {
                Ok(s) => Ok(Some(Zeroizing::new(s))),
                Err(Error::NoEntry) => Ok(None),
                Err(e) => Err(words(e)),
            }
        }
        fn remove(&self, id: &str) -> Result<(), String> {
            match self.entry(id)?.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(e) => Err(words(e)),
            }
        }
        fn lasts(&self) -> bool {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_of_ones_own_keeps_nothing_in_the_system() {
        let d = tempfile::tempdir().unwrap();
        assert!(!of(&Paths::new(d.path().join("d"))).lasts());
    }
}
