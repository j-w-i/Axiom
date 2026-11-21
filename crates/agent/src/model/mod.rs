//! Model access: the normalised message model and the [`ModelProvider`] trait
//! (`docs/protocols/agents.md` §Model provider interface), with the adapters.

pub mod anthropic;
pub(crate) mod http;
pub mod profiles;
pub mod scripted;
pub mod sse;

use axiom_protocol::agent::{ModelInfo, ProviderKind};
use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    pub blocks: Vec<Block>,
}

impl Message {
    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            blocks: vec![Block::text(text)],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Block {
    Text(TextBlock),
    #[serde(rename_all = "camelCase")]
    ToolCall {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename_all = "camelCase")]
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
    /// Provider-native content that must be sent back verbatim to the same provider kind and is
    /// dropped for any other (Anthropic `thinking` / `redacted_thinking`).
    Opaque {
        provider: String,
        raw: Value,
    },
}

/// Newtype so `Block::Text` serialises as `{"type":"text","text":…}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
}

impl Block {
    pub fn text(s: impl Into<String>) -> Self {
        Block::Text(TextBlock { text: s.into() })
    }
}

/// A tool offered to the model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    /// Axiom's dotted name (`fs.read`).
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolDef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TurnEvent {
    TextDelta {
        text: String,
    },
    ToolCallStart {
        id: String,
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    ToolCallArgsDelta {
        id: String,
        json_fragment: String,
    },
    ToolCallEnd {
        id: String,
    },
    OpaqueBlock {
        block: Block,
    },
    #[serde(rename_all = "camelCase")]
    Usage {
        input_tokens: u32,
        output_tokens: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cached_input_tokens: Option<u32>,
    },
    Finished {
        reason: StopReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProviderError {
    #[error("authentication failed (check the API key)")]
    Auth,
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("the conversation is too long for the model's context window")]
    ContextTooLong,
    #[error("the model service is unavailable")]
    Unavailable,
    #[error("the model service rejected the request: {0}")]
    InvalidRequest(String),
    #[error("cannot reach the model service: {0}")]
    Transport(String),
}

impl ProviderError {
    pub fn kind(&self) -> axiom_protocol::agent::ModelErrorKind {
        use axiom_protocol::agent::ModelErrorKind as K;
        match self {
            Self::Auth => K::Auth,
            Self::RateLimited { .. } => K::RateLimited,
            Self::ContextTooLong => K::ContextTooLong,
            Self::Unavailable => K::Unavailable,
            Self::InvalidRequest(_) => K::InvalidRequest,
            Self::Transport(_) => K::Transport,
        }
    }
}

pub type EventStream = BoxStream<'static, Result<TurnEvent, ProviderError>>;

/// One model service. Object-safe (boxed futures) so the daemon can hold `Arc<dyn …>`.
pub trait ModelProvider: Send + Sync {
    /// The provider configuration's id.
    fn id(&self) -> &str;
    fn kind(&self) -> ProviderKind;
    fn models(&self) -> BoxFuture<'_, Result<Vec<ModelInfo>, ProviderError>>;
    /// Streams one assistant turn. Dropping the stream or cancelling `cancel` ends it and
    /// aborts the request.
    fn stream(
        &self,
        req: TurnRequest,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, Result<EventStream, ProviderError>>;
}

/// Model APIs restrict tool names to `[a-zA-Z0-9_-]{1,64}`; Axiom's are dotted. Built per
/// request from its tool list; maps both ways.
#[derive(Debug, Default)]
pub(crate) struct ToolNames {
    to_wire: HashMap<String, String>,
    from_wire: HashMap<String, String>,
}

impl ToolNames {
    pub(crate) fn new(tools: &[ToolDef]) -> Self {
        let mut n = ToolNames::default();
        for t in tools {
            let base: String = t
                .name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .take(60)
                .collect();
            let mut wire = base.clone();
            let mut i = 2;
            while n.from_wire.contains_key(&wire) {
                wire = format!("{base}{i}");
                i += 1;
            }
            n.to_wire.insert(t.name.clone(), wire.clone());
            n.from_wire.insert(wire, t.name.clone());
        }
        n
    }

    /// A name not in the request's tool list (an old call in the history) is mapped the same
    /// way, without uniqueness.
    pub(crate) fn wire(&self, name: &str) -> String {
        self.to_wire.get(name).cloned().unwrap_or_else(|| {
            name.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect()
        })
    }

    /// The model may answer with a name we never offered; it passes through unchanged and the
    /// registry reports it as unknown.
    pub(crate) fn axiom(&self, wire: &str) -> String {
        self.from_wire
            .get(wire)
            .cloned()
            .unwrap_or_else(|| wire.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_names_round_trip_and_stay_unique() {
        let t = |n: &str| ToolDef {
            name: n.into(),
            description: String::new(),
            input_schema: json!({}),
        };
        let names = ToolNames::new(&[t("fs.read"), t("fs_read"), t("mcp.git.log")]);
        assert_eq!(names.wire("fs.read"), "fs_read");
        assert_eq!(names.wire("fs_read"), "fs_read2");
        assert_eq!(names.axiom("fs_read"), "fs.read");
        assert_eq!(names.axiom("fs_read2"), "fs_read");
        assert_eq!(names.axiom("mcp_git_log"), "mcp.git.log");
        assert_eq!(names.axiom("unknown"), "unknown");
    }

    #[test]
    fn blocks_serialise_with_a_type_tag() {
        assert_eq!(
            serde_json::to_value(Block::text("hi")).unwrap(),
            json!({"type":"text","text":"hi"})
        );
        let e: TurnEvent =
            serde_json::from_value(json!({"type":"finished","reason":"toolUse"})).unwrap();
        assert_eq!(
            e,
            TurnEvent::Finished {
                reason: StopReason::ToolUse
            }
        );
    }
}
