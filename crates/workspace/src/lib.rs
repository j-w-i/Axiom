//! Workspace registry and safe file access (`workspace/open|close`, `fs/list`,
//! `fs/readFile`, `fs/writeFile`). No watching, persistence or RPC here.

mod error;
mod ignore_rules;
mod registry;
mod workspace;

pub use error::WsError;
pub use registry::Workspaces;
pub use workspace::Workspace;
