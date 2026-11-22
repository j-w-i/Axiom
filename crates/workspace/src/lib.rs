//! Workspace registry and safe file access (`workspace/open|close`, `fs/list`,
//! `fs/readFile`, `fs/writeFile`), and text search and file removal for the agent's tools
//! (M3); creating, renaming and trashing entries (protocol 0.6). No persistence or RPC here.

mod error;
mod fileops;
mod ignore_rules;
mod registry;
mod search;
mod trash;
mod watch;
mod workspace;

pub mod git;
pub use error::WsError;
pub use registry::Workspaces;
pub use search::{
    FindMatch, FindQuery, FindResult, SearchMatch, SearchQuery, SearchResult, FIND_DEFAULT_LIMIT,
    FIND_MAX_LIMIT,
};
pub use watch::WatchHandle;
pub use workspace::{read_regular_file, Workspace};
