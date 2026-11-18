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
//!
//! Methods (M2-ENV-002, M2-ENV-003):
//! - `archive`: download → extract → optional `setup` run → verify.
//! - `delegate`: the ecosystem installer runs with its home variables inside the new root.
//!   When an `archive` method `provides` the delegate tool, the plan first installs it into
//!   the same root (rustup-init for rustup); otherwise the delegate must be on `PATH`.
//!
//! Plans show `${root}` expanded to the final root. Execution happens in the staging
//! directory: every argv element and variable naming the final root is rewritten to the
//! staging root, and the finished staging directory is renamed into place.

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
use std::collections::{BTreeMap, HashMap};
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
/// How much of a failed command's output an error carries.
const OUTPUT_TAIL: usize = 2000;

struct Issued {
    plan: InstallPlan,
    /// The manifest the plan was made from; the install is refused if it changed.
    manifest: ProviderManifest,
    /// The final root as it appears in the plan's steps.
    final_root: String,
    /// Where `Verify` looks, and the variables it sets (expanded for the final root).
    verify_bin_dirs: Vec<String>,
    verify_env: BTreeMap<String, String>,
    expires: Instant,
}

/// Steps and verification layout for one method.
struct Built {
    steps: Vec<Step>,
    bin_dirs: Vec<String>,
    env: BTreeMap<String, String>,
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

    /// `env/versions`: from the first offered method (an archive's version source, a delegate's
    /// `versions`, else the delegate's `list_versions` run from `PATH`), cached for an hour;
    /// and what is installed.
    pub async fn versions(&self, m: &ProviderManifest) -> Result<VersionsResult, EnvError> {
        let mut available = Vec::new();
        for i in &m.install {
            match i {
                InstallMethod::Archive { provides, .. } if !provides.is_empty() => continue,
                InstallMethod::Archive { versions, .. }
                | InstallMethod::Delegate {
                    versions: Some(versions),
                    ..
                } => {
                    available = self.cached_versions(&m.id, versions).await?;
                    break;
                }
                InstallMethod::Delegate {
                    using,
                    list_versions,
                    ..
                } => {
                    available = self.delegate_versions(m, using, list_versions).await?;
                    break;
                }
                InstallMethod::SystemPackage { .. } => continue,
            }
        }
        Ok(VersionsResult {
            available,
            installed: self.installed(&m.id),
        })
    }

    async fn cached_versions(
        &self,
        provider_id: &str,
        src: &axiom_protocol::provider::VersionSource,
    ) -> Result<Vec<String>, EnvError> {
        let key = format!(
            "{provider_id}\n{}",
            serde_json::to_string(src).unwrap_or_default()
        );
        let cached = lock(&self.versions)
            .get(&key)
            .filter(|(at, _)| at.elapsed() < VERSIONS_TTL)
            .map(|(_, v)| v.clone());
        if let Some(v) = cached {
            return Ok(v);
        }
        let v = self.http.versions(src).await?;
        lock(&self.versions).insert(key, (Instant::now(), v.clone()));
        Ok(v)
    }

    /// Runs `list_versions` with the delegate found on `PATH`; the first word of each line
    /// that is a usable version, newest first.
    async fn delegate_versions(
        &self,
        m: &ProviderManifest,
        using: &str,
        argv: &[String],
    ) -> Result<Vec<String>, EnvError> {
        let program = system_tool(m, using)?;
        let mut full = vec![program.to_string_lossy().into_owned()];
        full.extend(argv.iter().skip(1).cloned());
        let out = run_program(
            &full,
            &BTreeMap::new(),
            &std::env::temp_dir(),
            &CancellationToken::new(),
        )
        .await?;
        let mut v: Vec<String> = out
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| valid_version(w))
            .map(String::from)
            .collect();
        v.sort_by(|a, b| compare_versions(b, a));
        v.dedup();
        Ok(v)
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

    /// `env/planInstall`. Runs nothing; at most a `HEAD` request per download for its size.
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
        let final_root = self.toolchains_dir().join(&m.id).join(version);
        if final_root.exists() {
            return Err(EnvError::AlreadyExists(format!(
                "{} {version} is already installed.",
                m.name
            )));
        }
        let root = final_root.to_string_lossy().into_owned();
        let index = match method {
            Some(i) if (i as usize) < m.install.len() => i as usize,
            Some(i) => {
                return Err(EnvError::InvalidParams(format!(
                    "{} has no install method {i}",
                    m.id
                )))
            }
            None => m
                .install
                .iter()
                .position(|im| match im {
                    InstallMethod::Archive {
                        artifacts,
                        provides,
                        ..
                    } => {
                        provides.is_empty()
                            && artifacts
                                .iter()
                                .any(|a| platform_matches(&a.platform, &host))
                    }
                    InstallMethod::Delegate { .. } => true,
                    InstallMethod::SystemPackage { .. } => false,
                })
                .ok_or_else(|| {
                    EnvError::UnsupportedPlatform(format!(
                        "{} has no managed install for this platform.",
                        m.name
                    ))
                })?,
        };
        let built = match &m.install[index] {
            archive @ InstallMethod::Archive { .. } => {
                build_archive(m, archive, version, &root, &host)?
            }
            InstallMethod::Delegate {
                using,
                install,
                env,
                bin_dirs,
                ..
            } => {
                let mut steps = Vec::new();
                let bootstrap = m.install.iter().find(|im| match im {
                    InstallMethod::Archive {
                        provides,
                        artifacts,
                        ..
                    } => {
                        provides.contains(using)
                            && artifacts
                                .iter()
                                .any(|a| platform_matches(&a.platform, &host))
                    }
                    _ => false,
                });
                let tool = m.tools.iter().find(|t| &t.id == using).ok_or_else(|| {
                    EnvError::InvalidParams(format!("{} has no tool {using}", m.id))
                })?;
                let delegate = match bootstrap {
                    Some(b) => {
                        let built = build_archive(m, b, version, &root, &host)?;
                        steps.extend(built.steps);
                        let dir = built.bin_dirs.first().cloned().unwrap_or_default();
                        let name = detect::executable_names(&tool.program)
                            .into_iter()
                            .next()
                            .unwrap_or_default();
                        Path::new(&root).join(dir).join(name)
                    }
                    None => system_tool(m, using)?,
                };
                let mut argv = vec![delegate.to_string_lossy().into_owned()];
                argv.extend(
                    install
                        .iter()
                        .skip(1)
                        .map(|a| manifest::expand(a, version, &root)),
                );
                let env = expand_map(env, version, &root);
                steps.push(Step::Run {
                    argv,
                    env: env.clone(),
                });
                Built {
                    steps,
                    bin_dirs: bin_dirs.clone(),
                    env,
                }
            }
            InstallMethod::SystemPackage { .. } => {
                return Err(EnvError::Unsupported(
                    "System package installs are not available yet.".into(),
                ))
            }
        };
        let mut steps = built.steps;
        for t in m.tools.iter().filter(|t| t.required) {
            steps.push(Step::Verify {
                tool_id: t.id.clone(),
            });
        }
        // Sizes, where the servers state them.
        let mut total = Some(0u64);
        for s in steps.iter_mut() {
            if let Step::Download { url, bytes, .. } = s {
                *bytes = self.http.size(url).await;
                total = total.zip(*bytes).map(|(a, b)| a + b);
            }
        }
        let has_download = steps.iter().any(|s| matches!(s, Step::Download { .. }));
        let mut effects = Vec::new();
        if has_download {
            effects.push(Effect::Network);
        }
        effects.push(Effect::WriteManaged);
        effects.push(Effect::RunPrograms);
        if steps.iter().any(|s| matches!(s, Step::Run { .. })) && !has_download {
            // A delegate from PATH downloads by itself.
            effects.insert(0, Effect::Network);
        }
        let plan = InstallPlan {
            plan_id: self.new_plan_id(),
            provider_id: m.id.clone(),
            version: version.to_string(),
            scope: InstallScope::Managed,
            steps,
            download_bytes: if has_download { total } else { None },
            effects,
            expires_ms: now_ms() + self.plan_lifetime.as_millis() as u64,
        };
        let mut plans = lock(&self.plans);
        plans.retain(|_, p| p.expires > Instant::now());
        plans.insert(
            plan.plan_id.clone(),
            Issued {
                plan: plan.clone(),
                manifest: m.clone(),
                final_root: root,
                verify_bin_dirs: built.bin_dirs,
                verify_env: built.env,
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
        let staging_str = staging.to_string_lossy().into_owned();
        let relocate = |s: &str| s.replace(issued.final_root.as_str(), &staging_str);

        let mut archive_path: Option<PathBuf> = None;
        let mut download_name = String::new();
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
                    download_name = file_name_of(url);
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
                    let name = download_name.clone();
                    tokio::task::spawn_blocking(move || {
                        archive::extract(&src, format, strip, &name, &dest, &|| c.is_cancelled())
                    })
                    .await
                    .map_err(|_| EnvError::Unsupported("extraction task failed".into()))??;
                }
                Step::Verify { tool_id } => {
                    report(i, format!("Verifying {tool_id}"), None, None);
                    let tool = m.tools.iter().find(|t| &t.id == tool_id).ok_or_else(|| {
                        EnvError::NotFound(format!("The provider has no tool {tool_id}."))
                    })?;
                    let env: BTreeMap<String, String> = issued
                        .verify_env
                        .iter()
                        .map(|(k, v)| (k.clone(), relocate(v)))
                        .collect();
                    let version =
                        detect::tool_version_in_root(tool, &staging, &issued.verify_bin_dirs, &env)
                            .await
                            .ok_or_else(|| {
                                EnvError::NotFound(format!(
                                    "{tool_id} did not run from the installed files."
                                ))
                            })?;
                    verified.push((tool.id.clone(), version));
                }
                Step::Run { argv, env } => {
                    report(i, format!("Running {}", command_label(argv)), None, None);
                    let argv: Vec<String> = argv.iter().map(|a| relocate(a)).collect();
                    let env: BTreeMap<String, String> =
                        env.iter().map(|(k, v)| (k.clone(), relocate(v))).collect();
                    run_program(&argv, &env, &staging, cancel).await?;
                }
                Step::SystemPackages { .. } => {
                    return Err(EnvError::Unsupported(
                        "System package installs are not available yet.".into(),
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

    /// `env/uninstall`: runs a delegate's declared `uninstall` command with the delegate from
    /// inside the root (its failure is logged, not fatal), then removes
    /// `toolchains/<provider>/<version>` and nothing else.
    pub async fn uninstall(
        &self,
        provider_id: &str,
        version: &str,
        m: Option<&ProviderManifest>,
    ) -> Result<(), EnvError> {
        let dir = self.checked_install_dir(provider_id, version)?;
        if let Some(m) = m.filter(|m| m.id == provider_id) {
            if let Some((argv, env)) = delegate_uninstall(m, version, &dir) {
                if let Err(e) = run_program(&argv, &env, &dir, &CancellationToken::new()).await {
                    tracing::warn!("{provider_id} {version}: the uninstall command failed: {e}");
                }
            }
        }
        let resolved = self.checked_install_dir(provider_id, version)?;
        tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&resolved))
            .await
            .map_err(|_| EnvError::Unsupported("removal task failed".into()))?
            .map_err(EnvError::io("Cannot remove the install"))
    }

    /// The canonical version directory, if it is a real directory directly inside the
    /// provider's toolchains directory.
    fn checked_install_dir(&self, provider_id: &str, version: &str) -> Result<PathBuf, EnvError> {
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
        Ok(resolved)
    }
}

fn expand_map(
    env: &BTreeMap<String, String>,
    version: &str,
    root: &str,
) -> BTreeMap<String, String> {
    env.iter()
        .map(|(k, v)| (k.clone(), manifest::expand(v, version, root)))
        .collect()
}

/// Download, extract and (if declared) setup steps of an archive method for this host.
fn build_archive(
    m: &ProviderManifest,
    method: &InstallMethod,
    version: &str,
    root: &str,
    host: &Platform,
) -> Result<Built, EnvError> {
    let InstallMethod::Archive {
        artifacts,
        bin_dirs,
        env,
        setup,
        ..
    } = method
    else {
        return Err(EnvError::InvalidParams("not an archive method".into()));
    };
    let artifact = artifacts
        .iter()
        .find(|a| platform_matches(&a.platform, host))
        .ok_or_else(|| {
            EnvError::UnsupportedPlatform(format!("{} has no download for this platform.", m.name))
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
    let env = expand_map(env, version, root);
    let mut steps = vec![
        Step::Download {
            url,
            bytes: None,
            sha256,
        },
        Step::Extract {
            format: artifact.format,
            strip_components: artifact.strip_components,
        },
    ];
    if !setup.is_empty() {
        steps.push(Step::Run {
            argv: setup
                .iter()
                .map(|a| manifest::expand(a, version, root))
                .collect(),
            env: env.clone(),
        });
    }
    Ok(Built {
        steps,
        bin_dirs: bin_dirs.clone(),
        env,
    })
}

/// A tool of the provider found on the daemon's `PATH`.
fn system_tool(m: &ProviderManifest, tool_id: &str) -> Result<PathBuf, EnvError> {
    let tool = m
        .tools
        .iter()
        .find(|t| t.id == tool_id)
        .ok_or_else(|| EnvError::InvalidParams(format!("{} has no tool {tool_id}", m.id)))?;
    let path = std::env::var_os("PATH");
    detect::path_candidates(&tool.program, path.as_deref())
        .into_iter()
        .next()
        .ok_or_else(|| {
            EnvError::PrerequisiteMissing(
                format!(
                    "{} needs {} to install, and it was not found.",
                    m.name, tool.program
                ),
                vec![tool_id.to_string()],
            )
        })
}

/// The delegate's uninstall command for an installed root, if the delegate is inside it.
fn delegate_uninstall(
    m: &ProviderManifest,
    version: &str,
    root: &Path,
) -> Option<(Vec<String>, BTreeMap<String, String>)> {
    let (using, uninstall, env) = m.install.iter().find_map(|i| match i {
        InstallMethod::Delegate {
            using,
            uninstall,
            env,
            ..
        } if !uninstall.is_empty() => Some((using, uninstall, env)),
        _ => None,
    })?;
    let program = &m.tools.iter().find(|t| &t.id == using)?.program;
    let root_s = root.to_string_lossy().into_owned();
    let exe = detect::managed_layouts(m).into_iter().find_map(|(d, _)| {
        detect::executable_names(program)
            .into_iter()
            .map(|n| root.join(&d).join(n))
            .find(|p| p.is_file())
    })?;
    let mut argv = vec![exe.to_string_lossy().into_owned()];
    argv.extend(
        uninstall
            .iter()
            .skip(1)
            .map(|a| manifest::expand(a, version, &root_s)),
    );
    Some((argv, expand_map(env, version, &root_s)))
}

fn command_label(argv: &[String]) -> String {
    let mut parts = argv.iter();
    let program = parts
        .next()
        .map(|p| {
            Path::new(p)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.clone())
        })
        .unwrap_or_default();
    std::iter::once(program)
        .chain(parts.cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Runs argv (never through a shell) with extra variables, stdin closed. Returns stdout;
/// a non-zero exit is an error carrying the end of the combined output.
pub async fn run_program(
    argv: &[String],
    env: &BTreeMap<String, String>,
    cwd: &Path,
    cancel: &CancellationToken,
) -> Result<String, EnvError> {
    let Some((program, args)) = argv.split_first() else {
        return Err(EnvError::InvalidParams("empty command".into()));
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args)
        .envs(env)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let child = cmd
        .spawn()
        .map_err(|e| EnvError::Spawn(format!("Cannot start {}: {e}", command_label(&argv[..1]))))?;
    let out = tokio::select! {
        o = child.wait_with_output() => o.map_err(EnvError::io("Cannot run the command"))?,
        _ = cancel.cancelled() => return Err(EnvError::Cancelled),
    };
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        return Ok(stdout);
    }
    let mut all = stdout;
    all.push_str(&String::from_utf8_lossy(&out.stderr));
    let tail: String = {
        let chars: Vec<char> = all.chars().collect();
        chars[chars.len().saturating_sub(OUTPUT_TAIL)..]
            .iter()
            .collect()
    };
    Err(EnvError::CommandFailed {
        command: command_label(argv),
        code: out.status.code(),
        output: tail,
    })
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
