//! A minimal MCP server for tests: `initialize`, `tools/list` (one tool, `echo`, over two pages),
//! `tools/call`. Newline-delimited JSON-RPC on stdio.

use serde_json::{json, Value};
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { return };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = msg.get("id").cloned() else {
            continue;
        };
        let p = &msg["params"];
        let result = match msg["method"].as_str().unwrap_or("") {
            "initialize" => json!({
                "protocolVersion": p["protocolVersion"],
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "fake-mcp", "version": "0"}
            }),
            "tools/list" if p.get("cursor").is_none() => json!({
                "tools": [{"name": "echo", "description": "Echo the text back.",
                           "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]},
                           "annotations": {"readOnlyHint": true}}],
                "nextCursor": "2"
            }),
            "tools/list" => json!({
                "tools": [{"name": "fail", "description": "Always fails.", "inputSchema": {"type": "object"}}]
            }),
            "tools/call" => match p["name"].as_str() {
                Some("echo") => {
                    json!({"content": [{"type": "text", "text": format!("echo: {}", p["arguments"]["text"].as_str().unwrap_or(""))}]})
                }
                _ => json!({"content": [{"type": "text", "text": "it failed"}], "isError": true}),
            },
            _ => {
                let _ = writeln!(
                    out,
                    "{}",
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"no"}})
                );
                let _ = out.flush();
                continue;
            }
        };
        let _ = writeln!(out, "{}", json!({"jsonrpc":"2.0","id":id,"result":result}));
        let _ = out.flush();
    }
}
