//! Amazon Bedrock `ConverseStream` (docs.aws.amazon.com/bedrock/latest: APIReference/
//! API_runtime_ConverseStream and the types it names, userguide/conversation-inference; read
//! 2026-11-25).
//!
//! A block is an object with one member, which says its kind. In a stream only a tool's use has a
//! beginning; of the rest, the first piece is it.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    EncodeError, Encoded, Loss, Lost, MAX_BLOCKS, Unfinished, json as parse, object::Object,
    replays,
};
use crate::{
    error::Error,
    eventstream,
    profile::{CachePoints, Effort, Quirks},
    sse,
    types::*,
};

/// What the codec writes; a profile's `body` cannot.
const OWN: [&str; 5] =
    ["inferenceConfig", "additionalModelRequestFields", "toolConfig", "system", "messages"];

/// Longest name of a tool, and longest id of a use of one.
const NAME: usize = 64;

/// Where the request goes. A model is named by an id or by an ARN, which are one part of the address:
/// what is not "unreserved" is escaped (smithy.io/2.0/spec/http-bindings.html#httplabel-trait).
pub(super) fn path(model: &str) -> Result<String, EncodeError> {
    let plain = |b: u8| b.is_ascii_alphanumeric() || b"._-:/".contains(&b);
    // A server that undoes the escapes sees no part that would lead elsewhere.
    let leads = model.split('/').any(|part| part.bytes().all(|b| b == b'.'));
    if leads || !model.bytes().all(plain) {
        return Err(EncodeError::BadModel(model.to_owned()));
    }
    let label = model.replace(':', "%3A").replace('/', "%2F");
    Ok(format!("model/{label}/converse-stream"))
}

/// What a model is also called: without the ARN it is in, and without the geography its requests may
/// go to (`us.`, `global.`; userguide/inference-profiles-support).
pub(crate) fn plain_names(model: &str) -> impl Iterator<Item = &str> {
    let id = model.rsplit('/').next().unwrap_or(model);
    [Some(id), id.split_once('.').map(|(_, rest)| rest)].into_iter().flatten()
}

pub(super) fn encode(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<Encoded, EncodeError> {
    let settings = &request.settings;
    let mut body = Object::default();
    let mut config = Object::default();
    if let Some(max) = settings.max_output {
        config.put("maxTokens", &max)?;
    }
    if !settings.stop.is_empty() {
        config.put("stopSequences", &settings.stop)?;
    }
    if !config.is_empty() {
        body.put("inferenceConfig", &config)?;
    }
    match (&settings.effort, quirks.effort) {
        (None, _) => {}
        // As printed in userguide/claude-messages-adaptive-thinking#converse.
        (Some(effort), Some(Effort::AdaptiveThinking)) => {
            let more = Object::default()
                .with("thinking", &json!({ "type": "adaptive" }))?
                .with("output_config", &json!({ "effort": effort }))?;
            body.put("additionalModelRequestFields", &more)?;
        }
        (Some(_), _) => return Err(EncodeError::Unsupported("effort")),
    }
    for (key, value) in &quirks.body {
        if OWN.contains(&key.as_str()) {
            return Err(EncodeError::Reserved(key.clone()));
        }
        body.put(key, value)?;
    }
    let point = point(settings.cache, quirks);

    // What is cached goes tools, system, messages; a point covers all before it.
    let has_system = !request.system.is_empty();
    let mut tools = Vec::with_capacity(request.tools.len() + 1);
    for tool in &request.tools {
        tools.push(tool_spec(tool)?);
    }
    if let (Some(point), false, false) = (&point, has_system, tools.is_empty())
        && quirks.cache_points == Some(CachePoints::All)
    {
        tools.push(point.clone());
    }
    if !tools.is_empty() {
        body.put("toolConfig", &Object::default().with("tools", &tools)?)?;
    }
    if has_system {
        let text = Object::default().with("text", &request.system)?;
        body.put("system", &[Some(text), point.clone()].into_iter().flatten().collect::<Vec<_>>())?;
    }

    let (mut messages, lost) = history(request, quirks, scope)?;
    if let Some(point) = &point {
        // Two, because a hit is looked for only some blocks back from a point.
        for m in messages.iter_mut().rev().filter(|m| m.role == Role::User).take(2) {
            m.content.push(point.clone());
        }
    }
    let mut written = Vec::with_capacity(messages.len());
    for m in &messages {
        let role = match m.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        written.push(Object::default().with("role", role)?.with("content", &m.content)?);
    }
    body.put("messages", &written)?;
    Ok(Encoded { body: body.json()?.get().as_bytes().to_vec(), lost })
}

fn tool_spec(tool: &Tool) -> Result<Object, EncodeError> {
    // API_runtime_ToolSpecification. Sent under another name, a call would not be one of this tool.
    if tool.name.is_empty() || tool.name.len() > NAME || !tool.name.bytes().all(of_name) {
        return Err(EncodeError::ToolName(tool.name.clone()));
    }
    let mut spec = Object::default().with("name", &tool.name)?;
    if !tool.description.is_empty() {
        spec.put("description", &tool.description)?;
    }
    spec.put("inputSchema", &Object::default().with("json", tool.schema.raw())?)?;
    Ok(Object::default().with("toolSpec", &spec)?)
}

/// A hint: for a model that takes no points, the service keeps what it keeps.
fn point(cache: Cache, quirks: &Quirks) -> Option<Object> {
    let point = match (cache, quirks.cache_points?) {
        (Cache::Off, _) => return None,
        (Cache::Long, CachePoints::All) => json!({ "type": "default", "ttl": "1h" }),
        // "For models that support only the 5-minute TTL, omit the ttl field."
        (Cache::Short | Cache::Long, _) => json!({ "type": "default" }),
    };
    Object::default().with("cachePoint", &point).ok()
}

struct Written {
    role: Role,
    content: Vec<Object>,
}

/// Messages of one role that follow each other are one: that a conversation has to alternate is not
/// on the pages that were read, but the service is remembered to refuse one that does not.
fn history(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<(Vec<Written>, Vec<Lost>), EncodeError> {
    let (mut messages, mut lost): (Vec<Written>, _) = (Vec::new(), Vec::new());
    let mut unfinished = Unfinished::default();
    for (n, message) in request.messages.iter().enumerate() {
        unfinished.message(message);
        let mut content = Vec::with_capacity(message.blocks.len());
        for (b, block) in message.blocks.iter().enumerate() {
            match write(block, message.role, &unfinished, quirks, scope)? {
                Ok(Some(piece)) => content.push(piece),
                Ok(None) => {}
                Err(what) => lost.push(Lost { message: n, block: b, what }),
            }
        }
        match messages.last_mut() {
            _ if content.is_empty() => {}
            Some(last) if last.role == message.role => last.content.append(&mut content),
            _ => messages.push(Written { role: message.role, content }),
        }
    }
    Ok((messages, lost))
}

fn write(
    block: &Block,
    role: Role,
    unfinished: &Unfinished<'_>,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<Result<Option<Object>, Loss>, EncodeError> {
    let call_is_unfinished =
        matches!(&block.content, Content::ToolCall(call) if call.ready().is_none());
    if let (Some(native), false) = (replays(block, scope, quirks), call_is_unfinished) {
        return Ok(Ok(Some(serde_json::from_str(native.get())?)));
    }
    Ok(match (&block.content, role) {
        (Content::Text { text }, _) if text.trim().is_empty() => Ok(None),
        (Content::Text { text }, _) => Ok(Some(Object::default().with("text", text)?)),
        (Content::ToolCall(call), Role::Assistant) => match call.ready() {
            None => Err(Loss::UnfinishedCall),
            Some(ready) => {
                let call = Object::default()
                    .with("toolUseId", &tool_id(ready.id()))?
                    .with("name", &tool_name(ready.name()))?
                    .with("input", ready.args())?;
                Ok(Some(Object::default().with("toolUse", &call)?))
            }
        },
        (Content::ToolResult { call, .. }, Role::User) if unfinished.has(call) => {
            Err(Loss::ResultOfUnfinishedCall)
        }
        (Content::ToolResult { call, text, failed }, Role::User) => {
            // Content is required, and a text that is blank may be refused as elsewhere.
            let text = if text.trim().is_empty() { "(empty)" } else { text };
            let mut result = Object::default()
                .with("toolUseId", &tool_id(call))?
                .with("content", &[Object::default().with("text", text)?])?;
            // Where there is no word for it, the text says that it failed.
            if *failed && quirks.result_status == Some(true) {
                result.put("status", "error")?;
            }
            Ok(Some(Object::default().with("toolResult", &result)?))
        }
        (Content::ToolCall(_), Role::User) | (Content::ToolResult { .. }, Role::Assistant) => {
            Err(Loss::Misplaced)
        }
        (Content::Reasoning { .. }, _) => Err(Loss::Reasoning),
        (Content::Unknown { .. }, _) => Err(Loss::Unknown),
    })
}

fn of_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')
}

/// Of a call in the history: names that other providers' models made up may be none here.
fn tool_name(name: &str) -> String {
    fit(name, of_name)
}
fn tool_id(id: &str) -> String {
    fit(id, |b| of_name(b) || matches!(b, b'.' | b':'))
}

/// `text` where it may stand. Else with other characters escaped, and told apart by a digest where it
/// is too long: an id has to stay the same from request to request, and another than its neighbour's.
fn fit(text: &str, may: impl Fn(u8) -> bool) -> String {
    if !text.is_empty() && text.len() <= NAME && text.bytes().all(&may) {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match u8::try_from(c) {
            Ok(b) if may(b) => out.push(c),
            _ => out.push_str(&format!("_{:x}", u32::from(c))),
        }
    }
    if out.is_empty() || out.len() > NAME {
        let digest = ring::digest::digest(&ring::digest::SHA256, text.as_bytes());
        let digest: String = digest.as_ref().iter().take(16).map(|b| format!("{b:02x}")).collect();
        // All of it is ASCII.
        out.truncate(NAME - digest.len());
        out.push_str(&digest);
    }
    out
}

/// What a message of the stream is: an event, or an account of what went wrong, which may come after
/// an answer has begun.
pub(super) fn event(message: eventstream::Message) -> Result<sse::Event, Error> {
    let text = |name| message.text(name).unwrap_or_default();
    match text(":message-type") {
        "event" => {
            let name = text(":event-type").to_owned();
            if name.is_empty() {
                return Err(Error::protocol("an event of the stream has no name"));
            }
            let data = String::from_utf8(message.payload)
                .map_err(|_| Error::protocol("an event of the stream is not UTF-8"))?;
            Ok(sse::Event { name, data })
        }
        "exception" => {
            let body = String::from_utf8_lossy(&message.payload);
            Err(Error::named(None, message.text(":exception-type"), &body))
        }
        "error" => Err(Error::named(
            None,
            message.text(":error-code"),
            &Value::from(text(":error-message")).to_string(),
        )),
        _ => Err(Error::protocol("a message of the stream is neither an event nor an error")),
    }
}

#[derive(Debug, Default)]
pub(super) struct Decoder {
    /// By the provider's index.
    blocks: HashMap<u64, Open>,
    closed: HashSet<u64>,
    started: usize,
    stopped: bool,
}

#[derive(Debug)]
struct Open {
    block: usize,
    kind: Kind,
    /// Text, reasoning or arguments.
    text: String,
    signature: Option<String>,
    /// Encrypted reasoning, in base64.
    redacted: String,
    /// It came in pieces that cannot be put together.
    torn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Text,
    Reasoning,
    ToolUse { id: String, name: String },
    Unknown,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    content_block_index: Option<u64>,
    start: Option<Object>,
    delta: Option<Object>,
    stop_reason: Option<String>,
    usage: Option<Value>,
}

impl Decoder {
    /// The provider gave a reason to stop. All that is to come is what was used.
    pub fn stopped(&self) -> bool {
        self.stopped
    }

    pub fn event(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        let name = event.name.as_str();
        match name {
            "messageStart" => {
                parse::<serde::de::IgnoredAny>(&event.data)?;
                out.push(Event::Started { id: None, model: None });
            }
            "contentBlockStart" | "contentBlockDelta" | "contentBlockStop" if self.stopped => {
                return Err(Error::protocol("a block after the end of its message"));
            }
            "contentBlockStart" => {
                let wire: Wire = parse(&event.data)?;
                let (Some(index), Some(start)) = (wire.content_block_index, wire.start) else {
                    return Err(incomplete(name));
                };
                self.start(index, &start, out)?;
            }
            "contentBlockDelta" => {
                let wire: Wire = parse(&event.data)?;
                let (Some(index), Some(delta)) = (wire.content_block_index, wire.delta) else {
                    return Err(incomplete(name));
                };
                self.delta(index, &delta, out)?;
            }
            "contentBlockStop" => {
                let wire: Wire = parse(&event.data)?;
                let index = wire.content_block_index.ok_or_else(|| incomplete(name))?;
                match self.blocks.remove(&index) {
                    Some(open) => self.close(index, open, out)?,
                    // One that nothing was said in has not begun here.
                    None if !self.closed.contains(&index) => {}
                    None => return Err(Error::protocol("the end of a block that is not open")),
                }
            }
            "messageStop" => {
                let wire: Wire = parse(&event.data)?;
                let reason = wire.stop_reason.ok_or_else(|| incomplete(name))?;
                // A text is whole when its message is (the stream printed in
                // userguide/conversation-inference does not end it); a call needs its own end.
                let mut texts: Vec<(usize, u64)> = (self.blocks.iter())
                    .filter(|(_, b)| b.kind == Kind::Text)
                    .map(|(index, b)| (b.block, *index))
                    .collect();
                texts.sort_unstable();
                for (_, index) in texts {
                    if let Some(open) = self.blocks.remove(&index) {
                        self.close(index, open, out)?;
                    }
                }
                self.stopped = true;
                out.push(Event::Stop(stop(reason)));
            }
            "metadata" => {
                let wire: Wire = parse(&event.data)?;
                if let Some(told) = wire.usage.filter(Value::is_object) {
                    out.push(Event::Usage(count(&told)));
                }
                if self.stopped {
                    out.push(Event::End);
                }
            }
            // What is newer than this build.
            _ => {}
        }
        Ok(())
    }

    fn start(&mut self, index: u64, start: &Object, out: &mut Vec<Event>) -> Result<(), Error> {
        let call = start.read::<Object>("toolUse");
        // With a type it is one the provider runs itself.
        match call.filter(|c| c.get("type").is_none()) {
            Some(call) => {
                let string = |key| call.read::<String>(key).unwrap_or_default();
                let (id, name) = (string("toolUseId"), string("name"));
                let start = Start::ToolCall { id: id.clone(), name: name.clone() };
                self.open(index, Kind::ToolUse { id, name }, start, out)?;
            }
            None => {
                let unknown = Start::Unknown { name: only(start) };
                let block = self.open(index, Kind::Unknown, unknown, out)?;
                out.push(Event::Delta { block, delta: Delta::Other(kept(start)?) });
            }
        }
        Ok(())
    }

    fn open(
        &mut self,
        index: u64,
        kind: Kind,
        start: Start,
        out: &mut Vec<Event>,
    ) -> Result<usize, Error> {
        if self.closed.contains(&index) || self.blocks.contains_key(&index) {
            return Err(Error::protocol("a block that had begun before"));
        }
        if self.started == MAX_BLOCKS {
            return Err(Error::protocol(format!("the answer has more than {MAX_BLOCKS} blocks")));
        }
        let block = self.started;
        self.started += 1;
        out.push(Event::BlockStart { block, start });
        let open = Open {
            block,
            kind,
            text: String::new(),
            signature: None,
            redacted: String::new(),
            torn: false,
        };
        self.blocks.insert(index, open);
        Ok(block)
    }

    fn delta(&mut self, index: u64, delta: &Object, out: &mut Vec<Event>) -> Result<(), Error> {
        if !self.blocks.contains_key(&index) {
            let (kind, start) = if let Some(text) = delta.read::<String>("text") {
                if text.is_empty() {
                    return Ok(());
                }
                (Kind::Text, Start::Text)
            } else if delta.get("reasoningContent").is_some() {
                (Kind::Reasoning, Start::Reasoning)
            } else {
                (Kind::Unknown, Start::Unknown { name: only(delta) })
            };
            self.open(index, kind, start, out)?;
        }
        if let Some(open) = self.blocks.get_mut(&index)
            && let Some(delta) = open.fold(delta)?
        {
            out.push(Event::Delta { block: open.block, delta });
        }
        Ok(())
    }

    fn close(&mut self, index: u64, open: Open, out: &mut Vec<Event>) -> Result<(), Error> {
        self.closed.insert(index);
        let block = open.block;
        // It is whole and nothing came: a call without arguments. Nothing says how else it is sent.
        if matches!(open.kind, Kind::ToolUse { .. }) && open.text.is_empty() && !open.torn {
            out.push(Event::Delta { block, delta: Delta::Args("{}".into()) });
        }
        let broken = (matches!(open.kind, Kind::ToolUse { .. }) && open.torn)
            .then(|| "a piece of the arguments cannot be read".to_owned());
        let native = open.native().map_err(unwritable)?;
        out.push(Event::BlockStop { block, native, broken });
        Ok(())
    }

    /// Everything that is open stays as it is: it did not end.
    pub fn end(&mut self, out: &mut Vec<Event>) {
        out.push(Event::End);
    }
}

impl Open {
    /// What to pass on, if anything.
    fn fold(&mut self, delta: &Object) -> Result<Option<Delta>, Error> {
        match self.kind {
            Kind::Text => {
                if let Some(text) = delta.read::<String>("text") {
                    self.text.push_str(&text);
                    return Ok(Some(Delta::Text(text)));
                }
            }
            Kind::Reasoning => {
                if let Some(piece) = delta.read::<Object>("reasoningContent") {
                    let string = |key| piece.read::<String>(key);
                    if let Some(text) = string("text") {
                        self.text.push_str(&text);
                        return Ok(Some(Delta::Reasoning(text)));
                    }
                    if let Some(signature) = string("signature") {
                        self.signature.get_or_insert_default().push_str(&signature);
                        return Ok(None);
                    }
                    if let Some(bytes) = string("redactedContent") {
                        // Base64 goes on only where it has not ended.
                        self.torn |=
                            self.redacted.ends_with('=') || !self.redacted.len().is_multiple_of(4);
                        self.redacted.push_str(&bytes);
                        return Ok(None);
                    }
                }
            }
            Kind::ToolUse { .. } => {
                if let Some(piece) = delta.read::<Object>("toolUse")
                    && let Some(text) = piece.read::<String>("input")
                {
                    self.text.push_str(&text);
                    return Ok(Some(Delta::Args(text)));
                }
                self.torn = true;
            }
            Kind::Unknown => {}
        }
        kept(delta).map(|json| Some(Delta::Other(json)))
    }

    /// The block as the API would have sent it whole.
    fn native(self) -> serde_json::Result<Option<Json>> {
        let block = match self.kind {
            // A text that is blank is refused.
            Kind::Text if self.text.trim().is_empty() => return Ok(None),
            Kind::Text => Object::default().with("text", &self.text)?,
            // "Include the text and its signature unmodified", also where the text is empty
            // (userguide/claude-messages-thinking-block-binding). What nothing proves is not sent: the
            // example for a model that does not sign takes it out (userguide/model-parameters-deepseek).
            Kind::Reasoning => {
                let reasoning = match (self.signature, self.redacted.is_empty()) {
                    (Some(signature), true) => {
                        let text = Object::default()
                            .with("text", &self.text)?
                            .with("signature", &signature)?;
                        Object::default().with("reasoningText", &text)?
                    }
                    (None, false) if self.text.is_empty() && !self.torn => {
                        Object::default().with("redactedContent", &self.redacted)?
                    }
                    _ => return Ok(None),
                };
                Object::default().with("reasoningContent", &reasoning)?
            }
            Kind::ToolUse { id, name } => {
                let input = if self.text.is_empty() { "{}".to_owned() } else { self.text };
                let Ok(input) = Json::parse(input) else { return Ok(None) };
                let call = Object::default()
                    .with("toolUseId", &id)?
                    .with("name", &name)?
                    .with("input", input.raw())?;
                Object::default().with("toolUse", &call)?
            }
            Kind::Unknown => return Ok(None),
        };
        block.json().map(Some)
    }
}

/// The member that says what a union is.
fn only(union: &Object) -> String {
    union.members().next().map(|(k, _)| k.to_owned()).unwrap_or_default()
}
fn kept(object: &Object) -> Result<Json, Error> {
    object.json().map_err(unwritable)
}

fn stop(reason: String) -> Stop {
    match reason.as_str() {
        "end_turn" => Stop::EndTurn,
        "tool_use" => Stop::ToolUse,
        "max_tokens" => Stop::MaxOutput,
        "model_context_window_exceeded" => Stop::ContextFull,
        "stop_sequence" => Stop::Sequence(None),
        "guardrail_intervened" | "content_filtered" => Stop::Refusal(Some(reason)),
        // Also `malformed_model_output` and `malformed_tool_use`.
        _ => Stop::Other(reason),
    }
}

/// `inputTokens` is the prompt, or with a cache the part of it that was neither read nor written
/// (userguide/prompt-caching#prompt-caching-converse). What was spent on reasoning is not told.
fn count(told: &Value) -> Usage {
    let count = |key: &str| told.get(key).and_then(Value::as_u64);
    let input = count("inputTokens");
    let (read, write) = (count("cacheReadInputTokens"), count("cacheWriteInputTokens"));
    let cached = read.is_some() || write.is_some();
    let long = told.get("cacheDetails").and_then(Value::as_array).and_then(|all| {
        all.iter()
            .filter(|d| d.get("ttl").and_then(Value::as_str) == Some("1h"))
            .try_fold(0u64, |sum, d| sum.checked_add(d.get("inputTokens")?.as_u64()?))
    });
    Usage {
        prompt: match (input, read, write) {
            (Some(a), Some(b), Some(c)) => a.checked_add(b).and_then(|ab| ab.checked_add(c)),
            (input, None, None) => input,
            _ => None,
        },
        uncached: input.filter(|_| cached),
        cache_read: read,
        cache_write: write,
        cache_write_long: long,
        output: count("outputTokens"),
        ..Usage::default()
    }
}

fn incomplete(name: &str) -> Error {
    Error::protocol(format!("a {name} without what it is about"))
}
fn unwritable(e: serde_json::Error) -> Error {
    Error::protocol(format!("a block cannot be kept: {e}"))
}
