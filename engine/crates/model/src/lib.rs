//! Provider I/O: a request in one form, sent to whoever serves it, and its answer as one kind of
//! stream (decision 0005).
//!
//! [`codec`] turns a [`Request`] into bytes and a stream into [`Event`]s, [`profile`] says as data how
//! providers differ, [`transport`] is HTTP, and [`Accumulator`] folds events into a [`Reply`]. There is
//! no loop here: nothing is retried, and no tool is run.

pub mod error;
mod secret;
pub mod sse;

pub use error::{Class, Error, Retry};
pub use secret::{Auth, Credential, Endpoint, Origin, Secret, SetupError};
