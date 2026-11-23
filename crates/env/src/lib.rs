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
/// (AUD-044). The first of these that can be made private: the [`set_work_dir`] directory,
/// `$XDG_RUNTIME_DIR/axiom-work`, `~/.cache/axiom-work`. A directory that cannot be (a symlink
/// or a file in its place, one another user owns) is never used (review N8): with none, an
/// error, and the caller runs nothing.
pub fn work_dir() -> std::io::Result<std::path::PathBuf> {
    let mut candidates: Vec<std::path::PathBuf> = WORK_DIR.get().cloned().into_iter().collect();
    candidates.extend(
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .map(|p| p.join("axiom-work")),
    );
    candidates.extend(
        std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_absolute())
            .map(|h| h.join(".cache").join("axiom-work")),
    );
    first_private(&candidates)
}

/// The first of `candidates` made private, or why none could be.
fn first_private(candidates: &[std::path::PathBuf]) -> std::io::Result<std::path::PathBuf> {
    let mut why = Vec::new();
    for dir in candidates {
        match private_dir(dir) {
            Ok(()) => return Ok(dir.clone()),
            Err(e) => {
                tracing::warn!("cannot use {} as the work directory: {e}", dir.display());
                why.push(format!("{}: {e}", dir.display()));
            }
        }
    }
    Err(std::io::Error::other(if why.is_empty() {
        "no private work directory: neither XDG_RUNTIME_DIR nor HOME is set".to_string()
    } else {
        format!("no private work directory ({})", why.join("; "))
    }))
}

fn private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let md = std::fs::symlink_metadata(dir)?;
    if !md.is_dir() {
        return Err(std::io::Error::other("not a directory"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Fails for a directory another user owns.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let mode = std::fs::symlink_metadata(dir)?.permissions().mode();
        if mode & 0o777 != 0o700 {
            return Err(std::io::Error::other(format!(
                "mode {:o}, not 700",
                mode & 0o777
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_work_dir_is_private_and_not_the_shared_temp_dir() {
        let d = super::work_dir().unwrap();
        assert!(d.is_dir());
        assert_ne!(d, std::env::temp_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&d).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }

    /// A candidate that cannot be made private is skipped, never used; with none, an error
    /// (review N8).
    #[cfg(unix)]
    #[test]
    fn a_work_dir_that_cannot_be_made_private_is_not_used() {
        let d = tempfile::tempdir().unwrap();
        // A file where the directory should be.
        let file = d.path().join("file");
        std::fs::write(&file, "").unwrap();
        // A symlink to a directory someone else could control.
        let elsewhere = d.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        let good = d.path().join("good");

        let err = super::first_private(&[file.clone(), link.clone()]).unwrap_err();
        assert!(
            err.to_string().contains("no private work directory"),
            "{err}"
        );
        assert!(super::first_private(&[]).is_err());
        assert_eq!(
            super::first_private(&[file, link, good.clone()]).unwrap(),
            good
        );
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&good).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
