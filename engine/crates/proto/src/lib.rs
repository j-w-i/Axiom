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
    /// The whole prompt, where that is told: with what was read from a cache and written to one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<u64>,
    /// Of `cache_write`, what was written to be kept for an hour, which costs more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write_long: Option<u64>,
}

/// What a figure is the sum of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Covers {
    /// One answer of a model.
    Answer,
    /// From a prompt to the end of what came of it.
    Turn,
    /// Everything so far, as whoever runs the loop counts it: a running total, not to be added up.
    Session,
    /// Not said.
    #[default]
    #[serde(other)]
    Unknown,
}
impl Covers {
    fn unknown(&self) -> bool {
        *self == Self::Unknown
    }
}

/// Who is paid, and how.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lane {
    /// By use.
    Key,
    /// A plan that was signed in to: a cost is what it would have been by use.
    Plan,
    /// A program of the person's own, which signs in by itself. Which way is its own.
    Program,
    /// Nobody: a server that takes no key.
    Open,
    #[default]
    #[serde(other)]
    Unknown,
}

/// What was used, as it was then: accounts are removed, and agents run on something else.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsedOn {
    pub harness: String,
    #[serde(default)]
    pub lane: Lane,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    /// The one that was asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The one that is said to have answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<String>,
    /// How fast it is said to have been done, where that has a price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub amount: f64,
    /// ISO 4217, or what a provider calls its own.
    pub currency: String,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Covers::unknown")]
    pub of: Covers,
}

/// One model's part of everything so far, or all of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelTotal {
    /// Absent: of all models together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub usage: Usage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
}

/// One of the things an agent means to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    pub text: String,
    pub state: TodoState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoState {
    Todo,
    Doing,
    Done,
    #[serde(other)]
    Unknown,
}

fn not(b: &bool) -> bool {
    !*b
}

/// Where a branch begins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branched {
    pub agent: AgentId,
    pub item: ItemId,
    /// The item is the last of it; otherwise it is the first that is not.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub with: bool,
}

/// Whether there can be a branch, and what of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchInfo {
    /// It would work in a copy of its own.
    pub apart: bool,
    /// What the files were there is kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub files: bool,
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
        /// It works in a copy of the folder of its own.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        apart: bool,
        /// It goes on from another's conversation, whose items are its own up to there.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<Box<Branched>>,
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
    Usage {
        agent: AgentId,
        usage: Usage,
        provenance: Provenance,
        /// What `usage` covers.
        #[serde(default, skip_serializing_if = "Covers::unknown")]
        of: Covers,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<Box<UsedOn>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost: Option<Box<Cost>>,
        /// By model, where that is told.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        totals: Vec<ModelTotal>,
        /// The answer broke off or was stopped: there may have been more than was told by then.
        #[serde(default, skip_serializing_if = "not")]
        cut: bool,
        /// Not for the conversation itself, but to summarise it.
        #[serde(default, skip_serializing_if = "not")]
        aside: bool,
    },
    /// What an agent means to do, as it says itself: all of it, in place of what it said before.
    /// Without steps and text there is none any more.
    #[serde(rename = "todo")]
    Todo {
        agent: AgentId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run: Option<RunId>,
        #[serde(default)]
        steps: Vec<Todo>,
        /// Markdown.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
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

/// What it is that waits for a person.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    /// A question that is open.
    #[default]
    Asks,
    /// Its run failed, and nobody has seen that.
    Failed,
    /// What it runs on is spent.
    Limit,
    /// It has finished, and nobody has seen that.
    Finished,
    #[serde(other)]
    Unknown,
}

/// Something that waits for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Waits {
    #[serde(default)]
    pub need: Need,
    pub session: SessionId,
    pub title: String,
    pub agent: AgentId,
    pub name: String,
    /// Of a question.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<RequestId>,
    #[serde(default)]
    pub what: String,
    /// The beginning of what it is about. All of it is where it is asked, and is answered there.
    pub subject: String,
    /// Since when.
    pub at: u64,
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

/// What an agent has changed in its own copy of the folder.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyChanges {
    /// There is one: it has been spoken to.
    pub made: bool,
    pub files: Vec<CopyFile>,
    /// These are all.
    pub whole: bool,
    /// What git ignores and only the copy has: files, or folders of them.
    #[serde(default)]
    pub ignored: usize,
    /// What was not copied or is not told of: it has no name that can be written down, is too deep,
    /// or is no plain file.
    #[serde(default)]
    pub odd: usize,
    /// The last that was brought in, if it can be put back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back: Option<CopyBack>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyBack {
    pub id: String,
    pub files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyFile {
    pub path: String,
    /// `new`, `changed` or `removed`.
    pub what: String,
    /// What the copy has now; absent if it was removed there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// It can be run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub run: bool,
    /// What the folder has now; absent if it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub there: Option<String>,
    /// What bringing it in would do to the folder; absent if that cannot be shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// The folder's own has changed too: it is not brought in.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clash: bool,
    /// The folder's own had changed too when the copy was brought up to date, and the agent has not
    /// touched its own since.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unsettled: bool,
    /// Why it is never brought in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub never: Option<String>,
    /// It runs when someone builds, tests or installs.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub runs: bool,
    /// Other agents that have changed it in their own copies.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub others: Vec<String>,
}

/// A change, as a person has seen it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopySeen {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub run: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub there: Option<String>,
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
    /// It can be told how hard to try (`low`, `medium`, `high`).
    #[serde(default)]
    pub effort: bool,
    /// Axiom's tools can stand in place of its own (the environment `axiom`): then it is asked about
    /// and contained as Axiom's own agents are, and has every mode of access.
    #[serde(default)]
    pub hands: bool,
    /// What an account of it beside the first is called as a provider, if there can be any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accounts: Option<String>,
    /// The person added it themself.
    #[serde(default, skip_serializing_if = "not")]
    pub own: bool,
}

/// How a program is started, as its person has said it, of it or of one account of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgramInfo {
    /// [`HarnessInfo::id`].
    pub harness: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    /// Of one the person added themself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Absent: it is looked for by its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub vars: Vec<VarInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarInfo {
    pub name: String,
    /// Absent: it is a secret, and not shown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Whether it is there when the host has been restarted.
    #[serde(default)]
    pub lasts: bool,
}

/// A variable a program is to be given.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VarSet {
    pub name: String,
    /// Absent: the secret that is there stays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<SecretText>,
    /// It goes to the system's store. One whose name says that it is a secret does in any case.
    #[serde(default)]
    pub secret: bool,
}

/// A limit the host keeps, in words for a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limit {
    /// What it is a limit of; those of one kind come together.
    pub of: String,
    pub what: String,
    /// Absent: there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub most: Option<u64>,
    pub unit: Unit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Count,
    Bytes,
    Seconds,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Call {
    /// Must be the first line on a connection.
    #[serde(rename = "hello")]
    Hello { token: String, client: String, protocol: u32 },
    /// Instead of `hello`, from a program an agent is: opens `peer.call` for it and nothing else.
    #[serde(rename = "peer.hello")]
    PeerHello { grant: String },
    /// A call of one of the tools its answer named.
    #[serde(rename = "peer.call")]
    Peer { tool: String, args: serde_json::Value },
    #[serde(rename = "host.status")]
    Status {},
    /// The limits this host keeps: an array of [`Limit`].
    #[serde(rename = "host.limits")]
    Limits {},
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
        /// person's own that signs in by itself ([`HarnessInfo`]): its connection, if it has one, is
        /// another account of that program.
        execution: Execution,
        access: Access,
        /// The folder's own settings for such a program may take effect (SI-5).
        #[serde(default)]
        trust: bool,
        /// Instructions of the workspace a person has read and wants followed, by what they were.
        #[serde(default)]
        instructions: Vec<Instructions>,
        /// It works in a copy of the folder, made when it is first spoken to; what it changes there
        /// gets into the folder by `copy.bring` only.
        #[serde(default)]
        apart: bool,
    },
    /// What has been changed in a session's folder since its last commit.
    #[serde(rename = "workspace.changes")]
    Changes { session: SessionId },
    /// What an agent has changed in its own copy.
    #[serde(rename = "copy.changes")]
    CopyChanges { session: SessionId, agent: AgentId },
    /// Brings changes in, each as it was shown.
    #[serde(rename = "copy.bring")]
    CopyBring { session: SessionId, agent: AgentId, files: Vec<CopySeen> },
    /// Puts back what was there before the last that was brought in, as it was named.
    #[serde(rename = "copy.back")]
    CopyBack { session: SessionId, agent: AgentId, back: String },
    /// Takes over into the copy what has changed in the folder.
    #[serde(rename = "copy.renew")]
    CopyRenew { session: SessionId, agent: AgentId },
    /// Throws the copy away. With changes in it only if they are to be lost.
    #[serde(rename = "copy.remove")]
    CopyRemove {
        session: SessionId,
        agent: AgentId,
        #[serde(default)]
        lose: bool,
    },
    /// The harnesses this host can drive.
    #[serde(rename = "harness.list")]
    Harnesses {},
    /// What has been said of how programs are started: an array of [`ProgramInfo`].
    #[serde(rename = "program.list")]
    Programs {},
    /// Says how a program, or one account of it, is started; without `harness`, adds one that speaks
    /// ACP, with a name and a path. All of it is said anew. Answers with the harness. It is true of
    /// the next start.
    #[serde(rename = "program.set")]
    ProgramSet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        harness: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        vars: Vec<VarSet>,
    },
    /// Takes away one the person added, or what was said of another.
    #[serde(rename = "program.remove")]
    ProgramRemove {
        harness: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        account: Option<String>,
    },
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
    /// Whether a conversation can be branched at an item, and whether the files there are kept.
    #[serde(rename = "branch.look")]
    BranchLook { session: SessionId, agent: AgentId, item: ItemId },
    /// Adds an agent that begins with another's conversation, as it was before what a person said
    /// (`item`, a prompt that began a run), or after an answer that ended one. `files`: with the
    /// files of its copy as they were. Nothing is said to it. Answers with its id.
    #[serde(rename = "agent.branch")]
    Branch {
        session: SessionId,
        agent: AgentId,
        item: ItemId,
        name: String,
        #[serde(default)]
        files: bool,
    },
    /// Summarises an agent's conversation now, to make room. Only between runs, only for an agent of
    /// Axiom's own, and only when nothing waits to be heard. Answers with the run that does it.
    #[serde(rename = "agent.compact")]
    Compact { session: SessionId, agent: AgentId },
    /// A person's answer.
    #[serde(rename = "request.answer")]
    Answer {
        session: SessionId,
        request: RequestId,
        answer: Answer,
        /// The question's digest, as it was shown.
        digest: String,
        /// With a no: why, for whoever asked to read.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        words: Option<String>,
    },
    /// A person has seen what an agent came to.
    #[serde(rename = "agent.seen")]
    Seen { session: SessionId, agent: AgentId },
    /// What waits for a person, in all sessions: questions, the longest first, and what agents came
    /// to that nobody has seen.
    #[serde(rename = "host.waiting")]
    Waiting {},
    /// Shows a folder in the windows that are open, as where a session could begin. Nothing begins,
    /// and nothing is allowed. Answers with how many windows there are.
    #[serde(rename = "window.open")]
    Open { path: String },
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
    /// Of an account that was signed in to here, it is signed out of as well.
    #[serde(rename = "connection.remove")]
    ConnectionRemove { connection: String },
    /// Signs in to a ChatGPT plan, or again to an account of one. Answers with an address for the
    /// person's browser, which nothing here opens.
    #[serde(rename = "signin.begin")]
    SigninBegin {
        #[serde(default)]
        connection: Option<String>,
    },
    #[serde(rename = "signin.state")]
    SigninState { signin: String },
    /// Only while it waits for a person.
    #[serde(rename = "signin.cancel")]
    SigninCancel { signin: String },
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
    /// Of an account of a program of the person's own: the command they sign in with, in a terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_in: Option<String>,
    /// Why it cannot be used until somebody signs in to it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// How far a sign-in is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Signin {
    /// For a person, in their browser.
    Waiting,
    /// They are done there.
    Finishing,
    Done {
        connection: String,
    },
    Failed {
        problem: String,
    },
    #[serde(other)]
    Unknown,
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
    /// How hard it tries can be said (`low`, `medium`, `high`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub effort: bool,
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
    /// What waits for a person is something else now, and how many there are: `host.waiting` says
    /// what. Sent to every client that has said hello.
    Waiting {
        waiting: usize,
    },
    /// A person asked in a terminal for this folder to be shown. Only to windows.
    Open {
        open: String,
    },
}

/// How a client that has a window says hello.
pub const WINDOW: &str = "axiom-app/";

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
    "todo",
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
