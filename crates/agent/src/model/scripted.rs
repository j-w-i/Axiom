//! A deterministic provider for tests: replays a script of turns. Each `stream` call takes the
//! next turn; the turn's expectations are checked against the request first and a mismatch
//! fails the turn with `InvalidRequest` ("script expectation failed: …"), so a test whose
//! loop sends the wrong thing fails loudly. The daemon offers it only when
//! `AXIOM_TEST_SCRIPTED_MODEL` names a script file (ADR-0015 §4).

use super::{
    Block, EventStream, ModelProvider, ProviderError, Role, StopReason, TurnEvent, TurnRequest,
};
use axiom_protocol::agent::{ModelErrorKind, ModelInfo, ProviderKind};
use futures_util::future::BoxFuture;
use futures_util::{FutureExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Script {
    #[serde(default = "default_models")]
    pub models: Vec<String>,
    pub turns: Vec<ScriptTurn>,
}

fn default_models() -> Vec<String> {
    vec!["scripted".into()]
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptTurn {
    #[serde(default)]
    pub expect: Expect,
    pub events: Vec<ScriptEvent>,
}

/// Conditions on the request a turn answers. Every set field must hold.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Expect {
    /// The last user message's text contains this.
    pub user_contains: Option<String>,
    /// The system prompt contains this.
    pub system_contains: Option<String>,
    /// These tools are offered.
    #[serde(default)]
    pub tools_include: Vec<String>,
    /// The last message carries a tool result matching this.
    pub tool_result: Option<ExpectToolResult>,
    /// The last user message carries this many images.
    pub user_images: Option<usize>,
    /// Some text block anywhere in the messages contains this (resume tests).
    pub history_contains: Option<String>,
    /// No tools are offered.
    #[serde(default)]
    pub no_tools: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectToolResult {
    pub call_id: Option<String>,
    pub contains: Option<String>,
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ScriptEvent {
    /// Streamed as one text delta per word (so clients see streaming).
    Text {
        text: String,
    },
    /// A whole tool call: start, arguments in two fragments, end.
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename_all = "camelCase")]
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },
    /// Ends the turn; without one the turn ends with `toolUse` if it called a tool, else
    /// `endTurn`.
    Finish {
        reason: StopReason,
    },
    /// Fails the turn.
    Error {
        kind: ModelErrorKind,
        #[serde(default)]
        message: String,
    },
    Delay {
        ms: u64,
    },
    /// Waits until the turn is cancelled (for cancellation tests).
    Hang,
}

pub struct ScriptedProvider {
    id: String,
    models: Vec<String>,
    turns: Mutex<VecDeque<ScriptTurn>>,
    requests: Mutex<Vec<TurnRequest>>,
}

impl ScriptedProvider {
    pub fn new(id: impl Into<String>, script: Script) -> Self {
        Self {
            id: id.into(),
            models: script.models,
            turns: Mutex::new(script.turns.into()),
            requests: Mutex::default(),
        }
    }

    pub fn from_file(id: impl Into<String>, path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read the script {}: {e}", path.display()))?;
        let script: Script = serde_json::from_str(&text)
            .map_err(|e| format!("invalid script {}: {e}", path.display()))?;
        Ok(Self::new(id, script))
    }

    /// Every request received, in order.
    pub fn requests(&self) -> Vec<TurnRequest> {
        lock(&self.requests).clone()
    }

    /// Turns not yet played.
    pub fn remaining(&self) -> usize {
        lock(&self.turns).len()
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn check(expect: &Expect, req: &TurnRequest) -> Result<(), String> {
    if let Some(s) = &expect.system_contains {
        if !req.system.contains(s.as_str()) {
            return Err(format!("system prompt does not contain {s:?}"));
        }
    }
    for t in &expect.tools_include {
        if !req.tools.iter().any(|d| &d.name == t) {
            return Err(format!("tool {t} is not offered"));
        }
    }
    if let Some(s) = &expect.user_contains {
        let last_user = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User && m.blocks.iter().any(|b| matches!(b, Block::Text(_))))
            .map(|m| {
                m.blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::Text(t) => Some(t.text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if !last_user.contains(s.as_str()) {
            return Err(format!(
                "last user text {last_user:?} does not contain {s:?}"
            ));
        }
    }
    if let Some(h) = &expect.history_contains {
        let found = req
            .messages
            .iter()
            .flat_map(|m| &m.blocks)
            .any(|b| match b {
                Block::Text(t) => t.text.contains(h.as_str()),
                Block::ToolResult { content, .. } => content.contains(h.as_str()),
                _ => false,
            });
        if !found {
            return Err(format!("no message contains {h:?}"));
        }
    }
    if expect.no_tools && !req.tools.is_empty() {
        return Err(format!(
            "{} tools are offered, expected none",
            req.tools.len()
        ));
    }
    if let Some(n) = expect.user_images {
        let got = req
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| {
                m.blocks
                    .iter()
                    .filter(|b| matches!(b, Block::Image { .. }))
                    .count()
            })
            .unwrap_or(0);
        if got != n {
            return Err(format!(
                "the last user message has {got} images, expected {n}"
            ));
        }
    }
    if let Some(e) = &expect.tool_result {
        let last = req
            .messages
            .last()
            .map(|m| m.blocks.as_slice())
            .unwrap_or(&[]);
        let found = last.iter().any(|b| match b {
            Block::ToolResult {
                call_id,
                content,
                is_error,
            } => {
                e.call_id.as_ref().is_none_or(|c| c == call_id)
                    && e.contains
                        .as_ref()
                        .is_none_or(|s| content.contains(s.as_str()))
                    && e.is_error.is_none_or(|x| x == *is_error)
            }
            _ => false,
        });
        if !found {
            return Err(format!(
                "the last message has no tool result matching {e:?}; it is {}",
                serde_json::to_string(last).unwrap_or_default()
            ));
        }
    }
    Ok(())
}

fn to_error(kind: ModelErrorKind, message: String) -> ProviderError {
    match kind {
        ModelErrorKind::Auth => ProviderError::Auth,
        ModelErrorKind::RateLimited => ProviderError::RateLimited {
            retry_after: Some(Duration::from_millis(10)),
        },
        ModelErrorKind::ContextTooLong => ProviderError::ContextTooLong,
        ModelErrorKind::Unavailable => ProviderError::Unavailable,
        ModelErrorKind::InvalidRequest => ProviderError::InvalidRequest(message),
        ModelErrorKind::Transport => ProviderError::Transport(message),
    }
}

impl ModelProvider for ScriptedProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Scripted
    }

    fn models(&self) -> BoxFuture<'_, Result<Vec<ModelInfo>, ProviderError>> {
        let list = self
            .models
            .iter()
            .map(|m| ModelInfo {
                id: m.clone(),
                display_name: format!("Scripted ({m})"),
                capabilities: super::profiles::profile(ProviderKind::Scripted, "", m),
            })
            .collect();
        std::future::ready(Ok(list)).boxed()
    }

    fn stream(
        &self,
        req: TurnRequest,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, Result<EventStream, ProviderError>> {
        lock(&self.requests).push(req.clone());
        let turn = lock(&self.turns).pop_front();
        async move {
            let Some(turn) = turn else {
                return Err(ProviderError::InvalidRequest(
                    "script exhausted: the loop asked for more turns than the script has".into(),
                ));
            };
            check(&turn.expect, &req).map_err(|e| {
                ProviderError::InvalidRequest(format!("script expectation failed: {e}"))
            })?;
            let called_tool = turn
                .events
                .iter()
                .any(|e| matches!(e, ScriptEvent::ToolCall { .. }));
            let has_finish = turn
                .events
                .iter()
                .any(|e| matches!(e, ScriptEvent::Finish { .. } | ScriptEvent::Error { .. }));
            let mut events: VecDeque<ScriptEvent> = turn.events.into();
            if !has_finish {
                events.push_back(ScriptEvent::Finish {
                    reason: if called_tool {
                        StopReason::ToolUse
                    } else {
                        StopReason::EndTurn
                    },
                });
            }
            let c = cancel.clone();
            let out = futures_util::stream::unfold(
                (
                    events,
                    VecDeque::<Result<TurnEvent, ProviderError>>::new(),
                    false,
                ),
                move |(mut events, mut queue, mut done)| {
                    let c = c.clone();
                    async move {
                        loop {
                            if let Some(item) = queue.pop_front() {
                                return Some((item, (events, queue, done)));
                            }
                            if done {
                                return None;
                            }
                            let ev = events.pop_front()?;
                            match ev {
                                ScriptEvent::Text { text } => {
                                    let mut start = 0;
                                    for (i, ch) in text.char_indices() {
                                        if ch == ' ' && i > start {
                                            queue.push_back(Ok(TurnEvent::TextDelta {
                                                text: text[start..i].to_string(),
                                            }));
                                            start = i;
                                        }
                                    }
                                    if start < text.len() {
                                        queue.push_back(Ok(TurnEvent::TextDelta {
                                            text: text[start..].to_string(),
                                        }));
                                    }
                                }
                                ScriptEvent::ToolCall { id, name, input } => {
                                    let json = input.to_string();
                                    let mid = json
                                        .char_indices()
                                        .map(|(i, _)| i)
                                        .nth(json.chars().count() / 2)
                                        .unwrap_or(0);
                                    queue.push_back(Ok(TurnEvent::ToolCallStart {
                                        id: id.clone(),
                                        name,
                                    }));
                                    for part in [&json[..mid], &json[mid..]] {
                                        queue.push_back(Ok(TurnEvent::ToolCallArgsDelta {
                                            id: id.clone(),
                                            json_fragment: part.to_string(),
                                        }));
                                    }
                                    queue.push_back(Ok(TurnEvent::ToolCallEnd { id }));
                                }
                                ScriptEvent::Usage {
                                    input_tokens,
                                    output_tokens,
                                } => queue.push_back(Ok(TurnEvent::Usage {
                                    input_tokens,
                                    output_tokens,
                                    cached_input_tokens: None,
                                })),
                                ScriptEvent::Finish { reason } => {
                                    done = true;
                                    queue.push_back(Ok(TurnEvent::Finished { reason }));
                                }
                                ScriptEvent::Error { kind, message } => {
                                    done = true;
                                    queue.push_back(Err(to_error(kind, message)));
                                }
                                ScriptEvent::Delay { ms } => {
                                    tokio::time::sleep(Duration::from_millis(ms)).await;
                                }
                                ScriptEvent::Hang => {
                                    c.cancelled().await;
                                    return None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Message;
    use serde_json::json;

    fn req(text: &str) -> TurnRequest {
        TurnRequest {
            model: "scripted".into(),
            system: "sys".into(),
            messages: vec![Message::user_text(text)],
            tools: vec![],
            max_output_tokens: None,
            temperature: None,
        }
    }

    fn script(v: Value) -> ScriptedProvider {
        ScriptedProvider::new("s", serde_json::from_value(v).unwrap())
    }

    #[tokio::test]
    async fn scripted_replays_text_and_tool_calls_with_a_default_finish() {
        let p = script(json!({"turns":[{"events":[
            {"type":"text","text":"Reading it"},
            {"type":"toolCall","id":"c1","name":"fs.read","input":{"path":"a"}}
        ]}]}));
        let evs: Vec<_> = p
            .stream(req("hi"), CancellationToken::new())
            .await
            .unwrap()
            .collect()
            .await;
        let evs: Vec<TurnEvent> = evs.into_iter().map(Result::unwrap).collect();
        assert_eq!(
            evs[0],
            TurnEvent::TextDelta {
                text: "Reading".into()
            }
        );
        assert_eq!(evs[1], TurnEvent::TextDelta { text: " it".into() });
        let args: String = evs
            .iter()
            .filter_map(|e| match e {
                TurnEvent::ToolCallArgsDelta { json_fragment, .. } => Some(json_fragment.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            serde_json::from_str::<Value>(&args).unwrap(),
            json!({"path":"a"})
        );
        assert_eq!(
            evs.last().unwrap(),
            &TurnEvent::Finished {
                reason: StopReason::ToolUse
            }
        );
        assert_eq!(p.remaining(), 0);
    }

    #[tokio::test]
    async fn scripted_fails_loudly_on_an_unmet_expectation_and_when_exhausted() {
        let p = script(json!({"turns":[{"expect":{"userContains":"build"},"events":[]}]}));
        let e = p
            .stream(req("hello"), CancellationToken::new())
            .await
            .err()
            .unwrap();
        assert!(
            matches!(&e, ProviderError::InvalidRequest(m) if m.contains("expectation failed")),
            "{e:?}"
        );
        let e = p
            .stream(req("x"), CancellationToken::new())
            .await
            .err()
            .unwrap();
        assert!(matches!(&e, ProviderError::InvalidRequest(m) if m.contains("exhausted")));
    }

    #[tokio::test]
    async fn scripted_checks_tool_results() {
        let p = script(
            json!({"turns":[{"expect":{"toolResult":{"callId":"c1","isError":true,
            "contains":"denied"}},"events":[{"type":"text","text":"ok"}]}]}),
        );
        let mut r = req("x");
        r.messages.push(Message {
            role: Role::User,
            blocks: vec![Block::ToolResult {
                call_id: "c1".into(),
                content: "denied by policy".into(),
                is_error: true,
            }],
        });
        assert!(p.stream(r, CancellationToken::new()).await.is_ok());
    }

    #[tokio::test]
    async fn scripted_hang_ends_on_cancel() {
        let p = script(json!({"turns":[{"events":[{"type":"text","text":"a"},{"type":"hang"}]}]}));
        let cancel = CancellationToken::new();
        let mut s = p.stream(req("x"), cancel.clone()).await.unwrap();
        assert!(s.next().await.is_some());
        let c = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            c.cancel();
        });
        let t = std::time::Instant::now();
        assert!(s.next().await.is_none());
        assert!(t.elapsed() < Duration::from_millis(500));
    }
}
