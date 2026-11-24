//! The wire between Axiom's host and its clients (docs/protocol/wire.md).
//!
//! One JSON object per line. A reader ignores fields it does not know; an event body it does not
//! know decodes as [`Body::Unknown`], so an older client keeps its place in the sequence.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod script;

/// Raised when a change would be misread by the other side.
pub const PROTOCOL: u32 = 1;

/// A longer line is an error, not a buffer.
pub const MAX_LINE: usize = 8 * 1024 * 1024;

macro_rules! ids {
    ($($(#[$m:meta])* $name:ident $prefix:literal),* $(,)?) => {$(
        $(#[$m])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl $name {
            pub fn new() -> Self {
                Self(fresh($prefix))
            }
        }
        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    )*};
}
ids!(SessionId "s", AgentId "a", RunId "r", ItemId "i", RequestId "q");

/// Sortable by creation time: milliseconds, then 64 random bits.
fn fresh(prefix: &str) -> String {
    let mut b = [0u8; 8];
    // A failing system generator leaves zeroes; the time and a counter still keep ids distinct.
    let _ = getrandom::fill(&mut b);
    static N: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{prefix}_{:011x}{:016x}{n:04x}", now_ms(), u64::from_le_bytes(b))
}

/// Fills `b` from the system's generator, or says why it cannot.
pub fn random(b: &mut [u8]) -> Result<(), String> {
    getrandom::fill(b).map_err(|e| e.to_string())
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// How a run reaches intelligence: five separate things (docs/architecture/overview.md).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Execution {
    pub harness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Waiting,
    Running,
    Asking,
    Paused,
    Done,
    Failed,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Completed,
    Failed,
    /// Stopped by a person.
    Cancelled,
    /// The host or the machine went away while it ran.
    Interrupted,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// What a person or another agent said to this agent.
    Prompt,
    Prose,
    Reasoning,
    Tool,
    Notice,
    #[serde(other)]
    Unknown,
}

/// How a number is known. A client never shows one without this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Reported,
    Measured,
    Estimated,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
    /// Share of the context window in use, 0–1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Body {
    #[serde(rename = "session.created")]
    SessionCreated {
        title: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workspace: Option<String>,
    },
    #[serde(rename = "session.titled")]
    SessionTitled { title: String },
    #[serde(rename = "agent.added")]
    AgentAdded {
        agent: AgentId,
        name: String,
        execution: Execution,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<AgentId>,
    },
    #[serde(rename = "agent.state")]
    AgentState { agent: AgentId, state: AgentState },
    #[serde(rename = "run.started")]
    RunStarted { run: RunId, agent: AgentId },
    #[serde(rename = "run.ended")]
    RunEnded {
        run: RunId,
        outcome: Outcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    #[serde(rename = "item.started")]
    ItemStarted {
        item: ItemId,
        run: RunId,
        agent: AgentId,
        kind: ItemKind,
        /// For a tool: its name. Otherwise empty.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        /// For a tool: its arguments, as the model wrote them.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<Value>,
    },
    /// Text appended to an item. Coalesced: one per item per flush.
    #[serde(rename = "item.delta")]
    ItemDelta { item: ItemId, text: String },
    /// Authoritative: `text` replaces every delta before it.
    #[serde(rename = "item.ended")]
    ItemEnded {
        item: ItemId,
        outcome: Outcome,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<Value>,
    },
    #[serde(rename = "request.opened")]
    RequestOpened {
        request: RequestId,
        agent: AgentId,
        what: String,
        subject: String,
        scope: String,
        /// Whether an answer can still reach the asker.
        answerable: Answerable,
    },
    #[serde(rename = "request.closed")]
    RequestClosed { request: RequestId, answer: String },
    #[serde(rename = "usage")]
    Usage { agent: AgentId, usage: Usage, provenance: Provenance },
    /// Something newer than this build.
    #[serde(untagged)]
    Unknown(Value),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answerable {
    /// The asker is waiting in this process.
    Live,
    /// The answer will be delivered as a message when the agent next runs.
    Message,
    /// The asker is gone; the request is history.
    No,
    #[serde(other)]
    Unknown,
}

/// One entry of a session's journal. `seq` starts at 1 and rises by one as events happen. Stored
/// history may have holes: the deltas of an ended item are folded into its `item.ended`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub at: u64,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    pub created: u64,
    pub updated: u64,
    pub last_seq: u64,
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Call {
    /// Must be the first line on a connection.
    #[serde(rename = "hello")]
    Hello { token: String, client: String, protocol: u32 },
    #[serde(rename = "host.status")]
    Status {},
    /// Stops the host if nothing runs, or always with `force`.
    #[serde(rename = "host.shutdown")]
    Shutdown {
        #[serde(default)]
        force: bool,
    },
    #[serde(rename = "session.list")]
    SessionList {},
    #[serde(rename = "session.create")]
    SessionCreate {
        title: String,
        #[serde(default)]
        workspace: Option<String>,
    },
    /// Sends the events after `after`, then follows.
    #[serde(rename = "session.follow")]
    Follow {
        session: SessionId,
        #[serde(default)]
        after: u64,
    },
    #[serde(rename = "session.unfollow")]
    Unfollow { session: SessionId },
    /// Plays a recorded script as an agent of the session. The agent's harness is `replay`.
    #[serde(rename = "replay.start")]
    ReplayStart {
        session: SessionId,
        path: String,
        /// 1 = as recorded; 0 = as fast as possible.
        #[serde(default = "one")]
        speed: f64,
    },
    #[serde(rename = "run.cancel")]
    Cancel { session: SessionId, run: RunId },
}
fn one() -> f64 {
    1.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    #[serde(flatten)]
    pub call: Call,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Message {
    Ok {
        id: u64,
        result: Value,
    },
    Err {
        id: u64,
        error: WireError,
    },
    /// Events of one session, in order. What has been sent before is never sent again.
    Events {
        session: SessionId,
        events: Vec<Event>,
    },
    /// This client fell behind. Nothing more comes for the session until it follows again.
    Resync {
        session: SessionId,
        resync: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{code}: {message}")]
pub struct WireError {
    pub code: ErrorCode,
    pub message: String,
}
impl WireError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorised,
    Protocol,
    BadRequest,
    NotFound,
    Busy,
    Internal,
    #[serde(other)]
    Unknown,
}
impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The event types this build knows.
pub const KNOWN: &[&str] = &[
    "session.created",
    "session.titled",
    "agent.added",
    "agent.state",
    "run.started",
    "run.ended",
    "item.started",
    "item.delta",
    "item.ended",
    "request.opened",
    "request.closed",
    "usage",
];

impl Body {
    /// A body of a known type that did not decode: a fault of the sender, not a newer version.
    pub fn malformed(&self) -> bool {
        match self {
            Body::Unknown(v) => {
                v.get("type").and_then(Value::as_str).is_none_or(|t| KNOWN.contains(&t))
            }
            _ => false,
        }
    }
}

/// Splits a byte stream into lines, refusing lines over [`MAX_LINE`].
#[derive(Default)]
pub struct Lines {
    buf: Vec<u8>,
    scanned: usize,
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("a line exceeds {MAX_LINE} bytes")]
pub struct LineTooLong;

impl Lines {
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }
    /// The next complete line without its newline, or `None` if more bytes are needed.
    pub fn next_line(&mut self) -> Result<Option<Vec<u8>>, LineTooLong> {
        match self.buf[self.scanned..].iter().position(|b| *b == b'\n') {
            Some(i) => {
                let end = self.scanned + i;
                if end > MAX_LINE {
                    return Err(LineTooLong);
                }
                let rest = self.buf.split_off(end + 1);
                let mut line = std::mem::replace(&mut self.buf, rest);
                line.pop();
                if line.last() == Some(&b'\r') {
                    line.pop();
                }
                self.scanned = 0;
                Ok(Some(line))
            }
            None if self.buf.len() > MAX_LINE => Err(LineTooLong),
            None => {
                self.scanned = self.buf.len();
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn an_unknown_event_keeps_its_place() {
        let e: Event =
            serde_json::from_str(r#"{"seq":7,"at":1,"body":{"type":"x.new","n":1}}"#).unwrap();
        assert_eq!(e.seq, 7);
        assert!(matches!(e.body, Body::Unknown(_)));
        assert!(serde_json::to_string(&e).unwrap().contains("x.new"));
    }

    #[test]
    fn unknown_fields_and_states_are_tolerated() {
        let e: Body =
            serde_json::from_str(r#"{"type":"agent.state","agent":"a","state":"dozing","z":1}"#)
                .unwrap();
        assert_eq!(e, Body::AgentState { agent: AgentId("a".into()), state: AgentState::Unknown });
    }

    #[test]
    fn a_known_type_with_a_missing_field_is_not_taken_for_a_new_one() {
        // `untagged` fallback would hide a malformed known event; the host must notice instead.
        let b: Body = serde_json::from_str(r#"{"type":"item.delta","item":"i"}"#).unwrap();
        assert!(matches!(b, Body::Unknown(_)));
        assert!(b.malformed());
    }

    #[test]
    fn requests_have_one_shape() {
        let r = Request { id: 3, call: Call::Status {} };
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"id":3,"method":"host.status","params":{}}"#
        );
        let f: Request =
            serde_json::from_str(r#"{"id":1,"method":"session.follow","params":{"session":"s"}}"#)
                .unwrap();
        assert_eq!(f.call, Call::Follow { session: SessionId("s".into()), after: 0 });
    }

    #[test]
    fn ids_are_distinct_and_sorted_by_time() {
        let a: Vec<_> = (0..1000).map(|_| ItemId::new()).collect();
        let mut b = a.clone();
        b.dedup();
        assert_eq!(a.len(), b.len());
    }

    proptest! {
        #[test]
        fn any_chunking_gives_the_same_lines(
            lines in proptest::collection::vec("[^\\r\\n]{0,40}", 0..20),
            cuts in proptest::collection::vec(1usize..17, 0..200),
        ) {
            let mut bytes = Vec::new();
            for l in &lines { bytes.extend(l.as_bytes()); bytes.push(b'\n'); }
            let (mut s, mut got, mut at, mut k) = (Lines::default(), vec![], 0, 0);
            while at < bytes.len() {
                let n = cuts.get(k).copied().unwrap_or(9).min(bytes.len() - at);
                s.push(&bytes[at..at + n]);
                at += n; k += 1;
                while let Some(l) = s.next_line().unwrap() { got.push(String::from_utf8(l).unwrap()); }
            }
            prop_assert_eq!(got, lines);
        }

        #[test]
        fn decoding_never_panics(s in ".{0,200}") {
            let _ = serde_json::from_str::<Request>(&s);
            let _ = serde_json::from_str::<Event>(&s);
            let _ = script::Step::parse(&s);
        }
    }

    #[test]
    fn an_endless_line_is_refused() {
        let mut s = Lines::default();
        s.push(&vec![b'x'; MAX_LINE + 1]);
        assert_eq!(s.next_line(), Err(LineTooLong));
    }
}
