//! System installs through the host package manager (M2-ENV-004; ADR-0014 §2).
//!
//! A system plan is always explicit (never chosen by default), lists the exact command,
//! and is marked `scope: system` with the `systemChange` effect. Elevation is the OS's own
//! prompt: Linux runs the manager through `pkexec` (polkit's dialog), or `sudo -A` with the
//! desktop's askpass helper when pkexec is absent; macOS runs Homebrew unelevated (it
//! refuses root); Windows runs winget, which raises UAC itself. Axiom never sees, asks for
//! or stores a password.

use crate::detect;
use crate::error::EnvError;
use axiom_protocol::lifecycle::HostOs;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

/// Where to look for package managers and elevation helpers.
#[derive(Debug, Clone)]
pub struct SystemHost {
    pub os: HostOs,
    /// `PATH` to search; `None` = nothing is found.
    pub path: Option<OsString>,
    /// Contents of `/etc/os-release` (Linux), to prefer the distribution's own manager.
    pub os_release: Option<String>,
    /// `SUDO_ASKPASS`, the only way `sudo` is used (`-A`: the askpass program asks).
    pub sudo_askpass: Option<OsString>,
}

impl SystemHost {
    pub fn from_env() -> SystemHost {
        SystemHost {
            os: crate::install::host_platform().os,
            path: std::env::var_os("PATH"),
            os_release: std::fs::read_to_string("/etc/os-release").ok(),
            sudo_askpass: std::env::var_os("SUDO_ASKPASS").filter(|v| !v.is_empty()),
        }
    }

    fn which(&self, program: &str) -> Option<PathBuf> {
        detect::path_candidates(program, self.path.as_deref())
            .into_iter()
            .next()
    }
}

/// A package manager found on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manager {
    /// Key in a manifest's `packages` table: `apt`, `dnf`, `pacman`, `brew`, `winget`.
    pub name: &'static str,
    pub program: PathBuf,
}

const LINUX: [(&str, &str); 3] = [("apt", "apt-get"), ("dnf", "dnf"), ("pacman", "pacman")];

/// The host's package manager. On Linux the distribution's own (from `ID` and `ID_LIKE` in
/// os-release) wins when present, else the first of apt, dnf, pacman found.
pub fn detect_manager(host: &SystemHost) -> Option<Manager> {
    let found = |name: &'static str, program: &str| {
        host.which(program).map(|p| Manager { name, program: p })
    };
    match host.os {
        HostOs::Macos => found("brew", "brew"),
        HostOs::Windows => found("winget", "winget"),
        HostOs::Linux => {
            let ids: Vec<String> = host
                .os_release
                .as_deref()
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.strip_prefix("ID=").or_else(|| l.strip_prefix("ID_LIKE=")))
                .flat_map(|v| {
                    v.trim_matches('"')
                        .split_whitespace()
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .collect();
            let native = ids.iter().find_map(|id| match id.as_str() {
                "debian" | "ubuntu" => Some("apt"),
                "fedora" | "rhel" | "centos" => Some("dnf"),
                "arch" => Some("pacman"),
                _ => None,
            });
            native
                .and_then(|n| LINUX.iter().find(|(k, _)| *k == n))
                .and_then(|(k, p)| found(k, p))
                .or_else(|| LINUX.iter().find_map(|(k, p)| found(k, p)))
        }
    }
}

/// The exact argv (before elevation) for each command a system install runs.
pub fn install_commands(m: &Manager, packages: &[String]) -> Vec<Vec<String>> {
    let program = m.program.to_string_lossy().into_owned();
    let with = |args: &[&str]| -> Vec<String> {
        std::iter::once(program.clone())
            .chain(args.iter().map(|a| a.to_string()))
            .chain(packages.iter().cloned())
            .collect()
    };
    match m.name {
        // Non-interactive: the elevated process has no terminal to answer a prompt; the
        // plan shows the flag.
        // `--` ends the options where the manager supports it: no package can be read as one
        // (AUD-105; names are validated too, `manifest::valid_package`).
        "apt" => vec![with(&["install", "-y", "--"])],
        "dnf" => vec![with(&["install", "-y"])],
        "pacman" => vec![with(&["-S", "--needed", "--noconfirm", "--"])],
        "brew" => vec![with(&["install"])],
        // winget installs one id per call.
        "winget" => packages
            .iter()
            .map(|p| {
                vec![
                    program.clone(),
                    "install".into(),
                    "--exact".into(),
                    "--id".into(),
                    p.clone(),
                    "--accept-package-agreements".into(),
                    "--accept-source-agreements".into(),
                ]
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Wraps argv in the OS elevation prompt. `Ok(argv, elevated)`.
pub fn elevate(host: &SystemHost, argv: Vec<String>) -> Result<(Vec<String>, bool), EnvError> {
    match host.os {
        HostOs::Macos | HostOs::Windows => Ok((argv, false)),
        HostOs::Linux => {
            if let Some(pk) = host.which("pkexec") {
                let mut v = vec![pk.to_string_lossy().into_owned()];
                v.extend(argv);
                return Ok((v, true));
            }
            if let (Some(sudo), Some(_)) = (host.which("sudo"), &host.sudo_askpass) {
                let mut v = vec![sudo.to_string_lossy().into_owned(), "-A".into()];
                v.extend(argv);
                return Ok((v, true));
            }
            Err(EnvError::PrerequisiteMissing(
                "Installing system packages needs pkexec (polkit), or sudo with an askpass helper."
                    .into(),
                vec!["pkexec".into()],
            ))
        }
    }
}

/// The packages a manifest lists for this manager.
pub fn packages_for<'a>(
    packages: &'a BTreeMap<String, Vec<String>>,
    m: &Manager,
) -> Option<&'a Vec<String>> {
    packages.get(m.name).filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `--` ends the options before the packages where the manager supports it (AUD-105).
    #[test]
    fn packages_follow_an_end_of_options_marker() {
        let m = |name, program: &str| Manager {
            name,
            program: PathBuf::from(program),
        };
        let pkgs = ["hello".to_string()];
        assert_eq!(
            install_commands(&m("apt", "apt-get"), &pkgs),
            [["apt-get", "install", "-y", "--", "hello"]]
        );
        assert_eq!(
            install_commands(&m("pacman", "pacman"), &pkgs),
            [["pacman", "-S", "--needed", "--noconfirm", "--", "hello"]]
        );
    }
}
