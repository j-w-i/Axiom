//! The host: sessions, their journal, and the clients that follow them.
//!
//! Rules this crate keeps:
//! - nothing is sent to a client before it is in the journal;
//! - every queue is bounded; a client that cannot keep up is told to resynchronise;
//! - the listener is local, owned by one user, and needs a token (SI-20).

mod agents;
mod catalogue;
mod changes;
pub mod client;
pub mod connections;
mod copies;
mod delegated;
mod editor;
mod faults;
mod files;
mod folders;
mod limits;
mod log;
pub mod mcp;
mod net;
mod outbox;
pub mod paths;
mod peers;
mod plan;
mod planning;
pub mod programs;
mod replay;
mod server;
mod session;
pub mod signal;
mod signin;
mod store;
#[cfg(unix)]
mod terminals;
pub mod vault;
mod waiting;

pub use server::{Config, Host, HostError};
