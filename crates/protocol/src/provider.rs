//! Toolchain provider manifest (`provider.toml`). Normative prose:
//! `docs/protocols/providers.md`.
//!
//! Frozen for M2 (M2-ARCH-001; `axiom.provider/1`); M4-ARCH-001 added the optional `lsp` table;
//! M5.5-ARCH-001 its optional `requires`. A manifest is pure data: the
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
    /// Language servers for the provider's languages (M4, ADR-0018 §2; additive to
    /// `axiom.provider/1`: a manifest without it is unchanged).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lsp: Vec<LspServerSpec>,
}

/// A language server the daemon may start for any workspace, when a document of one of its
/// languages opens (M5.5, ADR-0021 §3; M4 required the provider to apply to the workspace).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LspServerSpec {
    /// Unique across providers, e.g. `rust-analyzer`.
    pub id: String,
    /// Language ids (Monaco's, which are LSP's) the server handles, e.g. `["rust"]`.
    pub languages: Vec<String>,
    /// argv, resolved in the workspace's environment (the provider's tools first); never a
    /// shell.
    pub command: Vec<String>,
    /// Sent as `initializationOptions` (merged under settings `lsp.<id>.initializationOptions`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<serde_json::Value>,
    /// One sentence shown when the command is not found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    /// Tool ids that must resolve in the environment for the server to run (an SDK, a runtime);
    /// unresolved, the server is `missing` and names the provider that supplies them (M5.5,
    /// ADR-0021 §6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
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
        /// Tool ids this method installs for another method to use (e.g. `rustup` for the
        /// `delegate` method). Such a method only bootstraps: it is not offered on its own.
        /// Added in M2-ENV-003.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        provides: Vec<String>,
        /// argv run once after extraction, inside the new root, with `env`; `${root}` and
        /// `${version}` expand. Never through a shell. Added in M2-ENV-003.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        setup: Vec<String>,
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
        /// Directories (relative to the install root) holding the installed executables.
        /// Added in M2-ENV-003.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        bin_dirs: Vec<String>,
        /// Where installable versions come from; absent = run `list_versions` through the
        /// delegate. Added in M2-ENV-003.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        versions: Option<VersionSource>,
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
    Json {
        url: String,
        pointer: String,
        /// Removed from the front of each version (`v` for nodejs.org's `v22.11.0`); versions
        /// without it are dropped. Added in M2-PROV-001.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        strip_prefix: String,
    },
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
    /// For `file` and `gz`: the executable's name in the root, when the URL's last segment is
    /// not it (`marksman-linux-x64` → `marksman`). M5.5, additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
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
    /// The download is the file itself (a single executable, e.g. `rustup-init`), placed at
    /// the root under its URL's last path segment, executable. Added in M2-ENV-003.
    #[serde(rename = "file")]
    File,
    /// One gzip-compressed file (rust-analyzer's releases): decompressed to the root under the
    /// URL's last segment without `.gz` (or the artifact's `file_name`), executable. M5.5.
    #[serde(rename = "gz")]
    Gz,
}

/// Integrity is mandatory: an artifact without a verifiable SHA-256 (or SHA-512) is not
/// installable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Checksum {
    /// Per-version digests in the manifest: version -> lowercase hex.
    Sha256 { values: BTreeMap<String, String> },
    /// URL template of a checksum file (`sha256sum` format or bare digest).
    Sha256Url { url: String },
    /// Per-version SHA-512 digests in the manifest: version -> lowercase hex (128 digits). For
    /// publishers that give no SHA-256 (Microsoft's .NET). M5.5, additive.
    Sha512 { values: BTreeMap<String, String> },
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
