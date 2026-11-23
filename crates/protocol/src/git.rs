//! Git state of a workspace (protocol 0.4, milestone M4; ADR-0018 §5), read through the `git`
//! program. Read-only: commit, stage and push are not part of 0.4.

use crate::{method, notification, WorkspaceId, WorkspacePath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

method!(
    /// Branch and per-file state. A workspace that is not in a repository, or a machine without
    /// `git`, gives `repo: false` with a reason (not an error). Paths outside the workspace are
    /// omitted. Errors: `workspace_not_open`, `timeout`.
    Status, "git/status", crate::workspace::WorkspaceRef => GitStatus
);
method!(
    /// A file's text at `HEAD` or in the index (for diffs and gutter markers). Errors:
    /// `workspace_not_open`, `git_unavailable` (no repository or no `git`), `not_found` (not in
    /// that revision), `invalid_params`.
    Show, "git/show", ShowParams => ShowResult
);
notification!(
    /// Daemon to client. The repository state may have changed (worktree or `.git`), debounced.
    DidChange,
    "git/didChange",
    crate::workspace::WorkspaceRef
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub repo: bool,
    /// Why `repo` is false: `git` not found, or not a repository.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The branch name; absent when HEAD is detached.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The commit HEAD points at (abbreviated), when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    #[serde(default)]
    pub ahead: u32,
    #[serde(default)]
    pub behind: u32,
    #[serde(default)]
    pub files: Vec<GitFile>,
    /// Protocol 0.6 (additive, AUD-012): no file watch could be made for this repository (the
    /// system's watch limit, for example), so its changes are looked for every few seconds and
    /// `git/didChange` comes late. Absent (false) otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub polling: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GitFile {
    pub path: WorkspacePath,
    /// The staged side (index vs HEAD).
    pub index: GitFileState,
    /// The unstaged side (worktree vs index).
    pub worktree: GitFileState,
    /// For a rename or copy: the path it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orig_path: Option<WorkspacePath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum GitFileState {
    Unmodified,
    Modified,
    Added,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Untracked,
    Ignored,
    Conflicted,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum GitRev {
    Head,
    Index,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShowParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
    pub rev: GitRev,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShowResult {
    /// The text (UTF-8); absent when the file is binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub binary: bool,
}
