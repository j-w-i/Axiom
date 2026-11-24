//! Recorded scripts: what an agent did, with its timing, as JSON lines. Used to replay a session
//! for a person, a test or a benchmark. A replay is always labelled as one.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AgentState, ItemKind, Outcome, Provenance, Usage};

/// One step, `after` milliseconds since the one before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    #[serde(default)]
    pub after: u64,
    #[serde(flatten)]
    pub act: Act,
}

/// `item` is a name local to the script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Act {
    /// First line: who this was.
    Agent {
        name: String,
        #[serde(default)]
        model: Option<String>,
    },
    State {
        state: AgentState,
    },
    Start {
        item: String,
        kind: ItemKind,
        #[serde(default)]
        name: String,
        #[serde(default)]
        args: Option<Value>,
    },
    Text {
        item: String,
        text: String,
    },
    End {
        item: String,
        outcome: Outcome,
        #[serde(default)]
        result: Option<Value>,
    },
    Ask {
        request: String,
        what: String,
        subject: String,
        scope: String,
    },
    Answered {
        request: String,
        answer: String,
    },
    Usage {
        usage: Usage,
        provenance: Provenance,
    },
    Finish {
        outcome: Outcome,
        #[serde(default)]
        error: Option<String>,
    },
}

impl Step {
    pub fn parse(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line)
    }
}
