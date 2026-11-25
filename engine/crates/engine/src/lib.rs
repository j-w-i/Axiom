//! An agent at work: what it is told goes to a model, what the model asks for is judged, done and
//! told back, until it has no more to ask. Everything is on record before it is done; nothing is
//! done twice (decision 0004).

use std::future::Future;

use axiom_policy::rules::Decision;
use axiom_proto::{AgentId, Body, ItemId, Outcome, RequestId};
use serde_json::Value;

mod compact;
pub mod context;
pub mod desk;
mod run;

pub use context::{Context, Turn};
pub use desk::Desk;
pub use run::{Agent, Ended, Limits, Retries, Word, said};

/// What is said to the session.
#[derive(Debug, Clone, PartialEq)]
pub enum Say {
    Event(Body),
    /// More of an item's text.
    Text(ItemId, String),
    /// An item ends, with the text it has.
    End(ItemId, Outcome, Option<Value>),
}

/// Why the session took nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    /// There is nobody to say it to: the run ends without another word.
    #[error("the session is closed")]
    Closed,
    /// Only this was refused, in words.
    #[error("{0}")]
    This(String),
}

/// What was done with authority, for the audit record.
#[derive(Debug, Clone)]
pub struct Deed {
    pub chain: Vec<AgentId>,
    pub workspace: String,
    pub tool: String,
    pub subject: String,
    pub decision: Decision,
    pub rules: Vec<String>,
    pub answered: Option<(RequestId, String)>,
}

/// What the host gives a run: its session's journal.
pub trait Stage: Send + Sync + 'static {
    /// All of it is taken, or none.
    fn say(&self, all: Vec<Say>) -> impl Future<Output = Result<(), Refused>> + Send;
    /// Resolves once everything said before is in the journal.
    fn sync(&self) -> impl Future<Output = Result<(), Refused>> + Send;
    /// Resolves once the turn is on the drive.
    fn remember(&self, n: u64, turn: String) -> impl Future<Output = Result<(), Refused>> + Send;
    /// Resolves once the deed is in the audit record, on the drive (SI-23).
    fn audit(&self, deed: Deed) -> impl Future<Output = Result<(), Refused>> + Send;
}
