//! Previews (protocol 0.5, milestone M5; ADR-0019 §2–3, ADR-0020 §8). A preview is the
//! workspace's web dev server (Vite) run as a task with Axiom's plugin injected: the plugin
//! stamps source locations into the page and injects the inspection adapter, which dials the
//! daemon (`docs/protocols/inspection.md`). The page opens in the person's browser; the shell's
//! Inspector shows what the adapter reports.

use crate::inspection::Capabilities;
use crate::{method, notification, TaskId, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Opaque identifier of a preview, minted by `preview/start`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct PreviewId(pub String);

method!(
    /// Starts the dev server with the plugin injected and answers once its URL is known (or it
    /// failed). The command: `argv` if given, else the provider's `dev` task, else
    /// `npm run dev`; the user's Vite config is imported, never edited. A second start of a
    /// running preview returns it. Errors: `workspace_not_open`, `spawn_failed`,
    /// `preview_failed` (the server exited or printed no URL within 60 s; `details.output` has
    /// its last lines).
    Start, "preview/start", StartParams => PreviewInfo
);
method!(
    /// Stops the dev server (the task is cancelled) and ends the adapter session. Idempotent.
    /// Errors: `not_found`.
    Stop, "preview/stop", PreviewRef => crate::lifecycle::Empty
);
method!(
    /// The workspace's previews. Errors: `workspace_not_open`.
    List, "preview/list", crate::workspace::WorkspaceRef => ListResult
);
notification!(
    /// Daemon to client. A preview's state changed: started, its URL known, its page's adapter
    /// connected or gone, stopped or failed.
    State,
    "preview/state",
    PreviewInfo
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartParams {
    pub workspace_id: WorkspaceId,
    /// The dev server's command and arguments, overriding the provider's `dev` task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRef {
    pub preview_id: PreviewId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ListResult {
    pub previews: Vec<PreviewInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PreviewState {
    /// The dev server is starting; no URL yet.
    Starting,
    /// The URL is known; `adapter` says whether a page is connected.
    Ready,
    /// The server exited or gave no URL; `message` and `output` say why.
    Failed,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub preview_id: PreviewId,
    pub workspace_id: WorkspaceId,
    pub state: PreviewState,
    /// The page to open (loopback), once known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The dev server's task (its output streams as `task/output`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// The connected page's adapter, if a page is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<Capabilities>,
    /// Why it failed, in one sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The last lines the server printed (on failure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}
