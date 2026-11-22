use axiom_protocol::fs::FileVersion;
use axiom_protocol::{ErrorKind, RpcError};
use serde_json::json;

/// Workspace-level failure. String payloads are workspace-relative paths (or
/// ids), never absolute host paths.
#[derive(thiserror::Error, Debug)]
pub enum WsError {
    #[error("`{0}` was not found")]
    NotFound(String),
    #[error("{0}")]
    InvalidPath(String),
    #[error("`{0}` is not a directory")]
    NotADirectory(String),
    #[error("`{0}` is a directory")]
    IsADirectory(String),
    #[error("`{0}` resolves outside the workspace")]
    Escapes(String),
    #[error("permission denied for `{0}`")]
    PermissionDenied(String),
    #[error("workspace `{0}` is not open")]
    WorkspaceNotOpen(String),
    #[error("the file changed on disk")]
    Conflict { current: Option<FileVersion> },
    #[error("`{0}` is not valid UTF-8")]
    UnsupportedEncoding(String),
    #[error("`{0}` already exists")]
    AlreadyExists(String),
    #[error("`{0}` is not a regular file")]
    NotAFile(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("file is too large ({bytes} bytes)")]
    TooLarge { bytes: u64 },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl WsError {
    /// Maps an OS error for the path `rel` to the most specific variant.
    pub(crate) fn from_io(e: std::io::Error, rel: &str) -> WsError {
        match e.kind() {
            std::io::ErrorKind::NotFound => WsError::NotFound(display_path(rel)),
            std::io::ErrorKind::PermissionDenied => WsError::PermissionDenied(display_path(rel)),
            _ => WsError::Io(e),
        }
    }
}

pub(crate) fn display_path(rel: &str) -> String {
    if rel.is_empty() {
        "the workspace root".to_string()
    } else {
        rel.to_string()
    }
}

impl From<WsError> for RpcError {
    fn from(e: WsError) -> RpcError {
        let sentence = |s: String| {
            if s.ends_with('.') {
                s
            } else {
                format!("{s}.")
            }
        };
        match e {
            WsError::NotFound(p) => RpcError::new(
                ErrorKind::NotFound,
                sentence(format!("`{p}` was not found")),
            ),
            WsError::InvalidPath(m) => RpcError::new(ErrorKind::InvalidParams, sentence(m)),
            WsError::NotADirectory(p) => RpcError::new(
                ErrorKind::InvalidParams,
                sentence(format!("`{p}` is not a directory")),
            ),
            WsError::IsADirectory(p) => RpcError::new(
                ErrorKind::InvalidParams,
                sentence(format!("`{p}` is a directory")),
            ),
            WsError::Escapes(p) => RpcError::new(
                ErrorKind::PermissionDenied,
                sentence(format!("`{p}` resolves outside the workspace")),
            ),
            WsError::PermissionDenied(p) => RpcError::new(
                ErrorKind::PermissionDenied,
                sentence(format!("Permission denied for `{p}`")),
            ),
            WsError::WorkspaceNotOpen(id) => RpcError::new(
                ErrorKind::WorkspaceNotOpen,
                sentence(format!("Workspace `{id}` is not open")),
            ),
            WsError::Conflict { current } => {
                RpcError::new(ErrorKind::Conflict, "The file changed on disk.")
                    .with_details(json!({ "currentVersion": current.map(|v| v.0) }))
            }
            WsError::UnsupportedEncoding(p) => RpcError::new(
                ErrorKind::UnsupportedEncoding,
                sentence(format!("`{p}` is not valid UTF-8")),
            ),
            WsError::AlreadyExists(p) => RpcError::new(
                ErrorKind::AlreadyExists,
                sentence(format!("`{p}` already exists")),
            ),
            WsError::NotAFile(p) => RpcError::new(
                ErrorKind::InvalidParams,
                sentence(format!("`{p}` is not a regular file")),
            ),
            WsError::Unsupported(m) => RpcError::new(ErrorKind::Unsupported, sentence(m)),
            WsError::TooLarge { bytes } => RpcError::new(
                ErrorKind::TooLarge,
                sentence(format!(
                    "The file is too large ({bytes} bytes; the limit is {})",
                    axiom_protocol::fs::MAX_TEXT_FILE_BYTES
                )),
            ),
            WsError::Io(err) => {
                let details = json!({ "osError": err.to_string() });
                match err.kind() {
                    std::io::ErrorKind::PermissionDenied => {
                        RpcError::new(ErrorKind::PermissionDenied, "Permission denied.")
                    }
                    std::io::ErrorKind::NotFound => {
                        RpcError::new(ErrorKind::NotFound, "The file or directory was not found.")
                    }
                    _ => RpcError::new(ErrorKind::Io, "A file system operation failed."),
                }
                .with_details(details)
            }
        }
    }
}
