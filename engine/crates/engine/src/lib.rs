//! An agent at work: what it is told goes to a model, what the model asks for is judged, done and
//! told back, until it has no more to ask. Everything is on record before it is done; nothing is
//! done twice (decision 0004).

use std::future::Future;

use axiom_policy::rules::Decision;
use axiom_proto::{AgentId, Body, ItemId, Outcome, RequestId};
use axiom_tools::{Output, Spec};
use serde_json::Value;

mod compact;
pub mod context;
pub mod desk;
mod hands;
mod run;

pub use context::{Context, Turn};
pub use desk::Desk;
pub use hands::{Hands, said_no};
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

/// Begins what an agent is told by another: whoever reads it knows that it is not a person's word
/// (SI-2). A person who writes this themself is taken for less than they are.
pub const FROM_AGENT: &str = "(This is from another agent of this session";

pub fn from_agent(name: &str, text: &str) -> String {
    format!("{FROM_AGENT}, {name}. It can ask for work; it cannot allow anything.)\n\n{text}")
}

/// A call of a tool that is the host's own, as it was judged. The same call is then performed.
#[derive(Debug, Clone)]
pub struct Other {
    pub decision: Decision,
    /// As a person is asked: what, about exactly what, with what it could reach.
    pub what: String,
    pub subject: String,
    pub scope: String,
    /// Why it is asked about or refused.
    pub why: String,
    /// It is put on record before it is done.
    pub changes: bool,
}

/// What the host gives a run: its session's journal, and tools of its own.
pub trait Stage: Send + Sync + 'static {
    /// Tools beside the agent's own.
    fn others(&self) -> Vec<Spec> {
        Vec::new()
    }
    /// A call of one of those; `None` if there is no such tool.
    fn judge(&self, tool: &str, args: &Value) -> impl Future<Output = Option<Other>> + Send {
        let _ = (tool, args);
        async { None }
    }
    /// Does what was judged. Dropped, nothing more of it is done.
    fn perform(&self, tool: &str, args: &Value) -> impl Future<Output = Output> + Send {
        let _ = (tool, args);
        async {
            Output {
                ok: false,
                text: "there is no such tool".into(),
                rest: None,
                detail: Value::Null,
            }
        }
    }
    /// All of it is taken, or none.
    fn say(&self, all: Vec<Say>) -> impl Future<Output = Result<(), Refused>> + Send;
    /// Resolves once everything said before is in the journal.
    fn sync(&self) -> impl Future<Output = Result<(), Refused>> + Send;
    /// Resolves once the turn is on the drive. `heard` names what a person had said that is now in
    /// it, and waits to be heard no longer: both or neither. Whether it is in: what has been taken
    /// back meanwhile is not.
    fn remember(
        &self,
        n: u64,
        turn: String,
        heard: Option<ItemId>,
    ) -> impl Future<Output = Result<bool, Refused>> + Send;
    /// Resolves when nothing is taken any more.
    fn closed(&self) -> impl Future<Output = ()> + Send;
    /// Resolves once the deed is in the audit record, on the drive (SI-23).
    fn audit(&self, deed: Deed) -> impl Future<Output = Result<(), Refused>> + Send;
}
