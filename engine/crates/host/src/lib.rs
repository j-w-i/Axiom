//! The host: sessions, their journal, and the clients that follow them.
//!
//! Rules this crate keeps:
//! - nothing is sent to a client before it is in the journal;
//! - every queue is bounded; a client that cannot keep up is told to resynchronise;
//! - the listener is local, owned by one user, and needs a token (SI-20).

mod agents;
pub mod client;
pub mod connections;
mod faults;
mod log;
mod net;
mod outbox;
pub mod paths;
mod replay;
mod server;
mod session;
pub mod signal;
mod store;

pub use server::{Config, Host, HostError};
