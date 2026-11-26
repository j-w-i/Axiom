//! OpenAI Chat Completions, as many servers speak it (research/providers.md §2.3).
//!
//! The protocol has no blocks: they are made of a delta's members, in the order they come. What a
//! server gets wrong about a tool call leaves that call unable to run; it is not repaired.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    EncodeError, Encoded, Loss, Lost, MAX_BLOCKS, Unfinished, json as parse,
    object::{Array, Object},
    replays,
};
use crate::{
    error::Error,
    profile::{Effort, MaxTokens, Quirks, Think},
    sse,
    types::*,
};

/// What the codec writes; a profile's `body` cannot.
const OWN: [&str; 10] = [
    "model",
    "stream",
    "stream_options",
    "max_tokens",
    "max_completion_tokens",
    "stop",
    "reasoning_effort",
    "tools",
    "messages",
    "n",
];

pub(super) fn encode(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<Encoded, EncodeError> {
    let settings = &request.settings;
    let mut body = Object::default().with("model", &request.model)?.with("stream", &true)?;
    if quirks.stream_usage != Some(false) {
        body.put("stream_options", &json!({ "include_usage": true }))?;
    }
    if let Some(max) = settings.max_output {
        let field = match quirks.max_tokens_field {
            Some(MaxTokens::MaxCompletionTokens) => "max_completion_tokens",
            Some(MaxTokens::MaxTokens) | None => "max_tokens",
        };
        body.put(field, &max)?;
    }
    if !settings.stop.is_empty() {
        body.put("stop", &settings.stop)?;
    }
    super::plain(settings)?;
    match (&settings.effort, quirks.effort) {
        (None, _) => {}
        (Some(effort), Some(Effort::ReasoningEffort)) => body.put("reasoning_effort", effort)?,
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
            let function = Object::default()
                .with("name", &tool.name)?
                .with("description", &tool.description)?
                .with("parameters", tool.schema.raw())?;
            tools.push(Object::default().with("type", "function")?.with("function", &function)?);
        }
        body.put("tools", &tools)?;
    }
    let (mut messages, mut lost) = (Vec::new(), Vec::new());
    if !request.system.is_empty() {
        messages.push(Object::default().with("role", "system")?.with("content", &request.system)?);
    }
    let mut unfinished = Unfinished::default();
    for (n, message) in request.messages.iter().enumerate() {
        unfinished.message(message);
        let mut lose = |block, what| lost.push(Lost { message: n, block, what });
        match message.role {
            Role::User => user(message, &unfinished, &mut messages, &mut lose)?,
            Role::Assistant => {
                messages.extend(assistant(message, quirks, scope, &mut lose)?);
            }
        }
    }
    body.put("messages", &messages)?;
    Ok(Encoded { body: body.json()?.get().as_bytes().to_vec(), lost })
}

fn user(
    message: &Message,
    unfinished: &Unfinished<'_>,
    out: &mut Vec<Object>,
    lose: &mut impl FnMut(usize, Loss),
) -> Result<(), EncodeError> {
    let mut texts = Vec::new();
    for (n, block) in message.blocks.iter().enumerate() {
        match &block.content {
            Content::Text { text } if text.is_empty() => {}
            Content::Text { text } => texts.push(text.as_str()),
            Content::ToolResult { call, .. } if unfinished.has(call) => {
                lose(n, Loss::ResultOfUnfinishedCall);
            }
            // The protocol has no way to say that a tool failed; its text does.
            Content::ToolResult { call, text, failed: _ } => out.push(
                Object::default()
                    .with("role", "tool")?
                    .with("tool_call_id", call)?
                    .with("content", text)?,
            ),
            Content::ToolCall(_) | Content::Reasoning { .. } => lose(n, Loss::Misplaced),
            Content::Unknown { .. } => lose(n, Loss::Unknown),
        }
    }
    // After the results, which must follow their calls.
    if !texts.is_empty() {
        out.push(Object::default().with("role", "user")?.with("content", &texts.join("\n\n"))?);
    }
    Ok(())
}

fn assistant(
    message: &Message,
    quirks: &Quirks,
    scope: &Scope,
    lose: &mut impl FnMut(usize, Loss),
) -> Result<Option<Object>, EncodeError> {
    let (mut text, mut calls, mut more) = (String::new(), Vec::new(), Object::default());
    for (n, block) in message.blocks.iter().enumerate() {
        let native = replays(block, scope, quirks);
        match (&block.content, native) {
            (Content::Text { text: t }, _) => text.push_str(t),
            (Content::ToolCall(call), native) => match (call.ready(), native) {
                (None, _) => lose(n, Loss::UnfinishedCall),
                (Some(_), Some(native)) => calls.push(native.clone()),
                (Some(_), None) => {
                    let function = json!({ "name": call.name, "arguments": call.raw });
                    let call = Object::default()
                        .with("id", &call.id)?
                        .with("type", "function")?
                        .with("function", &function)?;
                    calls.push(call.json()?);
                }
            },
            (Content::ToolResult { .. }, _) => lose(n, Loss::Misplaced),
            (_, Some(native)) if quirks.replay_reasoning == Some(true) => {
                let members: Object = serde_json::from_str(native.get())?;
                for (key, value) in members.members() {
                    // Reasoning on both sides of a text is one member again.
                    let texts = more.read::<String>(key).zip(members.read::<String>(key));
                    match texts {
                        Some((before, after)) => more.put(key, &(before + &after))?,
                        None => more.set(key, value.clone()),
                    }
                }
            }
            (Content::Reasoning { .. }, _) => lose(n, Loss::Reasoning),
            (Content::Unknown { .. }, _) => lose(n, Loss::Unknown),
        }
    }
    if text.is_empty() && calls.is_empty() && more.is_empty() {
        return Ok(None);
    }
    // The content is a string even beside calls: templates of local servers join it to others.
    let mut out = Object::default().with("role", "assistant")?.with("content", &text)?;
    for (key, value) in more.members().filter(|(k, _)| !["role", "content"].contains(k)) {
        out.set(key, value.clone());
    }
    if !calls.is_empty() {
        out.put("tool_calls", &Array(&calls))?;
    }
    Ok(Some(out))
}

/// Read in this order; one that is empty or absent leaves it to the next.
const REASONING: [&str; 2] = ["reasoning_content", "reasoning"];

#[derive(Debug)]
pub(super) struct Decoder {
    think: Splitter,
    empty_args: bool,
    cost_unit: Option<String>,
    started: bool,
    /// The answer's id, for calls that come without one.
    id: Option<String>,
    blocks: usize,
    /// Text or reasoning, which are open one at a time.
    open: Option<Run>,
    calls: Vec<CallState>,
    /// Which call the server last meant by an index.
    by_index: HashMap<u64, usize>,
    unknown: Vec<Unknown>,
    stop: Option<Stop>,
}

#[derive(Debug)]
struct Run {
    block: usize,
    kind: RunKind,
    text: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunKind {
    Text,
    /// From this member, or from between tags.
    Reasoning(Option<&'static str>),
}

#[derive(Debug, Default)]
struct CallState {
    /// Once its name is known.
    block: Option<usize>,
    id: Option<String>,
    name: Option<String>,
    args: String,
    more: Object,
    broken: Option<String>,
}

#[derive(Debug)]
struct Unknown {
    block: usize,
    name: String,
    pieces: Vec<Json>,
    whole: bool,
}

#[derive(Deserialize)]
struct Chunk {
    id: Option<String>,
    model: Option<String>,
    choices: Option<Vec<Choice>>,
    usage: Option<Value>,
    error: Option<Value>,
}
#[derive(Deserialize)]
struct Choice {
    index: Option<u64>,
    delta: Option<Object>,
    finish_reason: Option<String>,
}

impl Decoder {
    pub fn new(quirks: &Quirks) -> Self {
        Self {
            think: Splitter::new(quirks.think.unwrap_or(Think::Off)),
            empty_args: quirks.empty_args == Some(true),
            cost_unit: quirks.cost_unit.clone(),
            started: false,
            id: None,
            blocks: 0,
            open: None,
            calls: Vec::new(),
            by_index: HashMap::new(),
            unknown: Vec::new(),
            stop: None,
        }
    }

    pub fn stopped(&self) -> bool {
        self.stop.is_some()
    }

    pub fn event(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        match event.data.trim() {
            "" => return Ok(()),
            "[DONE]" if self.stopped() => {
                self.end(out);
                return Ok(());
            }
            "[DONE]" => return Err(Error::cut_short("the answer ended without a reason")),
            _ if event.name == "error" => return Err(Error::provider(None, &event.data)),
            _ => {}
        }
        let chunk: Chunk = parse(&event.data)?;
        if chunk.error.is_some() {
            return Err(Error::provider(None, &event.data));
        }
        if !std::mem::replace(&mut self.started, true) {
            self.id = chunk.id.clone();
            out.push(Event::Started { id: chunk.id, model: chunk.model });
        }
        for choice in chunk.choices.unwrap_or_default() {
            // Only one was asked for.
            if choice.index.unwrap_or(0) != 0 {
                continue;
            }
            if let Some(delta) = choice.delta {
                self.delta(&delta, out)?;
            }
            if let Some(reason) = choice.finish_reason {
                let stop = stop(reason);
                self.stop = Some(stop.clone());
                out.push(Event::Stop(stop));
            }
        }
        if let Some(usage) = chunk.usage.filter(Value::is_object) {
            out.push(Event::Usage(Usage { cost: self.cost(&usage), ..count(&usage) }));
        }
        Ok(())
    }

    fn cost(&self, told: &Value) -> Option<Cost> {
        let amount = told.get("cost")?.as_f64().filter(|a| *a >= 0.0)?;
        Some(Cost { amount, unit: self.cost_unit.clone()? })
    }

    fn delta(&mut self, delta: &Object, out: &mut Vec<Event>) -> Result<(), Error> {
        let reasoning = REASONING.iter().find_map(|key| {
            let text: String = delta.read(key)?;
            (!text.is_empty()).then_some((*key, text))
        });
        if let Some((key, text)) = &reasoning {
            self.run(RunKind::Reasoning(Some(key)), text, out)?;
        }
        if let Some(content) = delta.get("content") {
            self.content(content, out)?;
        }
        for call in delta.read::<Vec<Object>>("tool_calls").into_iter().flatten() {
            self.call(&call, out)?;
        }
        for (key, value) in delta.members() {
            let known = match key {
                "role" | "content" => true,
                "tool_calls" => delta.read::<Vec<Object>>(key).is_some(),
                key if REASONING.contains(&key) => delta.read::<String>(key).is_some(),
                _ => false,
            };
            if !known && value.get() != "null" {
                self.unknown(key, value, true, out)?;
            }
        }
        Ok(())
    }

    fn content(&mut self, content: &Json, out: &mut Vec<Event>) -> Result<(), Error> {
        if let Ok(text) = serde_json::from_str::<String>(content.get()) {
            for (reasoning, text) in self.think.feed(&text) {
                let kind = if reasoning { RunKind::Reasoning(None) } else { RunKind::Text };
                self.run(kind, &text, out)?;
            }
        } else if let Ok(parts) = serde_json::from_str::<Vec<Object>>(content.get()) {
            for part in parts {
                match part.read::<String>("text") {
                    Some(text) => self.run(RunKind::Text, &text, out)?,
                    None => {
                        self.unknown("content", &part.json().map_err(unwritable)?, false, out)?
                    }
                }
            }
        }
        Ok(())
    }

    /// Text or reasoning, in the block that is open if it is of that kind.
    fn run(&mut self, kind: RunKind, text: &str, out: &mut Vec<Event>) -> Result<(), Error> {
        if text.is_empty() {
            return Ok(());
        }
        let mut run = match self.open.take() {
            Some(run) if run.kind == kind => run,
            other => {
                self.close(other, out)?;
                let start = if kind == RunKind::Text { Start::Text } else { Start::Reasoning };
                Run { block: self.start(start, out)?, kind, text: String::new() }
            }
        };
        let delta = match kind {
            RunKind::Text => Delta::Text(text.to_owned()),
            RunKind::Reasoning(_) => Delta::Reasoning(text.to_owned()),
        };
        // Text needs no native form: it goes back as it was read.
        if kind != RunKind::Text {
            run.text.push_str(text);
        }
        out.push(Event::Delta { block: run.block, delta });
        self.open = Some(run);
        Ok(())
    }

    fn close(&mut self, run: Option<Run>, out: &mut Vec<Event>) -> Result<(), Error> {
        let Some(run) = run else { return Ok(()) };
        let native = match run.kind {
            RunKind::Reasoning(Some(key)) => {
                let members = Object::default().with(key, &run.text).map_err(unwritable)?;
                Some(members.json().map_err(unwritable)?)
            }
            RunKind::Reasoning(None) | RunKind::Text => None,
        };
        out.push(Event::BlockStop { block: run.block, native, broken: None });
        Ok(())
    }

    fn start(&mut self, start: Start, out: &mut Vec<Event>) -> Result<usize, Error> {
        if self.blocks == MAX_BLOCKS {
            return Err(Error::protocol(format!("the answer has more than {MAX_BLOCKS} blocks")));
        }
        let block = self.blocks;
        self.blocks += 1;
        out.push(Event::BlockStart { block, start });
        Ok(block)
    }

    fn unknown(
        &mut self,
        name: &str,
        piece: &Json,
        whole: bool,
        out: &mut Vec<Event>,
    ) -> Result<(), Error> {
        let n = match self.unknown.iter().position(|u| u.name == name) {
            Some(n) => n,
            None => {
                let block = self.start(Start::Unknown { name: name.to_owned() }, out)?;
                let name = name.to_owned();
                self.unknown.push(Unknown { block, name, pieces: Vec::new(), whole });
                self.unknown.len() - 1
            }
        };
        let u = &mut self.unknown[n];
        u.pieces.push(piece.clone());
        out.push(Event::Delta { block: u.block, delta: Delta::Other(piece.clone()) });
        Ok(())
    }

    fn call(&mut self, piece: &Object, out: &mut Vec<Event>) -> Result<(), Error> {
        let open = self.open.take();
        self.close(open, out)?;
        let function: Object = piece.read("function").unwrap_or_default();
        let text = |o: &Object, key: &str| o.read::<String>(key).filter(|t| !t.is_empty());
        let (id, name) = (text(piece, "id"), text(&function, "name"));
        let n = self.slot(piece.read("index"), id.as_deref(), name.is_some())?;
        let answer = self.id.clone();
        let call = &mut self.calls[n];
        // Too late for a call that has been announced under a name of its own making.
        if call.id.is_none() && call.block.is_none() {
            call.id = id;
        }
        match (&call.name, name) {
            (None, name) => call.name = name,
            (Some(a), Some(b)) if *a != b => {
                call.broken = Some("the call was given two names".into());
            }
            _ => {}
        }
        for (key, value) in piece.members() {
            if !["index", "id", "type", "function"].contains(&key) {
                call.more.set(key, value.clone());
            }
        }
        // A string, or what the string should have held.
        let args = match function.get("arguments").map(Json::get) {
            None | Some("null") => String::new(),
            Some(raw) => serde_json::from_str(raw).unwrap_or_else(|_| raw.to_owned()),
        };
        call.args.push_str(&args);
        let (block, args) = match (call.block, &call.name) {
            (Some(block), _) => (block, args),
            (None, None) => return Ok(()),
            (None, Some(name)) => {
                let name = name.clone();
                let id = match &call.id {
                    Some(id) => id.clone(),
                    None => format!("{}_{n}", answer.as_deref().unwrap_or("call")),
                };
                call.id = Some(id.clone());
                let args = call.args.clone();
                let block = self.start(Start::ToolCall { id, name }, out)?;
                self.calls[n].block = Some(block);
                (block, args)
            }
        };
        if !args.is_empty() {
            out.push(Event::Delta { block, delta: Delta::Args(args) });
        }
        Ok(())
    }

    /// The call a piece belongs to. Servers number calls, name them, do both, or neither; some
    /// give every call the same number.
    fn slot(&mut self, index: Option<u64>, id: Option<&str>, named: bool) -> Result<usize, Error> {
        let same = |n: usize| match (&self.calls[n].id, id) {
            (Some(a), Some(b)) => a == b,
            _ => true,
        };
        let found = match (index, id) {
            (Some(i), _) => self.by_index.get(&i).copied().filter(|n| same(*n)),
            (None, Some(id)) => self.calls.iter().rposition(|c| c.id.as_deref() == Some(id)),
            (None, None) if named => None,
            (None, None) => self.calls.len().checked_sub(1),
        };
        let n = match found {
            Some(n) => n,
            None if self.calls.len() == MAX_BLOCKS => {
                return Err(Error::protocol(format!(
                    "the answer has more than {MAX_BLOCKS} calls"
                )));
            }
            None => {
                self.calls.push(CallState::default());
                self.calls.len() - 1
            }
        };
        if let Some(i) = index {
            self.by_index.insert(i, n);
        }
        Ok(n)
    }

    /// Everything that is open ends.
    pub fn end(&mut self, out: &mut Vec<Event>) {
        for (reasoning, text) in self.think.flush() {
            let kind = if reasoning { RunKind::Reasoning(None) } else { RunKind::Text };
            // Past the limit on blocks, a few bytes that looked like a tag are lost.
            let _ = self.run(kind, &text, out);
        }
        let open = self.open.take();
        let _ = self.close(open, out);
        for call in std::mem::take(&mut self.calls) {
            let Some(block) = call.block else {
                // It never had a name. It is shown, and cannot run.
                let start = Start::ToolCall {
                    id: call.id.unwrap_or_default(),
                    name: call.name.unwrap_or_default(),
                };
                if let Ok(block) = self.start(start, out) {
                    out.push(Event::Delta { block, delta: Delta::Args(call.args) });
                    out.push(Event::BlockStop { block, native: None, broken: None });
                }
                continue;
            };
            let mut args = call.args.as_str();
            if self.empty_args && args.trim().is_empty() {
                args = "{}";
                out.push(Event::Delta { block, delta: Delta::Args(args.to_owned()) });
            }
            let native = whole(&call, args).ok();
            out.push(Event::BlockStop { block, native, broken: call.broken });
        }
        for u in std::mem::take(&mut self.unknown) {
            let native = if u.whole { joined(&u.name, &u.pieces) } else { None };
            out.push(Event::BlockStop { block: u.block, native, broken: None });
        }
        out.push(Event::End);
    }
}

/// The call as it stands in `tool_calls`.
fn whole(call: &CallState, args: &str) -> serde_json::Result<Json> {
    let function = Object::default().with("name", &call.name)?.with("arguments", args)?;
    let mut out = Object::default()
        .with("id", &call.id)?
        .with("type", "function")?
        .with("function", &function)?;
    for (key, value) in call.more.members() {
        out.set(key, value.clone());
    }
    out.json()
}

/// A member that came in pieces: strings and arrays go on, anything else only whole.
fn joined(name: &str, pieces: &[Json]) -> Option<Json> {
    let strings: Option<Vec<String>> =
        pieces.iter().map(|p| serde_json::from_str(p.get()).ok()).collect();
    let arrays = || {
        let arrays: Option<Vec<Vec<Box<serde_json::value::RawValue>>>> =
            pieces.iter().map(|p| serde_json::from_str(p.get()).ok()).collect();
        Some(arrays?.into_iter().flatten().map(|raw| Json::from(&*raw)).collect::<Vec<_>>())
    };
    let mut out = Object::default();
    match (strings, pieces) {
        (Some(strings), _) => out.put(name, &strings.concat()).ok()?,
        (None, [one]) => out.set(name, one.clone()),
        (None, _) => out.put(name, &Array(&arrays()?)).ok()?,
    }
    out.json().ok()
}

fn stop(reason: String) -> Stop {
    match reason.as_str() {
        "stop" => Stop::EndTurn,
        "tool_calls" => Stop::ToolUse,
        "length" => Stop::MaxOutput,
        "content_filter" => Stop::Refusal(Some(reason)),
        _ => Stop::Other(reason),
    }
}

fn count(told: &Value) -> Usage {
    let count = |path: &str| told.pointer(path).and_then(Value::as_u64);
    let (prompt, cache_read, cache_write) = (
        count("/prompt_tokens"),
        count("/prompt_tokens_details/cached_tokens"),
        count("/prompt_tokens_details/cache_write_tokens"),
    );
    Usage {
        prompt,
        // The prompt counts what was read from a cache, and what was written where that is told;
        // without the first, what was not is unknown.
        uncached: prompt
            .zip(cache_read)
            .and_then(|(all, read)| all.checked_sub(read)?.checked_sub(cache_write.unwrap_or(0))),
        cache_read,
        cache_write,
        cache_write_long: None,
        output: count("/completion_tokens"),
        reasoning: count("/completion_tokens_details/reasoning_tokens"),
        ..Usage::default()
    }
}

fn unwritable(e: serde_json::Error) -> Error {
    Error::protocol(format!("a block cannot be kept: {e}"))
}

/// Takes reasoning out of text that marks it with tags, however the tags are cut.
#[derive(Debug)]
struct Splitter {
    state: Place,
    /// What may yet become a tag.
    held: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    /// Nothing but space so far.
    Before,
    Inside,
    After,
}
const OPEN: &str = "<think>";
const CLOSE: &str = "</think>";
/// More space than this before a tag, and it is not the answer's beginning.
const MARGIN: usize = 64;

impl Splitter {
    fn new(think: Think) -> Self {
        let state = match think {
            Think::Off => Place::After,
            Think::Leading => Place::Before,
            Think::Open => Place::Inside,
        };
        Self { state, held: String::new() }
    }

    /// Pieces, and whether each is reasoning.
    fn feed(&mut self, text: &str) -> Vec<(bool, String)> {
        let mut out = Vec::new();
        if self.state == Place::After {
            out.push((false, text.to_owned()));
            return out;
        }
        self.held.push_str(text);
        if self.state == Place::Before {
            let rest = self.held.trim_start();
            let space = self.held.len() - rest.len();
            if space <= MARGIN && rest.starts_with(OPEN) {
                self.held.drain(..space + OPEN.len());
                self.state = Place::Inside;
            } else if space <= MARGIN && OPEN.starts_with(rest) {
                return out;
            } else {
                self.state = Place::After;
                out.push((false, std::mem::take(&mut self.held)));
            }
        }
        if self.state == Place::Inside {
            if let Some(at) = self.held.find(CLOSE) {
                let rest = self.held.split_off(at);
                out.push((true, std::mem::take(&mut self.held)));
                out.push((false, rest[CLOSE.len()..].to_owned()));
                self.state = Place::After;
            } else {
                let keep =
                    (1..CLOSE.len()).rev().find(|n| self.held.ends_with(&CLOSE[..*n])).unwrap_or(0);
                let kept = self.held.split_off(self.held.len() - keep);
                out.push((true, std::mem::replace(&mut self.held, kept)));
            }
        }
        out
    }

    fn flush(&mut self) -> Vec<(bool, String)> {
        vec![(self.state == Place::Inside, std::mem::take(&mut self.held))]
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn split(think: Think, pieces: &[&str]) -> (String, String) {
        let mut s = Splitter::new(think);
        let mut parts: Vec<_> = pieces.iter().flat_map(|p| s.feed(p)).collect();
        parts.extend(s.flush());
        let of = |want| parts.iter().filter(|p| p.0 == want).map(|p| p.1.as_str()).collect();
        (of(true), of(false))
    }

    #[test]
    fn tags() {
        let is = |think, text: &str, reasoning: &str, rest: &str| {
            assert_eq!(split(think, &[text]), (reasoning.into(), rest.into()), "{text:?}");
        };
        use Think::*;
        is(Off, "<think>a</think>b", "", "<think>a</think>b");
        is(Leading, "<think>a</think>b", "a", "b");
        is(Leading, "\n <think>a</think>\n\nb", "a", "\n\nb");
        is(Leading, "b <think>a</think>", "", "b <think>a</think>");
        is(Leading, "<think>a</think>b<think>c</think>", "a", "b<think>c</think>");
        is(Leading, "<think>a", "a", "");
        is(Leading, "<think>a</thi", "a</thi", "");
        is(Leading, "<thi", "", "<thi");
        is(Leading, "", "", "");
        is(Leading, "  ", "", "  ");
        is(Leading, &format!("{}<think>a</think>", " ".repeat(MARGIN)), "a", "");
        let wide = format!("{}<think>a</think>", " ".repeat(MARGIN + 1));
        is(Leading, &wide, "", &wide);
        is(Open, "a</think>b", "a", "b");
        is(Open, "a", "a", "");
        is(Leading, "<think></think>b", "", "b");
        is(Leading, "<think>a</</think>b", "a</", "b");
    }

    proptest! {
        #[test]
        fn cuts_do_not_matter(
            text in "[ <>/a]{0,12}(<think>)?[ </é]{0,6}(</think>)?[b<é]{0,6}",
            cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..6),
            think in prop_oneof![Just(Think::Leading), Just(Think::Open), Just(Think::Off)],
        ) {
            let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).chain([text.len()]).collect();
            let mut at: Vec<usize> = cuts.iter().map(|c| ends[c.index(ends.len())]).collect();
            at.sort_unstable();
            at.push(text.len());
            let mut from = 0;
            let pieces: Vec<&str> =
                at.iter().map(|to| { let p = &text[from..*to]; from = *to; p }).collect();
            prop_assert_eq!(split(think, &pieces), split(think, &[&text]));
        }
    }
}
