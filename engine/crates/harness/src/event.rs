//! What an adapter tells and is told, whatever its harness speaks. Everything in it that a harness
//! wrote is its own word: untrusted, bounded, and a report, not proof (T36).

use axiom_policy::{Access, Place, audit::Containment};
use axiom_proto::{Cost, Covers, ModelTotal, Outcome, Provenance, Usage};
use axiom_spawn::Ended;
use serde_json::Value;

use crate::frames::Frame;

/// What a harness can do, as it says itself or as its documentation does. What is not there is not
/// emulated.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caps {
    pub resume: bool,
    pub steer: Steer,
    /// A turn can be ended and the session kept.
    pub interrupt: bool,
    /// It may ask before it acts. That it always does is nobody's promise.
    pub approvals: bool,
    /// It may report tokens, or how full the context is.
    pub usage: bool,
    /// It may report how much of a plan is used up.
    pub quota: bool,
    /// It may tell its plan.
    pub plan: bool,
    /// What it has a documented mode for.
    pub access: Vec<Access>,
    pub models: Names,
    pub efforts: Names,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Steer {
    #[default]
    No,
    /// Words said during a turn are heard in it.
    Yes,
    /// They are taken; whether they join the turn or start another is the agent's choice.
    AgentDecides,
}

/// What may be chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Names {
    /// Nothing.
    #[default]
    None,
    /// By name; it does not say which there are.
    Any,
    Of(Vec<Name>),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Name {
    pub id: String,
    pub label: Option<String>,
    /// The efforts this model takes, where that depends on it.
    pub efforts: Vec<String>,
    /// The one of them it tries at when told none, where that is known.
    pub default_effort: Option<String>,
    /// It can be faster than the standard, for a price.
    pub fast: bool,
}

impl Names {
    pub fn has(&self, id: &str) -> bool {
        match self {
            Names::None => false,
            Names::Any => true,
            Names::Of(all) => all.iter().any(|n| n.id == id),
        }
    }
}

/// What was found out about a program without giving it work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub program: Place,
    pub name: Option<String>,
    pub version: Option<String>,
    /// What it and Axiom agreed to speak.
    pub protocol: String,
    pub caps: Caps,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The session is there. Once, before anything of a turn.
    Ready(Box<Ready>),
    /// It goes on under another id.
    Session(String),
    /// A turn begins: for a prompt, or because the agent does so by itself.
    Began {
        by_agent: bool,
    },
    Say(Say),
    /// The text is complete.
    Said {
        id: String,
    },
    ToolStarted(Box<Tool>),
    ToolUpdated(Box<Tool>),
    ToolOutput {
        id: String,
        text: String,
        /// Replaces what came before.
        whole: bool,
        /// Bytes left out.
        cut: u64,
    },
    /// Once for each that started, whatever happens.
    ToolEnded(Box<ToolEnd>),
    /// The plan as it is now. No steps and no text: it is gone.
    Plan(Plan),
    Asked(Box<Question>),
    /// Once for each that was asked.
    AskClosed {
        id: String,
        how: Settled,
    },
    Usage(Spent),
    Limit(Limit),
    Notice {
        level: Level,
        text: String,
    },
    /// What was said was not taken.
    NotHeard {
        why: String,
    },
    /// Something this build does not know, or that made no sense.
    Unknown {
        what: String,
        raw: String,
        /// All of it, in bytes.
        bytes: u64,
    },
    /// Once for each turn that began.
    Done(Done),
    /// The attempt is over and nothing of it runs. Always the last, and once.
    Closed(Box<Gone>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ready {
    /// What to resume it by.
    pub session: String,
    pub protocol: String,
    pub version: Option<String>,
    pub model: Option<String>,
    pub caps: Caps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Prose,
    Reasoning,
}

/// Text of the item `id`, which begins with the first of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Say {
    pub id: String,
    pub kind: Kind,
    pub text: String,
    /// All of it, in place of what came before.
    pub whole: bool,
    /// The sub-agent's tool call or session.
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tool {
    pub id: String,
    /// The program's name for it. Empty: not said.
    pub name: String,
    pub title: Option<String>,
    /// Absent if it was not said, or too long to keep: then `args_cut` says how long it was.
    pub args: Option<Value>,
    pub args_cut: u64,
    pub diffs: Vec<Diff>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolEnd {
    pub id: String,
    pub outcome: Outcome,
    /// In place of the output so far, if it is there.
    pub output: Option<String>,
    pub output_cut: u64,
    pub diffs: Vec<Diff>,
    pub error: Option<String>,
}

/// A change to a file, as it was reported: a patch, or both texts, or only that there is one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub path: String,
    pub patch: Option<String>,
    pub old: Option<String>,
    pub new: Option<String>,
    /// Bytes left out.
    pub cut: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    pub id: String,
    pub steps: Vec<(String, Progress)>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Todo,
    Doing,
    Done,
    Unknown,
}

/// Something the harness will not do without an answer. Nothing in it is cut: what is too long to
/// show is refused instead (SI-15).
#[derive(Debug, Clone, PartialEq)]
pub struct Question {
    pub id: String,
    pub tool: Option<String>,
    /// The agent's own words.
    pub title: String,
    pub detail: Option<String>,
    /// What it is about, as the protocol has it.
    pub subject: Value,
    pub options: Vec<Choice>,
    /// The sub-agent that asks.
    pub by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub name: String,
    pub kind: ChoiceKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceKind {
    AllowOnce,
    /// The agent remembers it, for what it sees fit.
    AllowAlways,
    RejectOnce,
    RejectAlways,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// One of the question's options, by its id.
    Choose(String),
    Refuse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settled {
    Answered,
    TimedOut,
    /// The harness no longer waits for it.
    Withdrawn,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spent {
    pub usage: Usage,
    /// Tokens in the context, and how many there may be.
    pub context: Option<(u64, u64)>,
    /// What `usage` covers.
    pub of: Covers,
    pub cost: Option<Cost>,
    /// By model, of the session so far.
    pub totals: Vec<ModelTotal>,
    pub provenance: Option<Provenance>,
}

/// How much of a plan is used up, where the harness says so.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Limit {
    pub name: String,
    /// 0 to 1.
    pub used: Option<f64>,
    /// Seconds since 1970.
    pub resets_at: Option<u64>,
    pub minutes: Option<u64>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// `Unknown`: it stopped and did not say why.
    pub outcome: Outcome,
    /// The protocol's word for why.
    pub stop: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gone {
    pub why: Why,
    /// Nothing if it ended because it was asked to.
    pub words: Option<String>,
    pub ended: Option<Ended>,
    /// The last that was said between the two.
    pub frames: Vec<Frame>,
    /// The end of what it wrote beside.
    pub stderr: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A person stopped it, or it was no longer wanted.
    Asked,
    /// It ended by itself.
    Died,
    /// It said nothing for too long.
    Silent,
    /// What it said cannot be read.
    Nonsense,
    /// It has too much open at once.
    Flood,
    /// It does not read what it is told.
    Deaf,
    /// Its own sign-in is needed.
    SignIn,
    /// No protocol in common.
    Version,
    /// It does not keep to what it was told to keep to: the mode, the sandbox.
    Boundary,
    /// It refused what it was asked for.
    Refused,
    /// What it said could not be put on record.
    Journal,
    /// Nobody listens.
    Unheard,
}

/// Whose words a prompt holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    Person,
    /// Another agent, a rule, a schedule.
    Agent,
}

/// Where a prompt went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Took {
    Began,
    Steered,
    /// The agent has it and decides.
    Left,
}

/// Why a command was not done. Nothing happened.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("the attempt is over")]
    Closed,
    #[error("it is not ready yet")]
    NotReady,
    #[error("it is at work and cannot be told anything meanwhile")]
    Busy,
    #[error("there is nothing in it")]
    Empty,
    #[error("it is longer than {0} bytes")]
    TooLong(usize),
    #[error("this question is not open")]
    NotAsked,
    #[error("the question has no such answer")]
    NoSuchOption,
    #[error("this harness may only be given a person's own words")]
    PersonOnly,
}

/// Why nothing was started.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "the workspace is not trusted, and this harness cannot be told to ignore what is in it"
    )]
    Untrusted,
    #[error("this harness has no mode for it: {0:?}")]
    NoMode(Access),
    #[error("this is not used as {0}: {1:?}")]
    BadValue(&'static str, String),
    #[error("{0:?} would switch off what holds it back")]
    Unsafe(String),
    #[error("{0:?} is not passed on to this program: it could change what holds it back")]
    NotPassed(String),
    #[error("this program cannot be told how fast to be")]
    NoSpeed,
    #[error("this program does not say how several accounts of it are kept apart")]
    OneAccount,
    #[error("this harness may only be started by a person")]
    PersonOnly,
    #[error("{0} is inside what an agent may write to")]
    Inside(Place),
    #[error(transparent)]
    Spawn(#[from] axiom_spawn::SpawnError),
    #[error("it has no pipes")]
    NoPipe,
    #[error("{0}")]
    Probe(String),
}

/// How it is to be started, before it is: to be put on record first (SI-23, SI-26).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub args: Vec<String>,
    pub containment: Containment,
}
