//! Environments: toolchain providers, detection, installation and resolution (protocol 0.2,
//! milestone M2; decisions in ADR-0014). Normative prose: `docs/protocols/providers.md` and
//! `docs/protocols/ipc.md` §0.2.
//!
//! Install flow: `env/planInstall` returns an immutable [`InstallPlan`] with a `planId`; the
//! client shows the whole plan to the user; `env/install` executes exactly that plan (an
//! unknown, expired or altered plan is `approval_denied`), streaming `env/installProgress`.
//! A successful install ends with the `env/installed` notification (to every client, so other
//! views refresh) and then the `env/install` response; a failed one with the error response. `$/cancel` on the `env/install`
//! request stops it and removes the staging directory.

use crate::lifecycle::Empty;
use crate::provider::{ArchiveFormat, InstallScope};
use crate::workspace::WorkspaceRef;
use crate::{method, notification, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

method!(
    /// The loaded providers (built-in, user and, for `workspaceId`, workspace providers) and
    /// the manifests that failed validation.
    Providers, "env/providers", ProvidersParams => ProvidersResult
);
method!(
    /// Which providers apply to the workspace, and for each, which of its tools are found
    /// (version and location) or missing. Detection only runs the declared version commands.
    /// Errors: `workspace_not_open`.
    Detect, "env/detect", WorkspaceRef => DetectResult
);
method!(
    /// Installable versions of a provider (its manifest's version source, or its delegate's
    /// listing) and the versions installed in managed storage. Errors: `not_found` (provider).
    Versions, "env/versions", VersionsParams => VersionsResult
);
method!(
    /// An install plan for a provider version on this host. Nothing runs. Errors: `not_found`,
    /// `unsupported_platform`, `prerequisite_missing`.
    PlanInstall, "env/planInstall", PlanInstallParams => InstallPlan
);
method!(
    /// Executes an issued plan, unchanged, within its lifetime; responds when done. Errors:
    /// `approval_denied` (unknown, expired or altered plan), `checksum_mismatch`, `io`,
    /// `spawn_failed`, `cancelled`.
    Install, "env/install", InstallParams => InstallResult
);
method!(
    /// Removes a managed install: its directory and nothing else. Errors: `not_found`.
    Uninstall, "env/uninstall", UninstallParams => Empty
);
method!(
    /// The workspace's environment from `.axiom/project.toml` (and the lock). Errors:
    /// `workspace_not_open`, `prerequisite_missing` (details `{ "missing": [toolId] }`),
    /// `invalid_params` (a malformed project file, details `{ "line": n, "reason": s }`).
    Resolve, "env/resolve", WorkspaceRef => Environment
);
method!(
    /// Trusts (or untrusts) a workspace's own providers (`.axiom/providers/`). Until trusted
    /// they are listed but never loaded. Persisted per workspace.
    Trust, "env/trust", TrustParams => Empty
);
notification!(
    /// Daemon to client. Progress of an `env/install`, at most 10 per second.
    Progress,
    "env/installProgress",
    InstallProgress
);

notification!(
    /// Daemon to client. A managed or system install completed and verified; sent before the
    /// `env/install` response.
    Installed,
    "env/installed",
    InstallResult
);

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersParams {
    /// Include this workspace's own providers (listed untrusted until `env/trust`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersResult {
    /// Every manifest found, including those that failed validation (with `errors`).
    pub providers: Vec<ProviderInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Version of the manifest.
    pub version: String,
    pub source: ProviderSource,
    /// Workspace providers: whether the workspace is trusted (they load only then). Always
    /// `true` for built-in and user providers.
    pub trusted: bool,
    /// Why the provider cannot be used (manifest validation errors). A provider with errors
    /// is never loaded; for a manifest that did not parse, `id` is its directory name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProviderSource {
    Builtin,
    User,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    /// The providers whose project detection matched, in registry order.
    pub providers: Vec<ProviderDetection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDetection {
    pub provider_id: String,
    /// The version `.axiom/project.toml` selects for this provider, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_version: Option<String>,
    pub tools: Vec<ToolStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub tool_id: String,
    pub required: bool,
    /// Absent when the tool was not found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub found: Option<Toolchain>,
}

/// A located, version-checked tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    pub provider_id: String,
    pub tool_id: String,
    pub version: String,
    pub location: ToolLocation,
    /// When the version was last checked (Unix ms).
    pub verified_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolLocation {
    /// Under Axiom's managed storage; `root` is the install root (host path).
    Managed { root: String },
    /// Found on the host `PATH`; `path` is the executable (host path).
    System { path: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionsParams {
    pub provider_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VersionsResult {
    /// Newest first.
    pub available: Vec<String>,
    /// Versions present in managed storage.
    pub installed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PlanInstallParams {
    pub provider_id: String,
    pub version: String,
    /// Index into the manifest's `install` methods; absent = the first one usable on this host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<u32>,
}

/// Everything an install will do, shown to the user before anything runs. Immutable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub plan_id: String,
    pub provider_id: String,
    pub version: String,
    pub scope: InstallScope,
    pub steps: Vec<Step>,
    /// Total download size when every download's size is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_bytes: Option<u64>,
    /// What the plan will do to the machine, for the approval summary.
    pub effects: Vec<Effect>,
    /// The plan expires at this time (Unix ms); `env/install` refuses it afterwards.
    pub expires_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Step {
    /// Download into the cache; verified against `sha256` before anything uses it.
    Download {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bytes: Option<u64>,
        sha256: Sha256Source,
    },
    /// Extract the preceding download into the staging root.
    #[serde(rename_all = "camelCase")]
    Extract {
        format: ArchiveFormat,
        strip_components: u32,
    },
    /// Run a program (never through a shell) with these extra variables.
    Run {
        argv: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// Install packages through the host package manager, elevated by the OS prompt.
    SystemPackages {
        manager: String,
        packages: Vec<String>,
        /// The exact command that will run (after the elevation wrapper).
        argv: Vec<String>,
    },
    /// Re-detect a tool inside the new install; failing this fails the install.
    #[serde(rename_all = "camelCase")]
    Verify { tool_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Sha256Source {
    /// The digest, from the manifest (lowercase hex).
    Digest { value: String },
    /// A checksum file fetched first (`sha256sum` format or a bare digest).
    Url { url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    /// Downloads from the network.
    Network,
    /// Writes under Axiom's managed storage only.
    WriteManaged,
    /// Runs programs the plan lists.
    RunPrograms,
    /// Changes the machine outside Axiom's storage (system packages).
    SystemChange,
    /// Needs the OS elevation prompt.
    Elevation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallParams {
    pub plan_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub plan_id: String,
    pub provider_id: String,
    pub version: String,
    /// The published install root (managed installs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
    /// The tools verified after the install.
    pub verified: Vec<Toolchain>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub plan_id: String,
    /// 0-based index of the current step in `InstallPlan.steps`.
    pub step: u32,
    pub steps: u32,
    /// Human-readable current activity, e.g. "Verifying SHA-256".
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_done: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes_total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UninstallParams {
    pub provider_id: String,
    pub version: String,
}

/// What a process started for the project runs with.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub toolchains: Vec<Toolchain>,
    /// Directories (host paths) put in front of `PATH`, in order.
    pub path_prepend: Vec<String>,
    /// Variables set for the process.
    pub vars: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrustParams {
    pub workspace_id: WorkspaceId,
    pub trusted: bool,
}
