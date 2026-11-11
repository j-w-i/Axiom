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
