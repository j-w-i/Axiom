//! Project environments (ADR-0014 §6; M2-ENV-005).
//!
//! `.axiom/project.toml` (human-written, committed):
//!
//! ```toml
//! [environment]
//! rust = "1.84.0"     # a managed install of that version
//! node = "system"     # whatever the host's PATH provides
//! ```
//!
//! Resolution turns each entry into toolchains, a `PATH` prepend and variables. The result is
//! recorded in `.axiom/project.lock` (written by Axiom, only when it changes): exact tool
//! versions, where they are, and the SHA-256 of every download that made the managed install.

use crate::detect::{self, managed_layouts};
use crate::error::EnvError;
use crate::install::{valid_version, RECEIPT};
use crate::manifest;
use crate::registry::Registry;
use axiom_protocol::env::{Environment, ToolLocation, Toolchain};
use axiom_protocol::provider::ProviderManifest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Workspace-relative paths of the two project files.
pub const PROJECT_FILE: &str = ".axiom/project.toml";
pub const LOCK_FILE: &str = ".axiom/project.lock";

/// The version that means "use the host's own toolchain from PATH".
pub const SYSTEM: &str = "system";

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    /// provider id → version (or `"system"`), in key order.
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

/// Parses `.axiom/project.toml`. Errors carry the 1-based line.
pub fn parse(text: &str) -> Result<ProjectFile, EnvError> {
    let p: ProjectFile = toml::from_str(text).map_err(|e| {
        let line = e
            .span()
            .map(|s| text[..s.start.min(text.len())].matches('\n').count() as u32 + 1);
        EnvError::ProjectFile {
            line,
            reason: e.message().to_string(),
        }
    })?;
    for (k, v) in &p.environment {
        let line = line_of(text, k);
        if !manifest::valid_id(k) {
            return Err(EnvError::ProjectFile {
                line,
                reason: format!("`{k}` is not a provider id"),
            });
        }
        if v != SYSTEM && !valid_version(v) {
            return Err(EnvError::ProjectFile {
                line,
                reason: format!("`{v}` is not a version (or \"system\")"),
            });
        }
    }
    Ok(p)
}

/// The first line whose key is `key` (for messages about valid TOML with a bad value).
fn line_of(text: &str, key: &str) -> Option<u32> {
    text.lines()
        .position(|l| {
            let l = l.trim_start();
            l.strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
        .map(|i| i as u32 + 1)
}

/// What `.axiom/project.lock` records.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lock {
    pub version: u32,
    #[serde(default, rename = "toolchain")]
    pub toolchains: Vec<LockedToolchain>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedToolchain {
    pub provider: String,
    /// As selected in project.toml (`"system"` or an installed version).
    pub selected: String,
    pub tool: String,
    /// The version the tool reported.
    pub version: String,
    /// `managed` or `system`.
    pub location: String,
    /// The downloads the managed install was made from.
    #[serde(default)]
    pub downloads: Vec<LockedDownload>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockedDownload {
    pub url: String,
    pub sha256: String,
}

/// The install receipt written into each managed root (see `install.rs`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Receipt {
    pub provider: String,
    pub version: String,
    #[serde(default)]
    pub downloads: Vec<LockedDownload>,
}

impl Lock {
    /// Stable text: same resolution, same bytes. No host paths (the lock is committed).
    pub fn to_text(&self) -> String {
        let body = toml::to_string(self).unwrap_or_default();
        format!("# Written by Axiom from .axiom/project.toml. Do not edit.\n{body}")
    }
}

/// The environment overlay of one managed install of [m] at [root] ([version]): the install's
/// existing `bin_dirs` (to prepend to `PATH`) and its methods' `env`, expanded for that root.
/// M5.5 (ADR-0021 §4): a language server not on the workspace's `PATH` runs from here.
pub fn managed_overlay(
    m: &ProviderManifest,
    root: &Path,
    version: &str,
) -> (Vec<String>, BTreeMap<String, String>) {
    let mut dirs = Vec::new();
    let mut vars = BTreeMap::new();
    for (dir, e) in detect::managed_layouts(m) {
        let d = root.join(detect::bin_dir(&dir, version));
        // Only the layouts this install has: a per-platform method's variables (macOS Java's
        // `Contents/Home`) do not leak into another platform's install.
        if !d.is_dir() {
            continue;
        }
        let s = d.to_string_lossy().into_owned();
        if !dirs.contains(&s) {
            dirs.push(s);
        }
        vars.extend(detect::expand_env(&e, version, root));
    }
    (dirs, vars)
}

/// Resolves a project's selections. `toolchains_dir` holds managed installs; `path` is the
/// host `PATH` for `"system"` entries. Missing tools (and unknown providers) are collected and
/// reported together as `prerequisite_missing`.
pub async fn resolve(
    registry: &Registry,
    project: &ProjectFile,
    toolchains_dir: &Path,
    path: Option<&std::ffi::OsStr>,
) -> Result<(Environment, Lock), EnvError> {
    let mut env = Environment::default();
    let mut lock = Lock {
        version: 1,
        toolchains: Vec::new(),
    };
    let mut missing: Vec<String> = Vec::new();
    for (provider, selected) in &project.environment {
        let Some(m) = registry.get(provider) else {
            missing.push(provider.clone());
            continue;
        };
        if selected == SYSTEM {
            for t in m.tools.iter().filter(|t| t.required) {
                match detect::detect_on_path(m, t, path).await {
                    Some(tc) => {
                        lock.toolchains
                            .push(locked(provider, selected, &tc, Vec::new()));
                        env.toolchains.push(tc);
                    }
                    None => missing.push(t.id.clone()),
                }
            }
            continue;
        }
        let root = toolchains_dir.join(provider).join(selected);
        if !root.is_dir() {
            missing.extend(m.tools.iter().filter(|t| t.required).map(|t| t.id.clone()));
            continue;
        }
        let receipt: Receipt = std::fs::read_to_string(root.join(RECEIPT))
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default();
        let layouts = managed_layouts(m);
        let mut vars = BTreeMap::new();
        for (dir, e) in &layouts {
            let d = root.join(detect::bin_dir(dir, selected));
            if d.is_dir() {
                let s = d.to_string_lossy().into_owned();
                if !env.path_prepend.contains(&s) {
                    env.path_prepend.push(s);
                }
            }
            vars.extend(detect::expand_env(e, selected, &root));
        }
        let dirs: Vec<String> = layouts
            .iter()
            .map(|(d, _)| detect::bin_dir(d, selected))
            .collect();
        for t in m.tools.iter().filter(|t| t.required) {
            match detect::tool_version_in_root(t, &root, &dirs, &vars).await {
                Some(version) => {
                    let tc = Toolchain {
                        provider_id: m.id.clone(),
                        tool_id: t.id.clone(),
                        version,
                        location: ToolLocation::Managed {
                            root: root.to_string_lossy().into_owned(),
                        },
                        verified_ms: now_ms(),
                    };
                    lock.toolchains.push(locked(
                        provider,
                        selected,
                        &tc,
                        receipt.downloads.clone(),
                    ));
                    env.toolchains.push(tc);
                }
                None => missing.push(t.id.clone()),
            }
        }
        // Providers in key order; a later provider's variable wins (recorded in the docs).
        env.vars.extend(vars);
    }
    if !missing.is_empty() {
        return Err(EnvError::PrerequisiteMissing(
            format!("Not installed: {}.", missing.join(", ")),
            missing,
        ));
    }
    Ok((env, lock))
}

fn locked(
    provider: &str,
    selected: &str,
    tc: &Toolchain,
    downloads: Vec<LockedDownload>,
) -> LockedToolchain {
    LockedToolchain {
        provider: provider.to_string(),
        selected: selected.to_string(),
        tool: tc.tool_id.clone(),
        version: tc.version.clone(),
        location: match tc.location {
            ToolLocation::Managed { .. } => "managed".into(),
            ToolLocation::System { .. } => "system".into(),
        },
        downloads,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The variables a process started in this environment gets on top of the inherited ones:
/// the environment's own, and `PATH` = `path_prepend` + `inherited_path`.
pub fn process_env(
    env: &Environment,
    inherited_path: Option<OsString>,
) -> BTreeMap<String, String> {
    let mut out = env.vars.clone();
    if !env.path_prepend.is_empty() {
        let mut dirs: Vec<PathBuf> = env.path_prepend.iter().map(PathBuf::from).collect();
        if let Some(p) = inherited_path {
            dirs.extend(std::env::split_paths(&p));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            out.insert("PATH".into(), joined.to_string_lossy().into_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_toml_parse_errors() {
        let ok = parse("[environment]\nrust = \"1.84.0\"\nnode = \"system\"\n").unwrap();
        assert_eq!(ok.environment["rust"], "1.84.0");
        assert_eq!(parse("").unwrap(), ProjectFile::default());

        let e = |t: &str| match parse(t) {
            Err(EnvError::ProjectFile { line, reason }) => (line, reason),
            other => panic!("{other:?}"),
        };
        let (line, reason) = e("[environment]\nrust = \"1.84.0\"\n\n[toolchains]\nx = 1\n");
        assert_eq!(line, Some(4), "{reason}");
        assert!(reason.contains("toolchains"), "{reason}");
        let (line, _) = e("[environment]\nrust = 1.84\n");
        assert_eq!(line, Some(2));
        let (line, reason) = e("[environment]\n\nRust = \"1.84.0\"\n");
        assert_eq!(line, Some(3));
        assert!(reason.contains("provider id"));
        let (line, reason) = e("[environment]\nrust = \"../../etc\"\n");
        assert_eq!(line, Some(2));
        assert!(reason.contains("not a version"));
        let (line, _) = e("[environment\n");
        assert_eq!(line, Some(1));
    }

    #[test]
    fn process_env_prepends_path() {
        let env = Environment {
            toolchains: vec![],
            path_prepend: vec!["/a/bin".into(), "/b/bin".into()],
            vars: BTreeMap::from([("X".to_string(), "1".to_string())]),
        };
        let inherited = std::env::join_paths(["/usr/bin"]).unwrap();
        let out = process_env(&env, Some(inherited));
        let path: Vec<PathBuf> = std::env::split_paths(&out["PATH"]).collect();
        assert_eq!(
            path,
            [PathBuf::from("/a/bin"), "/b/bin".into(), "/usr/bin".into()]
        );
        assert_eq!(out["X"], "1");
        assert!(!process_env(&Environment::default(), None).contains_key("PATH"));
    }

    #[test]
    fn lock_text_is_stable() {
        let lock = Lock {
            version: 1,
            toolchains: vec![LockedToolchain {
                provider: "rust".into(),
                selected: "1.84.0".into(),
                tool: "cargo".into(),
                version: "1.84.0".into(),
                location: "managed".into(),
                downloads: vec![LockedDownload {
                    url: "https://x/rustup-init".into(),
                    sha256: "a".repeat(64),
                }],
            }],
        };
        let t = lock.to_text();
        assert_eq!(t, lock.to_text());
        assert!(t.starts_with("# Written by Axiom"));
        let back: Lock = toml::from_str(&t).unwrap();
        assert_eq!(back, lock);
    }
}
