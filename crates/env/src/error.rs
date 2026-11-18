//! Errors of the environment services, mapped to `RpcError` at the daemon boundary.

use axiom_protocol::{ErrorKind, RpcError};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum EnvError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    UnsupportedPlatform(String),
    #[error("{0}")]
    ApprovalDenied(String),
    #[error("The download from {url} does not match its checksum.")]
    ChecksumMismatch {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("{0}")]
    PrerequisiteMissing(String, Vec<String>),
    #[error("{0}")]
    AlreadyExists(String),
    #[error("{0}")]
    InvalidParams(String),
    /// An archive entry would land outside the install root.
    #[error("The archive is unsafe: {0}")]
    UnsafeArchive(String),
    #[error("Download failed: {0}")]
    Network(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Spawn(String),
    #[error("`{command}` failed (exit code {code:?}).")]
    CommandFailed {
        command: String,
        code: Option<i32>,
        output: String,
    },
    #[error("{0}")]
    Unsupported(String),
    #[error("Cancelled.")]
    Cancelled,
}

impl EnvError {
    pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> EnvError {
        let context = context.into();
        move |source| EnvError::Io { context, source }
    }
}

impl From<EnvError> for RpcError {
    fn from(e: EnvError) -> RpcError {
        let msg = e.to_string();
        match e {
            EnvError::NotFound(_) => RpcError::new(ErrorKind::NotFound, msg),
            EnvError::UnsupportedPlatform(_) => RpcError::new(ErrorKind::UnsupportedPlatform, msg),
            EnvError::ApprovalDenied(_) => RpcError::new(ErrorKind::ApprovalDenied, msg),
            EnvError::ChecksumMismatch {
                url,
                expected,
                actual,
            } => RpcError::new(ErrorKind::ChecksumMismatch, msg)
                .with_details(json!({ "url": url, "expected": expected, "actual": actual })),
            EnvError::PrerequisiteMissing(_, missing) => {
                RpcError::new(ErrorKind::PrerequisiteMissing, msg)
                    .with_details(json!({ "missing": missing }))
            }
            EnvError::AlreadyExists(_) => RpcError::new(ErrorKind::AlreadyExists, msg),
            EnvError::InvalidParams(_) => RpcError::new(ErrorKind::InvalidParams, msg),
            EnvError::UnsafeArchive(reason) => RpcError::new(ErrorKind::PermissionDenied, msg)
                .with_details(json!({ "reason": reason })),
            EnvError::Network(_) => RpcError::new(ErrorKind::Io, msg).retryable(),
            EnvError::Io { source, .. } => RpcError::new(ErrorKind::Io, msg)
                .with_details(json!({ "osError": source.to_string() })),
            EnvError::Spawn(_) => RpcError::new(ErrorKind::SpawnFailed, msg),
            EnvError::CommandFailed { code, output, .. } => RpcError::new(ErrorKind::Io, msg)
                .with_details(json!({ "exitCode": code, "output": output })),
            EnvError::Unsupported(_) => RpcError::new(ErrorKind::Unsupported, msg),
            EnvError::Cancelled => RpcError::new(ErrorKind::Cancelled, msg),
        }
    }
}
