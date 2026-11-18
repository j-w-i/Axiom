//! Builds: provider tasks run in the resolved environment, with diagnostics parsed from their
//! output (protocol 0.2, milestone M2; ADR-0014). A build is a `task` (protocol 0.1): its
//! output and exit arrive as `task/output` and `task/exited` under the returned `taskId`, and
//! `task/cancel` stops it. `build/diagnostics` adds the parsed problems.

use crate::diagnostics::Diagnostic;
use crate::provider::TaskKind;
use crate::workspace::WorkspaceRef;
use crate::{method, notification, TaskId, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

method!(
    /// The tasks of every provider detected in the workspace. Errors: `workspace_not_open`.
    Tasks, "build/tasks", WorkspaceRef => TasksResult
);
method!(
    /// Starts a provider task in the workspace's resolved environment. Errors:
    /// `workspace_not_open`, `not_found` (provider or task), `prerequisite_missing` (the
    /// environment cannot be resolved; details `{ "missing": [toolId] }`), `spawn_failed`.
    Run, "build/run", RunParams => RunResult
);
notification!(
    /// Daemon to client. Diagnostics parsed from a build's output: each notification carries
    /// only the diagnostics new since the previous one; the last has `complete: true` and is
    /// sent before `task/exited`.
    Diagnostics,
    "build/diagnostics",
    BuildDiagnostics
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TasksResult {
    pub tasks: Vec<TaskInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TaskInfo {
    pub provider_id: String,
    /// Task id within the provider's manifest.
    pub id: String,
    pub kind: TaskKind,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunParams {
    pub workspace_id: WorkspaceId,
    pub provider_id: String,
    pub task: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunResult {
    pub task_id: TaskId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BuildDiagnostics {
    pub task_id: TaskId,
    pub diagnostics: Vec<Diagnostic>,
    /// No more diagnostics follow for this task.
    pub complete: bool,
}
