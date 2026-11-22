//! `axiom-env`: toolchain providers (ADR-0006, ADR-0014). Normative prose:
//! `docs/protocols/providers.md`.
//!
//! - [`manifest`]: `provider.toml` parsing and validation beyond serde.
//! - [`registry`]: built-in, user and workspace providers, override order and trust.
//! - [`detect`]: which providers apply to a workspace and which of their tools are present.
//! - [`install`]: install plans, managed archive installs, uninstall; [`fetch`] (HTTPS) and
//!   [`archive`] (safe extraction) beneath it; [`system`] for host package managers.
//! - [`project`]: `.axiom/project.toml`, resolution to an `Environment`, the lock.

pub mod archive;
pub mod detect;
pub mod error;
pub mod fetch;
pub mod install;
pub mod manifest;
pub mod project;
pub mod registry;
pub mod system;

pub use detect::{detect, HostPaths};
pub use error::EnvError;
pub use fetch::{Http, HttpConfig};
pub use install::Installer;
pub use registry::{Builtin, Registry, Sources};

static WORK_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Sets where version probes and system installs run (the first call wins; the daemon's
/// installer sets `<data dir>/work`).
pub fn set_work_dir(dir: std::path::PathBuf) {
    let _ = WORK_DIR.set(dir);
}

/// The working directory of version probes, `list_versions` programs and system package
/// managers: a private (`0700`), empty directory whose ancestors no other user can write.
///
/// Many tools read configuration from their working directory and its parents (rustup's
/// `rust-toolchain.toml` can name a toolchain to run, `global.json`, `.npmrc`, `go.work`), so
/// the shared temporary directory, which any local user can write into, is never used
/// (AUD-044). Without [`set_work_dir`]: `$XDG_RUNTIME_DIR/axiom-work`, else
/// `~/.cache/axiom-work`.
pub fn work_dir() -> std::path::PathBuf {
    let dir = WORK_DIR.get().cloned().unwrap_or_else(|| {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache"))
            })
            .unwrap_or_else(std::env::temp_dir)
            .join("axiom-work")
    });
    if let Err(e) = private_dir(&dir) {
        tracing::warn!("cannot prepare {}: {e}", dir.display());
    }
    dir
}

fn private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let md = std::fs::symlink_metadata(dir)?;
        if !md.is_dir() {
            return Err(std::io::Error::other("not a directory"));
        }
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_work_dir_is_private_and_not_the_shared_temp_dir() {
        let d = super::work_dir();
        assert!(d.is_dir());
        assert_ne!(d, std::env::temp_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&d).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }
}
