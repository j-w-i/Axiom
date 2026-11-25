//! OpenAI Responses, stateless (developers.openai.com/api/docs, read 2026-11-25): nothing is stored
//! there, and an answer's items go back as they came, encrypted reasoning and `phase` with them
//! (guides/reasoning, guides/conversation-state).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};

use super::{
    EncodeError, Encoded, Loss, Lost, MAX_BLOCKS, Unfinished, json as parse, name,
    object::{Array, Object},
    replays,
};
use crate::{error::Error, profile::Effort, profile::Quirks, sse, types::*};

/// What the codec writes, or what would make the provider keep the conversation or go on with one
/// it has kept; a profile's `body` cannot.
const OWN: [&str; 11] = [
    "model",
    "store",
    "stream",
    "max_output_tokens",
    "reasoning",
    "tools",
    "input",
    "instructions",
    "previous_response_id",
    "conversation",
    "background",
];

pub(super) fn encode(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<Encoded, EncodeError> {
    let settings = &request.settings;
    let mut body = Object::default()
        .with("model", &request.model)?
        .with("store", &false)?
        .with("stream", &true)?;
    if let Some(max) = settings.max_output.filter(|_| quirks.no_limit != Some(true)) {
        body.put("max_output_tokens", &max)?;
    }
    if !settings.stop.is_empty() {
        return Err(EncodeError::Unsupported("a stop sequence"));
    }
    match (&settings.effort, quirks.effort) {
        (None, _) => {}
        (Some(effort), Some(Effort::Reasoning)) => {
            body.put("reasoning", &json!({ "effort": effort }))?;
        }
        (Some(_), _) => return Err(EncodeError::Unsupported("effort")),
    }
    for (key, value) in &quirks.body {
        if OWN.contains(&key.as_str()) {
            return Err(EncodeError::Reserved(key.clone()));
        }
        body.put(key, value)?;
    }
    if !request.tools.is_empty() {
        let mut tools = Vec::with_capacity(request.tools.len());
        for tool in &request.tools {
            // Unless it is told not to, the API rewrites a schema into one it can enforce, where
            // every property is required (guides/function-calling, strict mode).
            tools.push(
                Object::default()
                    .with("type", "function")?
                    .with("name", &tool.name)?
                    .with("description", &tool.description)?
                    .with("parameters", tool.schema.raw())?
                    .with("strict", &false)?,
            );
        }
        body.put("tools", &tools)?;
    }
    let (mut input, mut lost) = (Vec::new(), Vec::new());
    if !request.system.is_empty() {
        // Not `instructions`: newer models get those after the input, so they are never part of
        // the cached prefix (guides/keep-prefix-unchanged).
        input.push(message("developer", &[&request.system])?);
    }
    let mut unfinished = Unfinished::default();
    for (n, message) in request.messages.iter().enumerate() {
        unfinished.message(message);
        let mut lose = |block, what| lost.push(Lost { message: n, block, what });
        match message.role {
            Role::User => user(message, &unfinished, &mut input, &mut lose)?,
            Role::Assistant => assistant(message, quirks, scope, &mut input, &mut lose)?,
        }
    }
    body.put("input", &Array(&input))?;
    Ok(Encoded { body: body.json()?.get().as_bytes().to_vec(), lost })
}

fn message(role: &str, texts: &[&str]) -> serde_json::Result<Json> {
    let mut content = Vec::with_capacity(texts.len());
    for text in texts {
        content.push(Object::default().with("type", "input_text")?.with("text", text)?);
    }
    Object::default().with("role", role)?.with("content", &content)?.json()
}

fn user(
    from: &Message,
    unfinished: &Unfinished<'_>,
    out: &mut Vec<Json>,
    lose: &mut impl FnMut(usize, Loss),
) -> Result<(), EncodeError> {
    let mut texts = Vec::new();
    for (n, block) in from.blocks.iter().enumerate() {
        match &block.content {
            Content::Text { text } if text.is_empty() => {}
            Content::Text { text } => texts.push(text.as_str()),
            Content::ToolResult { call, .. } if unfinished.has(call) => {
                lose(n, Loss::ResultOfUnfinishedCall);
            }
            // The protocol has no way to say that a tool failed; its text does.
            Content::ToolResult { call, text, failed: _ } => out.push(
                Object::default()
                    .with("type", "function_call_output")?
                    .with("call_id", &call_id(call))?
                    .with("output", text)?
                    .json()?,
            ),
            Content::ToolCall(_) | Content::Reasoning { .. } => lose(n, Loss::Misplaced),
            Content::Unknown { .. } => lose(n, Loss::Unknown),
        }
    }
    // After the results, which belong to the calls before.
    if !texts.is_empty() {
        out.push(message("user", &texts)?);
    }
    Ok(())
}

fn assistant(
    from: &Message,
    quirks: &Quirks,
    scope: &Scope,
    out: &mut Vec<Json>,
    lose: &mut impl FnMut(usize, Loss),
) -> Result<(), EncodeError> {
    // Reasoning is sent with what came of it. Without (the call it led to never became whole) it is
    // left out, which the API allows; that it would take it alone is not documented.
    let mut waiting: Vec<(usize, Json)> = Vec::new();
    for (n, block) in from.blocks.iter().enumerate() {
        let item = match (&block.content, replays(block, scope, quirks)) {
            (Content::ToolCall(call), native) => match (call.ready(), native) {
                (None, _) => {
                    lose(n, Loss::UnfinishedCall);
                    continue;
                }
                (Some(_), Some(native)) => native.clone(),
                (Some(ready), None) => Object::default()
                    .with("type", "function_call")?
                    .with("call_id", &call_id(ready.id()))?
                    .with("name", ready.name())?
                    .with("arguments", &call.raw)?
                    .json()?,
            },
            (Content::ToolResult { .. }, _) => {
                lose(n, Loss::Misplaced);
                continue;
            }
            (Content::Reasoning { .. }, Some(native)) => {
                waiting.push((n, native.clone()));
                continue;
            }
            (_, Some(native)) => native.clone(),
            (Content::Text { text }, None) if text.is_empty() => continue,
            (Content::Text { text }, None) => {
                Object::default().with("role", "assistant")?.with("content", text)?.json()?
            }
            (Content::Reasoning { .. }, None) => {
                lose(n, Loss::Reasoning);
                continue;
            }
            (Content::Unknown { .. }, None) => {
                lose(n, Loss::Unknown);
                continue;
            }
        };
        out.extend(waiting.drain(..).map(|(_, reasoning)| reasoning));
        out.push(item);
    }
    for (n, _) in waiting {
        lose(n, Loss::Reasoning);
    }
    Ok(())
}

/// Most characters of a `call_id` (reference, `function_call_output`).
const CALL_ID: usize = 64;

/// Another provider's id may be longer than is taken. It is cut, and told apart by what it was.
fn call_id(id: &str) -> String {
    if id.chars().count() <= CALL_ID {
        return id.to_owned();
    }
    // FNV-1a, 64 bits.
    let hash = id
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3));
    let kept: String = id.chars().take(CALL_ID - 17).collect();
    format!("{kept}_{hash:016x}")
}

#[derive(Debug, Default)]
pub(super) struct Decoder {
    /// By the provider's index.
    items: HashMap<u64, Open>,
    begun: usize,
    /// What the answer has in it, which decides what its end means.
    called: bool,
    refused: bool,
}

#[derive(Debug)]
struct Open {
    block: usize,
    kind: Kind,
    name: String,
    /// `call_id` and `name`, of a call.
    call: (String, String),
    /// What was passed on of it.
    text: String,
    /// The part that came last.
    part: Option<(&'static str, u64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Message,
    Reasoning,
    Call,
    Unknown,
}

#[derive(Deserialize)]
struct Typed {
    r#type: Option<String>,
}
/// Of the events that are known.
#[derive(Deserialize)]
struct Wire {
    output_index: Option<u64>,
    summary_index: Option<u64>,
    content_index: Option<u64>,
    item: Option<Box<RawValue>>,
    delta: Option<Box<RawValue>>,
    response: Option<Response>,
}
#[derive(Deserialize)]
struct Response {
    id: Option<String>,
    model: Option<String>,
    error: Option<Box<RawValue>>,
    incomplete_details: Option<Value>,
    usage: Option<Value>,
    service_tier: Option<Value>,
}
/// Of any event.
#[derive(Deserialize)]
struct About {
    output_index: Option<u64>,
}

/// What these say is said again by the item when it is done.
const REPEATED: [&str; 10] = [
    "response.content_part.added",
    "response.content_part.done",
    "response.output_text.done",
    "response.output_text.annotation.added",
    "response.refusal.done",
    "response.function_call_arguments.done",
    "response.reasoning_summary_part.added",
    "response.reasoning_summary_part.done",
    "response.reasoning_summary_text.done",
    "response.reasoning_text.done",
];

impl Decoder {
    pub fn event(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        let kind = parse::<Typed>(&event.data)?.r#type.unwrap_or_else(|| event.name.clone());
        match kind.as_str() {
            "error" => Err(Error::provider(None, &event.data)),
            "response.created"
            | "response.output_item.added"
            | "response.output_item.done"
            | "response.output_text.delta"
            | "response.refusal.delta"
            | "response.reasoning_summary_text.delta"
            | "response.reasoning_text.delta"
            | "response.function_call_arguments.delta"
            | "response.completed"
            | "response.incomplete"
            | "response.failed" => self.known(&kind, parse(&event.data)?, event, out),
            kind if REPEATED.contains(&kind) => Ok(()),
            _ => self.other(event, out),
        }
    }

    fn known(
        &mut self,
        kind: &str,
        wire: Wire,
        event: &sse::Event,
        out: &mut Vec<Event>,
    ) -> Result<(), Error> {
        let missing = || Error::protocol(format!("a {kind} without what it is about"));
        match kind {
            "response.created" => {
                let r = wire.response.ok_or_else(missing)?;
                out.push(Event::Started { id: r.id, model: r.model });
            }
            "response.output_item.added" => {
                let (index, item) = wire.output_index.zip(wire.item).ok_or_else(missing)?;
                self.start(index, &item, out)?;
            }
            "response.output_item.done" => {
                let (index, item) = wire.output_index.zip(wire.item).ok_or_else(missing)?;
                self.stop(index, &item, out)?;
            }
            "response.completed" => {
                usage(&wire.response.ok_or_else(missing)?, out);
                out.push(Event::Stop(match (self.refused, self.called) {
                    (true, _) => Stop::Refusal(None),
                    (false, true) => Stop::ToolUse,
                    (false, false) => Stop::EndTurn,
                }));
                out.push(Event::End);
            }
            "response.incomplete" => {
                let r = wire.response.ok_or_else(missing)?;
                usage(&r, out);
                out.push(Event::Stop(incomplete(r.incomplete_details.as_ref())));
                out.push(Event::End);
            }
            "response.failed" => {
                let r = wire.response.ok_or_else(missing)?;
                usage(&r, out);
                let said = match &r.error {
                    Some(e) => Object::default().with("error", e).and_then(|o| o.json()),
                    None => return Err(Error::provider(None, "")),
                };
                return Err(Error::provider(None, said.map_err(unwritable)?.get()));
            }
            _ => {
                let (index, delta) = wire.output_index.zip(wire.delta).ok_or_else(missing)?;
                let Some(open) = self.items.get_mut(&index) else {
                    return Err(Error::protocol("a delta for an item that is not open"));
                };
                let part = match kind {
                    "response.output_text.delta" => ("text", wire.content_index),
                    "response.refusal.delta" => ("refusal", wire.content_index),
                    "response.reasoning_summary_text.delta" => ("summary", wire.summary_index),
                    "response.reasoning_text.delta" => ("content", wire.content_index),
                    _ => ("arguments", None),
                };
                let part = (part.0, part.1.unwrap_or(0));
                let delta = serde_json::from_str::<String>(delta.get()).ok();
                match delta.and_then(|delta| open.fold(part, delta)) {
                    Some(delta) => out.push(Event::Delta { block: open.block, delta }),
                    None => self.other(event, out)?,
                }
            }
        }
        Ok(())
    }

    /// What is newer than this build, or the progress of what it does not know: kept if it is about
    /// an item.
    fn other(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        let about = serde_json::from_str::<About>(&event.data).ok().and_then(|a| a.output_index);
        if let Some(open) = about.and_then(|index| self.items.get(&index)) {
            let piece = Json::parse(event.data.as_str()).map_err(unwritable)?;
            out.push(Event::Delta { block: open.block, delta: Delta::Other(piece) });
        }
        Ok(())
    }

    fn start(&mut self, index: u64, item: &RawValue, out: &mut Vec<Event>) -> Result<(), Error> {
        if self.begun == MAX_BLOCKS {
            return Err(Error::protocol(format!("the answer has more than {MAX_BLOCKS} blocks")));
        }
        let item: Object = parse(item.get())?;
        let block = self.begun;
        self.begun += 1;
        let name: String = item.read("type").unwrap_or_default();
        let call = call_of(&item);
        let (kind, start) = match name.as_str() {
            "message" => (Kind::Message, Start::Text),
            "reasoning" => (Kind::Reasoning, Start::Reasoning),
            "function_call" => {
                self.called = true;
                let (id, name) = call.clone();
                (Kind::Call, Start::ToolCall { id, name })
            }
            // Also `web_search_call` and others, which the provider runs.
            _ => (Kind::Unknown, Start::Unknown { name: name.clone() }),
        };
        out.push(Event::BlockStart { block, start });
        let open = Open { block, kind, name, call, text: String::new(), part: None };
        if self.items.insert(index, open).is_some() {
            return Err(Error::protocol("an item that began while it was open"));
        }
        Ok(())
    }

    fn stop(&mut self, index: u64, raw: &RawValue, out: &mut Vec<Event>) -> Result<(), Error> {
        let Some(open) = self.items.remove(&index) else {
            return Err(Error::protocol("the end of an item that is not open"));
        };
        let item: Object = parse(raw.get())?;
        if item.read::<String>("type").unwrap_or_default() != open.name {
            return Err(Error::protocol("an item that became another"));
        }
        let block = open.block;
        let whole = open.whole(&item);
        // The pieces that came are the beginning of the whole, or the rest cannot be told.
        let rest = whole.as_deref().and_then(|w| w.strip_prefix(open.text.as_str()));
        if let Some(rest) = rest.filter(|r| !r.is_empty()).map(str::to_owned) {
            let delta = match open.kind {
                Kind::Reasoning => Delta::Reasoning(rest),
                Kind::Call => Delta::Args(rest),
                Kind::Message | Kind::Unknown => Delta::Text(rest),
            };
            out.push(Event::Delta { block, delta });
        }
        let broken = match open.kind {
            Kind::Call => broken(&open, &item, whole.as_deref()),
            _ => None,
        };
        if open.kind == Kind::Message {
            self.refused |= parts(&item, "content").any(|p| p.get("refusal").is_some());
        }
        out.push(Event::BlockStop { block, native: Some(Json::from(raw)), broken });
        Ok(())
    }
}

fn call_of(item: &Object) -> (String, String) {
    let string = |key| item.read::<String>(key).unwrap_or_default();
    (string("call_id"), string("name"))
}

fn parts(item: &Object, key: &str) -> impl Iterator<Item = Object> {
    item.read::<Vec<Object>>(key).into_iter().flatten()
}

/// Why a call cannot be run although its item is done.
fn broken(open: &Open, item: &Object, whole: Option<&str>) -> Option<String> {
    let status: Option<String> = item.read("status");
    let why = match whole {
        _ if status.is_some_and(|s| s != "completed") => "the provider says it is not complete",
        _ if call_of(item) != open.call => "the call was given two names or two ids",
        None => "the call has no arguments",
        Some(whole) if !whole.starts_with(&open.text) => "the arguments came in two versions",
        Some(_) => return None,
    };
    Some(why.to_owned())
}

/// Between the parts of reasoning, which are one block here.
const PARAGRAPH: &str = "\n\n";

impl Open {
    /// What to pass on, if it belongs to an item of this kind.
    fn fold(&mut self, part: (&'static str, u64), delta: String) -> Option<Delta> {
        let new = self.part.is_some_and(|last| last != part);
        let (text, wrap): (_, fn(String) -> Delta) = match (part.0, self.kind) {
            ("text" | "refusal", Kind::Message) => (delta, Delta::Text),
            ("summary" | "content", Kind::Reasoning) if new => {
                (format!("{PARAGRAPH}{delta}"), Delta::Reasoning)
            }
            ("summary" | "content", Kind::Reasoning) => (delta, Delta::Reasoning),
            ("arguments", Kind::Call) => (delta, Delta::Args),
            _ => return None,
        };
        self.part = Some(part);
        self.text.push_str(&text);
        Some(wrap(text))
    }

    /// All that the pieces of the finished item should have come to.
    fn whole(&self, item: &Object) -> Option<String> {
        let texts = |key| parts(item, key).filter_map(|p| p.read::<String>("text"));
        match self.kind {
            Kind::Message => Some(
                parts(item, "content")
                    .filter_map(|p| p.read("text").or_else(|| p.read::<String>("refusal")))
                    .collect(),
            ),
            Kind::Reasoning => {
                let all: Vec<String> = texts("summary").chain(texts("content")).collect();
                Some(all.join(PARAGRAPH))
            }
            Kind::Call => item.read("arguments"),
            Kind::Unknown => None,
        }
    }
}

fn incomplete(details: Option<&Value>) -> Stop {
    match details.and_then(|d| d.get("reason")?.as_str()) {
        Some("max_output_tokens") => Stop::MaxOutput,
        Some(reason @ "content_filter") => Stop::Refusal(Some(reason.to_owned())),
        reason => Stop::Other(reason.unwrap_or("incomplete").to_owned()),
    }
}

fn usage(response: &Response, out: &mut Vec<Event>) {
    let Some(told) = response.usage.as_ref().filter(|u| u.is_object()) else { return };
    let count = |path: &str| told.pointer(path).and_then(Value::as_u64);
    let (prompt, cache_read, cache_write) = (
        count("/input_tokens"),
        count("/input_tokens_details/cached_tokens"),
        count("/input_tokens_details/cache_write_tokens"),
    );
    out.push(Event::Usage(Usage {
        prompt,
        // The prompt counts both (guides/prompt-caching); what is not told was not written.
        uncached: prompt
            .zip(cache_read)
            .and_then(|(all, read)| all.checked_sub(read)?.checked_sub(cache_write.unwrap_or(0))),
        cache_read,
        cache_write,
        // It keeps for half an hour or half a day, and neither is an hour.
        cache_write_long: None,
        output: count("/output_tokens"),
        reasoning: count("/output_tokens_details/reasoning_tokens"),
        cost: None,
        // Which tier served it, not which was asked for (guides/priority-processing, read 2026-11-26).
        speed: response.service_tier.as_ref().and_then(name),
    }));
}

fn unwritable(e: serde_json::Error) -> Error {
    Error::protocol(format!("an item cannot be kept: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_id_is_cut_and_still_its_own() {
        let exact = "é".repeat(CALL_ID);
        assert_eq!(call_id(&exact), exact);
        assert_eq!(call_id("functions.f:0"), "functions.f:0");
        let long = |end: &str| format!("{}{end}", "é".repeat(CALL_ID));
        let (a, b) = (call_id(&long("a")), call_id(&long("b")));
        assert_eq!((a.chars().count(), b.chars().count()), (CALL_ID, CALL_ID));
        assert!(a != b && a.starts_with("ééé"));
        assert_eq!(a, call_id(&long("a")));
        // Known (FNV-1a of 65 times `a`), so that a conversation's prefix outlives a build.
        assert!(call_id(&"a".repeat(65)).ends_with(&format!("_{:016x}", fnv(&"a".repeat(65)))));
    }
    fn fnv(text: &str) -> u64 {
        let mut h: u64 = 14_695_981_039_346_656_037;
        for b in text.bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(1_099_511_628_211);
        }
        h
    }
}
