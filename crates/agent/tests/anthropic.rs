//! The Anthropic adapter against a loopback fixture server replaying SSE streams written from the
//! documented event format (`docs/protocols/agents.md`; M3-MODEL-001). No network.

mod common;

use axiom_agent::model::anthropic::{request_body, Anthropic};
use axiom_agent::model::{
    Block, Message, ModelProvider, ProviderError, Role, StopReason, ToolDef, TurnEvent, TurnRequest,
};
use axiom_protocol::agent::{ProviderConfig, ProviderKind};
use common::{Canned, Server};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/anthropic/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn provider(url: &str) -> Anthropic {
    Anthropic::new(
        &ProviderConfig {
            id: "anthropic".into(),
            kind: ProviderKind::Anthropic,
            name: "Anthropic".into(),
            base_url: Some(url.into()),
            api_key_env: None,
            models: vec![],
            capabilities: None,
        },
        Some("sk-test-secret".into()),
    )
    .unwrap()
}

fn turn() -> TurnRequest {
    TurnRequest {
        model: "claude-sonnet-4-5".into(),
        system: "You are Axiom's agent.".into(),
        messages: vec![Message::user_text("Read src/main.rs")],
        tools: vec![ToolDef {
            name: "fs.read".into(),
            description: "Read a file.".into(),
            input_schema: json!({"type":"object","properties":{"path":{"type":"string"}}}),
        }],
        max_output_tokens: Some(1024),
        temperature: None,
    }
}

async fn run(canned: Canned) -> (Vec<Result<TurnEvent, ProviderError>>, Server) {
    let server = Server::start(vec![canned]).await;
    let p = provider(&server.url);
    let out = p
        .stream(turn(), CancellationToken::new())
        .await
        .unwrap()
        .collect()
        .await;
    (out, server)
}

fn ok(evs: Vec<Result<TurnEvent, ProviderError>>) -> Vec<TurnEvent> {
    evs.into_iter().map(|e| e.expect("no error")).collect()
}

#[tokio::test]
async fn anthropic_text_stream() {
    let (evs, server) = run(Canned::sse(&fixture("text.sse"))).await;
    assert_eq!(
        ok(evs),
        vec![
            TurnEvent::TextDelta {
                text: "Hello".into()
            },
            TurnEvent::TextDelta {
                text: ", world.".into()
            },
            TurnEvent::Usage {
                input_tokens: 25,
                output_tokens: 7,
                cached_input_tokens: None
            },
            TurnEvent::Finished {
                reason: StopReason::EndTurn
            },
        ]
    );
    let r = &server.requests()[0];
    assert_eq!(r.method, "POST");
    assert_eq!(r.path, "/v1/messages");
    assert_eq!(r.header("x-api-key"), Some("sk-test-secret"));
    assert_eq!(r.header("anthropic-version"), Some("2023-06-01"));
}

#[tokio::test]
async fn anthropic_tool_use_with_split_json_maps_the_name_back() {
    let (evs, _s) = run(Canned::sse(&fixture("tool_use.sse"))).await;
    let evs = ok(evs);
    assert!(evs.contains(&TurnEvent::ToolCallStart {
        id: "toolu_01".into(),
        name: "fs.read".into()
    }));
    let args: String = evs
        .iter()
        .filter_map(|e| match e {
            TurnEvent::ToolCallArgsDelta { json_fragment, .. } => Some(json_fragment.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        serde_json::from_str::<Value>(&args).unwrap(),
        json!({"path":"src/main.rs"})
    );
    assert!(evs.contains(&TurnEvent::ToolCallEnd {
        id: "toolu_01".into()
    }));
    assert!(evs.contains(&TurnEvent::Usage {
        input_tokens: 700,
        output_tokens: 41,
        cached_input_tokens: Some(300)
    }));
    assert_eq!(
        evs.last(),
        Some(&TurnEvent::Finished {
            reason: StopReason::ToolUse
        })
    );
}

#[tokio::test]
async fn anthropic_thinking_blocks_are_opaque_and_replayed_verbatim() {
    let (evs, _s) = run(Canned::sse(&fixture("thinking_tool.sse"))).await;
    let opaque: Vec<Block> = ok(evs)
        .into_iter()
        .filter_map(|e| match e {
            TurnEvent::OpaqueBlock { block } => Some(block),
            _ => None,
        })
        .collect();
    assert_eq!(
        opaque,
        vec![
            Block::Opaque {
                provider: "anthropic".into(),
                raw: json!({"type":"thinking","thinking":"The build failed; run it again.",
                            "signature":"EqQBCgIYAhIM"})
            },
            Block::Opaque {
                provider: "anthropic".into(),
                raw: json!({"type":"redacted_thinking","data":"EmwKAhgBEgy3va3pzix"})
            },
        ]
    );
    // Sent back to Anthropic before the tool call; dropped for any other provider.
    let mut req = turn();
    req.messages.push(Message {
        role: Role::Assistant,
        blocks: vec![
            opaque[0].clone(),
            Block::Opaque {
                provider: "other".into(),
                raw: json!({"x":1}),
            },
            Block::ToolCall {
                id: "toolu_02".into(),
                name: "fs.read".into(),
                input: json!({"path":"a"}),
            },
        ],
    });
    let body = request_body(&req);
    let content = &body["messages"][1]["content"];
    assert_eq!(content.as_array().unwrap().len(), 2);
    assert_eq!(content[0]["type"], "thinking");
    assert_eq!(content[0]["signature"], "EqQBCgIYAhIM");
    assert_eq!(content[1]["name"], "fs_read");
}

#[tokio::test]
async fn anthropic_error_mid_stream_is_the_last_item() {
    let (evs, _s) = run(Canned::sse(&fixture("error_midstream.sse"))).await;
    assert_eq!(
        evs.first(),
        Some(&Ok(TurnEvent::TextDelta {
            text: "Part".into()
        }))
    );
    assert_eq!(evs.last(), Some(&Err(ProviderError::Unavailable)));
}

#[tokio::test]
async fn anthropic_stream_ending_early_is_a_transport_error() {
    let (evs, _s) = run(Canned::sse(&fixture("truncated.sse"))).await;
    assert!(
        matches!(evs.last(), Some(Err(ProviderError::Transport(_)))),
        "{evs:?}"
    );
}

#[tokio::test]
async fn anthropic_http_errors() {
    let server = Server::start(vec![
        Canned::json(
            429,
            r#"{"type":"error","error":{"type":"rate_limit_error","message":"Slow down"}}"#,
        )
        .header("retry-after", "3"),
        Canned::json(
            401,
            r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#,
        ),
    ])
    .await;
    let p = provider(&server.url);
    let e = p
        .stream(turn(), CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert_eq!(
        e,
        ProviderError::RateLimited {
            retry_after: Some(Duration::from_secs(3))
        }
    );
    let e = p
        .stream(turn(), CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert_eq!(e, ProviderError::Auth);
    assert!(!e.to_string().contains("sk-test-secret"));
    assert!(!format!("{p:?}").contains("sk-test-secret"));
}

#[tokio::test]
async fn anthropic_cancel_ends_the_stream_promptly() {
    let mut canned = Canned::sse(&fixture("truncated.sse"));
    canned.hang = true;
    let server = Server::start(vec![canned]).await;
    let p = provider(&server.url);
    let cancel = CancellationToken::new();
    let mut s = p.stream(turn(), cancel.clone()).await.unwrap();
    let c = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        c.cancel();
    });
    let t = Instant::now();
    while s.next().await.is_some() {}
    assert!(
        t.elapsed() < Duration::from_millis(50 + 100),
        "{:?}",
        t.elapsed()
    );
}

#[tokio::test]
async fn anthropic_request_body_matches_the_golden_file() {
    let mut req = turn();
    req.messages.push(Message {
        role: Role::Assistant,
        blocks: vec![
            Block::text("Let me read it."),
            Block::ToolCall {
                id: "toolu_01".into(),
                name: "fs.read".into(),
                input: json!({"path":"src/main.rs"}),
            },
        ],
    });
    req.messages.push(Message {
        role: Role::User,
        blocks: vec![Block::ToolResult {
            call_id: "toolu_01".into(),
            content: "1  fn main() {}".into(),
            is_error: false,
        }],
    });
    let golden: Value = serde_json::from_str(&fixture("request.json")).unwrap();
    assert_eq!(request_body(&req), golden);
}

#[tokio::test]
async fn anthropic_models_listed_from_the_server_with_profiles() {
    let server = Server::start(vec![Canned::json(
        200,
        r#"{"data":[{"type":"model","id":"claude-sonnet-4-5","display_name":"Claude Sonnet 4.5"}],"has_more":false}"#,
    )])
    .await;
    let models = provider(&server.url).models().await.unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].display_name, "Claude Sonnet 4.5");
    assert_eq!(models[0].capabilities.context_window, Some(200_000));
    assert_eq!(server.requests()[0].path, "/v1/models?limit=100");
}
