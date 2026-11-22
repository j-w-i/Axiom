//! The agent loop (ADR-0007, ADR-0015 §9; M3-LOOP-001): one turn is
//!
//! ```text
//! user event → [ context → model stream → assistant record → for each tool call:
//!   prepare → toolCall event → gate → checkpoint (first non-read of the turn) → execute →
//!   post-state → toolResult event ] → tool results record → repeat → turnEnd → compaction?
//! ```
//!
//! Every write is preceded by the turn's checkpoint; a refused call is a tool error the model
//! sees; tool intent is journalled before execution and the result after. The loop never
//! retries a round after a tool ran; a rate limit before any tool ran is retried once when the
//! server's wait is at most 30 s.

use crate::checkpoint::{CheckpointError, Shadow};
use crate::gate::{self, BeforeExecute, Gate, Policies};
use crate::live::Live;
use crate::model::{
    Block, Message, ModelProvider, ProviderError, StopReason, TurnEvent, TurnRequest,
};
use crate::session::{self, Record, JOURNAL_VERSION};
use crate::tools::{Prepared, Registry, ToolCtx, ToolError};
use axiom_protocol::agent::{
    Capabilities, Effect, EventBody, ImageRef, ModelErrorKind, TurnEndReason,
};
use axiom_protocol::ErrorKind;
use futures_util::future::BoxFuture;
use futures_util::StreamExt;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAX_TOOL_ROUNDS: u32 = 50;
const RATE_LIMIT_RETRY_MAX: Duration = Duration::from_secs(30);
/// When the context window is unknown.
pub const DEFAULT_CONTEXT_WINDOW: u32 = 32_768;
const COMPACT_AT: f64 = 0.75;
const DEFAULT_MAX_OUTPUT: u32 = 8192;

/// What a turn needs from the daemon.
pub struct Turn<'a> {
    pub live: &'a Live,
    pub provider: Arc<dyn ModelProvider>,
    pub model: String,
    pub capabilities: Capabilities,
    pub system: String,
    /// `None`: a conversation without a workspace (no tools).
    pub tools: Option<ToolEnv<'a>>,
    pub cancel: tokio_util::sync::CancellationToken,
}

pub struct ToolEnv<'a> {
    pub registry: &'a Registry,
    pub gate: &'a Gate,
    pub policies: Policies,
    pub ctx: ToolCtx,
    pub shadow: Arc<Shadow>,
    pub audit: PathBuf,
}

/// What the user sent.
pub struct UserInput {
    pub text: String,
    pub attachments: Vec<axiom_protocol::agent::Attachment>,
    /// The attachments' contents, already read (added to the context).
    pub attached_text: Option<String>,
    pub images: Vec<ImageRef>,
    pub from_turn: Option<u32>,
}

/// Starts a turn: the rewind and user events are journalled before this returns, so a client
/// sees them in order. `run` then drives the turn to its end.
pub fn begin(live: &Live, input: &UserInput) -> Result<u32, session::JournalError> {
    if let Some(n) = input.from_turn {
        live.rewind(n)?;
    }
    let ev = live.emit(EventBody::User {
        text: input.text.clone(),
        attachments: input.attachments.clone(),
        images: input.images.clone(),
    })?;
    if let Some(t) = &input.attached_text {
        live.record(Record::Attached {
            v: JOURNAL_VERSION,
            turn: ev.turn,
            at_seq: ev.seq,
            text: t.clone(),
        })?;
    }
    Ok(ev.turn)
}

/// The checkpoint before the first writing tool of the turn.
pub(crate) struct Checkpoint<'a> {
    live: &'a Live,
    shadow: &'a Shadow,
    turn: u32,
    done: AtomicBool,
}

impl<'a> Checkpoint<'a> {
    pub(crate) fn new(live: &'a Live, shadow: &'a Shadow, turn: u32) -> Self {
        Self {
            live,
            shadow,
            turn,
            done: AtomicBool::new(false),
        }
    }
}

impl BeforeExecute for Checkpoint<'_> {
    fn before<'b>(&'b self, p: &'b Prepared) -> BoxFuture<'b, Result<(), ToolError>> {
        Box::pin(async move {
            if p.effect == Effect::Read || self.done.load(Ordering::SeqCst) {
                return Ok(());
            }
            let info = self
                .shadow
                .create(&self.live.id.0, &format!("before {}", p.name), self.turn)
                .await
                .map_err(checkpoint_error)?;
            self.done.store(true, Ordering::SeqCst);
            let _ = self.live.emit(EventBody::Checkpoint { checkpoint: info });
            Ok(())
        })
    }
}

pub fn checkpoint_error(e: CheckpointError) -> ToolError {
    match e {
        CheckpointError::GitMissing => ToolError::new(
            ErrorKind::PrerequisiteMissing,
            "Axiom cannot take a checkpoint because git is not installed, so it will not change files.",
        )
        .hint("Ask the user to install git; until then only read tools work."),
        other => ToolError::new(
            ErrorKind::Io,
            format!("The checkpoint failed ({other}), so the tool did not run."),
        ),
    }
}

fn model_kind(e: &ProviderError) -> ModelErrorKind {
    e.kind()
}

/// Builds the model context from the journal.
fn context(live: &Live) -> Vec<Message> {
    let records = live.records();
    session::context(&records, &|r: &ImageRef| live.journal.get_image(r).ok())
}

/// Runs the turn to its end. Errors are events; the turn always ends with `turnEnd`.
pub async fn run(t: Turn<'_>, turn: u32) {
    let reason = drive(&t, turn).await;
    let _ = t.live.emit(EventBody::TurnEnd { reason });
    if reason != TurnEndReason::Cancelled && reason != TurnEndReason::Error {
        if let Err(e) = maybe_compact(&t).await {
            tracing::warn!("compaction failed: {e}");
        }
    }
}

fn emit_error(live: &Live, kind: &str, model: Option<ModelErrorKind>, message: String) {
    let _ = live.emit(EventBody::Error {
        kind: kind.into(),
        model_kind: model,
        message,
    });
}

async fn drive(t: &Turn<'_>, turn: u32) -> TurnEndReason {
    let live = t.live;
    let offer_tools = t.tools.is_some() && crate::model::profiles::offers_tools(&t.capabilities);
    let ckpt = t.tools.as_ref().map(|e| Checkpoint {
        live,
        shadow: &e.shadow,
        turn,
        done: AtomicBool::new(false),
    });
    let mut rounds = 0u32;
    let mut tools_ran = false;
    let mut retried = false;
    loop {
        if t.cancel.is_cancelled() {
            return TurnEndReason::Cancelled;
        }
        let req = TurnRequest {
            model: t.model.clone(),
            system: t.system.clone(),
            messages: context(live),
            tools: match (&t.tools, offer_tools) {
                (Some(e), true) => e.registry.defs(),
                _ => vec![],
            },
            max_output_tokens: Some(
                t.capabilities
                    .max_output
                    .unwrap_or(DEFAULT_MAX_OUTPUT)
                    .min(32_000),
            ),
            temperature: None,
        };
        let mut stream = match t.provider.stream(req, t.cancel.child_token()).await {
            Ok(s) => s,
            Err(ProviderError::RateLimited { retry_after })
                if !tools_ran
                    && !retried
                    && retry_after.is_some_and(|d| d <= RATE_LIMIT_RETRY_MAX) =>
            {
                retried = true;
                let wait = retry_after.unwrap_or_default();
                tokio::select! {
                    _ = tokio::time::sleep(wait) => continue,
                    _ = t.cancel.cancelled() => return TurnEndReason::Cancelled,
                }
            }
            Err(e) => {
                emit_error(
                    live,
                    "model_error",
                    Some(model_kind(&e)),
                    sentence(&e.to_string()),
                );
                return TurnEndReason::Error;
            }
        };

        // One model round.
        let mut blocks: Vec<Block> = Vec::new();
        let mut text = String::new();
        let mut calls: Vec<(String, String, String)> = Vec::new(); // (id, name, args)
        let mut stop: Option<StopReason> = None;
        let mut failed: Option<ProviderError> = None;
        let flush = |text: &mut String, blocks: &mut Vec<Block>| {
            if !text.is_empty() {
                let t2 = std::mem::take(text);
                let _ = live.emit(EventBody::AssistantText {
                    text: t2.clone(),
                    kind: None,
                });
                blocks.push(Block::text(t2));
            }
        };
        while let Some(item) = stream.next().await {
            match item {
                Ok(TurnEvent::TextDelta { text: d }) => {
                    text.push_str(&d);
                    live.emit_ephemeral(EventBody::TextDelta { text: d });
                }
                Ok(TurnEvent::ToolCallStart { id, name }) => {
                    flush(&mut text, &mut blocks);
                    calls.push((id, name, String::new()));
                }
                Ok(TurnEvent::ToolCallArgsDelta { id, json_fragment }) => {
                    if let Some(c) = calls.iter_mut().rev().find(|c| c.0 == id) {
                        c.2.push_str(&json_fragment);
                    }
                }
                Ok(TurnEvent::ToolCallEnd { .. }) => {}
                Ok(TurnEvent::OpaqueBlock { block }) => {
                    flush(&mut text, &mut blocks);
                    blocks.push(block);
                }
                Ok(TurnEvent::Usage {
                    input_tokens,
                    output_tokens,
                    cached_input_tokens,
                }) => {
                    let _ = live.emit(EventBody::Usage {
                        input_tokens,
                        output_tokens,
                        cached_input_tokens,
                    });
                }
                Ok(TurnEvent::Finished { reason }) => stop = Some(reason),
                Err(e) => {
                    failed = Some(e);
                    break;
                }
            }
        }
        drop(stream);
        flush(&mut text, &mut blocks);
        let cancelled = t.cancel.is_cancelled();
        // An incomplete round (error, cancel, or the stream just ended) keeps its text but not
        // its tool calls: they were never confirmed.
        let complete = failed.is_none() && !cancelled && stop.is_some();
        let mut parsed: Vec<(String, String, Result<Value, String>)> = Vec::new();
        if complete {
            for (id, name, args) in calls {
                let input = if args.trim().is_empty() {
                    Ok(Value::Object(Default::default()))
                } else {
                    serde_json::from_str::<Value>(&args).map_err(|e| e.to_string())
                };
                let input_for_block = input.clone().unwrap_or(Value::Object(Default::default()));
                blocks.push(Block::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    input: input_for_block,
                });
                parsed.push((id, name, input));
            }
        }
        if !blocks.is_empty() {
            let _ = live.record(Record::Assistant {
                v: JOURNAL_VERSION,
                turn,
                at_seq: live.last_seq(),
                blocks,
            });
        }
        if let Some(e) = failed {
            emit_error(
                live,
                "model_error",
                Some(model_kind(&e)),
                sentence(&e.to_string()),
            );
            return TurnEndReason::Error;
        }
        if cancelled {
            return TurnEndReason::Cancelled;
        }
        let Some(stop) = stop else {
            emit_error(
                live,
                "model_error",
                Some(ModelErrorKind::Transport),
                "The model's response ended unexpectedly.".into(),
            );
            return TurnEndReason::Error;
        };
        if parsed.is_empty() {
            return match stop {
                StopReason::MaxTokens => TurnEndReason::MaxTokens,
                StopReason::Refusal => TurnEndReason::Refusal,
                _ => TurnEndReason::EndTurn,
            };
        }
        rounds += 1;
        let mut results: Vec<Block> = Vec::new();
        let limit = rounds > MAX_TOOL_ROUNDS;
        for (id, name, input) in parsed {
            let answer = |content: String| Block::ToolResult {
                call_id: id.clone(),
                content,
                is_error: true,
            };
            if t.cancel.is_cancelled() {
                results.push(answer("Cancelled by the user before it ran.".into()));
                continue;
            }
            if limit {
                results.push(answer(format!(
                    "Not run: the turn reached its limit of {MAX_TOOL_ROUNDS} tool rounds."
                )));
                continue;
            }
            let (content, is_error) = run_call(t, ckpt.as_ref(), &id, &name, input).await;
            tools_ran = true;
            results.push(Block::ToolResult {
                call_id: id,
                content,
                is_error,
            });
        }
        let _ = live.record(Record::ToolResults {
            v: JOURNAL_VERSION,
            turn,
            at_seq: live.last_seq(),
            blocks: results,
        });
        if t.cancel.is_cancelled() {
            return TurnEndReason::Cancelled;
        }
        if limit {
            return TurnEndReason::ToolLimit;
        }
    }
}

/// One tool call: journalled intent, the gate pipeline, the post-state, the journalled result.
async fn run_call(
    t: &Turn<'_>,
    ckpt: Option<&Checkpoint<'_>>,
    id: &str,
    name: &str,
    input: Result<Value, String>,
) -> (String, bool) {
    let live = t.live;
    let intent = |effect: Effect, targets: Vec<String>, input: Value| {
        let _ = live.emit(EventBody::ToolCall {
            call_id: id.into(),
            tool: name.into(),
            input,
            effect,
            targets,
        });
    };
    let result = |out: crate::tools::ToolOutput, is_error: bool, rule: Option<String>| {
        let _ = live.emit(EventBody::ToolResult {
            call_id: id.into(),
            is_error,
            summary: out.summary.clone(),
            output: out.output.clone(),
            truncated: out.truncated,
            rule,
        });
        (out.text, is_error)
    };
    let Some(env) = &t.tools else {
        intent(Effect::Read, vec![], input.clone().unwrap_or_default());
        let e = ToolError::new(
            ErrorKind::Unsupported,
            "This conversation has no tools (no workspace is open).",
        );
        return result(e.into_output(), true, None);
    };
    let input = match input {
        Ok(v) => v,
        Err(reason) => {
            intent(Effect::Read, vec![], Value::Null);
            let e = ToolError::invalid("The tool input is not valid JSON.").hint(reason);
            return result(e.into_output(), true, None);
        }
    };
    let started = Instant::now();
    let prepared = match env.registry.prepare(&env.ctx, name, input.clone()).await {
        Ok(p) => p,
        Err(e) => {
            let effect = env
                .registry
                .get(name)
                .map_or(Effect::Read, |t| t.spec().effect);
            intent(effect, vec![], input);
            return result(e.into_output(), true, None);
        }
    };
    let effect = prepared.effect;
    intent(effect, prepared.targets.display(), input);
    let hook: &dyn BeforeExecute = match ckpt {
        Some(c) => c,
        None => &gate::NoHook,
    };
    let out = gate::run_prepared(
        env.registry,
        env.gate,
        &env.ctx,
        Some(&live.id),
        prepared,
        &env.policies,
        hook,
        Some(&env.audit),
        started,
    )
    .await;
    if effect != Effect::Read && !out.is_error {
        if let Err(e) = env.shadow.record_post(&live.id.0).await {
            tracing::warn!("cannot record the post-state: {e}");
        }
    }
    result(out.output, out.is_error, out.rule)
}

/// Compacts when the last request used more than 75 % of the context window.
async fn maybe_compact(t: &Turn<'_>) -> Result<(), String> {
    let live = t.live;
    let events = live.events();
    let used = events
        .iter()
        .rev()
        .find_map(|e| match e.event {
            EventBody::Usage { input_tokens, .. } => Some(input_tokens),
            _ => None,
        })
        .unwrap_or(0);
    let window = t
        .capabilities
        .context_window
        .unwrap_or(DEFAULT_CONTEXT_WINDOW);
    if (used as f64) < COMPACT_AT * window as f64 {
        return Ok(());
    }
    // Keep the last two exchanges: cut before the user event of the turn before this one.
    let turn = live.turn();
    if turn < 3 {
        return Ok(());
    }
    let keep_from = turn - 1;
    let Some(through) = events
        .iter()
        .take_while(|e| !(e.turn >= keep_from && matches!(e.event, EventBody::User { .. })))
        .map(|e| e.seq)
        .last()
    else {
        return Ok(());
    };
    let mut messages = context(live);
    messages.push(Message::user_text(
        "Summarise this conversation so far for your own later use: the user's goals, decisions made, files changed and why, commands run and their results, and what is still to do. Be complete but concise; plain text.",
    ));
    let req = TurnRequest {
        model: t.model.clone(),
        system: "You write faithful, concise summaries of coding conversations.".into(),
        messages,
        tools: vec![],
        max_output_tokens: Some(2048),
        temperature: None,
    };
    let mut s = t
        .provider
        .stream(req, t.cancel.child_token())
        .await
        .map_err(|e| e.to_string())?;
    let mut summary = String::new();
    while let Some(ev) = s.next().await {
        match ev.map_err(|e| e.to_string())? {
            TurnEvent::TextDelta { text } => summary.push_str(&text),
            TurnEvent::Finished { .. } => break,
            _ => {}
        }
    }
    if summary.trim().is_empty() {
        return Err("the model returned an empty summary".into());
    }
    live.emit(EventBody::Compaction {
        through_seq: through,
        summary,
    })
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn sentence(s: &str) -> String {
    let mut t = s.trim().to_string();
    if let Some(c) = t.get(..1) {
        let up = c.to_uppercase();
        t.replace_range(..1, &up);
    }
    if !t.ends_with('.') {
        t.push('.');
    }
    t
}

/// The system prompt (ADR-0015 §9): Axiom's rules, the workspace, and its `AGENTS.md`.
pub fn system_prompt(workspace: Option<(&str, Option<&str>)>) -> String {
    match workspace {
        None => "You are the assistant in Axiom, a development environment. This conversation has no project open, so you have no tools: answer directly. Use Markdown; put code in fenced blocks with a language.".into(),
        Some((name, agents_md)) => {
            let mut s = format!(
                "You are the coding agent in Axiom, working in the workspace \"{name}\".\n\
                 - Use the tools to look before you change anything: workspace.info, workspace.search, fs.read.\n\
                 - Change files only with fs.applyPatch; copy search text exactly from fs.read.\n\
                 - Build and test with build.run (see workspace.info for task ids) or task.run.\n\
                 - For a change to a running UI (preview.start, ui.*): find the element with ui.selection, ui.snapshot or ui.inspect; before patching, decide the checks that would show success (geometry against other elements, or a computed property); after fs.applyPatch, call ui.verify with them and report its verdict as it is.\n\
                 - Every change is checkpointed and the user can restore it; some calls need the user's approval, and a denial means: do not retry the same call.\n\
                 - Paths are workspace-relative with `/`.\n\
                 - Be brief in prose; use Markdown, with code in fenced blocks."
            );
            if let Some(md) = agents_md {
                let cap = 32 * 1024;
                let mut md = md.to_string();
                if md.len() > cap {
                    let mut cut = cap;
                    while !md.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    md.truncate(cut);
                    md.push_str("\n[AGENTS.md truncated]");
                }
                s.push_str("\n\nThe project's AGENTS.md (instructions from its maintainers):\n\n");
                s.push_str(&md);
            }
            s
        }
    }
}
