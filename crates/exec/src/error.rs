use axiom_protocol::{ErrorKind, RpcError};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum ExecError {
    #[error("could not start `{program}`: {os_error}")]
    SpawnFailed { program: String, os_error: String },
    #[error("the command is empty")]
    EmptyCommand,
    #[error("the working directory does not exist")]
    CwdNotFound,
    #[error("no such task")]
    NotFound,
}

impl From<ExecError> for RpcError {
    fn from(e: ExecError) -> RpcError {
        match e {
            ExecError::SpawnFailed { program, os_error } => RpcError::new(
                ErrorKind::SpawnFailed,
                format!("Could not start `{program}`."),
            )
            .with_details(json!({ "program": program, "osError": os_error })),
            ExecError::EmptyCommand => {
                RpcError::new(ErrorKind::InvalidParams, "The command is empty.")
            }
            ExecError::CwdNotFound => {
                RpcError::new(ErrorKind::NotFound, "The working directory was not found.")
            }
            ExecError::NotFound => RpcError::new(ErrorKind::NotFound, "The task was not found."),
        }
    }
}
