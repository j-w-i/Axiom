//! Domain-free serving of the Axiom protocol over any async byte stream.
//!
//! - [`serve`] runs one connection: NDJSON codec, `initialize` gate,
//!   concurrent dispatch to handlers registered on a [`Router`], `$/cancel`,
//!   `shutdown`.
//! - [`Client`] is a typed client over the same framing (tests, CLI).
//!
//! This crate knows nothing about workspaces, files or tasks.

mod client;
mod codec;
mod router;
mod server;

pub use client::Client;
pub use router::{Closed, Notifier, RequestCtx, Router, ServerInfo};
pub use server::{serve, serve_with, ServeError, ServeOptions};
