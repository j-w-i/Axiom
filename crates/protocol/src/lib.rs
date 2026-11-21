//! Axiom wire protocol, version 0.4.
//!
//! This crate is the single source of truth for the contract between the
//! daemon (`axiomd`) and every client (desktop shell, CLI, agents, tests).
//! It contains types only: no I/O, no async, no platform code.
//!
//! Normative prose lives in `docs/protocols/ipc.md`. Where the two disagree,
//! this crate wins and the document must be fixed.
//!
//! Layout:
//! - [`envelope`]: JSON-RPC 2.0 framing types and the error model.
//! - [`method`]: the [`Method`]/[`Notification`] traits binding names to types.
//! - one module per service domain: [`lifecycle`], [`workspace`], [`fs`],
//!   [`task`], [`settings`]. These make up protocol 0.1 (milestone M1).
//! - [`env`] and [`build`]: the protocol 0.2 additions (milestone M2, ADR-0014).
//! - [`agent`]: the protocol 0.3 additions (milestone M3, ADR-0015).
//! - [`lsp`], [`terminal`], [`git`] and `workspace/search`: the protocol 0.4 additions
//!   (milestone M4, ADR-0018).
//! - DRAFT contracts for later milestones, compiled and schema-exported but
//!   not served yet: [`inspection`]. [`diagnostics`] and [`provider`] were drafts and are
//!   frozen for M2 (used by [`env`] and [`build`]).
//!
//! Change rules (see `docs/architecture/governance.md`):
//! - Adding an optional field, a method, a notification, or an enum variant
//!   on a `#[non_exhaustive]`-style open enum is a MINOR change.
//! - Removing or renaming anything, or changing a type, is a MAJOR change and
//!   needs an ADR.

pub mod agent;
pub mod build;
pub mod diagnostics;
pub mod env;
pub mod envelope;
pub mod fs;
pub mod git;
pub mod inspection;
pub mod lifecycle;
pub mod lsp;
pub mod method;
pub mod provider;
pub mod registry;
pub mod settings;
pub mod task;
pub mod terminal;
pub mod workspace;

pub use envelope::{
    ErrorKind, Message, Notification as NotificationMessage, Request, RequestId, Response, RpcError,
};
pub use method::{Method, Notification};

/// Protocol version implemented by this crate. `MAJOR.MINOR`.
pub const PROTOCOL_VERSION: &str = "0.4";

/// A path inside a workspace: relative to the workspace root, `/`-separated,
/// UTF-8, with no `.`/`..` segments, no leading `/`, and no trailing `/`.
/// The workspace root itself is the empty string.
///
/// Clients never send absolute host paths except in `workspace/open`. The
/// daemon rejects any `WorkspacePath` that fails these rules with
/// [`ErrorKind::InvalidParams`] and any that resolves (through symlinks)
/// outside the root with [`ErrorKind::PermissionDenied`].
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct WorkspacePath(pub String);

impl WorkspacePath {
    /// The workspace root.
    pub fn root() -> Self {
        Self(String::new())
    }

    /// Checks the lexical rules above. Symlink containment is the daemon's job.
    pub fn is_lexically_valid(&self) -> bool {
        let s = self.0.as_str();
        if s.is_empty() {
            return true;
        }
        if s.starts_with('/') || s.ends_with('/') || s.contains('\\') || s.contains('\0') {
            return false;
        }
        s.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
    }
}

/// Opaque identifier of an open workspace, minted by `workspace/open`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct WorkspaceId(pub String);

/// Opaque identifier of a running or finished task, minted by `task/start`.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(transparent)]
pub struct TaskId(pub String);
