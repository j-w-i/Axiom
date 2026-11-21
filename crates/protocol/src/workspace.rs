//! Workspace lifecycle and per-workspace persisted UI state.

use crate::lifecycle::Empty;
use crate::{method, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

method!(
    /// Opens a directory as a workspace. Idempotent: opening a root that is
    /// already open returns the existing id. Errors: `not_found` (path does
    /// not exist), `invalid_params` (not absolute or not a directory),
    /// `permission_denied`.
    Open, "workspace/open", OpenParams => OpenResult
);
method!(
    /// Closes a workspace: cancels its tasks and stops its file watcher.
    /// Errors: `workspace_not_open`.
    Close, "workspace/close", WorkspaceRef => Empty
);
method!(
    /// Most recently opened workspaces, newest first, at most 20. Entries
    /// whose directory no longer exists are omitted.
    Recent, "workspace/recent", Empty => RecentResult
);
method!(
    /// Text or regular-expression search over the workspace's files (honouring `.gitignore`;
    /// binary and very large files skipped), or with `mode: files` the file paths themselves
    /// (for quick open). Bounded by `limit` (default 2 000, at most 20 000); `truncated` says
    /// more exist. A newer search from the same client is not cancelled implicitly: cancel the
    /// request (`$/cancelRequest`). Protocol 0.4 (ADR-0018 §6). Errors: `workspace_not_open`,
    /// `invalid_params` (a bad regular expression, with the reason).
    Search, "workspace/search", SearchParams => SearchResult
);
method!(
    /// Reads client UI state stored for this workspace. Missing keys are
    /// absent from the result.
    GetState, "workspace/getState", GetStateParams => StateResult
);
method!(
    /// Merges keys into the stored UI state. A `null` value deletes the key.
    /// Durable before the response is sent. Errors: `too_large` if a single
    /// value exceeds 1 MiB when serialized.
    SetState, "workspace/setState", SetStateParams => Empty
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OpenParams {
    /// Absolute host path in host-native form.
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    pub workspace_id: WorkspaceId,
    /// Canonical absolute root (symlinks resolved), host-native form.
    pub root: String,
    /// Display name: the final component of `root`.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRef {
    pub workspace_id: WorkspaceId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecentResult {
    pub workspaces: Vec<RecentWorkspace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecentWorkspace {
    pub root: String,
    pub name: String,
    /// Milliseconds since the Unix epoch.
    pub last_opened_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetStateParams {
    pub workspace_id: WorkspaceId,
    /// Keys to read; empty means all keys.
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StateResult {
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SetStateParams {
    pub workspace_id: WorkspaceId,
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    #[default]
    Text,
    Files,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchParams {
    pub workspace_id: WorkspaceId,
    /// The text or pattern; ignored in `files` mode.
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub mode: SearchMode,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    /// Comma-separated workspace-relative globs a path must match (`**` allowed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include: Option<String>,
    /// Comma-separated globs of paths to skip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// Text mode: the matches in path order, then line order.
    #[serde(default)]
    pub matches: Vec<SearchMatch>,
    /// Files mode: the workspace's file paths.
    #[serde(default)]
    pub files: Vec<crate::WorkspacePath>,
    pub truncated: bool,
    pub files_searched: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: crate::WorkspacePath,
    /// 1-based.
    pub line: u32,
    /// 1-based, in UTF-16 code units (Monaco's columns).
    pub column: u32,
    /// Exclusive end column on the same line.
    pub end_column: u32,
    /// The line, trimmed to 300 characters around the match.
    pub preview: String,
    /// Column of the match within `preview`, 1-based (differs from `column` when trimmed).
    pub preview_column: u32,
}
