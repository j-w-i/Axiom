//! Provider I/O: a request in one form, sent to whoever serves it, and its answer as one kind of
//! stream (decision 0005).
//!
//! [`codec`] turns a [`Request`] into bytes and a stream into [`Event`]s, [`profile`] says as data how
//! providers differ, [`transport`] is HTTP, and [`Accumulator`] folds events into a [`Reply`]. There is
//! no loop here: nothing is retried, and no tool is run.

pub mod codec;
pub mod error;
mod fold;
pub mod profile;
mod secret;
pub mod sse;
pub mod transport;
mod types;

pub use codec::{EncodeError, Encoded, Loss, Lost};
pub use error::{Class, Error, Retry};
pub use fold::{Accumulator, OutOfOrder, Reply};
pub use profile::{Profile, Quirks};
pub use secret::{Auth, Credential, Endpoint, Origin, Secret, SetupError};
pub use transport::{Answer, CallError, Client, ModelInfo, Options, Renew};
pub use types::*;
