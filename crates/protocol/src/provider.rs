//! Toolchain provider manifest (`provider.toml`). Normative prose:
//! `docs/protocols/providers.md`.
//!
//! Frozen for M2 (M2-ARCH-001; `axiom.provider/1`). A manifest is pure data: the
//! daemon interprets it; a manifest can never run code other than the
//! commands it declares, and each such command passes the permission gate.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::lifecycle::HostOs;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderManifest {
    /// Must be `axiom.provider/1`.
    pub schema: String,
    /// Globally unique, lowercase `[a-z0-9-]+`, e.g. `rust`, `apple-xtool`.
    pub id: String,
    pub name: String,
    pub description: String,
    /// Version of this manifest (semver), not of the toolchain.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Hosts this provider can run on. Empty = all.
    #[serde(default)]
    pub hosts: Vec<Platform>,
    /// Out-of-process implementation for operations that cannot be expressed
    /// declaratively. If present, the daemon starts it on first use and
    /// speaks the `provider/*` JSON-RPC methods to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<ProcessSpec>,
    /// How to recognise a project this provider applies to.
    #[serde(default)]
    pub project: ProjectDetection,
    /// Executables/SDKs this provider manages.
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    /// Ways to obtain the toolchain, in order of preference.
    #[serde(default)]
    pub install: Vec<InstallMethod>,
    /// Project operations offered (build, run, test...).
    #[serde(default)]
    pub tasks: Vec<TaskTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Platform {
    pub os: HostOs,
    /// `x86_64`, `aarch64`. Absent = any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProcessSpec {
    /// Program and arguments, relative to the manifest's directory.
    pub command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectDetection {
    /// Glob patterns relative to a candidate project directory; the provider
    /// applies if any matches. E.g. `["Cargo.toml"]`.
    #[serde(default)]
    pub any_file: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    /// Unique within the provider, e.g. `cargo`.
    pub id: String,
    /// Executable name looked up on PATH and in managed install roots.
    pub program: String,
    /// Arguments that print the version. Default `["--version"]`.
    #[serde(default = "default_version_args")]
    pub version_args: Vec<String>,
    /// Regex with one capture group extracting the version from the output.
    pub version_regex: String,
    /// Whether the provider is unusable without this tool.
    #[serde(default = "yes")]
    pub required: bool,
}

fn default_version_args() -> Vec<String> {
    vec!["--version".into()]
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallMethod {
    /// Download a release archive into Axiom-managed storage.
    Archive {
        /// Where the list of versions comes from.
        versions: VersionSource,
        artifacts: Vec<Artifact>,
        /// Directories (relative to the install root) to prepend to PATH.
        #[serde(default)]
        bin_dirs: Vec<String>,
        /// Extra environment variables; `${root}` expands to the install root.
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// Delegate to the ecosystem's own installer (rustup, swiftly, uv...).
    /// Runs with managed HOME-style variables so nothing global is modified
    /// unless `scope` is `system`.
    Delegate {
        /// Tool that must already be available (itself possibly installed by
        /// another provider), e.g. `rustup`.
        using: String,
        scope: InstallScope,
        /// Command templates; `${version}` and `${root}` expand.
        install: Vec<String>,
        #[serde(default)]
        uninstall: Vec<String>,
        list_versions: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// Install through the host's package manager. Always `system` scope and
    /// always requires explicit user approval.
    SystemPackage {
        /// Package names keyed by manager: `apt`, `dnf`, `pacman`, `brew`,
        /// `winget`...
        packages: BTreeMap<String, Vec<String>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum InstallScope {
    /// Under Axiom's data directory; removable without side effects.
    Managed,
    /// Modifies the user's or machine's global state.
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VersionSource {
    /// Fixed list in the manifest.
    Static { versions: Vec<String> },
    /// JSON document; `pointer` (RFC 6901, `*` wildcard for arrays) selects
    /// the version strings.
    Json { url: String, pointer: String },
    /// GitHub releases of `owner/repo`; tags stripped of `tag_prefix`.
    GithubReleases {
        repo: String,
        #[serde(default)]
        tag_prefix: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub platform: Platform,
    /// URL template; `${version}` expands.
    pub url: String,
    pub format: ArchiveFormat,
    /// Leading path components to strip on extraction.
    #[serde(default)]
    pub strip_components: u32,
    pub checksum: Checksum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum ArchiveFormat {
    #[serde(rename = "tar.gz")]
    TarGz,
    #[serde(rename = "tar.xz")]
    TarXz,
    #[serde(rename = "zip")]
    Zip,
}

/// Integrity is mandatory: an artifact without a verifiable SHA-256 is not
/// installable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Checksum {
    /// Per-version digests in the manifest: version -> lowercase hex.
    Sha256 { values: BTreeMap<String, String> },
    /// URL template of a checksum file (`sha256sum` format or bare digest).
    Sha256Url { url: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TaskTemplate {
    /// Unique within the provider, e.g. `build`.
    pub id: String,
    pub kind: TaskKind,
    pub label: String,
    /// argv; never passed through a shell.
    pub command: Vec<String>,
    /// How to turn the command's output into diagnostics.
    #[serde(default)]
    pub diagnostics: DiagnosticsFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TaskKind {
    Build,
    Run,
    Test,
    Clean,
    Other,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiagnosticsFormat {
    #[default]
    None,
    /// `cargo --message-format=json` stream on stdout.
    CargoJson,
    /// `file:line:col: severity: message` (GCC/Clang/Swift text form).
    GccText,
    /// SARIF 2.1.0 document on stdout.
    Sarif,
    /// Line regex with named groups `file`, `line`, optional `column`,
    /// `severity`, `code`, `message`.
    Regex { pattern: String },
}
