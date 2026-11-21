//! Integrated terminals (protocol 0.4, milestone M4; ADR-0018 §4). A terminal is the person's
//! own shell in a PTY run by the daemon in the workspace root, with the project's environment.
//! It is not an agent tool and is not gated. Terminals belong to the workspace: they survive a
//! client reconnecting (output since then is replayed from a bounded backlog), not a daemon
//! restart.

use crate::{method, notification, WorkspaceId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Opaque identifier of a terminal, minted by `terminal/create`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TerminalId(pub String);

method!(
    /// Starts a shell (or `argv`) in a new PTY of `cols`×`rows`. Errors: `workspace_not_open`,
    /// `spawn_failed`, `invalid_params` (zero size).
    Create, "terminal/create", CreateParams => TerminalInfo
);
method!(
    /// Input to the terminal, as the keyboard produced it (UTF-8). Errors: `not_found`.
    Write, "terminal/write", WriteParams => crate::lifecycle::Empty
);
method!(
    /// The terminal's new size. Errors: `not_found`, `invalid_params`.
    Resize, "terminal/resize", ResizeParams => crate::lifecycle::Empty
);
method!(
    /// Ends the terminal (hang-up to its process group, then kill after 2 s). Idempotent for an
    /// exited terminal. Errors: `not_found`.
    Close, "terminal/close", TerminalRef => crate::lifecycle::Empty
);
method!(
    /// The workspace's terminals, oldest first, each with its backlog (the output kept so a
    /// reconnecting client can redraw it; at most 1 MiB per terminal). Errors:
    /// `workspace_not_open`.
    List, "terminal/list", crate::workspace::WorkspaceRef => ListResult
);
notification!(
    /// Daemon to client. Output, coalesced (at most one per 16 ms per terminal); never splits a
    /// UTF-8 character.
    Output,
    "terminal/output",
    OutputParams
);
notification!(
    /// Daemon to client. The terminal's process ended.
    Exited,
    "terminal/exited",
    ExitedParams
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateParams {
    pub workspace_id: WorkspaceId,
    /// The program and its arguments; default: the person's login shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv: Option<Vec<String>>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub terminal_id: TerminalId,
    pub workspace_id: WorkspaceId,
    /// The program's name, e.g. `zsh`.
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRef {
    pub terminal_id: TerminalId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WriteParams {
    pub terminal_id: TerminalId,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResizeParams {
    pub terminal_id: TerminalId,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListResult {
    pub terminals: Vec<ListedTerminal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListedTerminal {
    #[serde(flatten)]
    pub info: TerminalInfo,
    /// The kept output; `seq` is the sequence number of the last output it includes.
    pub backlog: String,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OutputParams {
    pub terminal_id: TerminalId,
    /// Increases by one per notification of this terminal.
    pub seq: u64,
    pub data: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExitedParams {
    pub terminal_id: TerminalId,
    /// Absent when the process was ended by a signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
}
