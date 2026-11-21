//! Capability profiles: what a provider kind / server / model is known to support
//! (ADR-0015 §4). Values are `yes`, `no` or `unknown`; a provider configuration's
//! `capabilities` overrides them.

use axiom_protocol::agent::{Capabilities, CapabilityOverrides, ProviderKind, Support};

/// The server behind an OpenAI-compatible base URL, guessed from its port and host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Server {
    OpenAi,
    Ollama,
    LlamaCpp,
    LmStudio,
    Other,
}

pub fn server_of(base_url: &str) -> Server {
    let Ok(u) = reqwest::Url::parse(base_url) else {
        return Server::Other;
    };
    if u.host_str() == Some("api.openai.com") {
        return Server::OpenAi;
    }
    match u.port_or_known_default() {
        Some(11434) => Server::Ollama,
        Some(8080) => Server::LlamaCpp,
        Some(1234) => Server::LmStudio,
        _ => Server::Other,
    }
}

/// The default profile for a model of this kind (and, for OpenAI-compatible, server).
pub fn profile(kind: ProviderKind, base_url: &str, model: &str) -> Capabilities {
    use Support::*;
    match kind {
        ProviderKind::Anthropic => Capabilities {
            tools: Yes,
            streaming_tool_args: Yes,
            parallel_tool_calls: Yes,
            images: Yes,
            context_window: Some(200_000),
            max_output: Some(if model.contains("haiku") {
                8192
            } else {
                32_000
            }),
        },
        ProviderKind::OpenaiChat => match server_of(base_url) {
            Server::OpenAi => Capabilities {
                tools: Yes,
                streaming_tool_args: Yes,
                parallel_tool_calls: Yes,
                images: Yes,
                context_window: Some(128_000),
                max_output: Some(16_384),
            },
            // Ollama streams a tool call's arguments whole, in one chunk.
            Server::Ollama => Capabilities {
                tools: Unknown,
                streaming_tool_args: No,
                parallel_tool_calls: Unknown,
                images: Unknown,
                context_window: None,
                max_output: None,
            },
            Server::LlamaCpp | Server::LmStudio | Server::Other => Capabilities::default(),
        },
        // An external agent brings its own model; Axiom offers it no tools of its own through
        // the model API (its tools come through ACP and MCP; ADR-0016).
        ProviderKind::Acp => Capabilities {
            tools: No,
            streaming_tool_args: No,
            parallel_tool_calls: No,
            images: Unknown,
            context_window: None,
            max_output: None,
        },
        ProviderKind::Scripted => Capabilities {
            tools: Yes,
            streaming_tool_args: Yes,
            parallel_tool_calls: Yes,
            images: No,
            context_window: Some(32_768),
            max_output: Some(4096),
        },
    }
}

pub fn apply(mut c: Capabilities, o: Option<&CapabilityOverrides>) -> Capabilities {
    let Some(o) = o else { return c };
    if let Some(v) = o.tools {
        c.tools = v;
    }
    if let Some(v) = o.streaming_tool_args {
        c.streaming_tool_args = v;
    }
    if let Some(v) = o.parallel_tool_calls {
        c.parallel_tool_calls = v;
    }
    if let Some(v) = o.images {
        c.images = v;
    }
    if o.context_window.is_some() {
        c.context_window = o.context_window;
    }
    if o.max_output.is_some() {
        c.max_output = o.max_output;
    }
    c
}

/// Whether the loop should offer tools: only a definite `no` turns them off (an `unknown`
/// server is tried; a model that cannot call tools just answers in text).
pub fn offers_tools(c: &Capabilities) -> bool {
    c.tools != Support::No
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servers_are_recognised_by_url() {
        assert_eq!(server_of("https://api.openai.com/v1"), Server::OpenAi);
        assert_eq!(server_of("http://127.0.0.1:11434/v1"), Server::Ollama);
        assert_eq!(server_of("http://localhost:8080/v1"), Server::LlamaCpp);
        assert_eq!(server_of("http://localhost:1234/v1"), Server::LmStudio);
        assert_eq!(server_of("https://example.com/v1"), Server::Other);
    }

    #[test]
    fn overrides_win() {
        let c = apply(
            profile(
                ProviderKind::OpenaiChat,
                "http://127.0.0.1:11434/v1",
                "qwen3",
            ),
            Some(&CapabilityOverrides {
                tools: Some(Support::Yes),
                context_window: Some(40_960),
                ..Default::default()
            }),
        );
        assert_eq!(c.tools, Support::Yes);
        assert_eq!(c.streaming_tool_args, Support::No);
        assert_eq!(c.context_window, Some(40_960));
        assert!(offers_tools(&c));
        let none = apply(
            Capabilities::default(),
            Some(&CapabilityOverrides {
                tools: Some(Support::No),
                ..Default::default()
            }),
        );
        assert!(!offers_tools(&none));
    }
}
