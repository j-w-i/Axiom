//! JSON-RPC 2.0 envelope and the Axiom error model.
//!
//! Framing (all transports): one JSON value per line, UTF-8, terminated by
//! `\n`, with no raw newline inside a message. Batches are not supported.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Request correlation id. Clients choose ids; they must be unique among that
/// client's in-flight requests on one connection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Request {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Notification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// Exactly one of `result` / `error` is present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Response {
    pub jsonrpc: JsonRpcVersion,
    /// `null` only when the request id could not be read (parse error).
    pub id: Option<RequestId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// Any message that can appear on the wire, in either direction.
///
/// Classification on receipt: an object with `method` and `id` is a request;
/// with `method` and no `id`, a notification; otherwise a response.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
    Notification(Notification),
}

impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let v = Value::deserialize(d)?;
        let obj = v
            .as_object()
            .ok_or_else(|| D::Error::custom("message must be a JSON object"))?;
        let parsed = match (obj.contains_key("method"), obj.contains_key("id")) {
            (true, true) => serde_json::from_value(v).map(Message::Request),
            (true, false) => serde_json::from_value(v).map(Message::Notification),
            (false, _) => {
                if obj.contains_key("result") == obj.contains_key("error") {
                    return Err(D::Error::custom(
                        "response must have exactly one of result/error",
                    ));
                }
                serde_json::from_value(v).map(Message::Response)
            }
        };
        parsed.map_err(D::Error::custom)
    }
}

/// The literal string `"2.0"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
pub enum JsonRpcVersion {
    #[default]
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RpcError {
    /// Numeric code; always equals `data.kind.code()`.
    pub code: i32,
    /// Human-readable, single sentence, safe to show to a user.
    pub message: String,
    pub data: ErrorData,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ErrorData {
    /// Machine-readable category. Clients branch on this, never on `message`.
    pub kind: ErrorKind,
    /// Whether retrying the identical request may succeed.
    #[serde(default)]
    pub retryable: bool,
    /// Kind-specific structured detail (documented per method).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

/// Closed set of error categories. Unknown values received from a newer peer
/// deserialize as [`ErrorKind::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    // JSON-RPC defined.
    ParseError,
    InvalidRequest,
    MethodNotFound,
    InvalidParams,
    Internal,
    // Protocol level.
    NotInitialized,
    Cancelled,
    Timeout,
    Unsupported,
    // Domain level.
    NotFound,
    AlreadyExists,
    PermissionDenied,
    Conflict,
    Io,
    UnsupportedEncoding,
    TooLarge,
    WorkspaceNotOpen,
    SpawnFailed,
    // Domain level, protocol 0.2 (ADR-0014).
    ChecksumMismatch,
    UnsupportedPlatform,
    ApprovalDenied,
    /// Details: `{ "missing": [toolId] }`.
    PrerequisiteMissing,
    // Domain level, protocol 0.3 (ADR-0015).
    /// The gate asked for approval and no connection can give it; the tool did not run.
    ApprovalRequired,
    /// Details: `{ "conflicts": [RestoreConflict] }`.
    CheckpointConflict,
    /// Details: `{ "kind": ModelErrorKind }`.
    ModelError,
    #[serde(other)]
    Unknown,
}

impl ErrorKind {
    pub const fn code(self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::Internal => -32603,
            Self::NotInitialized => -32002,
            Self::Cancelled => -32800,
            Self::Timeout => -32801,
            Self::Unsupported => -32802,
            Self::NotFound => -32010,
            Self::AlreadyExists => -32011,
            Self::PermissionDenied => -32012,
            Self::Conflict => -32013,
            Self::Io => -32014,
            Self::UnsupportedEncoding => -32015,
            Self::TooLarge => -32016,
            Self::WorkspaceNotOpen => -32017,
            Self::SpawnFailed => -32018,
            Self::ChecksumMismatch => -32019,
            Self::UnsupportedPlatform => -32020,
            Self::ApprovalDenied => -32021,
            Self::PrerequisiteMissing => -32022,
            Self::ApprovalRequired => -32023,
            Self::CheckpointConflict => -32024,
            Self::ModelError => -32025,
            Self::Unknown => -32099,
        }
    }
}

impl RpcError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            code: kind.code(),
            message: message.into(),
            data: ErrorData {
                kind,
                retryable: false,
                details: None,
            },
        }
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.data.details = Some(details);
        self
    }

    pub fn retryable(mut self) -> Self {
        self.data.retryable = true;
        self
    }
}
