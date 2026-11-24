//! The host: sessions, their journal, and the clients that follow them.
//!
//! Rules this crate keeps:
//! - nothing is sent to a client before it is in the journal;
//! - every queue is bounded; a client that cannot keep up is told to resynchronise;
//! - the listener is local, owned by one user, and needs a token (SI-20).

pub mod client;
mod net;
mod outbox;
pub mod paths;
mod replay;
mod server;
mod session;
mod store;

pub use server::{Config, Host, HostError};
