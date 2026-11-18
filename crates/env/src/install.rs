//! Install plans, managed archive installs and uninstall (ADR-0014 §2; `providers.md`
//! §Core data).
//!
//! Layout under the data directory:
//! - `cache/<sha256>`: verified downloads, content-addressed by their digest;
//!   `cache/.part-<planId>` while downloading.
//! - `toolchains/<provider>/<version>/`: published installs.
//!   `toolchains/<provider>/.staging-<version>-<planId>/` while extracting and verifying.
//!
//! A plan is issued once, lives 10 minutes, executes at most once, and only while the
//! provider's manifest is unchanged. Install executes the plan's own steps; nothing is
//! extracted or run before its download's SHA-256 matches.

use crate::archive;
use crate::detect::{self, compare_versions};
use crate::error::EnvError;
use crate::fetch::{digest_from_checksum_file, Http};
use crate::manifest;
use axiom_protocol::env::{
    Effect, InstallPlan, InstallProgress, InstallResult, Sha256Source, Step, ToolLocation,
    Toolchain, VersionsResult,
};
use axiom_protocol::lifecycle::HostOs;
use axiom_protocol::provider::{Checksum, InstallMethod, InstallScope, Platform, ProviderManifest};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

/// How long an issued plan stays executable.
pub const PLAN_LIFETIME: Duration = Duration::from_secs(10 * 60);
/// How long a provider's version list is reused.
pub const VERSIONS_TTL: Duration = Duration::from_secs(60 * 60);
/// At most this many progress notifications per second.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

struct Issued {
    plan: InstallPlan,
    /// The manifest the plan was made from; the install is refused if it changed.
    manifest: ProviderManifest,
    method: usize,
    expires: Instant,
}

/// Plans and executes installs. One per daemon.
pub struct Installer {
    data_dir: PathBuf,
    http: Http,
    plans: Mutex<HashMap<String, Issued>>,
    versions: Mutex<HashMap<String, (Instant, Vec<String>)>>,
    plan_lifetime: Duration,
    counter: std::sync::atomic::AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A version string usable as a directory name: `[A-Za-z0-9._+-]`, not starting with `.`.
pub fn valid_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && !v.starts_with('.')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// This host as a manifest `Platform`.
pub fn host_platform() -> Platform {
    Platform {
        os: match std::env::consts::OS {
            "macos" => HostOs::Macos,
            "windows" => HostOs::Windows,
            _ => HostOs::Linux,
        },
        arch: Some(std::env::consts::ARCH.to_string()),
    }
}

fn platform_matches(want: &Platform, host: &Platform) -> bool {
    want.os == host.os
        && want
            .arch
            .as_ref()
            .is_none_or(|a| Some(a) == host.arch.as_ref())
}

/// Expands `${version}` only; any other variable is an error here (URLs).
fn expand_version(template: &str, version: &str) -> Result<String, EnvError> {
    let s = template.replace("${version}", version);
    if s.contains("${") {
        return Err(EnvError::InvalidParams(format!(
            "Only ${{version}} may appear in a download URL: {template}"
        )));
    }
    Ok(s)
}

fn file_name_of(url: &str) -> String {
    url.rsplit('/').next().unwrap_or_default().to_string()
}

/// Removes a directory when dropped, unless disarmed.
struct Cleanup(Option<PathBuf>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(p) = self.0.take() {
            if let Err(e) = std::fs::remove_dir_all(&p) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!("cannot remove {}: {e}", p.display());
                }
            }
        }
    }
}

impl Installer {
    pub fn new(data_dir: PathBuf, http: Http) -> Installer {
        Installer {
            data_dir,
            http,
            plans: Mutex::default(),
            versions: Mutex::default(),
            plan_lifetime: PLAN_LIFETIME,
            counter: Default::default(),
        }
    }

    /// TEST ONLY: shorter plan lifetime.
    pub fn with_plan_lifetime(mut self, d: Duration) -> Installer {
        self.plan_lifetime = d;
        self
    }

    pub fn toolchains_dir(&self) -> PathBuf {
        self.data_dir.join("toolchains")
    }

    fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }

    /// Published versions of a provider in managed storage, newest first.
    pub fn installed(&self, provider_id: &str) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.toolchains_dir().join(provider_id))
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| valid_version(n))
            .collect();
        v.sort_by(|a, b| compare_versions(b, a));
        v
    }

    /// `env/versions`: the first archive method's version source (cached for an hour), and
    /// what is installed.
    pub async fn versions(&self, m: &ProviderManifest) -> Result<VersionsResult, EnvError> {
        let source = m.install.iter().find_map(|i| match i {
            InstallMethod::Archive { versions, .. } => Some(versions),
            _ => None,
        });
        let available = match source {
            None => Vec::new(),
            Some(src) => {
                let key = format!(
                    "{}\n{}",
                    m.id,
                    serde_json::to_string(src).unwrap_or_default()
                );
                let cached = lock(&self.versions)
                    .get(&key)
                    .filter(|(at, _)| at.elapsed() < VERSIONS_TTL)
                    .map(|(_, v)| v.clone());
                match cached {
                    Some(v) => v,
                    None => {
                        let v = self.http.versions(src).await?;
                        lock(&self.versions).insert(key, (Instant::now(), v.clone()));
                        v
                    }
                }
            }
        };
        Ok(VersionsResult {
            available,
            installed: self.installed(&m.id),
        })
    }

    fn new_plan_id(&self) -> String {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(
            self.counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        );
        let mut d = Sha256::new();
        d.update(h.finish().to_le_bytes());
        d.update(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
                .to_le_bytes(),
        );
        d.update(std::process::id().to_le_bytes());
        hex::encode(&d.finalize()[..16])
    }

    /// `env/planInstall`. Runs nothing; at most a `HEAD` request for the download size.
    pub async fn plan(
        &self,
        m: &ProviderManifest,
        version: &str,
        method: Option<u32>,
    ) -> Result<InstallPlan, EnvError> {
        if !valid_version(version) {
            return Err(EnvError::InvalidParams(format!(
                "\"{version}\" is not a valid version"
            )));
        }
        let host = host_platform();
        if !m.hosts.is_empty() && !m.hosts.iter().any(|p| platform_matches(p, &host)) {
            return Err(EnvError::UnsupportedPlatform(format!(
                "{} does not support this platform.",
                m.name
            )));
        }
        if self.toolchains_dir().join(&m.id).join(version).exists() {
            return Err(EnvError::AlreadyExists(format!(
                "{} {version} is already installed.",
                m.name
            )));
        }
        let (index, chosen) = match method {
            Some(i) => {
                let im = m.install.get(i as usize).ok_or_else(|| {
                    EnvError::InvalidParams(format!("{} has no install method {i}", m.id))
                })?;
                (i as usize, im)
            }
            None => m
                .install
                .iter()
                .enumerate()
                .find(|(_, im)| match im {
                    InstallMethod::Archive { artifacts, .. } => artifacts
                        .iter()
                        .any(|a| platform_matches(&a.platform, &host)),
                    _ => false,
                })
                .ok_or_else(|| {
                    EnvError::UnsupportedPlatform(format!(
                        "{} has no managed download for this platform.",
                        m.name
                    ))
                })?,
        };
        let InstallMethod::Archive { artifacts, .. } = chosen else {
            return Err(EnvError::Unsupported(
                "Only archive installs are available yet.".into(),
            ));
        };
        let artifact = artifacts
            .iter()
            .find(|a| platform_matches(&a.platform, &host))
            .ok_or_else(|| {
                EnvError::UnsupportedPlatform(format!(
                    "{} has no download for this platform.",
                    m.name
                ))
            })?;
        let url = expand_version(&artifact.url, version)?;
        let sha256 = match &artifact.checksum {
            Checksum::Sha256 { values } => Sha256Source::Digest {
                value: values.get(version).cloned().ok_or_else(|| {
                    EnvError::NotFound(format!(
                        "The provider lists no checksum for {} {version}.",
                        m.name
                    ))
                })?,
            },
            Checksum::Sha256Url { url } => Sha256Source::Url {
                url: expand_version(url, version)?,
            },
        };
        let bytes = self.http.size(&url).await;
        let mut steps = vec![
            Step::Download { url, bytes, sha256 },
            Step::Extract {
                format: artifact.format,
                strip_components: artifact.strip_components,
            },
        ];
        for t in m.tools.iter().filter(|t| t.required) {
            steps.push(Step::Verify {
                tool_id: t.id.clone(),
            });
        }
        let plan = InstallPlan {
            plan_id: self.new_plan_id(),
            provider_id: m.id.clone(),
            version: version.to_string(),
            scope: InstallScope::Managed,
            steps,
            download_bytes: bytes,
            effects: vec![Effect::Network, Effect::WriteManaged, Effect::RunPrograms],
            expires_ms: now_ms() + self.plan_lifetime.as_millis() as u64,
        };
        let mut plans = lock(&self.plans);
        plans.retain(|_, p| p.expires > Instant::now());
        plans.insert(
            plan.plan_id.clone(),
            Issued {
                plan: plan.clone(),
                manifest: m.clone(),
                method: index,
                expires: Instant::now() + self.plan_lifetime,
            },
        );
        Ok(plan)
    }

    /// The provider an issued, unexpired plan is for (to look up its current manifest).
    pub fn plan_provider(&self, plan_id: &str) -> Option<String> {
        lock(&self.plans)
            .get(plan_id)
            .map(|p| p.plan.provider_id.clone())
    }

    /// `env/install`: executes an issued plan once. `current` is the provider's manifest now.
    pub async fn install(
        &self,
        plan_id: &str,
        current: Option<&ProviderManifest>,
        cancel: &CancellationToken,
        progress: &(dyn Fn(InstallProgress) + Send + Sync),
    ) -> Result<InstallResult, EnvError> {
        let issued = lock(&self.plans).remove(plan_id).ok_or_else(|| {
            EnvError::ApprovalDenied("This install plan is unknown or was already used.".into())
        })?;
        if issued.expires <= Instant::now() {
            return Err(EnvError::ApprovalDenied(
                "This install plan has expired; review a new plan.".into(),
            ));
        }
        if current != Some(&issued.manifest) {
            return Err(EnvError::ApprovalDenied(
                "The provider changed after the plan was made; review a new plan.".into(),
            ));
        }
        let plan = &issued.plan;
        let m = &issued.manifest;
        let bin_dirs = match &m.install[issued.method] {
            InstallMethod::Archive { bin_dirs, .. } => bin_dirs.clone(),
            _ => Vec::new(),
        };
        let steps = plan.steps.len() as u32;
        let report = |step: usize, label: String, done: Option<u64>, total: Option<u64>| {
            progress(InstallProgress {
                plan_id: plan.plan_id.clone(),
                step: step as u32,
                steps,
                label,
                bytes_done: done,
                bytes_total: total,
            })
        };

        let provider_dir = self.toolchains_dir().join(&m.id);
        let final_root = provider_dir.join(&plan.version);
        let staging = provider_dir.join(format!(".staging-{}-{}", plan.version, plan.plan_id));
        std::fs::create_dir_all(&provider_dir)
            .map_err(EnvError::io("Cannot create the toolchains directory"))?;
        std::fs::create_dir(&staging)
            .map_err(EnvError::io("Cannot create the staging directory"))?;
        let mut cleanup = Cleanup(Some(staging.clone()));

        let mut archive_path: Option<PathBuf> = None;
        let mut verified = Vec::new();
        for (i, step) in plan.steps.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(EnvError::Cancelled);
            }
            match step {
                Step::Download { url, sha256, .. } => {
                    report(i, format!("Downloading {}", file_name_of(url)), None, None);
                    let path = self.download(plan, i, url, sha256, cancel, &report).await?;
                    archive_path = Some(path);
                }
                Step::Extract {
                    format,
                    strip_components,
                } => {
                    report(i, "Extracting".into(), None, None);
                    let Some(src) = archive_path.clone() else {
                        return Err(EnvError::InvalidParams("Extract without a download".into()));
                    };
                    let (format, strip, dest, c) =
                        (*format, *strip_components, staging.clone(), cancel.clone());
                    tokio::task::spawn_blocking(move || {
                        archive::extract(&src, format, strip, &dest, &|| c.is_cancelled())
                    })
                    .await
                    .map_err(|_| EnvError::Unsupported("extraction task failed".into()))??;
                }
                Step::Verify { tool_id } => {
                    report(i, format!("Verifying {tool_id}"), None, None);
                    let tool = m.tools.iter().find(|t| &t.id == tool_id).ok_or_else(|| {
                        EnvError::NotFound(format!("The provider has no tool {tool_id}."))
                    })?;
                    let version = detect::tool_version_in_root(tool, &staging, &bin_dirs)
                        .await
                        .ok_or_else(|| {
                            EnvError::NotFound(format!(
                                "{tool_id} did not run from the installed files."
                            ))
                        })?;
                    verified.push((tool.id.clone(), version));
                }
                Step::Run { .. } | Step::SystemPackages { .. } => {
                    return Err(EnvError::Unsupported(
                        "This plan step is not available yet.".into(),
                    ));
                }
            }
        }
        if cancel.is_cancelled() {
            return Err(EnvError::Cancelled);
        }
        // Publish: one rename, so a version directory is either complete or absent.
        if final_root.exists() {
            return Err(EnvError::AlreadyExists(format!(
                "{} {} is already installed.",
                m.name, plan.version
            )));
        }
        std::fs::rename(&staging, &final_root)
            .map_err(EnvError::io("Cannot publish the install"))?;
        cleanup.0 = None;
        let root = final_root.to_string_lossy().into_owned();
        let verified_ms = now_ms();
        Ok(InstallResult {
            plan_id: plan.plan_id.clone(),
            provider_id: m.id.clone(),
            version: plan.version.clone(),
            root: Some(root.clone()),
            verified: verified
                .into_iter()
                .map(|(tool_id, version)| Toolchain {
                    provider_id: m.id.clone(),
                    tool_id,
                    version,
                    location: ToolLocation::Managed { root: root.clone() },
                    verified_ms,
                })
                .collect(),
        })
    }

    /// Fetches into the content-addressed cache and verifies; returns the cached path.
    async fn download(
        &self,
        plan: &InstallPlan,
        step: usize,
        url: &str,
        sha256: &Sha256Source,
        cancel: &CancellationToken,
        report: &(dyn Fn(usize, String, Option<u64>, Option<u64>) + Sync),
    ) -> Result<PathBuf, EnvError> {
        let expected = match sha256 {
            Sha256Source::Digest { value } => value.to_ascii_lowercase(),
            Sha256Source::Url { url: sums } => {
                let text = tokio::select! {
                    t = self.http.get_text(sums) => t?,
                    _ = cancel.cancelled() => return Err(EnvError::Cancelled),
                };
                digest_from_checksum_file(&text, &file_name_of(url)).ok_or_else(|| {
                    EnvError::NotFound(format!(
                        "The checksum file has no entry for {}.",
                        file_name_of(url)
                    ))
                })?
            }
        };
        if !(expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit())) {
            return Err(EnvError::InvalidParams(
                "The expected checksum is malformed.".into(),
            ));
        }
        let cache = self.cache_dir();
        std::fs::create_dir_all(&cache)
            .map_err(EnvError::io("Cannot create the download cache"))?;
        let cached = cache.join(&expected);
        if cached.is_file() {
            let path = cached.clone();
            let actual = tokio::task::spawn_blocking(move || sha256_file(&path))
                .await
                .map_err(|_| EnvError::Unsupported("hashing task failed".into()))??;
            if actual == expected {
                return Ok(cached);
            }
            let _ = std::fs::remove_file(&cached);
        }
        let part = cache.join(format!(".part-{}", plan.plan_id));
        let name = file_name_of(url);
        let mut last = Instant::now() - PROGRESS_INTERVAL;
        let mut on_chunk = |done: u64, total: Option<u64>| {
            if last.elapsed() >= PROGRESS_INTERVAL || Some(done) == total {
                last = Instant::now();
                report(step, format!("Downloading {name}"), Some(done), total);
            }
        };
        let result = self.http.download(url, &part, cancel, &mut on_chunk).await;
        let (actual, _bytes) = match result {
            Ok(r) => r,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(e);
            }
        };
        if actual != expected {
            let _ = std::fs::remove_file(&part);
            return Err(EnvError::ChecksumMismatch {
                url: url.to_string(),
                expected,
                actual,
            });
        }
        std::fs::rename(&part, &cached).map_err(EnvError::io("Cannot store the download"))?;
        Ok(cached)
    }

    /// `env/uninstall`: removes `toolchains/<provider>/<version>` and nothing else.
    pub fn uninstall(&self, provider_id: &str, version: &str) -> Result<(), EnvError> {
        if !manifest::valid_id(provider_id) || !valid_version(version) {
            return Err(EnvError::InvalidParams(
                "Not a provider id and version.".into(),
            ));
        }
        let provider_dir = self.toolchains_dir().join(provider_id);
        let dir = provider_dir.join(version);
        let md = std::fs::symlink_metadata(&dir).map_err(|_| {
            EnvError::NotFound(format!("{provider_id} {version} is not installed."))
        })?;
        if !md.is_dir() {
            return Err(EnvError::UnsafeArchive(format!(
                "{provider_id}/{version} is not a plain directory; not removed"
            )));
        }
        let parent = std::fs::canonicalize(&provider_dir)
            .map_err(EnvError::io("Cannot resolve the toolchains directory"))?;
        let resolved =
            std::fs::canonicalize(&dir).map_err(EnvError::io("Cannot resolve the install"))?;
        if resolved.parent() != Some(parent.as_path())
            || !resolved.starts_with(
                std::fs::canonicalize(self.toolchains_dir())
                    .map_err(EnvError::io("Cannot resolve the toolchains directory"))?,
            )
        {
            return Err(EnvError::UnsafeArchive(format!(
                "{provider_id}/{version} resolves outside the toolchains directory; not removed"
            )));
        }
        std::fs::remove_dir_all(&resolved).map_err(EnvError::io("Cannot remove the install"))
    }
}

fn sha256_file(path: &Path) -> Result<String, EnvError> {
    let mut f = std::fs::File::open(path).map_err(EnvError::io("Cannot read the cache"))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(EnvError::io("Cannot read the cache"))?;
    Ok(hex::encode(h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_as_directory_names() {
        assert!(valid_version("1.84.0") && valid_version("v22.1.0-rc.1+b2"));
        assert!(!valid_version("../x") && !valid_version(".staging") && !valid_version("a/b"));
        assert!(!valid_version("") && !valid_version("a b"));
    }
}
