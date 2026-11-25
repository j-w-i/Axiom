//! Requests into bytes, streams into events; one module per shape of message (decision 0005).

use std::collections::HashSet;

use serde::Deserialize;

use crate::{
    error::Error,
    profile::{Profile, Quirks, Replay},
    sse,
    types::*,
};

pub mod chat;
pub mod gemini;
pub mod messages;
mod object;
pub mod responses;
mod schema;

/// Most an answer may bring, as event data. Everything a decoder or an accumulator holds comes out
/// of this.
pub const MAX_ANSWER: usize = 64 * 1024 * 1024;
/// Most blocks of one answer: a block costs more to hold than to send.
pub const MAX_BLOCKS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeError {
    #[error("{0} cannot be set for this provider and model")]
    Unsupported(&'static str),
    #[error("{0} must be set for this provider")]
    Missing(&'static str),
    #[error("the profile sets {0:?}, which belongs to the codec")]
    Reserved(String),
    #[error("the model's name {0:?} cannot be part of an address")]
    BadModel(String),
    #[error("the schema of the tool {tool:?} cannot be sent to this provider: {why}")]
    Schema { tool: String, why: String },
    #[error("the request cannot be written: {0}")]
    Json(String),
}
impl From<serde_json::Error> for EncodeError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e.to_string())
    }
}

/// A request as it goes out. Nothing after the first message depends on what comes before the last:
/// appending a message leaves every byte before it as it was, but for cache markers, which move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encoded {
    pub body: Vec<u8>,
    /// What of the history this provider and model cannot be told, in its order.
    pub lost: Vec<Lost>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lost {
    pub message: usize,
    pub block: usize,
    pub what: Loss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loss {
    /// Reasoning of another model or provider, or without what would prove it. Of a text or a call
    /// that is sent, the signature that held it.
    Reasoning,
    /// A block only its own provider understands.
    Unknown,
    /// A call that never became whole, which no provider takes back.
    UnfinishedCall,
    /// The result of such a call.
    ResultOfUnfinishedCall,
    /// The result of a call the history does not have, for a provider that has to be told its name.
    ResultWithoutCall,
    /// A block this codec has no place for in a message of this role.
    Misplaced,
}

pub fn encode(request: &Request, profile: &Profile) -> Result<Encoded, EncodeError> {
    let quirks = profile.quirks(&request.model);
    let scope = scope(request, profile);
    let mut encoded = match profile.codec {
        Codec::Messages => messages::encode(request, &quirks, &scope),
        Codec::Chat => chat::encode(request, &quirks, &scope),
        Codec::Responses => responses::encode(request, &quirks, &scope),
        Codec::Gemini => gemini::encode(request, &quirks, &scope),
    }?;
    encoded.lost.extend(unsigned(request, &quirks, &scope));
    encoded.lost.sort_by_key(|l| (l.message, l.block));
    encoded.lost.dedup_by_key(|l| (l.message, l.block));
    Ok(encoded)
}

/// Texts and calls that go without the signature they came with.
fn unsigned<'a>(
    request: &'a Request,
    quirks: &'a Quirks,
    scope: &'a Scope,
) -> impl Iterator<Item = Lost> + 'a {
    let signed = move |block: &Block| {
        matches!(block.content, Content::Text { .. } | Content::ToolCall(_))
            && replays(block, scope, quirks).is_none()
            && (block.native.as_ref())
                .is_some_and(|n| n.scope.codec == Codec::Gemini && gemini::is_signed(&n.json))
    };
    request.messages.iter().enumerate().flat_map(move |(message, m)| {
        let lost = move |block| Lost { message, block, what: Loss::Reasoning };
        (m.blocks.iter().enumerate()).filter(move |(_, b)| signed(b)).map(move |(n, _)| lost(n))
    })
}

/// Where the request goes, under the provider's address.
pub(crate) fn path(codec: Codec, model: &str) -> Result<String, EncodeError> {
    match codec {
        Codec::Messages => Ok("messages".into()),
        Codec::Chat => Ok("chat/completions".into()),
        Codec::Responses => Ok("responses".into()),
        Codec::Gemini => gemini::path(model),
    }
}

/// Where the answer's native blocks will belong.
pub fn scope(request: &Request, profile: &Profile) -> Scope {
    Scope { codec: profile.codec, provider: profile.provider.clone(), model: request.model.clone() }
}

/// Whether a block's native JSON may go to `scope`.
fn replays<'a>(block: &'a Block, scope: &Scope, quirks: &Quirks) -> Option<&'a Json> {
    let native = block.native.as_ref()?;
    let from = &native.scope;
    let model = from.model == scope.model || quirks.replay == Some(Replay::SameProvider);
    (from.codec == scope.codec && from.provider == scope.provider && model).then_some(&native.json)
}

/// The calls left out so far, so that their results are too. Ids need not be unique in a history:
/// a result belongs to the last message that called.
#[derive(Default)]
struct Unfinished<'a>(HashSet<&'a str>);
impl<'a> Unfinished<'a> {
    fn message(&mut self, message: &'a Message) {
        let calls = || {
            message.blocks.iter().filter_map(|b| match &b.content {
                Content::ToolCall(call) => Some(call),
                _ => None,
            })
        };
        self.0.extend(calls().filter(|c| c.ready().is_none()).map(|c| c.id.as_str()));
        for call in calls().filter_map(ToolCall::ready) {
            self.0.remove(call.id());
        }
    }
    fn has(&self, id: &str) -> bool {
        self.0.contains(id)
    }
}

/// A stream's events, one answer long.
#[derive(Debug)]
pub struct Decoder {
    codec: Inner,
    taken: usize,
    ended: bool,
}
#[derive(Debug)]
enum Inner {
    Messages(messages::Decoder),
    Chat(chat::Decoder),
    Responses(responses::Decoder),
    Gemini(gemini::Decoder),
}

impl Decoder {
    pub fn new(codec: Codec, quirks: &Quirks) -> Self {
        let codec = match codec {
            Codec::Messages => Inner::Messages(messages::Decoder::default()),
            Codec::Chat => Inner::Chat(chat::Decoder::new(quirks)),
            Codec::Responses => Inner::Responses(responses::Decoder::default()),
            Codec::Gemini => Inner::Gemini(gemini::Decoder::default()),
        };
        Self { codec, taken: 0, ended: false }
    }

    /// Adds what `event` means to `out`, also what comes before an error. After an error, or after
    /// [`Event::End`], nothing more does.
    pub fn event(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        if self.ended {
            return Ok(());
        }
        self.taken = self.taken.saturating_add(event.data.len());
        let done = match &mut self.codec {
            _ if self.taken > MAX_ANSWER => {
                Err(Error::protocol(format!("the answer is larger than {MAX_ANSWER} bytes")))
            }
            Inner::Messages(d) => d.event(event, out),
            Inner::Chat(d) => d.event(event, out),
            Inner::Responses(d) => d.event(event, out),
            Inner::Gemini(d) => d.event(event, out),
        };
        self.ended = done.is_err() || out.last() == Some(&Event::End);
        done
    }

    /// The stream is over. If the provider had not said that it was done, the answer is cut short.
    pub fn finish(&mut self, out: &mut Vec<Event>) -> Result<(), Error> {
        if std::mem::replace(&mut self.ended, true) {
            return Ok(());
        }
        match &mut self.codec {
            // Some servers close instead of saying `[DONE]`.
            Inner::Chat(d) if d.stopped() => {
                d.end(out);
                return Ok(());
            }
            Inner::Gemini(d) if d.stopped() => {
                d.end(out);
                return Ok(());
            }
            Inner::Messages(_) | Inner::Chat(_) | Inner::Responses(_) | Inner::Gemini(_) => {}
        }
        Err(Error::cut_short("the stream ended before the answer did"))
    }

    pub fn ended(&self) -> bool {
        self.ended
    }
}

/// An event's JSON, or what is wrong with it.
fn json<'a, T: Deserialize<'a>>(text: &'a str) -> Result<T, Error> {
    serde_json::from_str(text)
        .map_err(|e| Error::protocol(format!("an event of the stream cannot be read: {e}")))
}
