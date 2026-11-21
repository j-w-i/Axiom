//! Agents: model providers, sessions, the tool registry, the permission gate and checkpoints
//! (protocol 0.3, milestone M3; decisions in ADR-0015). Normative prose:
//! `docs/protocols/agents.md` and `docs/protocols/ipc.md` §Protocol 0.3.
//!
//! A session belongs to a workspace (by its root) and is an append-only journal of
//! [`AgentEvent`]s. `agent/sessionOpen` attaches the calling connection: it receives the events
//! so far in the response and every later one as `agent/event`. `agent/send` starts a turn and
//! responds as soon as the turn is accepted; the turn ends with a `turnEnd` event.
//!
//! Every tool call, from a session or from `agent/toolCall`, passes the permission gate. An
//! `ask` decision is sent as `agent/approvalRequested` to every connection that declared
//! `capabilities.approvals` in `initialize`, and waits for `agent/approve`. With no such
//! connection the call fails with `approval_required` (the tool does not run).

use crate::lifecycle::Empty;
use crate::workspace::WorkspaceRef;
use crate::{method, notification, WorkspaceId, WorkspacePath};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

method!(
    /// Configured model providers, each with its models (listed from the server where the
    /// provider supports it, else the configured list) and capability profiles. A provider
    /// that cannot be reached is listed with `error`.
    Models, "agent/models", ModelsParams => ModelsResult
);
method!(
    /// Adds, replaces or removes a provider configuration. An `apiKey` goes to the OS
    /// credential store, never to settings. Errors: `invalid_params` (bad URL: only HTTPS, or
    /// HTTP to a loopback host), `unsupported` (no credential store: use `apiKeyEnv`).
    ConfigureProvider, "agent/configureProvider", ConfigureProviderParams => Empty
);
method!(
    /// A new session for the workspace. Errors: `workspace_not_open`, `not_found` (provider).
    SessionCreate, "agent/sessionCreate", SessionCreateParams => SessionInfo
);
method!(
    /// The workspace's sessions (or, without `workspaceId`, the conversations that have no
    /// workspace), most recently updated first. Errors: `workspace_not_open`.
    SessionList, "agent/sessionList", SessionListParams => SessionListResult
);
method!(
    /// Renames a session. Errors: `not_found`, `invalid_params` (empty title).
    SessionRename, "agent/sessionRename", SessionRenameParams => SessionInfo
);
method!(
    /// Deletes a session's journal and index row (its checkpoints stay with the workspace's).
    /// A running turn is cancelled first. Errors: `not_found`.
    SessionDelete, "agent/sessionDelete", SessionRef => Empty
);
method!(
    /// An image attached to a session (by its `sha256` from a `user` event).
    /// Errors: `not_found`.
    Image, "agent/image", ImageParams => ImageData
);
method!(
    /// Attaches this connection to a session and returns its persisted events (the replay).
    /// Errors: `workspace_not_open`, `not_found`.
    SessionOpen, "agent/sessionOpen", SessionRef => SessionOpenResult
);
method!(
    /// Appends a user message and starts a turn; responds when the turn is accepted. Errors:
    /// `not_found`, `conflict` (a turn is already running), `prerequisite_missing`.
    Send, "agent/send", SendParams => Empty
);
method!(
    /// Stops the running turn: the model stream and any running tool. No-op when idle.
    Cancel, "agent/cancel", SessionRef => Empty
);
method!(
    /// Answers an approval request. The first answer wins; later ones are ignored. Errors:
    /// `not_found` (unknown or already answered request).
    Approve, "agent/approve", ApproveParams => Empty
);
method!(
    /// The session's checkpoints, oldest first.
    Checkpoints, "agent/checkpoints", SessionRef => CheckpointsResult
);
method!(
    /// Restores the workspace files the agent changed since a checkpoint. A file changed by
    /// anyone else since the agent last wrote it is a conflict: with any conflict and
    /// `skipConflicts` false nothing is restored and the error is `checkpoint_conflict`
    /// (details `{ "conflicts": [RestoreConflict] }`). A checkpoint is taken first, so a restore
    /// can itself be undone. Errors: `not_found`, `prerequisite_missing` (`git`).
    Restore, "agent/restore", RestoreParams => RestoreResult
);
method!(
    /// The tool registry for a workspace: names, descriptions, input schemas and effects.
    Tools, "agent/tools", WorkspaceRef => ToolsResult
);
method!(
    /// Calls one tool outside a session, as `principal` (the MCP front end, the CLI). The same
    /// gate applies. Errors: `approval_denied`, `approval_required`, `invalid_params`.
    ToolCall, "agent/toolCall", ToolCallParams => ToolCallResult
);
method!(
    /// The user policy (settings `agent.policy`) and, for `workspaceId`, the workspace's
    /// stricter-only rules (`.axiom/project.toml` `[agent.policy]`).
    GetPolicy, "agent/policy", PolicyParams => PolicyResult
);
method!(
    /// Replaces the user policy. Errors: `invalid_params`.
    SetPolicy, "agent/setPolicy", Policy => Empty
);

notification!(
    /// Daemon to client. One session event, to every connection attached to the session.
    /// `textDelta` events are never persisted or replayed; the `assistantText` that follows
    /// carries the whole text.
    Event,
    "agent/event",
    AgentEvent
);
notification!(
    /// Daemon to client. An approval is pending; only to connections that can approve.
    ApprovalRequested,
    "agent/approvalRequested",
    ApprovalRequest
);
notification!(
    /// Daemon to client. A pending approval was answered (by any connection) or withdrawn
    /// (turn cancelled, connection gone): clients drop their prompt for it.
    ApprovalResolved,
    "agent/approvalResolved",
    ApprovalResolution
);
notification!(
    /// Daemon to client. The `editor.open` tool asks attached clients to reveal a location.
    Reveal,
    "agent/reveal",
    RevealParams
);

/// Opaque session identifier.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct SessionId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    /// Anthropic Messages API.
    Anthropic,
    /// OpenAI Chat Completions, and the servers compatible with it (Ollama, llama.cpp,
    /// LM Studio, vLLM).
    OpenaiChat,
    /// TEST ONLY: replays a script; offered only when `AXIOM_TEST_SCRIPTED_MODEL` is set.
    Scripted,
    /// An external agent spoken to over the Agent Client Protocol (Claude Code, Codex, Gemini
    /// CLI; ADR-0016). Its one model is `default`.
    Acp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Stable id chosen by the user, `[a-z0-9-]+`.
    pub id: String,
    pub kind: ProviderKind,
    /// Display name.
    pub name: String,
    /// API base URL; default per kind. HTTPS, or HTTP to a loopback host.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Read the API key from this environment variable instead of the credential store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    /// Models to offer when the server cannot list them (or to restrict the list).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<String>,
    /// Overrides of the detected capability profile, applied to every model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<CapabilityOverrides>,
    /// `acp` only: the agent's program and arguments (ADR-0016).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command: Vec<String>,
    /// `acp` only: extra environment variables for the agent process (no secrets: agents use
    /// their own sign-in).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Support {
    Yes,
    No,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub tools: Support,
    pub streaming_tool_args: Support,
    pub parallel_tool_calls: Support,
    pub images: Support,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Support>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming_tool_args: Option<Support>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<Support>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<Support>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelsParams {
    /// Only this provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelsResult {
    pub providers: Vec<ProviderStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub config: ProviderConfig,
    /// A key is stored in the credential store (or `apiKeyEnv` is set in the daemon's
    /// environment). The key itself is never sent.
    pub has_key: bool,
    pub models: Vec<ModelInfo>,
    /// Why the models could not be listed (one sentence).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub display_name: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureProviderParams {
    pub id: String,
    /// The new configuration (its `id` must equal `id`); absent removes the provider and its
    /// stored key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ProviderConfig>,
    /// Stores this key in the credential store.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Deletes the stored key.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_key: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionCreateParams {
    /// Absent: a conversation without a workspace (no workspace tools; ADR-0017).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
    pub provider_id: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionRef {
    /// Absent for a conversation without a workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: SessionId,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListParams {
    /// Absent: the conversations without a workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionRenameParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: SessionId,
    /// 1–120 characters.
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: SessionId,
    pub sha256: String,
}

/// An image, base64-encoded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageData {
    /// `image/png`, `image/jpeg`, `image/gif` or `image/webp`.
    pub media_type: String,
    /// Base64 (standard alphabet, padded). At most 5 MiB decoded.
    pub data: String,
}

/// An image stored with a session (shown with its `user` event; fetched with `agent/image`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImageRef {
    pub media_type: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: SessionId,
    pub title: String,
    pub provider_id: String,
    pub model: String,
    /// Milliseconds since the Unix epoch.
    pub created_ms: u64,
    pub updated_ms: u64,
    /// A turn is running.
    pub running: bool,
    /// Approvals waiting for an answer.
    pub pending_approvals: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResult {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionOpenResult {
    pub session: SessionInfo,
    /// Every persisted event, in `seq` order.
    pub events: Vec<AgentEvent>,
    /// Approvals of this session still waiting (also re-sent as `agent/approvalRequested`
    /// to an approving connection).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending: Vec<ApprovalRequest>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: SessionId,
    pub text: String,
    /// Context the user attached (an editor selection, a file).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    /// Pasted or dropped images (ADR-0017 §3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageData>,
    /// Edit / regenerate: this message replaces turn N and every later turn in the context
    /// (a `rewind` event is journalled first). Errors: `invalid_params` if N is not a turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_turn: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub path: WorkspacePath,
    /// 1-based inclusive line range; absent means the whole file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<(u32, u32)>,
}

/// One journal event. `seq` increases by one per persisted event within a session;
/// a `textDelta` carries the `seq` of the event it will become part of and is never persisted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub session_id: SessionId,
    pub seq: u64,
    pub ts_ms: u64,
    /// 1-based turn number (a turn starts with a `user` event).
    pub turn: u32,
    pub event: EventBody,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EventBody {
    User {
        text: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<Attachment>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        images: Vec<ImageRef>,
    },
    /// Turns from `fromTurn` on are replaced by what follows (edit, regenerate). Clients show
    /// the latest branch; the journal keeps the earlier one.
    #[serde(rename_all = "camelCase")]
    Rewind {
        from_turn: u32,
    },
    /// Streamed assistant text (ephemeral).
    TextDelta {
        text: String,
    },
    /// A complete assistant text block.
    AssistantText {
        text: String,
        /// For an external agent's plan or reasoning (ADR-0016); absent for the answer.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<TextKind>,
    },
    /// The model called a tool (written before the tool runs).
    #[serde(rename_all = "camelCase")]
    ToolCall {
        call_id: String,
        tool: String,
        input: Value,
        effect: Effect,
        /// Resolved targets, for display: workspace paths, or the argv joined.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        targets: Vec<String>,
    },
    /// The tool's outcome (written after it ran, or after it was refused).
    #[serde(rename_all = "camelCase")]
    ToolResult {
        call_id: String,
        is_error: bool,
        /// One line for the tool row ("42 lines", "exit 0 · 3.1 s", "denied by policy").
        summary: String,
        /// The structured result (the tool's output type), or `{kind, message, hint}`.
        output: Value,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        truncated: bool,
        /// The policy rule that decided, for display.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
    },
    /// A tool call whose result was never written (the daemon stopped while it ran). It is not
    /// re-run.
    #[serde(rename_all = "camelCase")]
    Interrupted {
        call_id: String,
    },
    /// An approval was requested for a call in this session.
    Approval {
        request: ApprovalRequest,
    },
    /// …and answered (or withdrawn).
    ApprovalResolved {
        resolution: ApprovalResolution,
    },
    Checkpoint {
        checkpoint: CheckpointInfo,
    },
    Restored {
        result: RestoreResult,
    },
    /// Earlier context was summarised; the journal keeps the original events.
    #[serde(rename_all = "camelCase")]
    Compaction {
        /// Events up to and including this `seq` are replaced by the summary in the context.
        through_seq: u64,
        summary: String,
    },
    #[serde(rename_all = "camelCase")]
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cached_input_tokens: Option<u32>,
    },
    TurnEnd {
        reason: TurnEndReason,
    },
    /// A turn-level error (model unreachable, authentication…). `kind` is an `ErrorKind` wire
    /// name; for `model_error`, `modelKind` gives the provider failure.
    #[serde(rename_all = "camelCase")]
    Error {
        kind: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model_kind: Option<ModelErrorKind>,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TextKind {
    Plan,
    Thought,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TurnEndReason {
    /// The model finished.
    EndTurn,
    MaxTokens,
    Refusal,
    Cancelled,
    /// The per-turn tool round limit was reached.
    ToolLimit,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ModelErrorKind {
    Auth,
    RateLimited,
    ContextTooLong,
    Unavailable,
    InvalidRequest,
    Transport,
}

/// What a tool can do. Ordered from least to most consequential for display only; the gate
/// decides by rule, not by order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    Read,
    WriteWorkspace,
    Execute,
    Install,
    Network,
    WriteOutside,
    Delete,
    Credentials,
    Deploy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub request_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Who asked: `session:<id>`, `mcp:<client name>`, `cli`.
    pub principal: String,
    pub tool: String,
    pub effect: Effect,
    /// Resolved targets: workspace paths, or the argv joined.
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    /// For edits: the unified diff that would be applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// The rule that decided `ask` (display), or absent for the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    /// A checkpoint will be taken before the tool runs.
    pub checkpoint_planned: bool,
    /// "Allow for session" is offered (never for `credentials` or `deploy`).
    pub can_remember: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalDecision {
    Once,
    Session,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApproveParams {
    pub request_id: String,
    pub decision: ApprovalDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResolution {
    pub request_id: String,
    /// Absent when withdrawn (cancelled, or the asking connection went away).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<ApprovalDecision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointInfo {
    /// 1-based within the session.
    pub number: u32,
    /// Why it was taken ("before fs.applyPatch", "before restore to 2").
    pub label: String,
    pub created_ms: u64,
    pub turn: u32,
    /// Files too large to capture (over 16 MiB).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<WorkspacePath>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointsResult {
    pub checkpoints: Vec<CheckpointInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreParams {
    /// Required in practice (checkpoints exist only for workspace sessions); optional on the
    /// wire with the other session references (ADR-0017 §2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
    pub session_id: SessionId,
    pub checkpoint: u32,
    /// Restore the files without conflicts and leave the others.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip_conflicts: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub checkpoint: u32,
    /// The checkpoint taken just before restoring.
    pub undo_checkpoint: u32,
    pub restored: Vec<WorkspacePath>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<RestoreConflict>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreConflict {
    pub path: WorkspacePath,
    pub reason: ConflictReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ConflictReason {
    /// Its content differs from what the agent last left.
    ModifiedSinceAgent,
    /// It was deleted after the agent last wrote it.
    DeletedSinceAgent,
    /// It was created by someone else after the checkpoint.
    CreatedSinceAgent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolsResult {
    pub tools: Vec<ToolInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    /// Dotted name, e.g. `fs.read`.
    pub name: String,
    pub description: String,
    /// JSON Schema of the input object.
    pub input_schema: Value,
    pub effect: Effect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallParams {
    pub workspace_id: WorkspaceId,
    pub tool: String,
    pub input: Value,
    /// Recorded in the audit log and shown in approvals, e.g. `mcp:claude-code`.
    pub principal: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallResult {
    /// The structured result, or `{kind, message, hint}` when `isError`.
    pub output: Value,
    /// The same result rendered as text for a model.
    pub text: String,
    pub is_error: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyResult {
    pub user: Policy,
    /// The workspace's rules (they can only make `user` stricter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Policy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    /// Decision when no rule matches.
    pub default: Decision,
    #[serde(default)]
    pub rules: Vec<PolicyRule>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Allow,
    Ask,
    Deny,
}

/// A rule matches a call when every field it sets matches. Rules with a `tool`, `path` or
/// `argvPrefix` are more specific than rules without; among matching rules of equal
/// specificity, deny > ask > allow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<Effect>,
    /// Exact tool name, or a prefix ending in `.*` (`mcp.github.*`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Workspace-relative glob (`src/**`); matches when every target path matches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Matches an `Execute` call whose argv starts with these elements.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv_prefix: Option<Vec<String>>,
    pub decision: Decision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RevealParams {
    pub workspace_id: WorkspaceId,
    pub path: WorkspacePath,
    /// 1-based.
    pub line: u32,
    /// 1-based.
    #[serde(default = "one")]
    pub column: u32,
}

fn one() -> u32 {
    1
}
