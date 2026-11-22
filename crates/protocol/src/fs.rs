//! Workspace-scoped file access. Every path is a [`WorkspacePath`]; nothing
//! in this module can address a file outside an open workspace.

use crate::lifecycle::Empty;
use crate::{method, notification, WorkspaceId, WorkspacePath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Largest file `fs/readFile` returns and `fs/writeFile` accepts: 16 MiB.
pub const MAX_TEXT_FILE_BYTES: u64 = 16 * 1024 * 1024;

method!(
    /// Lists one directory (non-recursive). Entries are sorted directories
    /// first, then by name, case-insensitively. Errors: `not_found`,
    /// `invalid_params` (path is a file), `workspace_not_open`.
    List, "fs/list", ListParams => ListResult
);
method!(
    /// Reads a UTF-8 text file. Errors: `not_found`, `unsupported_encoding`
    /// (not valid UTF-8), `too_large`, `invalid_params` (path is a directory).
    ReadFile, "fs/readFile", ReadFileParams => ReadFileResult
);
method!(
    /// Atomically replaces a file's content (write to a sibling temp file,
    /// fsync, rename). Creates the file if absent; never creates parent
    /// directories. Errors: `conflict` if `expectedVersion` is given and does
    /// not match the file on disk (details: `{ "currentVersion": string|null }`),
    /// `not_found` (parent directory missing), `too_large`.
    WriteFile, "fs/writeFile", WriteFileParams => WriteFileResult
);
method!(
    /// Starts or stops change notifications for the whole workspace. Watching
    /// is off by default on each connection.
    Watch, "fs/watch", WatchParams => Empty
);
method!(
    /// Protocol 0.6 (audit AUD-022). Creates a new file with `text` (default empty), atomically
    /// (temp file, fsync, rename that never replaces). The directory must exist. Errors:
    /// `already_exists` (anything has that name: a file, a directory, a symlink), `not_found`
    /// (directory missing), `permission_denied` (resolves outside the workspace), `too_large`,
    /// `invalid_params` (bad path, the root).
    CreateFile, "fs/createFile", CreateFileParams => WriteFileResult
);
method!(
    /// Protocol 0.6. Creates a directory; with `parents`, the missing ones above it too. Errors:
    /// `already_exists`, `not_found` (parent missing, without `parents`), `permission_denied`,
    /// `invalid_params`.
    CreateDirectory, "fs/createDirectory", CreateDirectoryParams => Empty
);
method!(
    /// Protocol 0.6. Renames or moves a file, directory or symlink inside the workspace. A
    /// symlink is moved itself, never followed. Never replaces: `already_exists` if `to` exists
    /// (a case-only rename of the same entry is allowed). `to`'s directory must exist
    /// (`not_found`); a directory cannot move into itself (`invalid_params`); either path
    /// resolving outside the workspace is `permission_denied`.
    Rename, "fs/rename", RenameParams => Empty
);
method!(
    /// Protocol 0.6. Moves a file, directory or symlink (the link itself) to the person's trash:
    /// the freedesktop.org trash on Linux, `~/.Trash` on macOS. `unsupported` where there is no
    /// trash the entry can be moved to (other platforms, another drive without a trash
    /// folder): nothing is ever deleted permanently. Errors besides: `not_found`,
    /// `permission_denied`, `invalid_params` (the root).
    Trash, "fs/trash", TrashParams => Empty
);
notification!(
    /// Daemon to client. Coalesced (at most one per 100 ms per workspace)
    /// set of paths that changed on disk, from any cause including this
    /// client's own writes. Paths under ignored directories are not reported.
    DidChange,
    "fs/didChange",
    DidChangeParams
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListResult {
    pub entries: Vec<DirEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DirEntry {
    /// Final path component only.
    pub name: String,
    pub kind: EntryKind,
    /// Size in bytes; present for files only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// True if matched by `.gitignore`/`.ignore` rules or is `.git`. Ignored
    /// entries are still listed so the UI can dim them.
    #[serde(default)]
    pub ignored: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    File,
    Directory,
    /// A symlink is reported as such and never followed by `fs/list`.
    Symlink,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReadFileResult {
    /// File content exactly as on disk, including its line endings.
    pub text: String,
    pub version: FileVersion,
}

/// Opaque content fingerprint (lowercase hex SHA-256 of the file bytes).
/// Two reads return equal versions iff the bytes are equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct FileVersion(pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteFileParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
    pub text: String,
    /// Version the client last read. If set, the write is refused with
    /// `conflict` unless the file on disk still has this version. Omit to
    /// create a new file (fails with `conflict` if the file exists) —
    /// set [`WriteFileParams::overwrite`] to bypass both checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<FileVersion>,
    /// Write unconditionally. Used for "overwrite anyway" after a conflict.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteFileResult {
    pub version: FileVersion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WatchParams {
    pub workspace_id: WorkspaceId,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DidChangeParams {
    pub workspace_id: WorkspaceId,
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FileChange {
    pub path: WorkspacePath,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
}

/// `fs/createFile` (0.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
    /// The new file's content; empty when absent.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// `fs/createDirectory` (0.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateDirectoryParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
    /// Also create missing parent directories (`mkdir -p`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub parents: bool,
}

/// `fs/rename` (0.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameParams {
    pub workspace_id: WorkspaceId,
    pub from: WorkspacePath,
    pub to: WorkspacePath,
}

/// `fs/trash` (0.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TrashParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
}
