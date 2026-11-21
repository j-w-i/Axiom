//! `axiom mcp`: an MCP server on stdio (ADR-0015 §7; M3-MCP-001) for Claude Code, Codex and any
//! other MCP client. It is a protocol client of a daemon: `tools/list` is `agent/tools` and
//! `tools/call` is `agent/toolCall` with principal `mcp:<client name>`, so the daemon's gate,
//! checkpoints and audit log apply, and an `ask` surfaces in the Axiom window when one is
//! attached (else the call is refused with a reason the model can read).
//!
//! Transport: newline-delimited JSON-RPC 2.0 on stdin/stdout (MCP's stdio transport). Logs go
//! to stderr only.

use axiom_protocol::agent::{Effect, ToolCall, ToolCallParams, Tools};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::WorkspaceId;
use axiom_rpc::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// Protocol revisions this server speaks, newest first.
pub const VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

fn err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn effect_name(e: Effect) -> String {
    serde_json::to_value(e)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default()
}

/// The principal for a client name: `mcp:` and the name's safe characters (≤ 64).
pub fn principal(client_name: &str) -> String {
    let n: String = client_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(64)
        .collect();
    format!("mcp:{}", if n.is_empty() { "client" } else { &n })
}

pub async fn serve(client: Client, ws: WorkspaceId) -> Result<(), String> {
    let stdout = Arc::new(Mutex::new(tokio::io::stdout()));
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let name: Arc<Mutex<String>> = Arc::new(Mutex::new("client".into()));
    while let Some(line) = lines.next_line().await.map_err(|e| e.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        let msg: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => {
                write(&stdout, err(&Value::Null, -32700, "Parse error")).await;
                continue;
            }
        };
        let Some(method) = msg.get("method").and_then(Value::as_str).map(String::from) else {
            continue; // a response to something we never send
        };
        let Some(id) = msg.get("id").cloned() else {
            continue; // notifications: initialized, cancelled
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        let (client, ws, stdout, name) = (client.clone(), ws.clone(), stdout.clone(), name.clone());
        // Calls run concurrently (an approval can take a while); replies carry their ids.
        tokio::spawn(async move {
            let reply = handle(&client, &ws, &name, &method, &id, params).await;
            write(&stdout, reply).await;
        });
    }
    Ok(())
}

async fn write(stdout: &Mutex<tokio::io::Stdout>, v: Value) {
    let mut out = stdout.lock().await;
    let _ = out.write_all(format!("{v}\n").as_bytes()).await;
    let _ = out.flush().await;
}

async fn handle(
    client: &Client,
    ws: &WorkspaceId,
    name: &Mutex<String>,
    method: &str,
    id: &Value,
    params: Value,
) -> Value {
    match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = if VERSIONS.contains(&asked) {
                asked
            } else {
                VERSIONS[0]
            };
            if let Some(n) = params["clientInfo"]["name"].as_str() {
                *name.lock().await = n.to_string();
            }
            ok(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "axiom", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": "Axiom's tools for the open workspace: search, read and patch files, run builds and tasks, inspect toolchains. Writes are checkpointed and may need the user's approval in the Axiom window; a refused call says why.",
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => match client
            .request::<Tools>(WorkspaceRef {
                workspace_id: ws.clone(),
            })
            .await
        {
            Ok(t) => ok(
                id,
                json!({"tools": t.tools.iter().map(|t| json!({
                    "name": t.name,
                    "title": t.name,
                    "description": format!("{} (effect: {})", t.description, effect_name(t.effect)),
                    "inputSchema": t.input_schema,
                    "annotations": {
                        "readOnlyHint": t.effect == Effect::Read,
                        "destructiveHint": matches!(t.effect, Effect::WriteWorkspace | Effect::Delete | Effect::Install | Effect::Deploy | Effect::WriteOutside),
                        "openWorldHint": matches!(t.effect, Effect::Network | Effect::Execute),
                    },
                })).collect::<Vec<_>>()}),
            ),
            Err(e) => err(id, -32603, &e.message),
        },
        "tools/call" => {
            let Some(tool) = params["name"].as_str() else {
                return err(id, -32602, "tools/call needs a `name`.");
            };
            let input = match &params["arguments"] {
                Value::Null => json!({}),
                v => v.clone(),
            };
            let principal = principal(&name.lock().await);
            match client
                .request::<ToolCall>(ToolCallParams {
                    workspace_id: ws.clone(),
                    tool: tool.into(),
                    input,
                    principal,
                })
                .await
            {
                Ok(r) => {
                    let mut result = json!({
                        "content": [{"type": "text", "text": r.text}],
                        "isError": r.is_error,
                    });
                    if r.output.is_object() && !r.is_error {
                        result["structuredContent"] = r.output;
                    }
                    ok(id, result)
                }
                // Refused by the gate: a tool result the model can read, not a protocol error.
                Err(e) => ok(
                    id,
                    json!({"content": [{"type": "text", "text": format!("Not run: {}", e.message)}], "isError": true}),
                ),
            }
        }
        _ => err(id, -32601, &format!("Method not found: {method}")),
    }
}
