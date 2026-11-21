//! The OpenAI Chat adapter against a loopback fixture server (M3-MODEL-002). Fixtures are written
//! from the documented chunk format; `ollama_whole.sse` follows Ollama's OpenAI-compatible
//! output (each tool call whole in one chunk, `finish_reason: "stop"`). No network.

mod common;

use axiom_agent::model::openai::{request_body, OpenAiChat};
use axiom_agent::model::{
    Block, Message, ModelProvider, ProviderError, Role, StopReason, ToolDef, TurnEvent, TurnRequest,
};
use axiom_protocol::agent::{ProviderConfig, ProviderKind, Support};
use common::{Canned, Server};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/openai/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn provider(url: &str, key: Option<&str>) -> OpenAiChat {
    OpenAiChat::new(
        &ProviderConfig {
            id: "local".into(),
            kind: ProviderKind::OpenaiChat,
            name: "Local".into(),
            base_url: Some(url.into()),
            api_key_env: None,
            models: vec![],
            capabilities: None,
        },
        key.map(String::from),
    )
    .unwrap()
}

fn turn() -> TurnRequest {
    TurnRequest {
        model: "qwen3:8b".into(),
        system: "You are Axiom's agent.".into(),
        messages: vec![Message::user_text("Read src/main.rs")],
        tools: vec![
            ToolDef {
                name: "fs.read".into(),
                description: "Read a file.".into(),
                input_schema: json!({"type":"object","properties":{"path":{"type":"string"}}}),
            },
            ToolDef {
                name: "workspace.search".into(),
                description: "Search.".into(),
                input_schema: json!({"type":"object"}),
            },
        ],
        max_output_tokens: Some(1024),
        temperature: None,
    }
}

async fn run(name: &str) -> Vec<Result<TurnEvent, ProviderError>> {
    let server = Server::start(vec![Canned::sse(&fixture(name))]).await;
    provider(&format!("{}/v1", server.url), None)
        .stream(turn(), CancellationToken::new())
        .await
        .unwrap()
        .collect()
        .await
}

fn ok(evs: Vec<Result<TurnEvent, ProviderError>>) -> Vec<TurnEvent> {
    evs.into_iter().map(|e| e.expect("no error")).collect()
}

fn args(evs: &[TurnEvent], id: &str) -> Value {
    let s: String = evs
        .iter()
        .filter_map(|e| match e {
            TurnEvent::ToolCallArgsDelta {
                id: i,
                json_fragment,
            } if i == id => Some(json_fragment.as_str()),
            _ => None,
        })
        .collect();
    serde_json::from_str(&s).unwrap()
}

#[tokio::test]
async fn openai_text_with_usage() {
    assert_eq!(
        ok(run("text.sse").await),
        vec![
            TurnEvent::TextDelta {
                text: "Hello".into()
            },
            TurnEvent::TextDelta {
                text: " there.".into()
            },
            TurnEvent::Usage {
                input_tokens: 20,
                output_tokens: 3,
                cached_input_tokens: Some(0)
            },
            TurnEvent::Finished {
                reason: StopReason::EndTurn
            },
        ]
    );
}

#[tokio::test]
async fn openai_one_tool_call_over_several_chunks() {
    let evs = ok(run("tool_call.sse").await);
    assert_eq!(
        evs[0],
        TurnEvent::ToolCallStart {
            id: "call_abc".into(),
            name: "fs.read".into()
        }
    );
    assert_eq!(args(&evs, "call_abc"), json!({"path":"src/main.rs"}));
    assert_eq!(
        &evs[evs.len() - 2..],
        &[
            TurnEvent::ToolCallEnd {
                id: "call_abc".into()
            },
            TurnEvent::Finished {
                reason: StopReason::ToolUse
            }
        ]
    );
}

#[tokio::test]
async fn openai_parallel_tool_calls_by_index() {
    let evs = ok(run("parallel.sse").await);
    assert!(evs.contains(&TurnEvent::ToolCallStart {
        id: "call_2".into(),
        name: "workspace.search".into()
    }));
    assert_eq!(args(&evs, "call_1"), json!({"path":"a.rs"}));
    assert_eq!(args(&evs, "call_2"), json!({"query":"fn main"}));
    let ends: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, TurnEvent::ToolCallEnd { .. }))
        .collect();
    assert_eq!(ends.len(), 2);
}

#[tokio::test]
async fn openai_ollama_whole_arguments_and_stop_with_tools_is_tool_use() {
    let evs = ok(run("ollama_whole.sse").await);
    assert_eq!(args(&evs, "call_x1"), json!({"path":"Cargo.toml"}));
    assert!(evs.contains(&TurnEvent::Usage {
        input_tokens: 310,
        output_tokens: 24,
        cached_input_tokens: None
    }));
    assert_eq!(
        evs.last(),
        Some(&TurnEvent::Finished {
            reason: StopReason::ToolUse
        })
    );
}

#[tokio::test]
async fn openai_error_chunk_and_missing_done() {
    assert_eq!(
        run("error_chunk.sse").await.last(),
        Some(&Err(ProviderError::ContextTooLong))
    );
    // A finish reason without `[DONE]` still finishes.
    assert_eq!(
        ok(run("no_done.sse").await).last(),
        Some(&TurnEvent::Finished {
            reason: StopReason::EndTurn
        })
    );
}

#[tokio::test]
async fn openai_http_error_json_and_bearer_key() {
    let server = Server::start(vec![
        Canned::json(404, r#"{"error":"model 'nope' not found"}"#),
        Canned::sse(&fixture("text.sse")),
    ])
    .await;
    let p = provider(&server.url, Some("sk-local"));
    assert_eq!(
        p.stream(turn(), CancellationToken::new())
            .await
            .err()
            .unwrap(),
        ProviderError::InvalidRequest("model 'nope' not found".into())
    );
    let _ = p
        .stream(turn(), CancellationToken::new())
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;
    let r = &server.requests()[1];
    assert_eq!(r.path, "/chat/completions");
    assert_eq!(r.header("authorization"), Some("Bearer sk-local"));
    assert!(!format!("{p:?}").contains("sk-local"));
}

#[tokio::test]
async fn openai_request_body_matches_the_golden_file() {
    let mut req = turn();
    req.tools.truncate(1);
    req.messages.push(Message {
        role: Role::Assistant,
        blocks: vec![
            Block::text("Let me read it."),
            Block::ToolCall {
                id: "call_abc".into(),
                name: "fs.read".into(),
                input: json!({"path":"src/main.rs"}),
            },
            Block::Opaque {
                provider: "anthropic".into(),
                raw: json!({"type":"thinking"}),
            },
        ],
    });
    req.messages.push(Message {
        role: Role::User,
        blocks: vec![
            Block::ToolResult {
                call_id: "call_abc".into(),
                content: "1  fn main() {}".into(),
                is_error: false,
            },
            Block::text("Now build it."),
        ],
    });
    let golden: Value = serde_json::from_str(&fixture("request.json")).unwrap();
    assert_eq!(request_body(&req, "http://127.0.0.1:11434/v1"), golden);
    // OpenAI's own API takes `max_completion_tokens`.
    let b = request_body(&req, "https://api.openai.com/v1");
    assert_eq!(b["max_completion_tokens"], 1024);
    assert!(b.get("max_tokens").is_none());
}

#[tokio::test]
async fn openai_models_from_the_server_with_the_ollama_profile() {
    let server = Server::start(vec![Canned::json(
        200,
        r#"{"object":"list","data":[{"id":"qwen3:8b","object":"model"},{"id":"llama3.2","object":"model"}]}"#,
    )])
    .await;
    // Profiles are chosen by port; the fixture server's port is random, so check the list
    // and the generic profile here and the Ollama one in `profiles` unit tests.
    let models = provider(&format!("{}/v1", server.url), None)
        .models()
        .await
        .unwrap();
    let ids: Vec<_> = models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["llama3.2", "qwen3:8b"]);
    assert_eq!(models[0].capabilities.tools, Support::Unknown);
    assert_eq!(server.requests()[0].path, "/v1/models");
}
