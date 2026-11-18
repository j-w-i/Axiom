//! Connection lifecycle, cancellation and logging.

use crate::envelope::RequestId;
use crate::{method, notification};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

method!(
    /// First request on every connection. Any other request before a
    /// successful `initialize` fails with `not_initialized`.
    Initialize, "initialize", InitializeParams => InitializeResult
);
method!(
    /// Asks the daemon to finish. The daemon cancels running tasks, flushes
    /// state, replies, then exits when the transport closes (or after 2 s).
    Shutdown, "shutdown", Empty => Empty
);
notification!(
    /// Either direction. Requests cancellation of an in-flight request. The
    /// receiver still sends exactly one response: the result if it finished
    /// first, otherwise an error of kind `cancelled`.
    Cancel,
    "$/cancel",
    CancelParams
);
notification!(
    /// Daemon to client. Diagnostic log line; not for end-user display.
    Log,
    "$/log",
    LogParams
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Empty {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// Highest protocol version the client speaks, `MAJOR.MINOR`.
    pub protocol_version: String,
    pub client: PeerInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// Version the daemon will speak on this connection: same MAJOR as the
    /// client, MINOR = min(client, daemon). A MAJOR mismatch is an
    /// `unsupported` error instead.
    pub protocol_version: String,
    pub server: PeerInfo,
    /// Wire names of every method this daemon build serves. Clients must
    /// feature-detect with this list rather than by version number.
    pub methods: Vec<String>,
    pub host: HostInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PeerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HostInfo {
    pub os: HostOs,
    /// Rust target architecture string, e.g. `x86_64`, `aarch64`.
    pub arch: String,
    /// `/` or `\`. For display only; the protocol itself always uses `/`.
    pub path_separator: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum HostOs {
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CancelParams {
    pub id: RequestId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LogParams {
    pub level: LogLevel,
    pub target: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}
