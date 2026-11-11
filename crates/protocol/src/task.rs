//! Process execution as supervised tasks with streamed output.
//!
//! Lifecycle: `task/start` → zero or more `task/output` → exactly one
//! `task/exited`. `task/start` returns as soon as the process has spawned;
//! spawn failure is reported as the `task/start` error (`spawn_failed`) and
//! no notifications follow.

use crate::lifecycle::Empty;
use crate::{method, notification, TaskId, WorkspaceId, WorkspacePath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

method!(
    /// Spawns a process. Errors: `spawn_failed` (details:
    /// `{ "program": string, "osError": string }`), `not_found` (cwd),
    /// `workspace_not_open`.
    Start, "task/start", StartParams => StartResult
);
method!(
    /// Requests termination: graceful signal to the whole process tree, then
    /// a forced kill after 3 s. Returns immediately; completion is signalled
    /// by `task/exited` with `outcome: "cancelled"`. Cancelling a finished
    /// task succeeds and does nothing. Errors: `not_found`.
    Cancel, "task/cancel", TaskRef => Empty
);
method!(
    /// Tasks of a workspace: all running ones plus the 50 most recently
    /// finished, newest first.
    ListTasks, "task/list", ListTasksParams => ListTasksResult
);
notification!(
    /// Daemon to client. A chunk of output. Chunks of one task carry strictly
    /// increasing `seq` starting at 0; chunk boundaries are arbitrary (not
    /// line-aligned). Invalid UTF-8 is replaced with U+FFFD.
    Output, "task/output", OutputParams
);
notification!(
    /// Daemon to client. Sent exactly once per started task, after its final
    /// `task/output`.
    Exited, "task/exited", ExitedParams
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    pub workspace_id: WorkspaceId,
    pub command: Command,
    /// Working directory; defaults to the workspace root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<WorkspacePath>,
    /// Variables added to (or overriding) the daemon's environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Display label; defaults to the command line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// What to run. `argv` is preferred: it involves no shell and no quoting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    /// Program and arguments, executed directly. `argv[0]` is resolved on
    /// `PATH`. Must be non-empty.
    Argv(Vec<String>),
    /// A command line for the host's default shell (`sh -c` on Unix,
    /// `cmd /C` on Windows). For user-typed commands only.
    Shell(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListTasksParams {
    pub workspace_id: WorkspaceId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListTasksResult {
    pub tasks: Vec<TaskInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub task_id: TaskId,
    pub label: String,
    pub command: Command,
    /// Milliseconds since the Unix epoch.
    pub started_ms: u64,
    /// Absent while the task is running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<ExitStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutputParams {
    pub task_id: TaskId,
    pub seq: u64,
    pub stream: Stream,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExitedParams {
    pub task_id: TaskId,
    #[serde(flatten)]
    pub status: ExitStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExitStatus {
    pub outcome: Outcome,
    /// Process exit code; absent if the process was killed by a signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Unix signal number that terminated the process, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<i32>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Exit code 0.
    Succeeded,
    /// Non-zero exit code, or killed by a signal Axiom did not send.
    Failed,
    /// Terminated because of `task/cancel`, workspace close, or shutdown.
    Cancelled,
}
