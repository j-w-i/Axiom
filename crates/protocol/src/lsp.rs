//! Language servers (protocol 0.4, milestone M4; ADR-0018 §1–2). The daemon supervises one
//! server per (workspace, server definition) and owns the LSP conversation with it: it assigns
//! its own request ids, answers the server's requests, and mirrors open documents. A client asks
//! through `lsp/request` and syncs documents through `lsp/notify`; LSP payloads are carried as
//! JSON values (the LSP 3.17 specification is their contract), with absolute `file://` URIs.

use crate::{method, notification, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

method!(
    /// The language servers that apply to the workspace (from its providers' `[[lsp]]`
    /// definitions), with their state. Errors: `workspace_not_open`.
    Servers, "lsp/servers", crate::workspace::WorkspaceRef => ServersResult
);
method!(
    /// One LSP request to a server, started if needed (waits up to 60 s for it to be ready).
    /// The result is the server's `result`. Cancelling this request (`$/cancel`) sends
    /// `$/cancelRequest` to the server. Errors: `workspace_not_open`, `not_found` (no such
    /// server), `server_unavailable` (missing, failed or stopped; details `{ "state" }`),
    /// `lsp_error` (the server's error; details `{ "code", "data"? }`), `cancelled`.
    Request, "lsp/request", RequestParams => RequestResult
);
method!(
    /// One LSP notification to a server: document sync (`textDocument/didOpen`, `didChange`,
    /// `didClose`, `didSave`). A `didOpen` for a language a server handles starts it. Errors:
    /// `workspace_not_open`, `not_found`, `server_unavailable`.
    Notify, "lsp/notify", NotifyParams => crate::lifecycle::Empty
);
method!(
    /// Stops and starts a server (also out of `failed`). Errors: `workspace_not_open`,
    /// `not_found`.
    Restart, "lsp/restart", ServerRef => crate::lifecycle::Empty
);
notification!(
    /// Daemon to client. A server notification, relayed to the workspace's clients:
    /// `textDocument/publishDiagnostics`, `$/progress`, `window/showMessage`, `window/logMessage`.
    LspNotification,
    "lsp/notification",
    LspNotificationParams
);
notification!(
    /// Daemon to client. A server's state changed.
    ServerStateChanged,
    "lsp/serverState",
    ServerStateParams
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServersResult {
    pub servers: Vec<ServerInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    /// The definition's id, e.g. `rust-analyzer`.
    pub server_id: String,
    /// The provider whose manifest defines it.
    pub provider_id: String,
    /// Monaco/LSP language ids it serves, e.g. `["rust"]`.
    pub languages: Vec<String>,
    pub state: ServerState,
    /// One sentence for the person: why it failed, what is missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// How to get the server when it is `missing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_hint: Option<String>,
    /// The server's `ServerCapabilities` once `ready`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ServerState {
    /// Defined, not started yet (it starts on first use).
    Idle,
    Starting,
    Ready,
    /// Crashed too often (3 times in 3 minutes) or failed to initialize; `lsp/restart` retries.
    Failed,
    /// The command was not found in the workspace's environment.
    Missing,
    Stopped,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerRef {
    pub workspace_id: WorkspaceId,
    pub server_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestParams {
    pub workspace_id: WorkspaceId,
    pub server_id: String,
    /// The LSP method, e.g. `textDocument/completion`.
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestResult {
    /// The LSP result (may be `null`).
    pub result: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyParams {
    pub workspace_id: WorkspaceId,
    pub server_id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LspNotificationParams {
    pub workspace_id: WorkspaceId,
    pub server_id: String,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerStateParams {
    pub workspace_id: WorkspaceId,
    pub server: ServerInfo,
}
