//! Google Gemini `streamGenerateContent` (aistudio.google.com/docs with `generate-content=true`, and
//! the v1beta discovery document of 2026-11-23; read 2026-11-25).
//!
//! The protocol has parts. One that is signed goes back as it came and in its place, and is never
//! joined to another; the pieces of text between are one part again
//! (docs/function-calling#thought-signatures).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};

use super::{
    EncodeError, Encoded, Loss, Lost, MAX_BLOCKS, Unfinished, json as parse,
    object::{Array, Object},
    replays, schema,
};
use crate::{error::Error, profile::Effort, profile::Quirks, sse, types::*};

/// What the codec writes; a profile's `body` cannot.
const OWN: [&str; 5] = ["model", "contents", "systemInstruction", "tools", "generationConfig"];

/// For the first call of a step that another model made, which nothing signed: "transferring a trace
/// from a different model" (docs/thought-signatures#faqs). Without one the request is refused.
const UNSIGNED: &str = "skip_thought_signature_validator";

/// Where the request goes. The model's name is part of an address, so it has no character that means
/// something there.
pub(super) fn path(model: &str) -> Result<String, EncodeError> {
    let plain = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-');
    // No name is all dots, the empty one included.
    if !model.bytes().all(plain) || model.bytes().all(|b| b == b'.') {
        return Err(EncodeError::BadModel(model.to_owned()));
    }
    Ok(format!("models/{model}:streamGenerateContent?alt=sse"))
}

pub(super) fn encode(
    request: &Request,
    quirks: &Quirks,
    scope: &Scope,
) -> Result<Encoded, EncodeError> {
    let settings = &request.settings;
    let mut config = Object::default();
    if let Some(max) = settings.max_output {
        config.put("maxOutputTokens", &max)?;
    }
    if !settings.stop.is_empty() {
        config.put("stopSequences", &settings.stop)?;
    }
    match (&settings.effort, quirks.effort) {
        (None, _) => {}
        (Some(effort), Some(Effort::ThinkingLevel)) => {
            config.put("thinkingConfig", &json!({ "thinkingLevel": effort }))?;
        }
        (Some(_), _) => return Err(EncodeError::Unsupported("effort")),
    }
    let mut body = Object::default();
    if !config.is_empty() {
        body.put("generationConfig", &config)?;
    }
    for (key, value) in &quirks.body {
        if OWN.contains(&key.as_str()) {
            return Err(EncodeError::Reserved(key.clone()));
        }
        body.put(key, value)?;
    }
    if !request.system.is_empty() {
        body.put("systemInstruction", &json!({ "parts": [{ "text": request.system }] }))?;
    }
    if !request.tools.is_empty() {
        let mut tools = Vec::with_capacity(request.tools.len());
        for tool in &request.tools {
            let schema = schema::gemini(&tool.schema)
                .map_err(|why| EncodeError::Schema { tool: tool.name.clone(), why })?;
            tools.push(
                Object::default()
                    .with("name", &tool.name)?
                    .with("description", &tool.description)?
                    .with("parametersJsonSchema", schema.raw())?,
            );
        }
        body.put("tools", &[Object::default().with("functionDeclarations", &tools)?])?;
    }
    let (mut contents, mut lost) = (Vec::new(), Vec::new());
    let mut unfinished = Unfinished::default();
    let mut calls = HashMap::new();
    for (n, message) in request.messages.iter().enumerate() {
        unfinished.message(message);
        let mut lose = |block, what| lost.push(Lost { message: n, block, what });
        let (role, parts) = match message.role {
            Role::User => ("user", user(message, &unfinished, &calls, &mut lose)?),
            Role::Assistant => ("model", model(message, quirks, scope, &mut calls, &mut lose)?),
        };
        if !parts.is_empty() {
            contents.push(Object::default().with("role", role)?.with("parts", &Array(&parts))?);
        }
    }
    body.put("contents", &contents)?;
    Ok(Encoded { body: body.json()?.get().as_bytes().to_vec(), lost })
}

/// What a result has to say of the call it answers: its name, and its id as the provider knows it.
type Calls<'a> = HashMap<&'a str, (&'a str, Option<String>)>;

fn user(
    from: &Message,
    unfinished: &Unfinished<'_>,
    calls: &Calls<'_>,
    lose: &mut impl FnMut(usize, Loss),
) -> Result<Vec<Json>, EncodeError> {
    let (mut parts, mut texts) = (Vec::new(), Vec::new());
    for (n, block) in from.blocks.iter().enumerate() {
        match &block.content {
            Content::Text { text } if text.is_empty() => {}
            Content::Text { text } => texts.push(Object::default().with("text", text)?.json()?),
            Content::ToolResult { call, .. } if unfinished.has(call) => {
                lose(n, Loss::ResultOfUnfinishedCall);
            }
            Content::ToolResult { call, text, failed } => {
                let Some((name, id)) = calls.get(call.as_str()) else {
                    lose(n, Loss::ResultWithoutCall);
                    continue;
                };
                let mut result = Object::default();
                if let Some(id) = id {
                    result.put("id", id)?;
                }
                result.put("name", name)?;
                let key = if *failed { "error" } else { "output" };
                result.put("response", &Object::default().with(key, text)?)?;
                parts.push(Object::default().with("functionResponse", &result)?.json()?);
            }
            Content::ToolCall(_) | Content::Reasoning { .. } => lose(n, Loss::Misplaced),
            Content::Unknown { .. } => lose(n, Loss::Unknown),
        }
    }
    // After the results, which follow their calls.
    parts.append(&mut texts);
    Ok(parts)
}

fn model<'a>(
    from: &'a Message,
    quirks: &Quirks,
    scope: &Scope,
    calls: &mut Calls<'a>,
    lose: &mut impl FnMut(usize, Loss),
) -> Result<Vec<Json>, EncodeError> {
    let mut parts = Vec::new();
    // The first call of a step has to be signed, and no other.
    let (mut called, mut left_out) = (false, false);
    for (n, block) in from.blocks.iter().enumerate() {
        match (&block.content, replays(block, scope, quirks)) {
            (Content::ToolCall(call), native) => match (call.ready(), native) {
                (None, _) => {
                    left_out = true;
                    lose(n, Loss::UnfinishedCall);
                }
                (Some(ready), Some(native)) => {
                    calls.insert(ready.id(), (ready.name(), own_id(native)));
                    let first = !std::mem::replace(&mut called, true);
                    // The one that was signed may have been left out.
                    if first && left_out && !is_signed(native) {
                        let part: Object = serde_json::from_str(native.get())?;
                        parts.push(part.with("thoughtSignature", UNSIGNED)?.json()?);
                    } else {
                        parts.push(native.clone());
                    }
                }
                (Some(ready), None) => {
                    calls.insert(ready.id(), (ready.name(), Some(ready.id().to_owned())));
                    let function = Object::default()
                        .with("id", ready.id())?
                        .with("name", ready.name())?
                        .with("args", ready.args())?;
                    let mut part = Object::default().with("functionCall", &function)?;
                    if !std::mem::replace(&mut called, true) {
                        part.put("thoughtSignature", UNSIGNED)?;
                    }
                    parts.push(part.json()?);
                }
            },
            (Content::ToolResult { .. }, _) => lose(n, Loss::Misplaced),
            (_, Some(native)) => match serde_json::from_str::<Vec<Box<RawValue>>>(native.get()) {
                Ok(several) => parts.extend(several.iter().map(|raw| Json::from(&**raw))),
                Err(_) => parts.push(native.clone()),
            },
            (Content::Text { text }, None) if text.is_empty() => {}
            (Content::Text { text }, None) => {
                parts.push(Object::default().with("text", text)?.json()?);
            }
            (Content::Reasoning { .. }, None) => lose(n, Loss::Reasoning),
            (Content::Unknown { .. }, None) => lose(n, Loss::Unknown),
        }
    }
    Ok(parts)
}

/// The id the provider gave a call, if it did.
fn own_id(part: &Json) -> Option<String> {
    let part: Object = serde_json::from_str(part.get()).ok()?;
    part.read::<Object>("functionCall")?.read("id")
}

/// Whether a native block holds what only its own model can read of its reasoning.
pub(super) fn is_signed(native: &Json) -> bool {
    let signed = |part: &Object| part.get("thoughtSignature").is_some();
    match serde_json::from_str::<Vec<Object>>(native.get()) {
        Ok(parts) => parts.iter().any(signed),
        Err(_) => serde_json::from_str(native.get()).is_ok_and(|part| signed(&part)),
    }
}

#[derive(Debug, Default)]
pub(super) struct Decoder {
    started: bool,
    /// The answer's id, for calls that come without one.
    id: Option<String>,
    blocks: usize,
    calls: usize,
    /// Text or thoughts, which are open one at a time.
    open: Option<Run>,
    stop: Option<Stop>,
}

#[derive(Debug)]
struct Run {
    block: usize,
    thought: bool,
    /// The parts that are whole.
    parts: Vec<Json>,
    /// What came since, which is one part.
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Chunk {
    candidates: Option<Vec<Candidate>>,
    prompt_feedback: Option<Value>,
    usage_metadata: Option<Value>,
    model_version: Option<String>,
    response_id: Option<String>,
    error: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    index: Option<u64>,
    content: Option<Parts>,
    finish_reason: Option<String>,
}
#[derive(Deserialize)]
struct Parts {
    parts: Option<Vec<Box<RawValue>>>,
}

impl Decoder {
    /// The provider gave a reason to stop, and says no more than that: the end of the stream is the
    /// end.
    pub fn stopped(&self) -> bool {
        self.stop.is_some()
    }

    pub fn event(&mut self, event: &sse::Event, out: &mut Vec<Event>) -> Result<(), Error> {
        let chunk: Chunk = parse(&event.data)?;
        if chunk.error.is_some() {
            return Err(Error::provider(None, &event.data));
        }
        if !std::mem::replace(&mut self.started, true) {
            self.id = chunk.response_id.clone();
            out.push(Event::Started { id: chunk.response_id, model: chunk.model_version });
        }
        for candidate in chunk.candidates.unwrap_or_default() {
            // Only one was asked for.
            if candidate.index.unwrap_or(0) != 0 {
                continue;
            }
            for part in candidate.content.and_then(|c| c.parts).unwrap_or_default() {
                self.part(&part, out)?;
            }
            if let Some(reason) = candidate.finish_reason {
                self.ends(stop(reason, self.calls > 0), out);
            }
        }
        // Then there is no candidate.
        let blocked = chunk.prompt_feedback.as_ref().and_then(|f| f.get("blockReason")?.as_str());
        if let Some(reason) = blocked {
            self.ends(Stop::Refusal(Some(reason.to_owned())), out);
        }
        if let Some(told) = chunk.usage_metadata.filter(Value::is_object) {
            out.push(Event::Usage(count(&told)));
        }
        Ok(())
    }

    fn ends(&mut self, stop: Stop, out: &mut Vec<Event>) {
        self.stop = Some(stop.clone());
        out.push(Event::Stop(stop));
    }

    fn part(&mut self, raw: &RawValue, out: &mut Vec<Event>) -> Result<(), Error> {
        let part: Object = parse(raw.get())?;
        if let Some(call) = part.get("functionCall") {
            return self.call(raw, call, out);
        }
        let Some(text) = part.read::<String>("text") else { return self.unknown(raw, &part, out) };
        let plain = part.members().all(|(k, _)| ["text", "thought"].contains(&k));
        if plain && text.is_empty() {
            return Ok(());
        }
        let thought = match (&self.open, text.is_empty()) {
            (_, false) => part.read("thought").unwrap_or(false),
            // A signature may come with an empty text (docs/thought-signatures#faqs). It belongs to
            // what it follows, and is a thought of its own where it follows nothing.
            (Some(run), true) => run.thought,
            (None, true) => true,
        };
        let mut run = match self.open.take() {
            Some(run) if run.thought == thought => run,
            other => {
                self.close(other, out);
                let start = if thought { Start::Reasoning } else { Start::Text };
                let block = self.start(start, out)?;
                Run { block, thought, parts: Vec::new(), text: String::new() }
            }
        };
        if plain {
            run.text.push_str(&text);
        } else {
            run.flush();
            run.parts.push(Json::from(raw));
        }
        if !text.is_empty() {
            let delta = if thought { Delta::Reasoning(text) } else { Delta::Text(text) };
            out.push(Event::Delta { block: run.block, delta });
        }
        self.open = Some(run);
        Ok(())
    }

    fn call(&mut self, raw: &RawValue, call: &Json, out: &mut Vec<Event>) -> Result<(), Error> {
        let open = self.open.take();
        self.close(open, out);
        let call: Object = parse(call.get())?;
        let name = call.read("name").unwrap_or_default();
        let id = call
            .read::<String>("id")
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| format!("{}_{}", self.id.as_deref().unwrap_or("call"), self.calls));
        self.calls += 1;
        let block = self.start(Start::ToolCall { id, name }, out)?;
        let args = match call.get("args").map(Json::get) {
            None | Some("null") => "{}",
            Some(args) => args,
        };
        out.push(Event::Delta { block, delta: Delta::Args(args.to_owned()) });
        // Vertex can send arguments in pieces. This API has no members for that, so what they would
        // mean here is not known.
        let pieces = ["partialArgs", "willContinue"].iter().any(|k| call.get(k).is_some());
        let broken = pieces.then(|| "the arguments came in pieces".to_owned());
        out.push(Event::BlockStop { block, native: Some(Json::from(raw)), broken });
        Ok(())
    }

    /// Also `executableCode`, `toolCall` and others, which the provider runs.
    fn unknown(
        &mut self,
        raw: &RawValue,
        part: &Object,
        out: &mut Vec<Event>,
    ) -> Result<(), Error> {
        let open = self.open.take();
        self.close(open, out);
        let about = ["thought", "thoughtSignature", "partMetadata"];
        let name = part.members().map(|(k, _)| k).find(|k| !about.contains(k)).unwrap_or_default();
        let block = self.start(Start::Unknown { name: name.to_owned() }, out)?;
        out.push(Event::BlockStop { block, native: Some(Json::from(raw)), broken: None });
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

    fn close(&mut self, run: Option<Run>, out: &mut Vec<Event>) {
        let Some(mut run) = run else { return };
        run.flush();
        let native = match run.parts.as_slice() {
            [one] => Some(one.clone()),
            several => Json::of(&Array(several)).ok(),
        };
        out.push(Event::BlockStop { block: run.block, native, broken: None });
    }

    /// Everything that is open ends.
    pub fn end(&mut self, out: &mut Vec<Event>) {
        let open = self.open.take();
        self.close(open, out);
        out.push(Event::End);
    }
}

impl Run {
    fn flush(&mut self) {
        let text = std::mem::take(&mut self.text);
        if !text.is_empty() {
            let part = match self.thought {
                true => json!({ "text": text, "thought": true }),
                false => json!({ "text": text }),
            };
            self.parts.push(part.into());
        }
    }
}

fn stop(reason: String, called: bool) -> Stop {
    match reason.as_str() {
        // Also the end at a stop sequence, which is not told apart.
        "STOP" if called => Stop::ToolUse,
        "STOP" => Stop::EndTurn,
        "MAX_TOKENS" => Stop::MaxOutput,
        "SAFETY"
        | "RECITATION"
        | "LANGUAGE"
        | "BLOCKLIST"
        | "PROHIBITED_CONTENT"
        | "SPII"
        | "IMAGE_SAFETY"
        | "IMAGE_PROHIBITED_CONTENT"
        | "IMAGE_RECITATION"
        | "ESCALATION"
        | "PUP_LIMITED_DISABLED" => Stop::Refusal(Some(reason)),
        // Also `CONTINUATION`: the rest is asked for with a token and without what came of the
        // answer (docs/long-decode), which this build does not do.
        _ => Stop::Other(reason),
    }
}

fn count(told: &Value) -> Usage {
    let count = |key: &str| told.get(key).and_then(Value::as_u64);
    let prompt = count("promptTokenCount");
    let (shown, thoughts) = (count("candidatesTokenCount"), count("thoughtsTokenCount"));
    // The total is of the prompt, that of the provider's own tools, thoughts and the answer.
    let rest = count("totalTokenCount").zip(prompt).and_then(|(all, prompt)| {
        all.checked_sub(prompt)?.checked_sub(count("toolUsePromptTokenCount").unwrap_or(0))
    });
    let cache_read = count("cachedContentTokenCount");
    Usage {
        prompt,
        // The prompt counts what was read from a cache.
        uncached: prompt.zip(cache_read).and_then(|(all, read)| all.checked_sub(read)),
        cache_read,
        cache_write: None,
        cache_write_long: None,
        output: rest.or_else(|| shown.zip(thoughts).and_then(|(a, b)| a.checked_add(b))),
        reasoning: thoughts,
    }
}
