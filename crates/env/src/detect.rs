//! Project and tool detection (`providers.md` §Operations: detect project, detect tools).
//!
//! Detection changes nothing: it lists directories and runs each tool's declared version
//! command (absolute program path, declared arguments, no shell, 5 s limit, stdin closed,
//! working directory outside the workspace so project-local toolchain files cannot trigger an
//! install).

use crate::manifest;
use crate::registry::Registry;
use axiom_protocol::env::{DetectResult, ProviderDetection, ToolLocation, ToolStatus, Toolchain};
use axiom_protocol::provider::{InstallMethod, ProviderManifest, ToolSpec};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a version command may run.
pub const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

/// The host facts detection depends on, explicit so tests can control them.
#[derive(Debug, Clone)]
pub struct HostPaths {
    /// `<data dir>/toolchains`: managed installs live in `<provider>/<version>/`.
    pub toolchains_dir: PathBuf,
    /// The `PATH` to search; `None` = no system lookup.
    pub path: Option<OsString>,
}

impl HostPaths {
    /// The daemon's own `PATH`.
    pub fn from_env(toolchains_dir: PathBuf) -> HostPaths {
        HostPaths {
            toolchains_dir,
            path: std::env::var_os("PATH"),
        }
    }
}

/// Which active providers apply to the workspace, and the state of each of their tools.
pub async fn detect(registry: &Registry, root: &Path, host: &HostPaths) -> DetectResult {
    let mut providers = Vec::new();
    for (_, m) in registry.active() {
        if !applies(m, root) {
            continue;
        }
        let mut tools = Vec::new();
        for t in &m.tools {
            tools.push(ToolStatus {
                tool_id: t.id.clone(),
                required: t.required,
                found: detect_tool(m, t, host).await,
            });
        }
        providers.push(ProviderDetection {
            provider_id: m.id.clone(),
            selected_version: None,
            tools,
        });
    }
    DetectResult { providers }
}

/// Whether any `project.any_file` pattern matches a file or directory in the workspace root
/// or in one of its first-level directories (hidden directories are skipped).
pub fn applies(m: &ProviderManifest, root: &Path) -> bool {
    if m.project.any_file.is_empty() {
        return false;
    }
    let mut dirs = vec![root.to_path_buf()];
    if let Ok(read) = std::fs::read_dir(root) {
        let mut sub: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.path())
            .collect();
        sub.sort();
        dirs.extend(sub);
    }
    dirs.iter().any(|d| {
        let Ok(read) = std::fs::read_dir(d) else {
            return false;
        };
        let names: Vec<String> = read
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        m.project
            .any_file
            .iter()
            .any(|p| names.iter().any(|n| glob_match(p, n)))
    })
}

/// `*` (any run) and `?` (one character) against a single name. Case-sensitive.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Managed installs first (newest version first), then `PATH`. The first candidate whose
/// version command succeeds and matches wins.
pub async fn detect_tool(
    m: &ProviderManifest,
    tool: &ToolSpec,
    host: &HostPaths,
) -> Option<Toolchain> {
    let Ok(re) = manifest::version_regex(&tool.version_regex) else {
        return None;
    };
    for (version_root, bin) in managed_candidates(m, &tool.program, host) {
        if let Some(version) = run_version(&bin, &tool.version_args, &re).await {
            return Some(toolchain(
                m,
                tool,
                version,
                ToolLocation::Managed {
                    root: version_root.to_string_lossy().into_owned(),
                },
            ));
        }
    }
    for bin in path_candidates(&tool.program, host.path.as_deref()) {
        if let Some(version) = run_version(&bin, &tool.version_args, &re).await {
            return Some(toolchain(
                m,
                tool,
                version,
                ToolLocation::System {
                    path: bin.to_string_lossy().into_owned(),
                },
            ));
        }
    }
    None
}

fn toolchain(
    m: &ProviderManifest,
    t: &ToolSpec,
    version: String,
    location: ToolLocation,
) -> Toolchain {
    Toolchain {
        provider_id: m.id.clone(),
        tool_id: t.id.clone(),
        version,
        location,
        verified_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
    }
}

/// `(install root, executable)` for each managed version, newest first.
fn managed_candidates(
    m: &ProviderManifest,
    program: &str,
    host: &HostPaths,
) -> Vec<(PathBuf, PathBuf)> {
    let provider_dir = host.toolchains_dir.join(&m.id);
    let Ok(read) = std::fs::read_dir(&provider_dir) else {
        return Vec::new();
    };
    let mut versions: Vec<(String, PathBuf)> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        // Staging directories of installs in progress are not installs.
        .filter(|(n, _)| !n.starts_with('.'))
        .collect();
    versions.sort_by(|a, b| compare_versions(&b.0, &a.0));
    let bin_dirs = managed_bin_dirs(m);
    let mut out = Vec::new();
    for (_, root) in versions {
        for d in &bin_dirs {
            for name in executable_names(program) {
                let p = root.join(d).join(&name);
                if p.is_file() {
                    out.push((root.clone(), p));
                }
            }
        }
    }
    out
}

/// Directories under a managed root that hold executables: each archive method's `bin_dirs`.
fn managed_bin_dirs(m: &ProviderManifest) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for i in &m.install {
        if let InstallMethod::Archive { bin_dirs, .. } = i {
            for d in bin_dirs {
                if !dirs.contains(d) {
                    dirs.push(d.clone());
                }
            }
        }
    }
    dirs
}

/// Dotted numeric comparison (`1.10.0` > `1.9.2`); non-numeric parts compare as text.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<&str> = a.split(['.', '-', '+']).collect();
    let pb: Vec<&str> = b.split(['.', '-', '+']).collect();
    for (x, y) in pa.iter().zip(&pb) {
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => x.cmp(y),
        };
        if o.is_ne() {
            return o;
        }
    }
    pa.len().cmp(&pb.len())
}

fn executable_names(program: &str) -> Vec<String> {
    if cfg!(windows) {
        ["exe", "cmd", "bat"]
            .iter()
            .map(|e| format!("{program}.{e}"))
            .collect()
    } else {
        vec![program.to_string()]
    }
}

fn path_candidates(program: &str, path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    let Some(path) = path else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for dir in std::env::split_paths(path) {
        if dir.as_os_str().is_empty() || !dir.is_absolute() {
            continue;
        }
        for name in executable_names(program) {
            let p = dir.join(name);
            if p.is_file() && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// Runs `bin args…` and extracts the version from stdout, else stderr.
async fn run_version(bin: &Path, args: &[String], re: &regex::Regex) -> Option<String> {
    let mut cmd = tokio::process::Command::new(bin);
    cmd.args(args)
        .current_dir(std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let out = match tokio::time::timeout(VERSION_TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            tracing::debug!("version command {} failed to start: {e}", bin.display());
            return None;
        }
        Err(_) => {
            tracing::warn!("version command {} timed out", bin.display());
            return None;
        }
    };
    [&out.stdout, &out.stderr].iter().find_map(|bytes| {
        let text = String::from_utf8_lossy(bytes);
        re.captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob_match("Cargo.toml", "Cargo.toml"));
        assert!(!glob_match("Cargo.toml", "cargo.toml"));
        assert!(glob_match("*.xcodeproj", "App.xcodeproj"));
        assert!(glob_match("CMake*.txt", "CMakeLists.txt"));
        assert!(glob_match("?.c", "a.c") && !glob_match("?.c", "ab.c"));
        assert!(glob_match("*", "") && !glob_match("a*", "b"));
        assert!(glob_match("*a*b", "xxaxxb") && !glob_match("*a*b", "xxaxxbx"));
    }

    #[test]
    fn version_order() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("1.10.0", "1.9.2"), Greater);
        assert_eq!(compare_versions("22.11.0", "22.11.0"), Equal);
        assert_eq!(compare_versions("1.84", "1.84.0"), Less);
    }
}
