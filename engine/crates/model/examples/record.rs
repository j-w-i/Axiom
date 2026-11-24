//! Records answers of a Chat Completions server on this machine, byte for byte, head and framing
//! included: `cargo run -p axiom-model --example record -- 127.0.0.1:1234 crates/model/fixtures/lmstudio-recorded`.
//! Without further arguments everything, else the named ones. It is sent nothing secret and few tokens.
// What it prints is what it is for.
#![allow(clippy::print_stdout)]

use std::{error::Error, fs, path::Path, time::Duration};

use axiom_model::{Block, Message, Profile, Request, Role, Settings, Tool, codec};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::timeout,
};

const SMALL: &str = "qwen2.5-0.5b-instruct-mlx";
const OTHER: &str = "qwen3.5-0.8b-mlx";
const CALLS: &str = "qwen/qwen3-4b-2507";

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [server, dir, only @ ..] = args.as_slice() else {
        return Err("usage: record <host:port> <directory> [name…]".into());
    };
    fs::create_dir_all(dir)?;
    for (name, request) in scenarios()? {
        if !only.is_empty() && !only.iter().any(|o| o == name) {
            continue;
        }
        let body = codec::encode(&request, &Profile::lm_studio())?.body;
        if body.len() < 4096 {
            fs::write(Path::new(dir).join(format!("{name}.request.json")), &body)?;
        }
        let answer = ask(server, "POST /v1/chat/completions", &body).await?;
        println!("{name}: {} bytes", answer.len());
        fs::write(Path::new(dir).join(format!("{name}.http")), answer)?;
    }
    if only.is_empty() || only.iter().any(|o| o == "models") {
        let answer = ask(server, "GET /v1/models", b"").await?;
        println!("models: {} bytes", answer.len());
        fs::write(Path::new(dir).join("models.http"), answer)?;
    }
    Ok(())
}

async fn ask(server: &str, line: &str, body: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut socket = TcpStream::connect(server).await?;
    let head = format!(
        "{line} HTTP/1.1\r\nhost: {server}\r\nuser-agent: axiom-record\r\ncontent-type: application/json\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    socket.write_all(head.as_bytes()).await?;
    socket.write_all(body).await?;
    let mut answer = Vec::new();
    timeout(Duration::from_secs(300), socket.take(1 << 20).read_to_end(&mut answer)).await??;
    Ok(answer)
}

fn scenarios() -> Result<Vec<(&'static str, Request)>, Box<dyn Error>> {
    let ask = |model: &str, system: &str, text: &str, max: u32| Request {
        model: model.into(),
        system: system.into(),
        tools: Vec::new(),
        messages: vec![Message::user(text)],
        settings: Settings { max_output: Some(max), ..Settings::default() },
    };
    let weather = Tool {
        name: "get_weather".into(),
        description: "The weather now at a place.".into(),
        schema: axiom_model::Json::parse(
            r#"{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}"#,
        )?,
    };
    let clock = Tool {
        name: "get_time".into(),
        description: "The time now. Takes nothing.".into(),
        schema: axiom_model::Json::parse(r#"{"type":"object","properties":{}}"#)?,
    };
    let with = |mut request: Request, tools: &[&Tool]| {
        request.tools = tools.iter().map(|t| (*t).clone()).collect();
        request
    };
    let tool = with(ask(CALLS, "Use the tools.", "Weather in Paris?", 120), &[&weather]);
    let mut result = tool.clone();
    result.messages.push(Message {
        role: Role::Assistant,
        blocks: vec![Block::new(axiom_model::Content::ToolCall(axiom_model::ToolCall {
            id: "call_1".into(),
            name: "get_weather".into(),
            raw: r#"{"city":"Paris"}"#.into(),
            args: axiom_model::Args::Ready(serde_json::from_str(r#"{"city":"Paris"}"#)?),
        }))],
    });
    result.messages.push(Message {
        role: Role::User,
        blocks: vec![Block::result("call_1", "18 degrees, rain", false)],
    });
    let mut stop = ask(SMALL, "", "Count from 1 to 9 with spaces between.", 40);
    stop.settings.stop = vec!["4".into()];
    Ok(vec![
        ("text", ask(SMALL, "Answer with one word.", "What is the capital of France?", 16)),
        ("length", ask(SMALL, "", "Tell me a story about a fox.", 6)),
        ("stop-sequence", stop),
        // The model can reason; this server did not make it.
        ("text-second-model", ask(OTHER, "", "What is 2+2? Answer with the number.", 40)),
        ("tool", tool.clone()),
        ("tool-result", result),
        (
            "tool-parallel",
            with(
                ask(
                    CALLS,
                    "Use the tools. Call several at once.",
                    "Weather in Paris and in Rome?",
                    160,
                ),
                &[&weather],
            ),
        ),
        ("tool-no-args", with(ask(CALLS, "Use the tools.", "What time is it?", 80), &[&clock])),
        ("tool-cut", with(ask(CALLS, "Use the tools.", "Weather in Paris?", 12), &[&weather])),
        ("unknown-model", ask("no-such-model", "", "Hi", 4)),
        ("error-request", Request { messages: Vec::new(), ..ask(SMALL, "", "", 4) }),
        ("too-long", ask(SMALL, "", &"word ".repeat(40_000), 4)),
    ])
}
