//! OpenAI Chat Completions adapter (streamed), also used for the compatible local servers
//! (Ollama, llama.cpp `llama-server`, LM Studio, vLLM). Chunks are `data: {choices:[{delta}]}`
//! and the stream ends with `data: [DONE]`. Tool calls arrive by `index`, their arguments as
//! string fragments (Ollama sends each call whole in one chunk).

use super::http::{check_base_url, client, error_message, status_error, transport};
use super::profiles::{self, Server};
use super::{
    sse, Block, EventStream, ModelProvider, ProviderError, Role, StopReason, ToolNames, TurnEvent,
    TurnRequest,
};
use axiom_protocol::agent::{CapabilityOverrides, ModelInfo, ProviderConfig, ProviderKind};
use futures_util::future::BoxFuture;
use futures_util::{FutureExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";

pub struct OpenAiChat {
    id: String,
    base_url: String,
    api_key: Option<String>,
    models: Vec<String>,
    overrides: Option<CapabilityOverrides>,
    server: Server,
    http: reqwest::Client,
}

impl std::fmt::Debug for OpenAiChat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAiChat")
            .field("id", &self.id)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl OpenAiChat {
    pub fn new(config: &ProviderConfig, api_key: Option<String>) -> Result<Self, String> {
        let base_url = check_base_url(config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
        Ok(Self {
            id: config.id.clone(),
            server: profiles::server_of(&base_url),
            base_url,
            api_key: api_key.filter(|k| !k.is_empty()),
            models: config.models.clone(),
            overrides: config.capabilities.clone(),
            http: client().map_err(|e| e.to_string())?,
        })
    }

    fn info(&self, id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: id.to_string(),
            capabilities: profiles::apply(
                profiles::profile(ProviderKind::OpenaiChat, &self.base_url, id),
                self.overrides.as_ref(),
            ),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut r = self
            .http
            .request(method, format!("{}{path}", self.base_url));
        if let Some(k) = &self.api_key {
            r = r.bearer_auth(k);
        }
        r
    }
}

impl ModelProvider for OpenAiChat {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenaiChat
    }

    fn models(&self) -> BoxFuture<'_, Result<Vec<ModelInfo>, ProviderError>> {
        async move {
            if !self.models.is_empty() {
                return Ok(self.models.iter().map(|m| self.info(m)).collect());
            }
            let resp = self
                .request(reqwest::Method::GET, "/models")
                .send()
                .await
                .map_err(transport)?;
            let status = resp.status();
            let body = resp.text().await.map_err(transport)?;
            if !status.is_success() {
                return Err(status_error(status, None, &body));
            }
            let v: Value = serde_json::from_str(&body)
                .map_err(|e| ProviderError::Transport(format!("bad model list: {e}")))?;
            let mut ids: Vec<String> = v["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| m["id"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            ids.sort();
            Ok(ids.iter().map(|m| self.info(m)).collect())
        }
        .boxed()
    }

    fn stream(
        &self,
        req: TurnRequest,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, Result<EventStream, ProviderError>> {
        async move {
            let names = ToolNames::new(&req.tools);
            let body = request_body_with(&req, &names, self.server);
            let send = self
                .request(reqwest::Method::POST, "/chat/completions")
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .body(body.to_string())
                .send();
            let resp = tokio::select! {
                r = send => r.map_err(transport)?,
                _ = cancel.cancelled() => return Ok(futures_util::stream::empty().boxed()),
            };
            let status = resp.status();
            if !status.is_success() {
                let retry = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .map(String::from);
                let text = resp.text().await.unwrap_or_default();
                return Err(status_error(status, retry.as_deref(), &text));
            }
            let events = sse::events(resp.bytes_stream().boxed());
            let out = futures_util::stream::unfold(
                (events.boxed(), Decoder::new(names), VecDeque::new(), false),
                |(mut events, mut st, mut queue, mut done)| async move {
                    loop {
                        if let Some(item) = queue.pop_front() {
                            return Some((item, (events, st, queue, done)));
                        }
                        if done {
                            return None;
                        }
                        match events.next().await {
                            Some(Ok(ev)) => match st.data(&ev.data) {
                                Ok(evs) => {
                                    done = st.done;
                                    queue.extend(evs.into_iter().map(Ok));
                                }
                                Err(e) => {
                                    done = true;
                                    queue.push_back(Err(e));
                                }
                            },
                            Some(Err(e)) => {
                                done = true;
                                queue.push_back(Err(transport(e)));
                            }
                            None => {
                                done = true;
                                // Some servers end without `[DONE]` after a finish reason.
                                match st.end() {
                                    Some(evs) => queue.extend(evs.into_iter().map(Ok)),
                                    None => queue.push_back(Err(ProviderError::Transport(
                                        "the response ended before the message did".into(),
                                    ))),
                                }
                            }
                        }
                    }
                },
            );
            Ok(out.take_until(cancel.cancelled_owned()).boxed())
        }
        .boxed()
    }
}

/// The Chat Completions request body (public for golden tests).
pub fn request_body(req: &TurnRequest, base_url: &str) -> Value {
    request_body_with(
        req,
        &ToolNames::new(&req.tools),
        profiles::server_of(base_url),
    )
}

fn request_body_with(req: &TurnRequest, names: &ToolNames, server: Server) -> Value {
    let mut messages = Vec::new();
    if !req.system.is_empty() {
        messages.push(json!({"role":"system","content":req.system}));
    }
    for m in &req.messages {
        match m.role {
            Role::User => {
                // Tool results are their own `tool` messages and come first (they answer the
                // previous assistant message); any text follows as a user message.
                let mut text = Vec::new();
                for b in &m.blocks {
                    match b {
                        Block::ToolResult {
                            call_id, content, ..
                        } => messages.push(json!({
                            "role":"tool","tool_call_id":call_id,"content":content
                        })),
                        Block::Text(t) if !t.text.is_empty() => text.push(t.text.as_str()),
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    messages.push(json!({"role":"user","content":text.join("\n\n")}));
                }
            }
            Role::Assistant => {
                let text: Vec<&str> = m
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::Text(t) if !t.text.is_empty() => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect();
                let calls: Vec<Value> = m
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::ToolCall { id, name, input } => Some(json!({
                            "id":id,"type":"function",
                            "function":{"name":names.wire(name),"arguments":input.to_string()}
                        })),
                        _ => None,
                    })
                    .collect();
                if text.is_empty() && calls.is_empty() {
                    continue;
                }
                let mut msg = json!({"role":"assistant"});
                msg["content"] = if text.is_empty() {
                    Value::Null
                } else {
                    json!(text.join(""))
                };
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls);
                }
                messages.push(msg);
            }
        }
    }
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
    });
    if let Some(n) = req.max_output_tokens {
        // OpenAI's own API renamed the field; the compatible servers read the old one.
        let key = if server == Server::OpenAi {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        body[key] = json!(n);
    }
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| {
                    json!({"type":"function","function":{
                        "name":names.wire(&t.name),"description":t.description,
                        "parameters":t.input_schema}})
                })
                .collect(),
        );
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    body
}

struct Call {
    id: String,
    started: bool,
    name: String,
    /// Fragments that arrived before the id/name.
    pending: String,
}

struct Decoder {
    names: ToolNames,
    calls: BTreeMap<u64, Call>,
    finish: Option<StopReason>,
    done: bool,
}

impl Decoder {
    fn new(names: ToolNames) -> Self {
        Self {
            names,
            calls: BTreeMap::new(),
            finish: None,
            done: false,
        }
    }

    fn data(&mut self, data: &str) -> Result<Vec<TurnEvent>, ProviderError> {
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok(self.close());
        }
        let v: Value = serde_json::from_str(data)
            .map_err(|e| ProviderError::Transport(format!("bad chunk: {e}")))?;
        if v.get("error").is_some() {
            let msg = error_message(data).unwrap_or_else(|| "error".into());
            return Err(if super::http::looks_like_context_overflow(&msg) {
                ProviderError::ContextTooLong
            } else {
                ProviderError::InvalidRequest(msg)
            });
        }
        let mut out = Vec::new();
        for choice in v["choices"].as_array().into_iter().flatten() {
            let d = &choice["delta"];
            if let Some(t) = d["content"].as_str().filter(|t| !t.is_empty()) {
                out.push(TurnEvent::TextDelta { text: t.into() });
            }
            for tc in d["tool_calls"].as_array().into_iter().flatten() {
                let index = tc["index"].as_u64().unwrap_or(self.calls.len() as u64);
                let call = self.calls.entry(index).or_insert_with(|| Call {
                    id: String::new(),
                    started: false,
                    name: String::new(),
                    pending: String::new(),
                });
                if let Some(id) = tc["id"].as_str().filter(|s| !s.is_empty()) {
                    call.id = id.into();
                }
                if let Some(n) = tc["function"]["name"].as_str().filter(|s| !s.is_empty()) {
                    call.name.push_str(n);
                }
                let args = tc["function"]["arguments"].as_str().unwrap_or_default();
                if !call.started && !call.name.is_empty() {
                    if call.id.is_empty() {
                        call.id = format!("call_{index}");
                    }
                    call.started = true;
                    out.push(TurnEvent::ToolCallStart {
                        id: call.id.clone(),
                        name: self.names.axiom(&call.name),
                    });
                    if !call.pending.is_empty() {
                        out.push(TurnEvent::ToolCallArgsDelta {
                            id: call.id.clone(),
                            json_fragment: std::mem::take(&mut call.pending),
                        });
                    }
                }
                if !args.is_empty() {
                    if call.started {
                        out.push(TurnEvent::ToolCallArgsDelta {
                            id: call.id.clone(),
                            json_fragment: args.into(),
                        });
                    } else {
                        call.pending.push_str(args);
                    }
                }
            }
            if let Some(r) = choice["finish_reason"].as_str() {
                self.finish = Some(match r {
                    "stop" => StopReason::EndTurn,
                    "tool_calls" | "function_call" => StopReason::ToolUse,
                    "length" => StopReason::MaxTokens,
                    "content_filter" => StopReason::Refusal,
                    other => StopReason::Other(other.into()),
                });
            }
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            out.push(TurnEvent::Usage {
                input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                cached_input_tokens: u["prompt_tokens_details"]["cached_tokens"]
                    .as_u64()
                    .map(|n| n as u32),
            });
        }
        Ok(out)
    }

    /// Ends every open tool call and finishes.
    fn close(&mut self) -> Vec<TurnEvent> {
        let mut out: Vec<TurnEvent> = std::mem::take(&mut self.calls)
            .into_values()
            .filter(|c| c.started)
            .map(|c| TurnEvent::ToolCallEnd { id: c.id })
            .collect();
        let had_calls = !out.is_empty();
        let reason = self.finish.take().unwrap_or(if had_calls {
            StopReason::ToolUse
        } else {
            StopReason::EndTurn
        });
        // Some servers say `stop` even when they called tools.
        let reason = if had_calls && reason == StopReason::EndTurn {
            StopReason::ToolUse
        } else {
            reason
        };
        out.push(TurnEvent::Finished { reason });
        out
    }

    /// The byte stream ended without `[DONE]`: fine if a finish reason arrived.
    fn end(&mut self) -> Option<Vec<TurnEvent>> {
        if self.done {
            return Some(vec![]);
        }
        self.finish.is_some().then(|| self.close())
    }
}
