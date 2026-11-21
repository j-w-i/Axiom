//! Anthropic Messages API adapter (streamed). Event format: `message_start`,
//! `content_block_start|delta|stop`, `message_delta`, `message_stop`, `ping`, `error`.
//! Thinking blocks are kept as [`Block::Opaque`] (with their signatures) and sent back verbatim.

use super::http::{check_base_url, client, status_error, transport};
use super::{
    profiles, sse, Block, EventStream, ModelProvider, ProviderError, Role, StopReason, ToolNames,
    TurnEvent, TurnRequest,
};
use axiom_protocol::agent::{CapabilityOverrides, ModelInfo, ProviderConfig, ProviderKind};
use futures_util::future::BoxFuture;
use futures_util::{FutureExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use tokio_util::sync::CancellationToken;

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
pub const API_VERSION: &str = "2023-06-01";
/// Provider tag on opaque blocks this adapter produces and accepts.
pub const OPAQUE_TAG: &str = "anthropic";
const DEFAULT_MAX_TOKENS: u32 = 8192;

pub struct Anthropic {
    id: String,
    base_url: String,
    api_key: Option<String>,
    models: Vec<String>,
    overrides: Option<CapabilityOverrides>,
    http: reqwest::Client,
}

impl std::fmt::Debug for Anthropic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Anthropic")
            .field("id", &self.id)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl Anthropic {
    /// Errors are one sentence for the user (a bad base URL).
    pub fn new(config: &ProviderConfig, api_key: Option<String>) -> Result<Self, String> {
        let base_url = check_base_url(config.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL))?;
        Ok(Self {
            id: config.id.clone(),
            base_url,
            api_key,
            models: config.models.clone(),
            overrides: config.capabilities.clone(),
            http: client().map_err(|e| e.to_string())?,
        })
    }

    fn info(&self, id: &str, display: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            display_name: display.to_string(),
            capabilities: profiles::apply(
                profiles::profile(ProviderKind::Anthropic, &self.base_url, id),
                self.overrides.as_ref(),
            ),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut r = self
            .http
            .request(method, format!("{}{path}", self.base_url))
            .header("anthropic-version", API_VERSION);
        if let Some(k) = &self.api_key {
            r = r.header("x-api-key", k);
        }
        r
    }
}

impl ModelProvider for Anthropic {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn models(&self) -> BoxFuture<'_, Result<Vec<ModelInfo>, ProviderError>> {
        async move {
            if !self.models.is_empty() {
                return Ok(self.models.iter().map(|m| self.info(m, m)).collect());
            }
            let resp = self
                .request(reqwest::Method::GET, "/v1/models?limit=100")
                .send()
                .await
                .map_err(transport)?;
            let status = resp.status();
            let retry = header(&resp, "retry-after");
            let body = resp.text().await.map_err(transport)?;
            if !status.is_success() {
                return Err(status_error(status, retry.as_deref(), &body));
            }
            let v: Value = serde_json::from_str(&body)
                .map_err(|e| ProviderError::Transport(format!("bad model list: {e}")))?;
            Ok(v["data"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|m| {
                            let id = m["id"].as_str()?;
                            Some(self.info(id, m["display_name"].as_str().unwrap_or(id)))
                        })
                        .collect()
                })
                .unwrap_or_default())
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
            let body = request_body_with(&req, &names);
            let send = self
                .request(reqwest::Method::POST, "/v1/messages")
                .header("content-type", "application/json")
                .header("accept", "text/event-stream")
                .body(body.to_string())
                .send();
            let resp = tokio::select! {
                r = send => r.map_err(transport)?,
                _ = cancel.cancelled() => {
                    return Ok(futures_util::stream::empty().boxed());
                }
            };
            let status = resp.status();
            if !status.is_success() {
                let retry = header(&resp, "retry-after");
                let text = resp.text().await.unwrap_or_default();
                return Err(status_error(status, retry.as_deref(), &text));
            }
            let events = sse::events(resp.bytes_stream().boxed());
            let state = Decoder::new(names);
            let out = futures_util::stream::unfold(
                (events.boxed(), state, VecDeque::new(), false),
                |(mut events, mut st, mut queue, mut done)| async move {
                    loop {
                        if let Some(item) = queue.pop_front() {
                            return Some((item, (events, st, queue, done)));
                        }
                        if done {
                            return None;
                        }
                        match events.next().await {
                            Some(Ok(ev)) => match st.event(&ev) {
                                Ok(evs) => {
                                    if st.finished {
                                        done = true;
                                    }
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
                                if !st.finished {
                                    queue.push_back(Err(ProviderError::Transport(
                                        "the response ended before the message did".into(),
                                    )));
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

fn header(resp: &reqwest::Response, name: &str) -> Option<String> {
    resp.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(String::from)
}

/// The Messages API request body for a turn (public for golden tests).
pub fn request_body(req: &TurnRequest) -> Value {
    request_body_with(req, &ToolNames::new(&req.tools))
}

fn request_body_with(req: &TurnRequest, names: &ToolNames) -> Value {
    let messages: Vec<Value> = req
        .messages
        .iter()
        .filter_map(|m| {
            let content: Vec<Value> = m
                .blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Text(t) if t.text.is_empty() => None,
                    Block::Text(t) => Some(json!({"type":"text","text":t.text})),
                    Block::ToolCall { id, name, input } => Some(json!({
                        "type":"tool_use","id":id,"name":names.wire(name),"input":input
                    })),
                    Block::ToolResult {
                        call_id,
                        content,
                        is_error,
                    } => Some(json!({
                        "type":"tool_result","tool_use_id":call_id,"content":content,
                        "is_error":is_error
                    })),
                    Block::Opaque { provider, raw } if provider == OPAQUE_TAG => Some(raw.clone()),
                    Block::Image { media_type, data } => Some(json!({
                        "type":"image",
                        "source":{"type":"base64","media_type":media_type,"data":data}
                    })),
                    Block::Opaque { .. } => None,
                })
                .collect();
            if content.is_empty() {
                return None;
            }
            let role = match m.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            Some(json!({"role":role,"content":content}))
        })
        .collect();
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_output_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": messages,
        "stream": true,
    });
    if !req.system.is_empty() {
        body["system"] = json!(req.system);
    }
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| {
                    json!({"name":names.wire(&t.name),"description":t.description,
                           "input_schema":t.input_schema})
                })
                .collect(),
        );
    }
    if let Some(t) = req.temperature {
        body["temperature"] = json!(t);
    }
    body
}

enum Open {
    Text,
    Tool { id: String },
    Thinking { thinking: String, signature: String },
    Raw(Value),
}

/// Turns SSE events into [`TurnEvent`]s.
struct Decoder {
    names: ToolNames,
    open: HashMap<u64, Open>,
    input_tokens: u32,
    cached: Option<u32>,
    stop: Option<StopReason>,
    finished: bool,
}

impl Decoder {
    fn new(names: ToolNames) -> Self {
        Self {
            names,
            open: HashMap::new(),
            input_tokens: 0,
            cached: None,
            stop: None,
            finished: false,
        }
    }

    fn event(&mut self, ev: &sse::SseEvent) -> Result<Vec<TurnEvent>, ProviderError> {
        let v: Value = serde_json::from_str(&ev.data)
            .map_err(|e| ProviderError::Transport(format!("bad event data: {e}")))?;
        let kind = ev
            .event
            .as_deref()
            .or_else(|| v["type"].as_str())
            .unwrap_or("");
        let idx = v["index"].as_u64().unwrap_or(0);
        let mut out = Vec::new();
        match kind {
            "message_start" => {
                let u = &v["message"]["usage"];
                let n = |k: &str| u[k].as_u64().unwrap_or(0) as u32;
                let cached = n("cache_read_input_tokens");
                self.input_tokens = n("input_tokens") + cached + n("cache_creation_input_tokens");
                self.cached = u.get("cache_read_input_tokens").map(|_| cached);
            }
            "content_block_start" => {
                let b = &v["content_block"];
                let open = match b["type"].as_str().unwrap_or("") {
                    "text" => {
                        if let Some(t) = b["text"].as_str().filter(|t| !t.is_empty()) {
                            out.push(TurnEvent::TextDelta { text: t.into() });
                        }
                        Open::Text
                    }
                    "tool_use" => {
                        let id = b["id"].as_str().unwrap_or_default().to_string();
                        out.push(TurnEvent::ToolCallStart {
                            id: id.clone(),
                            name: self.names.axiom(b["name"].as_str().unwrap_or_default()),
                        });
                        Open::Tool { id }
                    }
                    "thinking" => Open::Thinking {
                        thinking: b["thinking"].as_str().unwrap_or_default().into(),
                        signature: b["signature"].as_str().unwrap_or_default().into(),
                    },
                    _ => Open::Raw(b.clone()),
                };
                self.open.insert(idx, open);
            }
            "content_block_delta" => {
                let d = &v["delta"];
                match (self.open.get_mut(&idx), d["type"].as_str().unwrap_or("")) {
                    (Some(Open::Text), "text_delta") => out.push(TurnEvent::TextDelta {
                        text: d["text"].as_str().unwrap_or_default().into(),
                    }),
                    (Some(Open::Tool { id }), "input_json_delta") => {
                        out.push(TurnEvent::ToolCallArgsDelta {
                            id: id.clone(),
                            json_fragment: d["partial_json"].as_str().unwrap_or_default().into(),
                        })
                    }
                    (Some(Open::Thinking { thinking, .. }), "thinking_delta") => {
                        thinking.push_str(d["thinking"].as_str().unwrap_or_default())
                    }
                    (Some(Open::Thinking { signature, .. }), "signature_delta") => {
                        signature.push_str(d["signature"].as_str().unwrap_or_default())
                    }
                    _ => {}
                }
            }
            "content_block_stop" => match self.open.remove(&idx) {
                Some(Open::Tool { id }) => out.push(TurnEvent::ToolCallEnd { id }),
                Some(Open::Thinking {
                    thinking,
                    signature,
                }) => out.push(TurnEvent::OpaqueBlock {
                    block: Block::Opaque {
                        provider: OPAQUE_TAG.into(),
                        raw: json!({"type":"thinking","thinking":thinking,"signature":signature}),
                    },
                }),
                Some(Open::Raw(raw)) => out.push(TurnEvent::OpaqueBlock {
                    block: Block::Opaque {
                        provider: OPAQUE_TAG.into(),
                        raw,
                    },
                }),
                Some(Open::Text) | None => {}
            },
            "message_delta" => {
                if let Some(r) = v["delta"]["stop_reason"].as_str() {
                    self.stop = Some(match r {
                        "end_turn" | "stop_sequence" => StopReason::EndTurn,
                        "tool_use" => StopReason::ToolUse,
                        "max_tokens" => StopReason::MaxTokens,
                        "refusal" => StopReason::Refusal,
                        other => StopReason::Other(other.into()),
                    });
                }
                if let Some(o) = v["usage"]["output_tokens"].as_u64() {
                    out.push(TurnEvent::Usage {
                        input_tokens: self.input_tokens,
                        output_tokens: o as u32,
                        cached_input_tokens: self.cached,
                    });
                }
            }
            "message_stop" => {
                self.finished = true;
                out.push(TurnEvent::Finished {
                    reason: self.stop.take().unwrap_or(StopReason::EndTurn),
                });
            }
            "error" => {
                let e = &v["error"];
                let msg = e["message"].as_str().unwrap_or("error").to_string();
                return Err(match e["type"].as_str().unwrap_or("") {
                    "overloaded_error" | "api_error" => ProviderError::Unavailable,
                    "rate_limit_error" => ProviderError::RateLimited { retry_after: None },
                    "authentication_error" | "permission_error" => ProviderError::Auth,
                    "invalid_request_error" if super::http::looks_like_context_overflow(&msg) => {
                        ProviderError::ContextTooLong
                    }
                    "invalid_request_error" => ProviderError::InvalidRequest(msg),
                    _ => ProviderError::Transport(msg),
                });
            }
            _ => {} // ping and anything new
        }
        Ok(out)
    }
}
