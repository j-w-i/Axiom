//! One request and one stream for every provider (decision 0005, research/providers.md §6).
//!
//! A block is two things at once: what Axiom reads (text, a call, a result) and the provider's own
//! JSON, kept to the byte. Signed and encrypted reasoning is only worth sending back unchanged, and
//! only to where it came from; [`Scope`] says where that is.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use serde_json::{Map, Value, value::RawValue};

/// Well-formed JSON whose text is never touched. It is stored as a string, so it also survives
/// being carried through a [`Value`], which would sort its members and round its numbers.
#[derive(Clone)]
pub struct Json(Box<RawValue>);

impl Json {
    pub fn parse(text: impl Into<String>) -> serde_json::Result<Self> {
        RawValue::from_string(text.into()).map(Self)
    }
    pub fn of(value: &impl Serialize) -> serde_json::Result<Self> {
        serde_json::value::to_raw_value(value).map(Self)
    }
    pub fn get(&self) -> &str {
        self.0.get()
    }
    pub(crate) fn value(&self) -> Value {
        serde_json::from_str(self.0.get()).unwrap_or(Value::Null)
    }
    pub(crate) fn raw(&self) -> &RawValue {
        &self.0
    }
}
impl From<&RawValue> for Json {
    fn from(raw: &RawValue) -> Self {
        Self(raw.to_owned())
    }
}
impl From<Value> for Json {
    fn from(value: Value) -> Self {
        // A Value always has a text; `null` stands for what cannot be.
        Self::parse(value.to_string()).unwrap_or_else(|_| Self(RawValue::NULL.to_owned()))
    }
}
impl PartialEq for Json {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}
impl Eq for Json {}
impl fmt::Debug for Json {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.get())
    }
}
impl Serialize for Json {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.get())
    }
}
impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(d)?).map_err(D::Error::custom)
    }
}

/// The shape of the messages, not who serves them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    /// Anthropic Messages.
    Messages,
    /// OpenAI Chat Completions.
    Chat,
    /// OpenAI Responses.
    Responses,
    /// Google Gemini `generateContent`.
    Gemini,
    /// Amazon Bedrock `ConverseStream`.
    Converse,
}

/// Where a block's native JSON came from, and so where it may be valid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    pub codec: Codec,
    pub provider: String,
    /// The model that was asked, not the one that says it answered.
    pub model: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Native {
    pub scope: Scope,
    /// In the Messages and Converse codecs the block itself, in the Responses codec the item. In the Chat codec,
    /// which has no blocks, an entry of `tool_calls`, or an object with the members of the assistant
    /// message that it was. In the Gemini codec a part, or several as an array.
    pub json: Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub blocks: Vec<Block>,
}
impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self { role: Role::User, blocks: vec![Block::text(text)] }
    }
    /// The calls that may be run. A call that was cut short or cannot be read is not among them.
    pub fn calls(&self) -> impl Iterator<Item = Call<'_>> {
        self.blocks.iter().filter_map(|b| match &b.content {
            Content::ToolCall(call) => call.ready(),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub content: Content,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native: Option<Native>,
    /// Pieces of the stream this build could not fold into the block, as they came.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<Json>,
    /// The stream ended inside this block.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cut: bool,
}
impl Block {
    pub fn new(content: Content) -> Self {
        Self { content, native: None, extra: Vec::new(), cut: false }
    }
    pub fn text(text: impl Into<String>) -> Self {
        Self::new(Content::Text { text: text.into() })
    }
    pub fn result(call: impl Into<String>, text: impl Into<String>, failed: bool) -> Self {
        Self::new(Content::ToolResult { call: call.into(), text: text.into(), failed })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    /// What the provider shows of the reasoning: all of it, a summary, or nothing.
    Reasoning {
        text: String,
    },
    ToolCall(ToolCall),
    ToolResult {
        call: String,
        text: String,
        failed: bool,
    },
    /// A kind this build does not know, kept in `native`. It is never run.
    Unknown {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// The arguments as the model wrote them.
    pub raw: String,
    pub args: Args,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Args {
    Ready(Map<String, Value>),
    /// The answer ended before the model was done, or not in a way that lets a call be run.
    Cut,
    /// Complete, but not something that can be run; why.
    Broken(String),
}

/// A call that may be run. There is no other way to get one than [`ToolCall::ready`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Call<'a> {
    call: &'a ToolCall,
    args: &'a Map<String, Value>,
}
impl<'a> Call<'a> {
    pub fn id(&self) -> &'a str {
        &self.call.id
    }
    pub fn name(&self) -> &'a str {
        &self.call.name
    }
    pub fn args(&self) -> &'a Map<String, Value> {
        self.args
    }
}
impl ToolCall {
    pub fn ready(&self) -> Option<Call<'_>> {
        match &self.args {
            Args::Ready(args) => Some(Call { call: self, args }),
            Args::Cut | Args::Broken(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    /// A JSON schema, sent as it is written: its order is part of the cached prefix.
    pub schema: Json,
}

/// What the caller would like the provider to keep of the prompt. A hint.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cache {
    #[default]
    Off,
    /// Minutes.
    Short,
    /// An hour, where the provider has that.
    Long,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub max_output: Option<u32>,
    /// In the provider's own words. Asking a provider that has no such control is an error.
    pub effort: Option<String>,
    /// Whether it thinks, where that can be told.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<bool>,
    /// How fast, in the provider's own words, where that can be told. It has a price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    pub stop: Vec<String>,
    pub cache: Cache,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub model: String,
    pub system: String,
    pub tools: Vec<Tool>,
    pub messages: Vec<Message>,
    pub settings: Settings,
}

/// Tokens as the provider counted them. What it did not say, and what does not follow from what it
/// said, is absent.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Usage {
    /// The whole prompt: uncached, read and written.
    pub prompt: Option<u64>,
    pub uncached: Option<u64>,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
    /// Of `cache_write`, what was written to be kept an hour.
    pub cache_write_long: Option<u64>,
    /// All of it, reasoning included.
    pub output: Option<u64>,
    /// Part of `output`.
    pub reasoning: Option<u64>,
    /// What the provider says it took for the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<Cost>,
    /// How fast the provider says it was answered, where that has a price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub amount: f64,
    /// What the provider calls what it took.
    pub unit: String,
}

/// Why the model stopped, by its provider's account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stop {
    EndTurn,
    ToolUse,
    /// The request's own limit.
    MaxOutput,
    /// The model's context window is full.
    ContextFull,
    Sequence(Option<String>),
    /// A tool of the provider's needs another request; the answer so far is sent back as it is.
    Pause,
    /// The model or its provider declined; the category, if one was given.
    Refusal(Option<String>),
    Other(String),
}
impl Stop {
    /// Whether calls in an answer that ended so may be run. Everything else may have stopped the
    /// model inside its arguments, where what is left can still happen to parse.
    pub fn runs_calls(&self) -> bool {
        matches!(self, Stop::EndTurn | Stop::ToolUse)
    }
}

/// What a block is when it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    Text,
    Reasoning,
    ToolCall { id: String, name: String },
    Unknown { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delta {
    Text(String),
    Reasoning(String),
    Args(String),
    /// A piece this build does not know, or that does not belong to a block of this kind.
    Other(Json),
}

/// The stream, whoever it comes from. Blocks are numbered from 0 as they start, and several may be
/// open at once.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Started {
        id: Option<String>,
        /// Who the provider says answers. LM Studio lets a model it has answer for one it has not,
        /// and says so nowhere else.
        model: Option<String>,
    },
    BlockStart {
        block: usize,
        start: Start,
    },
    Delta {
        block: usize,
        delta: Delta,
    },
    /// The block is whole. `native` is what to send back.
    BlockStop {
        block: usize,
        native: Option<Json>,
        /// The block cannot be used although it is whole; why.
        broken: Option<String>,
    },
    /// All that is known so far; replaces the one before.
    Usage(Usage),
    Stop(Stop),
    /// The provider said it was done. Without it an answer is cut short.
    End,
}
