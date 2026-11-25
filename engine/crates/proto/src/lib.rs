//! The wire between Axiom's host and its clients (docs/protocol/wire.md).
//!
//! One JSON object per line. A reader ignores fields it does not know; an event body it does not
//! know decodes as [`Body::Unknown`], so an older client keeps its place in the sequence.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod limits;
pub mod script;

/// Raised when a change would be misread by the other side.
pub const PROTOCOL: u32 = 1;

/// A longer line is an error, not a buffer.
pub const MAX_LINE: usize = 8 * 1024 * 1024;

/// Most sessions in one answer.
pub const MAX_LIST: u32 = 1000;

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

/// What an agent may do without asking (`axiom-policy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Read,
    Ask,
    Work,
    #[serde(other)]
    Unknown,
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

fn none(n: &u64) -> bool {
    *n == 0
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
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access: Option<Access>,
    },
    /// What it runs on or may do is another from here on.
    #[serde(rename = "agent.changed")]
    AgentChanged {
        agent: AgentId,
        execution: Execution,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        access: Option<Access>,
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
        /// How long they were, if they were too long to keep: then `args` is absent.
        #[serde(default, skip_serializing_if = "none")]
        args_cut: u64,
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
        /// As `args_cut`.
        #[serde(default, skip_serializing_if = "none")]
        result_cut: u64,
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
        /// Names exactly this question. Sent back with the answer, so that what was shown is what is
        /// answered (SI-2).
        #[serde(default, skip_serializing_if = "String::is_empty")]
        digest: String,
        /// The tool call it is about.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        item: Option<ItemId>,
        /// Whether it can be allowed for the rest of the session.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        session: bool,
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

/// What has been changed in a folder since its last commit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    /// Git has something to do with the folder. If not, nothing else is known.
    pub repository: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub files: Vec<Changed>,
    /// A unified diff of them, without prefixes.
    pub diff: String,
    /// All of it is here.
    pub whole: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changed {
    pub path: String,
    /// `new`, `changed`, `removed` or `conflict`.
    pub what: String,
}

/// A harness, as it was found without being run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessInfo {
    pub id: String,
    pub name: String,
    /// The program, if it is there and may be started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// It cannot be told to ignore a folder's own settings for it.
    pub needs_trust: bool,
    /// What it has a mode for.
    pub access: Vec<Access>,
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
    /// Newest first. A list shorter than `limit` is the last.
    #[serde(rename = "session.list")]
    SessionList {
        /// At most [`MAX_LIST`].
        #[serde(default = "two_hundred")]
        limit: u32,
        #[serde(default)]
        offset: u32,
    },
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
    /// Adds an agent to a session that has a workspace. Answers with its id.
    #[serde(rename = "agent.add")]
    AgentAdd {
        session: SessionId,
        name: String,
        /// With the harness `axiom`, `connection` and `model` are needed. Another is a program of the
        /// person's own that signs in by itself ([`HarnessInfo`]): it has no connection.
        execution: Execution,
        access: Access,
        /// The folder's own settings for such a program may take effect (SI-5).
        #[serde(default)]
        trust: bool,
        /// Instructions of the workspace a person has read and wants followed, by what they were.
        #[serde(default)]
        instructions: Vec<Instructions>,
    },
    /// What has been changed in a session's folder since its last commit.
    #[serde(rename = "workspace.changes")]
    Changes { session: SessionId },
    /// The harnesses this host can drive.
    #[serde(rename = "harness.list")]
    Harnesses {},
    /// What a folder's owner would have agents told. For a person to read: nothing of it is followed
    /// unless they say so.
    #[serde(rename = "workspace.look")]
    Look { path: String },
    /// Changes what an agent runs on or may do, for its next run.
    #[serde(rename = "agent.set")]
    AgentSet {
        session: SessionId,
        agent: AgentId,
        #[serde(default)]
        execution: Option<Execution>,
        #[serde(default)]
        access: Option<Access>,
    },
    /// Says something to an agent. Starts a run, or is heard by the one that runs, before its next
    /// step. Answers with the run.
    #[serde(rename = "agent.prompt")]
    Prompt {
        session: SessionId,
        agent: AgentId,
        text: String,
        /// The client's own name for it. Said twice with the same, it is said once.
        #[serde(default)]
        key: Option<String>,
    },
    /// A person's answer.
    #[serde(rename = "request.answer")]
    Answer {
        session: SessionId,
        request: RequestId,
        answer: Answer,
        /// The question's digest, as it was shown.
        digest: String,
    },
    #[serde(rename = "connection.list")]
    Connections {},
    /// Adds an account, a key or a server. The secret goes to the system's store and nowhere else.
    #[serde(rename = "connection.add")]
    ConnectionAdd {
        name: String,
        provider: String,
        /// For a server of one's own; a provider's own otherwise.
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        secret: Option<SecretText>,
    },
    #[serde(rename = "connection.remove")]
    ConnectionRemove { connection: String },
    /// What can be used through a connection. Asks the provider.
    #[serde(rename = "model.list")]
    Models { connection: String },
    /// The record of what was done, after `after`.
    #[serde(rename = "audit.read")]
    Audit {
        #[serde(default)]
        after: u64,
        #[serde(default = "two_hundred")]
        limit: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Once,
    /// Exactly this, by this agent, as long as the host runs.
    Session,
    Deny,
}

/// A secret on its way to the system's store. It is not shown.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretText(pub String);
impl std::fmt::Debug for SecretText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("(secret)")
    }
}

/// An account, a key or a server: one way to reach a provider. There may be several.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Whether it has a secret.
    pub secret: bool,
    /// Whether that is in the system's store, and not only as long as the host runs.
    #[serde(default)]
    pub lasts: bool,
}

/// Words of a workspace for agents: a file, and which content of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instructions {
    pub path: String,
    pub digest: String,
    /// To be read by a person; absent where only named.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// What a provider says of a model. What it does not say is not there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<u64>,
}
fn one() -> f64 {
    1.0
}
fn two_hundred() -> u32 {
    200
}
impl Call {
    /// `session.list` with its defaults.
    pub fn list() -> Self {
        Call::SessionList { limit: two_hundred(), offset: 0 }
    }
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
    /// This client fell behind, or, with 0, its `after` is ahead of this history. Nothing more comes
    /// for the session until it follows again.
    Resync {
        session: SessionId,
        resync: u64,
    },
    /// The host cannot read or write this session, in words. Nothing more comes for it; a client
    /// does not follow again by itself.
    Fault {
        session: SessionId,
        fault: String,
    },
}

/// What `host.status` says of a session it cannot read or write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fault {
    pub session: SessionId,
    pub words: String,
}

/// The line of [`Message::Events`] around events that are already encoded.
pub fn events_line<'a>(
    session: &SessionId,
    events: impl IntoIterator<Item = &'a [u8]>,
) -> serde_json::Result<Vec<u8>> {
    let mut line = br#"{"session":"#.to_vec();
    serde_json::to_writer(&mut line, session)?;
    line.extend(br#","events":["#);
    for (n, e) in events.into_iter().enumerate() {
        if n > 0 {
            line.push(b',');
        }
        line.extend(e);
    }
    line.extend(b"]}");
    Ok(line)
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
    "agent.changed",
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

/// Splits a byte stream into lines, refusing lines over its limit.
pub struct Lines {
    buf: Vec<u8>,
    scanned: usize,
    limit: usize,
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("a line exceeds {0} bytes")]
pub struct LineTooLong(usize);

impl Default for Lines {
    fn default() -> Self {
        Self::new(MAX_LINE)
    }
}
impl Lines {
    pub fn new(limit: usize) -> Self {
        Self { buf: Vec::new(), scanned: 0, limit }
    }
    /// Bytes that are not yet taken as lines.
    pub fn waiting(&self) -> usize {
        self.buf.len()
    }
    /// For the lines after this one.
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }
    /// The next complete line without its newline, or `None` if more bytes are needed.
    pub fn next_line(&mut self) -> Result<Option<Vec<u8>>, LineTooLong> {
        match self.buf[self.scanned..].iter().position(|b| *b == b'\n') {
            Some(i) => {
                let end = self.scanned + i;
                if end > self.limit {
                    return Err(LineTooLong(self.limit));
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
            None if self.buf.len() > self.limit => Err(LineTooLong(self.limit)),
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
        let some = || (0..1000).map(|_| ItemId::new()).collect::<std::collections::BTreeSet<_>>();
        let a = some();
        assert_eq!(a.len(), 1000);
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = some();
        assert!(a.last() < b.first(), "later ids sort later");
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
        assert_eq!(s.next_line(), Err(LineTooLong(MAX_LINE)));
    }

    #[test]
    fn a_long_line_is_refused_though_it_ends() {
        let mut s = Lines::new(8);
        s.push(b"12345678\n123456789\n");
        assert_eq!(s.next_line(), Ok(Some(b"12345678".to_vec())));
        assert_eq!(s.next_line(), Err(LineTooLong(8)));
        s.set_limit(9);
        assert_eq!(s.next_line(), Ok(Some(b"123456789".to_vec())));
    }

    #[test]
    fn events_encoded_apart_make_the_same_line() {
        let events: Vec<_> = (1..4)
            .map(|seq| Event { seq, at: 1, body: Body::SessionTitled { title: "t\"".into() } })
            .collect();
        let session = SessionId("s\"".into());
        let parts: Vec<_> = events.iter().map(|e| serde_json::to_vec(e).unwrap()).collect();
        let line = events_line(&session, parts.iter().map(Vec::as_slice)).unwrap();
        assert_eq!(line, serde_json::to_vec(&Message::Events { session, events }).unwrap());
    }

    #[test]
    fn a_list_has_defaults() {
        let r: Request =
            serde_json::from_str(r#"{"id":1,"method":"session.list","params":{}}"#).unwrap();
        assert_eq!(r.call, Call::SessionList { limit: 200, offset: 0 });
        assert_eq!(r.call, Call::list());
    }
}
