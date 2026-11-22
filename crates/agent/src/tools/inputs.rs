//! Input types of the built-in tools (M3; `preview.*` and `ui.*` from M5) (`docs/protocols/agents.md` §Tools). Each derives its JSON Schema,
//! which is what models are offered; inputs are validated by deserialising into these types
//! (`deny_unknown_fields`), so the schema and the validation cannot drift apart.

use super::patch::Edit;
use axiom_protocol::WorkspacePath;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceInfoInput {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchInput {
    /// Text to find (literal unless `regex` is true).
    pub query: String,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    /// Only paths matching this glob, e.g. `src/**/*.rs`.
    #[serde(default)]
    pub glob: Option<String>,
    /// From a previous result's `nextCursor`.
    #[serde(default)]
    pub cursor: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListInput {
    /// Workspace-relative directory; empty for the root.
    #[serde(default)]
    pub path: Option<WorkspacePath>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadInput {
    /// Workspace-relative file path.
    pub path: WorkspacePath,
    /// First line to return, 1-based (default 1).
    #[serde(default)]
    pub start_line: Option<u32>,
    /// Last line to return, inclusive (default: as many as fit).
    #[serde(default)]
    pub end_line: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyPatchInput {
    /// Workspace-relative file path.
    pub path: WorkspacePath,
    /// Search/replace edits applied in order; each search must match exactly once.
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// Create the file with this content (it must not exist).
    #[serde(default)]
    pub create: Option<String>,
    /// Delete the file.
    #[serde(default)]
    pub delete: bool,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskRunInput {
    /// Program and arguments; no shell. Use `["sh", "-c", "…"]` only when a shell is needed.
    pub argv: Vec<String>,
    /// Seconds to wait before stopping it (default 120, at most 600).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildRunInput {
    /// A task id from `workspace.info`'s `tasks` (e.g. `build`, `test`).
    pub task: String,
    /// The provider offering it (default: the first that does).
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EmptyInput {}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanInstallInput {
    pub provider: String,
    /// A version from the provider's list (default: its newest).
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallInput {
    /// The `planId` from `env.planInstall`.
    pub plan_id: String,
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenInput {
    pub path: WorkspacePath,
    /// 1-based.
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub column: Option<u32>,
}

/// `preview.start` (M5).
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewStartInput {
    /// The dev server command; default: the project's Vite (`node_modules/.bin/vite`), else
    /// `npm run dev`.
    #[serde(default)]
    pub argv: Option<Vec<String>>,
}

/// `ui.snapshot` (M5).
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiSnapshotInput {
    /// Default: the workspace's running preview.
    #[serde(default)]
    pub preview_id: Option<String>,
    /// At most this many nodes (default 300, at most 2000).
    #[serde(default)]
    pub max_nodes: Option<u32>,
}

/// `ui.inspect` (M5): one node, by `stableKey` (preferred, survives reloads) or by `node` id.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiInspectInput {
    #[serde(default)]
    pub preview_id: Option<String>,
    #[serde(default)]
    pub stable_key: Option<String>,
    /// A node id from the latest `ui.snapshot`.
    #[serde(default)]
    pub node: Option<String>,
}

/// `ui.selection` (M5).
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiSelectionInput {
    #[serde(default)]
    pub preview_id: Option<String>,
}

/// `ui.verify` (M5): the checks stated before the patch, evaluated against the reloaded page.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiVerifyInput {
    #[serde(default)]
    pub preview_id: Option<String>,
    /// At least one geometry or property assertion that would show the change succeeded.
    pub assertions: Vec<axiom_protocol::ui::Assertion>,
    /// The element the change was about.
    #[serde(default)]
    pub target: Option<axiom_protocol::ui::NodeTarget>,
    /// How long to wait for the page to settle (ms; default 5000, at most 30000).
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// The JSON Schema of `T` as a plain object schema (no `$schema`/`title`), as model APIs want.
pub fn schema_of<T: JsonSchema>() -> serde_json::Value {
    let mut g = schemars::generate::SchemaSettings::draft2020_12()
        .with(|s| s.inline_subschemas = true)
        .into_generator();
    let mut v = serde_json::to_value(g.root_schema_for::<T>()).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.remove("$schema");
        o.remove("title");
        o.entry("properties")
            .or_insert_with(|| serde_json::json!({}));
    }
    v
}
