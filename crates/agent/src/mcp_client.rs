//! MCP client (ADR-0015 §7; M3-MCP-001): a user-configured MCP server (settings
//! `agent.mcpServers`: `[{name, argv, env}]`) started over stdio. Its tools join a session's
//! registry as `mcp.<server>.<tool>` with effect `execute`; annotations never lower that.

use crate::tools::{Allowed, Targets, Tool, ToolCtx, ToolError, ToolOutput, ToolSpec};
use axiom_protocol::agent::Effect;
use axiom_protocol::ErrorKind;
use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;

pub const PROTOCOL_VERSION: &str = "2025-06-18";
const START_TIMEOUT: Duration = Duration::from_secs(20);
const CALL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerConfig {
    /// `[a-z0-9-]+`; tools are named `mcp.<name>.<tool>`.
    pub name: String,
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.is_empty()
            || self.name.len() > 40
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(format!(
                "MCP server name `{}` must be 1–40 of a-z, 0-9, -",
                self.name
            ));
        }
        if self.argv.is_empty() || self.argv[0].is_empty() {
            return Err(format!("MCP server `{}` has no command", self.name));
        }
        Ok(())
    }
}

type Waiters = Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>>;

/// One running server.
pub struct McpClient {
    pub name: String,
    stdin: tokio::sync::Mutex<tokio::process::ChildStdin>,
    waiters: Waiters,
    next: AtomicI64,
    /// The server's process, in a process group of its own registered with the daemon's exit
    /// watchdog (AUD-027); killed when the client is dropped.
    _process: crate::process::Supervised,
    /// The server's output ended: it exited or became unusable (AUD-032).
    ended: Arc<std::sync::atomic::AtomicBool>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A tool the server offers.
#[derive(Debug, Clone)]
pub struct RemoteTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl McpClient {
    /// Starts the server, initialises it and lists its tools. It gets the daemon's environment
    /// minus `withhold` (Axiom's model-key variables, AUD-010), plus its configured `env`.
    pub async fn start(
        cfg: &ServerConfig,
        cwd: &std::path::Path,
        withhold: &[String],
    ) -> Result<(Arc<McpClient>, Vec<RemoteTool>), String> {
        cfg.validate()?;
        let mut cmd = tokio::process::Command::new(&cfg.argv[0]);
        for name in withhold {
            cmd.env_remove(name);
        }
        cmd.args(&cfg.argv[1..])
            .envs(&cfg.env)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let (process, pipes) = crate::process::spawn(&mut cmd)
            .map_err(|e| format!("cannot start MCP server `{}`: {e}", cfg.name))?;
        let (Some(stdin), Some(stdout)) = (pipes.stdin, pipes.stdout) else {
            return Err("no stdio".into());
        };
        let waiters: Waiters = Arc::default();
        let w = waiters.clone();
        let ended: Arc<std::sync::atomic::AtomicBool> = Arc::default();
        let end = ended.clone();
        tokio::spawn(async move {
            let mut lines = crate::lines::Lines::new(stdout, crate::lines::MAX_LINE);
            while let Ok(Some(l)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&l) else {
                    continue;
                };
                let Some(id) = v.get("id").and_then(Value::as_i64) else {
                    continue;
                };
                if v.get("method").is_some() {
                    continue; // a request from the server: none are supported (no capabilities)
                }
                let outcome = match (v.get("result"), v.get("error")) {
                    (_, Some(e)) => Err(e["message"].as_str().unwrap_or("error").to_string()),
                    (Some(r), None) => Ok(r.clone()),
                    _ => Ok(Value::Null),
                };
                if let Some(tx) = lock(&w).remove(&id) {
                    let _ = tx.send(outcome);
                }
            }
            end.store(true, Ordering::SeqCst);
            for (_, tx) in lock(&w).drain() {
                let _ = tx.send(Err("the MCP server exited".into()));
            }
        });
        let c = Arc::new(McpClient {
            name: cfg.name.clone(),
            stdin: tokio::sync::Mutex::new(stdin),
            waiters,
            next: AtomicI64::new(1),
            _process: process,
            ended,
        });
        let init = tokio::time::timeout(
            START_TIMEOUT,
            c.request(
                "initialize",
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "axiom", "version": env!("CARGO_PKG_VERSION")},
                }),
            ),
        )
        .await
        .map_err(|_| format!("MCP server `{}` did not initialise in time", cfg.name))??;
        let _ = init;
        c.notify("notifications/initialized").await?;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let params = match &cursor {
                Some(c) => json!({"cursor": c}),
                None => json!({}),
            };
            let page = tokio::time::timeout(START_TIMEOUT, c.request("tools/list", params))
                .await
                .map_err(|_| "tools/list timed out".to_string())??;
            for t in page["tools"].as_array().into_iter().flatten() {
                let Some(name) = t["name"].as_str() else {
                    continue;
                };
                let mut schema = t["inputSchema"].clone();
                if !schema.is_object() {
                    schema = json!({"type": "object", "properties": {}});
                }
                tools.push(RemoteTool {
                    name: name.into(),
                    description: t["description"].as_str().unwrap_or("").into(),
                    input_schema: schema,
                });
            }
            cursor = page["nextCursor"].as_str().map(String::from);
            if cursor.is_none() {
                break;
            }
        }
        Ok((c, tools))
    }

    /// The server has exited (or its output became unusable): start it again rather than
    /// reuse it.
    pub fn is_ended(&self) -> bool {
        self.ended.load(Ordering::SeqCst)
    }

    async fn send(&self, v: Value) -> Result<(), String> {
        let mut s = self.stdin.lock().await;
        s.write_all(format!("{v}\n").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        s.flush().await.map_err(|e| e.to_string())
    }

    async fn notify(&self, method: &str) -> Result<(), String> {
        self.send(json!({"jsonrpc":"2.0","method":method})).await
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.waiters).insert(id, tx);
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await?;
        rx.await.map_err(|_| "the MCP server exited".to_string())?
    }
}

/// A server tool in Axiom's registry.
pub struct McpTool {
    spec: ToolSpec,
    remote: String,
    client: Arc<McpClient>,
}

impl McpTool {
    pub fn new(client: Arc<McpClient>, t: RemoteTool) -> Self {
        Self {
            spec: ToolSpec {
                name: format!("mcp.{}.{}", client.name, t.name),
                description: format!("[MCP server {}] {}", client.name, t.description),
                input_schema: t.input_schema,
                effect: Effect::Execute,
                max_result_bytes: crate::tools::DEFAULT_MAX_RESULT_BYTES,
            },
            remote: t.name,
            client,
        }
    }
}

impl Tool for McpTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn prepare<'a>(
        &'a self,
        _ctx: &'a ToolCtx,
        input: &'a Value,
    ) -> BoxFuture<'a, Result<Targets, ToolError>> {
        Box::pin(async move {
            if !input.is_object() {
                return Err(ToolError::invalid("The input must be an object."));
            }
            // Its arguments decide what it does and are not in the targets: an approval covers
            // this call only (AUD-009). The card shows them.
            let args = serde_json::to_string_pretty(input).unwrap_or_default();
            Ok(Targets {
                argv: Some(vec![
                    format!("mcp:{}", self.client.name),
                    self.remote.clone(),
                ]),
                preview: Some(format!("arguments:\n{args}")),
                opaque: true,
                ..Default::default()
            })
        })
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a ToolCtx,
        input: Value,
        _t: &'a Targets,
        _a: Allowed,
    ) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        Box::pin(async move {
            let call = self.client.request(
                "tools/call",
                json!({"name": self.remote, "arguments": input}),
            );
            let r = tokio::select! {
                r = tokio::time::timeout(CALL_TIMEOUT, call) => r
                    .map_err(|_| ToolError::new(ErrorKind::Timeout, "The MCP server did not answer in time."))?
                    .map_err(|e| ToolError::new(ErrorKind::Io, format!("The MCP server failed: {e}.")))?,
                _ = ctx.cancel.cancelled() => return Err(ToolError::new(ErrorKind::Cancelled, "Cancelled.")),
            };
            let text: String = r["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| match c["type"].as_str() {
                    Some("text") => c["text"].as_str().map(String::from),
                    Some(other) => Some(format!("[{other} content]")),
                    None => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let is_error = r["isError"].as_bool().unwrap_or(false);
            if is_error {
                return Err(ToolError::new(
                    ErrorKind::Io,
                    if text.is_empty() {
                        "The MCP tool failed.".into()
                    } else {
                        text
                    },
                ));
            }
            Ok(ToolOutput {
                summary: format!("{} · {} chars", self.client.name, text.len()),
                output: r
                    .get("structuredContent")
                    .cloned()
                    .unwrap_or_else(|| json!({"text": text})),
                text,
                truncated: false,
            })
        })
    }
}
