//! Process supervision: spawn, stream output as ordered chunks, cancel whole
//! process trees, keep a bounded history. No RPC and no workspace knowledge.

mod backend;
mod error;
mod executor;
mod tree;
#[cfg(windows)]
mod tree_windows;
mod utf8;

pub use backend::{ExecBackend, HostBackend, SpawnSpec};
pub use error::ExecError;
pub use executor::{Executor, TaskEvent};
pub use tree::ProcessTree;
