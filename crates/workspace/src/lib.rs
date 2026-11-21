//! Workspace registry and safe file access (`workspace/open|close`, `fs/list`,
//! `fs/readFile`, `fs/writeFile`), and text search and file removal for the agent's tools
//! (M3). No persistence or RPC here.

mod error;
mod ignore_rules;
mod registry;
mod search;
mod watch;
mod workspace;

pub use error::WsError;
pub use registry::Workspaces;
pub use search::{SearchMatch, SearchQuery, SearchResult};
pub use watch::WatchHandle;
pub use workspace::Workspace;
