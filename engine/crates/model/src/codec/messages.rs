//! Anthropic Messages (platform.claude.com/docs/en/build-with-claude/streaming, read 2026-11-25).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    EncodeError, Encoded, Loss, Lost, MAX_BLOCKS, Unfinished, json as parse, name,
    object::{Array, Object},
    replays,
};
use crate::{error::Error, profile::Effort, profile::Quirks, profile::Thinking, sse, types::*};

/// What the codec writes; a profile's `body` cannot.
const OWN: [&str; 11] = [
    "thinking",
    "speed",
    "model",
    "max_tokens",
    "stream",
    "stop_sequences",
    "output_config",
    "tools",
    "system",
    "messages",
    "cache_control",
];

pub(super) fn encode(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<Encoded, EncodeError> {
    let settings = &request.settings;
    let mut body = Object::default().with("model", &request.model)?;
    let max = settings.max_output.ok_or(EncodeError::Missing("the limit on output"))?;
    body.put("max_tokens", &max)?;
    body.put("stream", &true)?;
    if !settings.stop.is_empty() {
        body.put("stop_sequences", &settings.stop)?;
    }
    match (&settings.effort, quirks.effort) {
        (None, _) => {}
        (Some(effort), Some(Effort::OutputConfig)) => {
            body.put("output_config", &json!({ "effort": effort }))?;
        }
        (Some(_), _) => return Err(EncodeError::Unsupported("effort")),
    }
    match (settings.thinking, quirks.thinking) {
        (None, _) => {}
        (Some(on), Some(Thinking::Adaptive)) => {
            body.put("thinking", &json!({ "type": if on { "adaptive" } else { "disabled" } }))?;
        }
        (Some(_), None) => return Err(EncodeError::Unsupported("whether to think")),
    }
    if let Some(s) = &settings.speed {
        if !quirks.speeds.as_ref().is_some_and(|all| all.contains(s)) {
            return Err(EncodeError::Unsupported("the speed"));
        }
        body.put("speed", s)?;
    }
    for (key, value) in &quirks.body {
        if OWN.contains(&key.as_str()) {
            return Err(EncodeError::Reserved(key.clone()));
        }
        body.put(key, value)?;
    }
    let marker = marker(settings.cache, quirks);

    // What is cached goes tools, system, messages; a marker covers all before it.
    let mut tools = Vec::with_capacity(request.tools.len());
    for tool in &request.tools {
        let mut t = Object::default()
            .with("name", &tool.name)?
            .with("description", &tool.description)?
            .with("input_schema", tool.schema.raw())?;
        if quirks.eager_tool_args == Some(true) {
            t.put("eager_input_streaming", &true)?;
        }
        tools.push(t);
    }
    let has_system = !request.system.is_empty();
    if let (Some(marker), Some(last), false) = (&marker, tools.last_mut(), has_system) {
        last.put("cache_control", marker)?;
    }
    if !tools.is_empty() {
        body.put("tools", &tools)?;
    }
    if has_system {
        let mut system = Object::default().with("type", "text")?.with("text", &request.system)?;
        if let Some(marker) = &marker {
            system.put("cache_control", marker)?;
        }
        body.put("system", &[system])?;
    }

    let (mut messages, lost) = history(request, quirks, scope)?;
    if let Some(marker) = &marker {
        // Two, because a hit is looked for only some blocks back from a marker.
        let users = messages.iter_mut().rev().filter(|m| m.role == "user");
        for piece in users.filter_map(|m| m.content.last_mut()).take(2) {
            piece.put("cache_control", marker)?;
        }
    }
    let mut written = Vec::with_capacity(messages.len());
    for m in &messages {
        written.push(Object::default().with("role", m.role)?.with("content", &m.content)?);
    }
    body.put("messages", &written)?;
    Ok(Encoded { body: body.json()?.get().as_bytes().to_vec(), lost })
}

/// A hint: a provider that has no markers keeps what it keeps.
fn marker(cache: Cache, quirks: &Quirks) -> Option<Value> {
    match cache {
        _ if quirks.cache_markers != Some(true) => None,
        Cache::Off => None,
        Cache::Short => Some(json!({ "type": "ephemeral" })),
        Cache::Long => Some(json!({ "type": "ephemeral", "ttl": "1h" })),
    }
}

struct Written {
    role: &'static str,
    content: Vec<Object>,
}

fn history(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<(Vec<Written>, Vec<Lost>), EncodeError> {
    let (mut messages, mut lost) = (Vec::new(), Vec::new());
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
        // A message without content is refused.
        if !content.is_empty() {
            let role = match message.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            messages.push(Written { role, content });
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
        // The API refuses a text of nothing but space.
        (Content::Text { text }, _) if text.trim().is_empty() => Ok(None),
        (Content::Text { text }, _) => {
            Ok(Some(Object::default().with("type", "text")?.with("text", text)?))
        }
        (Content::ToolCall(call), Role::Assistant) => match call.ready() {
            None => Err(Loss::UnfinishedCall),
            Some(ready) => Ok(Some(
                Object::default()
                    .with("type", "tool_use")?
                    .with("id", &tool_id(ready.id()))?
                    .with("name", ready.name())?
                    .with("input", ready.args())?,
            )),
        },
        (Content::ToolResult { call, .. }, Role::User) if unfinished.has(call) => {
            Err(Loss::ResultOfUnfinishedCall)
        }
        (Content::ToolResult { call, text, failed }, Role::User) => {
            let mut o = Object::default()
                .with("type", "tool_result")?
                .with("tool_use_id", &tool_id(call))?;
            if !text.is_empty() {
                o.put("content", text)?;
            }
            if *failed {
                o.put("is_error", &true)?;
            }
            Ok(Some(o))
        }
        (Content::ToolCall(_), Role::User) | (Content::ToolResult { .. }, Role::Assistant) => {
            Err(Loss::Misplaced)
        }
        (Content::Reasoning { .. }, _) => Err(Loss::Reasoning),
        (Content::Unknown { .. }, _) => Err(Loss::Unknown),
    })
}

/// Other providers' ids have characters (`functions.bash:0`) that this API refuses.
fn tool_id(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => out.push(c),
            c => out.push_str(&format!("_{:x}", u32::from(c))),
        }
    }
    out
}

#[derive(Debug, Default)]
pub(super) struct Decoder {
    /// By the provider's index.
    blocks: HashMap<u64, Open>,
    usage: Usage,
    started: usize,
}

#[derive(Debug)]
struct Open {
    block: usize,
    kind: Kind,
    /// As it began; its text, signature and arguments are put in when it ends.
    object: Object,
    text: String,
    citations: Vec<Json>,
    /// Something could not be folded in.
    odd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Thinking,
    /// Comes whole.
    Redacted,
    ToolUse,
    Unknown,
}

#[derive(Deserialize)]
struct Typed {
    r#type: Option<String>,
}
/// Of the events that are known.
#[derive(Deserialize)]
struct Wire {
    index: Option<u64>,
    message: Option<MessageStart>,
    content_block: Option<Object>,
    delta: Option<Object>,
    usage: Option<Value>,
}
#[derive(Deserialize)]
struct MessageStart {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Value>,
}

impl Decoder {
    pub fn event(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        let kind = parse::<Typed>(&event.data)?.r#type.unwrap_or_else(|| event.name.clone());
        match kind.as_str() {
            "message_stop" => out.push(Event::End),
            "error" => return Err(Error::provider(None, &event.data)),
            "message_start"
            | "content_block_start"
            | "content_block_delta"
            | "content_block_stop"
            | "message_delta" => {
                self.known(&kind, parse(&event.data)?, out)?;
            }
            // `ping`, and what is newer than this build: the versioning policy allows it.
            _ => {}
        }
        Ok(())
    }

    fn known(&mut self, kind: &str, wire: Wire, out: &mut Vec<Event>) -> Result<(), Error> {
        match kind {
            "message_start" => {
                let Some(m) = wire.message else { return Err(incomplete("message_start")) };
                out.push(Event::Started { id: m.id, model: m.model });
                self.usage(m.usage.as_ref(), out);
            }
            "content_block_start" => {
                let (Some(index), Some(object)) = (wire.index, wire.content_block) else {
                    return Err(incomplete("content_block_start"));
                };
                self.start(index, object, out)?;
            }
            "content_block_delta" => {
                let (Some(index), Some(delta)) = (wire.index, wire.delta) else {
                    return Err(incomplete("content_block_delta"));
                };
                let Some(open) = self.blocks.get_mut(&index) else {
                    return Err(Error::protocol("a delta for a block that is not open"));
                };
                if let Some(delta) = open.fold(delta).map_err(unwritable)? {
                    out.push(Event::Delta { block: open.block, delta });
                }
            }
            "content_block_stop" => {
                let open = wire.index.and_then(|i| self.blocks.remove(&i));
                let Some(open) = open else {
                    return Err(Error::protocol("the end of a block that is not open"));
                };
                let block = open.block;
                // Without a piece of it, the input is what the block began with.
                if let (Kind::ToolUse, true, Some(input)) =
                    (open.kind, open.text.is_empty(), open.object.get("input"))
                {
                    let delta = Delta::Args(input.get().to_owned());
                    out.push(Event::Delta { block, delta });
                }
                let native = open.native().map_err(unwritable)?;
                out.push(Event::BlockStop { block, native, broken: None });
            }
            // message_delta
            _ => {
                self.usage(wire.usage.as_ref(), out);
                if let Some(stop) = wire.delta.as_ref().and_then(stop) {
                    out.push(Event::Stop(stop));
                }
            }
        }
        Ok(())
    }

    fn start(&mut self, index: u64, object: Object, out: &mut Vec<Event>) -> Result<(), Error> {
        if self.started == MAX_BLOCKS {
            return Err(Error::protocol(format!("the answer has more than {MAX_BLOCKS} blocks")));
        }
        let block = self.started;
        self.started += 1;
        let name: String = object.read("type").unwrap_or_default();
        let string = |key| object.read::<String>(key).unwrap_or_default();
        let (kind, start, text) = match name.as_str() {
            "text" => (Kind::Text, Start::Text, string("text")),
            "thinking" => (Kind::Thinking, Start::Reasoning, string("thinking")),
            // Encrypted: it has nothing to show and comes whole.
            "redacted_thinking" => (Kind::Redacted, Start::Reasoning, String::new()),
            "tool_use" => {
                let start = Start::ToolCall { id: string("id"), name: string("name") };
                (Kind::ToolUse, start, String::new())
            }
            // Also `server_tool_use`, which the provider runs.
            _ => (Kind::Unknown, Start::Unknown { name }, String::new()),
        };
        out.push(Event::BlockStart { block, start });
        let delta = match kind {
            Kind::Text => Some(Delta::Text(text.clone())),
            Kind::Thinking => Some(Delta::Reasoning(text.clone())),
            _ => None,
        };
        if let Some(delta) = delta.filter(|_| !text.is_empty()) {
            out.push(Event::Delta { block, delta });
        }
        let open = Open { block, kind, object, text, citations: Vec::new(), odd: false };
        if self.blocks.insert(index, open).is_some() {
            return Err(Error::protocol("a block that began while it was open"));
        }
        Ok(())
    }

    /// Counts are cumulative, and each event has only some.
    fn usage(&mut self, told: Option<&Value>, out: &mut Vec<Event>) {
        let Some(told) = told.filter(|t| t.is_object()) else { return };
        let count = |path: &str| told.pointer(path).and_then(Value::as_u64);
        let u = &mut self.usage;
        u.uncached = count("/input_tokens").or(u.uncached);
        u.cache_read = count("/cache_read_input_tokens").or(u.cache_read);
        u.cache_write = count("/cache_creation_input_tokens").or(u.cache_write);
        u.cache_write_long =
            count("/cache_creation/ephemeral_1h_input_tokens").or(u.cache_write_long);
        u.output = count("/output_tokens").or(u.output);
        u.reasoning = count("/output_tokens_details/thinking_tokens").or(u.reasoning);
        // build-with-claude/fast-mode, read 2026-11-26.
        u.speed = told.get("speed").and_then(name).or(u.speed.take());
        u.prompt = match (u.uncached, u.cache_read, u.cache_write) {
            (Some(a), Some(b), Some(c)) => a.checked_add(b).and_then(|ab| ab.checked_add(c)),
            _ => None,
        };
        out.push(Event::Usage(u.clone()));
    }
}

impl Open {
    /// What to pass on, if anything.
    fn fold(&mut self, delta: Object) -> serde_json::Result<Option<Delta>> {
        let kind: String = delta.read("type").unwrap_or_default();
        let text = |key| delta.read::<String>(key);
        match (kind.as_str(), self.kind) {
            ("text_delta", Kind::Text) => {
                if let Some(t) = text("text") {
                    self.text.push_str(&t);
                    return Ok(Some(Delta::Text(t)));
                }
            }
            ("thinking_delta", Kind::Thinking) => {
                if let Some(t) = text("thinking") {
                    self.text.push_str(&t);
                    return Ok(Some(Delta::Reasoning(t)));
                }
            }
            ("signature_delta", Kind::Thinking) => {
                if let Some(signature) = delta.get("signature") {
                    self.object.set("signature", signature.clone());
                    return Ok(None);
                }
            }
            ("citations_delta", Kind::Text) => {
                if let Some(citation) = delta.get("citation") {
                    self.citations.push(citation.clone());
                    return Ok(None);
                }
            }
            ("input_json_delta", Kind::ToolUse | Kind::Unknown) => {
                if let Some(t) = text("partial_json") {
                    self.text.push_str(&t);
                    return Ok(Some(Delta::Args(t)));
                }
            }
            _ => {}
        }
        self.odd = true;
        delta.json().map(|json| Some(Delta::Other(json)))
    }

    /// The block as the API would have sent it whole. A kind that is not known cannot be put
    /// together from pieces that are not known either; of a known kind, what is known is sent back
    /// rather than nothing, which would break the turn for certain.
    fn native(self) -> serde_json::Result<Option<Json>> {
        let mut object = self.object;
        match self.kind {
            Kind::Text => object.put("text", &self.text)?,
            Kind::Thinking => object.put("thinking", &self.text)?,
            Kind::Unknown if self.odd => return Ok(None),
            Kind::Redacted => {}
            Kind::ToolUse | Kind::Unknown => {
                if !self.text.is_empty() {
                    match Json::parse(self.text) {
                        Ok(input) => object.set("input", input),
                        Err(_) => return Ok(None),
                    }
                }
            }
        }
        if !self.citations.is_empty() {
            object.put("citations", &Array(&self.citations))?;
        }
        object.json().map(Some)
    }
}

fn stop(delta: &Object) -> Option<Stop> {
    let reason: String = delta.read("stop_reason")?;
    Some(match reason.as_str() {
        "end_turn" => Stop::EndTurn,
        "tool_use" => Stop::ToolUse,
        "max_tokens" => Stop::MaxOutput,
        "model_context_window_exceeded" => Stop::ContextFull,
        "stop_sequence" => Stop::Sequence(delta.read("stop_sequence")),
        "pause_turn" => Stop::Pause,
        "refusal" => Stop::Refusal(
            delta
                .read::<Value>("stop_details")
                .and_then(|d| d.get("category")?.as_str().map(str::to_owned)),
        ),
        _ => Stop::Other(reason),
    })
}

fn incomplete(name: &str) -> Error {
    Error::protocol(format!("a {name} without what it is about"))
}
fn unwritable(e: serde_json::Error) -> Error {
    Error::protocol(format!("a block cannot be kept: {e}"))
}
