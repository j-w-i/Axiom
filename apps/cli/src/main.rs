//! `axiom`: the command-line client (ADR-0015 §6; M3-DAEMON-001). Every command is protocol
//! calls to a daemon: the one listening on the data dir's socket if there is one (sharing the
//! desktop app's workspaces, tasks and sessions), else one it starts over stdio.

mod conn;
mod mcp;

use axiom_protocol::agent::{
    AgentEvent, ApprovalDecision, ApprovalRequest, Approve, ApproveParams, EventBody, Models,
    ModelsParams, Send, SendParams, SessionCreate, SessionCreateParams, SessionId, SessionList,
    SessionListParams, TurnEndReason,
};
use axiom_protocol::build::{Run, RunParams, Tasks};
use axiom_protocol::task::{
    Command, ExitedParams, Outcome, OutputParams, Start, StartParams, Stream,
};
use axiom_protocol::workspace::{Open, OpenParams, WorkspaceRef};
use axiom_protocol::{RpcError, WorkspaceId};
use axiom_rpc::Client;
use clap::{Parser, Subcommand};
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "axiom", version, about = "Axiom from the command line")]
struct Cli {
    /// The daemon's data directory (default: AXIOM_DATA_DIR or the platform's).
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a program in a workspace and stream its output; exits with its exit code.
    Task {
        #[command(subcommand)]
        what: TaskCmd,
    },
    /// Run a provider task (e.g. `build`, `test`) in the project's environment.
    Build {
        task: String,
        #[arg(long)]
        provider: Option<String>,
        /// The workspace folder (default: the current directory).
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
    /// Agent sessions.
    Agent {
        #[command(subcommand)]
        what: AgentCmd,
    },
    /// Serve Axiom's tools over MCP on stdio (for Claude Code, Codex and other MCP clients).
    Mcp {
        /// The workspace the tools act on (default: the current directory).
        #[arg(long)]
        workspace: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum TaskCmd {
    Run {
        #[arg(long)]
        workspace: Option<PathBuf>,
        #[arg(required = true, last = true)]
        argv: Vec<String>,
    },
}

#[derive(Subcommand)]
enum AgentCmd {
    /// List sessions (of the workspace, or the conversations without one with --chat).
    Sessions {
        #[arg(long, conflicts_with = "chat")]
        workspace: Option<PathBuf>,
        #[arg(long)]
        chat: bool,
    },
    /// Send a message and stream the answer.
    Send {
        message: String,
        #[arg(long, conflicts_with = "chat")]
        workspace: Option<PathBuf>,
        /// A conversation without a workspace (no tools).
        #[arg(long)]
        chat: bool,
        /// Continue this session (default: a new one).
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
    },
}

/// Text from a model, a file or an agent made safe for a terminal: every control character
/// but newline and tab (C0, DEL, C1, which start escape sequences that could erase or rewrite
/// what is shown, such as an approval prompt) is shown as an escape (`\x1b`) instead of being
/// sent (AUD-045).
fn tty_safe(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {
                let n = c as u32;
                if n <= 0xff {
                    out.push_str(&format!("\\x{n:02x}"));
                } else {
                    out.push_str(&format!("\\u{{{n:x}}}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn fail(e: impl std::fmt::Display) -> ExitCode {
    eprintln!("axiom: {e}");
    ExitCode::from(1)
}

fn rpc(e: RpcError) -> String {
    e.message
}

fn workspace_path(p: Option<PathBuf>) -> Result<String, String> {
    let p = match p {
        Some(p) => p,
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    Ok(p.to_string_lossy().into_owned())
}

async fn open(c: &Client, path: Option<PathBuf>) -> Result<WorkspaceId, String> {
    Ok(c.request::<Open>(OpenParams {
        path: workspace_path(path)?,
    })
    .await
    .map_err(rpc)?
    .workspace_id)
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let data_dir = conn::data_dir(cli.data_dir);
    // `axiom mcp` owns stdin (the MCP stream), so it never asks for approvals itself: they go to
    // the Axiom window.
    let approvals = matches!(cli.command, Cmd::Agent { .. }) && std::io::stdin().is_terminal();
    let conn = match conn::connect(&data_dir, approvals).await {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let code = match cli.command {
        Cmd::Task {
            what: TaskCmd::Run { workspace, argv },
        } => task_run(&conn.client, workspace, argv).await,
        Cmd::Build {
            task,
            provider,
            workspace,
        } => build(&conn.client, task, provider, workspace).await,
        Cmd::Agent { what } => agent(&conn.client, what, approvals).await,
        Cmd::Mcp { workspace } => match open(&conn.client, workspace).await {
            Ok(ws) => mcp::serve(conn.client.clone(), ws)
                .await
                .map(|()| ExitCode::SUCCESS),
            Err(e) => Err(e),
        },
    };
    conn.close().await;
    match code {
        Ok(c) => c,
        Err(e) => fail(e),
    }
}

/// Prints a task's output as it arrives and returns its exit status.
async fn follow(
    c: &Client,
    rx: &mut tokio::sync::broadcast::Receiver<(String, serde_json::Value)>,
    task: &str,
) -> Result<ExitCode, String> {
    loop {
        let (method, params) = rx.recv().await.map_err(|e| e.to_string())?;
        match method.as_str() {
            "task/output" => {
                let o: OutputParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                if o.task_id.0 != task {
                    continue;
                }
                match o.stream {
                    Stream::Stdout => {
                        let mut out = std::io::stdout();
                        let _ = out.write_all(o.data.as_bytes());
                        let _ = out.flush();
                    }
                    Stream::Stderr => {
                        let _ = std::io::stderr().write_all(o.data.as_bytes());
                    }
                }
            }
            "task/exited" => {
                let x: ExitedParams = serde_json::from_value(params).map_err(|e| e.to_string())?;
                if x.task_id.0 != task {
                    continue;
                }
                let _ = c;
                return Ok(match (x.status.outcome, x.status.exit_code) {
                    (Outcome::Succeeded, _) => ExitCode::SUCCESS,
                    (Outcome::Cancelled, _) => ExitCode::from(130),
                    (Outcome::Failed, Some(code)) => ExitCode::from(code.clamp(1, 255) as u8),
                    (Outcome::Failed, None) => ExitCode::from(1),
                });
            }
            _ => {}
        }
    }
}

async fn task_run(
    c: &Client,
    workspace: Option<PathBuf>,
    argv: Vec<String>,
) -> Result<ExitCode, String> {
    let ws = open(c, workspace).await?;
    let mut rx = c.notifications();
    let r = c
        .request::<Start>(StartParams {
            workspace_id: ws,
            command: Command::Argv(argv),
            cwd: None,
            env: Default::default(),
            label: None,
        })
        .await
        .map_err(rpc)?;
    follow(c, &mut rx, &r.task_id.0).await
}

async fn build(
    c: &Client,
    task: String,
    provider: Option<String>,
    workspace: Option<PathBuf>,
) -> Result<ExitCode, String> {
    let ws = open(c, workspace).await?;
    let provider = match provider {
        Some(p) => p,
        None => {
            let tasks = c
                .request::<Tasks>(WorkspaceRef {
                    workspace_id: ws.clone(),
                })
                .await
                .map_err(rpc)?
                .tasks;
            tasks
                .iter()
                .find(|t| t.id == task)
                .map(|t| t.provider_id.clone())
                .ok_or_else(|| {
                    format!(
                        "no provider offers a task `{task}` here (tasks: {})",
                        tasks
                            .iter()
                            .map(|t| format!("{}:{}", t.provider_id, t.id))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?
        }
    };
    let mut rx = c.notifications();
    let r = c
        .request::<Run>(RunParams {
            workspace_id: ws,
            provider_id: provider,
            task,
        })
        .await
        .map_err(rpc)?;
    follow(c, &mut rx, &r.task_id.0).await
}

async fn agent(c: &Client, what: AgentCmd, approvals: bool) -> Result<ExitCode, String> {
    match what {
        AgentCmd::Sessions { workspace, chat } => {
            let ws = if chat {
                None
            } else {
                Some(open(c, workspace).await?)
            };
            let list = c
                .request::<SessionList>(SessionListParams { workspace_id: ws })
                .await
                .map_err(rpc)?;
            for s in list.sessions {
                println!(
                    "{}\t{}\t{}/{}\t{}",
                    s.session_id.0, s.updated_ms, s.provider_id, s.model, s.title
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        AgentCmd::Send {
            message,
            workspace,
            chat,
            session,
            provider,
            model,
        } => {
            let ws = if chat {
                None
            } else {
                Some(open(c, workspace).await?)
            };
            let sid = match session {
                Some(s) => SessionId(s),
                None => {
                    let (provider, model) = pick_model(c, provider, model).await?;
                    c.request::<SessionCreate>(SessionCreateParams {
                        workspace_id: ws.clone(),
                        provider_id: provider,
                        model,
                        title: None,
                    })
                    .await
                    .map_err(rpc)?
                    .session_id
                }
            };
            let mut rx = c.notifications();
            c.request::<Send>(SendParams {
                workspace_id: ws.clone(),
                session_id: sid.clone(),
                text: message,
                attachments: vec![],
                images: vec![],
                from_turn: None,
            })
            .await
            .map_err(rpc)?;
            eprintln!("session {}", sid.0);
            stream_turn(c, &mut rx, &sid, ws, approvals).await
        }
    }
}

/// The given provider/model, or the first model of the first provider that lists one.
async fn pick_model(
    c: &Client,
    provider: Option<String>,
    model: Option<String>,
) -> Result<(String, String), String> {
    if let (Some(p), Some(m)) = (&provider, &model) {
        return Ok((p.clone(), m.clone()));
    }
    let m = c
        .request::<Models>(ModelsParams {
            provider_id: provider.clone(),
        })
        .await
        .map_err(rpc)?;
    for p in m.providers {
        if let Some(first) = p.models.first() {
            return Ok((p.config.id, model.unwrap_or_else(|| first.id.clone())));
        }
    }
    Err("no model is configured: add a provider in Axiom's Settings → Models".into())
}

async fn stream_turn(
    c: &Client,
    rx: &mut tokio::sync::broadcast::Receiver<(String, serde_json::Value)>,
    sid: &SessionId,
    ws: Option<WorkspaceId>,
    approvals: bool,
) -> Result<ExitCode, String> {
    let mut out = std::io::stdout();
    loop {
        let (method, params) = tokio::select! {
            m = rx.recv() => m.map_err(|e| e.to_string())?,
            _ = tokio::signal::ctrl_c() => {
                let _ = c.request::<axiom_protocol::agent::Cancel>(axiom_protocol::agent::SessionRef {
                    workspace_id: ws.clone(),
                    session_id: sid.clone(),
                }).await;
                continue;
            }
        };
        match method.as_str() {
            "agent/event" => {
                let e: AgentEvent = serde_json::from_value(params).map_err(|e| e.to_string())?;
                if &e.session_id != sid {
                    continue;
                }
                match e.event {
                    EventBody::TextDelta { text } => {
                        let text = if std::io::stdout().is_terminal() {
                            tty_safe(&text)
                        } else {
                            text
                        };
                        let _ = out.write_all(text.as_bytes());
                        let _ = out.flush();
                    }
                    EventBody::AssistantText { .. } => {
                        let _ = out.write_all(b"\n");
                    }
                    EventBody::ToolCall { tool, targets, .. } => {
                        eprintln!("  ▸ {} {}", tty_safe(&tool), tty_safe(&targets.join(", ")));
                    }
                    EventBody::ToolResult {
                        is_error, summary, ..
                    } => {
                        eprintln!(
                            "    {} {}",
                            if is_error { "✗" } else { "✓" },
                            tty_safe(&summary)
                        );
                    }
                    EventBody::Checkpoint { checkpoint } => {
                        eprintln!(
                            "  ⚑ checkpoint {} · {}",
                            checkpoint.number, checkpoint.label
                        );
                    }
                    EventBody::Error { message, .. } => eprintln!("axiom: {}", tty_safe(&message)),
                    EventBody::TurnEnd { reason } => {
                        return Ok(match reason {
                            TurnEndReason::Error => ExitCode::from(1),
                            TurnEndReason::Cancelled => ExitCode::from(130),
                            _ => ExitCode::SUCCESS,
                        });
                    }
                    _ => {}
                }
            }
            "agent/approvalRequested" if approvals => {
                let r: ApprovalRequest =
                    serde_json::from_value(params).map_err(|e| e.to_string())?;
                if r.session_id.as_ref() != Some(sid) {
                    continue;
                }
                let decision = ask(&r).await;
                let _ = c
                    .request::<Approve>(ApproveParams {
                        request_id: r.request_id,
                        decision,
                    })
                    .await;
            }
            _ => {}
        }
    }
}

/// The approval prompt, every untrusted part made terminal-safe and the targets one per line
/// (an argument cannot pass for another target).
fn prompt_text(r: &ApprovalRequest) -> String {
    let effect = serde_json::to_value(r.effect)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let mut t = String::new();
    if let Some(p) = &r.preview {
        t.push_str(&tty_safe(p));
        t.push('\n');
    }
    t.push_str(&format!("Allow {} ({effect}) on:\n", tty_safe(&r.tool)));
    for target in &r.targets {
        t.push_str(&format!("  {}\n", tty_safe(target)));
    }
    t.push_str(&format!(
        "[y] once{} / [N]o: ",
        if r.can_remember {
            " / [s] for this session"
        } else {
            ""
        }
    ));
    t
}

/// Asks on the terminal. Anything but y/s is a denial.
async fn ask(r: &ApprovalRequest) -> ApprovalDecision {
    eprint!("{}", prompt_text(r));
    let _ = std::io::stderr().flush();
    let line = tokio::task::spawn_blocking(|| {
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
        s
    })
    .await
    .unwrap_or_default();
    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => ApprovalDecision::Once,
        "s" if r.can_remember => ApprovalDecision::Session,
        _ => ApprovalDecision::Deny,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_sequences_are_shown_not_sent() {
        assert_eq!(tty_safe("ok\n\tline"), "ok\n\tline");
        assert_eq!(tty_safe("\x1b[2K\x1b[1Ahi\r"), "\\x1b[2K\\x1b[1Ahi\\x0d");
        assert_eq!(tty_safe("\u{9b}31m\u{7f}"), "\\x9b31m\\x7f");
        assert_eq!(tty_safe("é ✓"), "é ✓");
    }

    #[test]
    fn the_approval_prompt_cannot_be_rewritten_by_its_content() {
        let r = ApprovalRequest {
            request_id: "r1".into(),
            session_id: None,
            principal: "session:s".into(),
            tool: "task.run".into(),
            effect: axiom_protocol::agent::Effect::Execute,
            targets: vec!["cargo test\x1b[2K\r rm -rf /".into()],
            argv: None,
            preview: Some("\x1b]0;title\x07diff".into()),
            rule: None,
            checkpoint_planned: true,
            can_remember: false,
        };
        let p = prompt_text(&r);
        assert!(!p.chars().any(|c| c.is_control() && c != '\n'), "{p:?}");
        assert!(p.contains("\\x1b[2K\\x0d rm -rf /"), "{p}");
        assert!(p.contains("\\x1b]0;title\\x07diff"), "{p}");
    }
}
