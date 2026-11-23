//! The agent's built-in tools (`docs/protocols/agents.md` §Tools; M3-TOOLS-001): thin adapters,
//! one per tool, from a validated input to one service call (workspace, exec, build, env; preview
//! and inspect from M5) and a bounded result. The registry and the gate are in `axiom-agent`; nothing here decides policy.

use crate::app::App;
use crate::handlers::env::{project_environment, registry as env_registry, workspace};
use axiom_agent::tools::inputs::{self, schema_of};
use axiom_agent::tools::patch;
use axiom_agent::tools::{
    parse, Allowed, Registry, Targets, Tool, ToolCtx, ToolError, ToolOutput, ToolSpec,
    DEFAULT_MAX_RESULT_BYTES,
};
use axiom_exec::{SpawnSpec, TaskEvent};
use axiom_protocol::agent::{Effect, Reveal, RevealParams};
use axiom_protocol::fs::{EntryKind, FileVersion};
use axiom_protocol::task::{Command, Outcome, Stream};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{ErrorKind, RpcError, WorkspacePath};
use axiom_workspace::{SearchQuery, Workspace, WsError};
use futures_util::future::BoxFuture;
use futures_util::FutureExt;
use serde_json::{json, Value};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

const READ_MAX_LINES: usize = 2000;
const READ_MAX_BYTES: usize = 30 * 1024;
const SEARCH_PAGE: usize = 100;
const TAIL_LINES: usize = 200;
const TAIL_BYTES: usize = 16 * 1024;
const DEFAULT_TIMEOUT: u64 = 120;
const MAX_TIMEOUT: u64 = 600;
/// `build.run`: builds take longer than most commands (AUD-035: it had no limit at all).
const BUILD_DEFAULT_TIMEOUT: u64 = 600;
const BUILD_MAX_TIMEOUT: u64 = 3600;
const MAX_DIAGNOSTICS: usize = 50;
/// What a running command's output is reduced to while it runs: enough for the tail the
/// result shows (AUD-035: the whole output was kept).
const KEEP_BYTES: usize = 4 * TAIL_BYTES;
/// The most an ACP terminal keeps of its output, whatever the agent asks (AUD-039).
const MAX_ACP_OUTPUT: u64 = 16 * 1024 * 1024;
/// Settings key: names of variables that look like secrets but an agent-started process may
/// still get (AUD-010).
const PASS_ENV_KEY: &str = "agent.passEnv";

type Run = for<'a> fn(
    &'a Arc<App>,
    &'a ToolCtx,
    Value,
    &'a Targets,
) -> BoxFuture<'a, Result<ToolOutput, ToolError>>;
type Prep =
    for<'a> fn(&'a Arc<App>, &'a ToolCtx, &'a Value) -> BoxFuture<'a, Result<Targets, ToolError>>;

/// One tool: its spec plus two plain functions over the daemon.
struct DaemonTool {
    spec: ToolSpec,
    app: Weak<App>,
    prep: Prep,
    run: Run,
}

fn gone() -> ToolError {
    ToolError::new(ErrorKind::Internal, "The daemon is shutting down.")
}

impl Tool for DaemonTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn prepare<'a>(
        &'a self,
        ctx: &'a ToolCtx,
        input: &'a Value,
    ) -> BoxFuture<'a, Result<Targets, ToolError>> {
        async move {
            let app = self.app.upgrade().ok_or_else(gone)?;
            (self.prep)(&app, ctx, input).await
        }
        .boxed()
    }

    fn execute<'a>(
        &'a self,
        ctx: &'a ToolCtx,
        input: Value,
        targets: &'a Targets,
        _allowed: Allowed,
    ) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
        async move {
            let app = self.app.upgrade().ok_or_else(gone)?;
            (self.run)(&app, ctx, input, targets).await
        }
        .boxed()
    }
}

/// The registry of built-in tools.
pub fn registry(app: Weak<App>) -> Registry {
    let mut r = Registry::new();
    let mut add = |name: &str, description: &str, schema: Value, effect, prep: Prep, run: Run| {
        r.register(Arc::new(DaemonTool {
            spec: ToolSpec {
                name: name.into(),
                description: description.into(),
                input_schema: schema,
                effect,
                max_result_bytes: DEFAULT_MAX_RESULT_BYTES,
            },
            app: app.clone(),
            prep,
            run,
        }))
    };
    add(
        "workspace.info",
        "The open workspace: its name, top-level entries, and the build/test tasks its providers offer (ids for build.run).",
        schema_of::<inputs::WorkspaceInfoInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::WorkspaceInfoInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, _, _| workspace_info(a, c).boxed(),
    );
    add(
        "workspace.search",
        "Search the workspace's text files (ignored files skipped). Literal by default; `regex: true` for a regular expression; `glob` limits paths. Returns path:line:column matches, 100 per page; pass `nextCursor` back as `cursor` for more.",
        schema_of::<inputs::SearchInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::SearchInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, i, _| search(a, c, i).boxed(),
    );
    add(
        "fs.list",
        "List a directory of the workspace (empty path = the root). Directories first; ignored entries are marked.",
        schema_of::<inputs::ListInput>(),
        Effect::Read,
        |_, _, i| {
            async move {
                let l: inputs::ListInput = parse(i)?;
                Ok(Targets {
                    paths: vec![l.path.unwrap_or_else(WorkspacePath::root)],
                    ..Default::default()
                })
            }
            .boxed()
        },
        |a, c, i, _| list(a, c, i).boxed(),
    );
    add(
        "fs.read",
        "Read a text file with line numbers. Use startLine/endLine for a range; a long file is returned in pages (continue from `nextStartLine`).",
        schema_of::<inputs::ReadInput>(),
        Effect::Read,
        |_, _, i| {
            async move {
                let r: inputs::ReadInput = parse(i)?;
                Ok(Targets {
                    paths: vec![r.path],
                    ..Default::default()
                })
            }
            .boxed()
        },
        |a, c, i, _| read(a, c, i).boxed(),
    );
    add(
        "fs.applyPatch",
        "Edit a file with search/replace blocks: each `search` must match the current file exactly once (copy it from fs.read, indentation included). Or `create` a new file with content, or `delete: true` to remove one. Returns the new version and a unified diff.",
        schema_of::<inputs::ApplyPatchInput>(),
        Effect::WriteWorkspace,
        |a, c, i| prepare_patch(a, c, i).boxed(),
        |a, c, i, t| apply_patch(a, c, i, t).boxed(),
    );
    add(
        "task.run",
        "Run a program in the workspace root, in the project's environment, and wait for it (default 120 s, at most 600). argv only, no shell. Returns the exit code and the last 200 lines of output.",
        schema_of::<inputs::TaskRunInput>(),
        Effect::Execute,
        |a, _, i| {
            async move {
                let t: inputs::TaskRunInput = parse(i)?;
                if t.argv.is_empty() || t.argv[0].is_empty() {
                    return Err(ToolError::invalid("argv is empty.").hint("Give the program and its arguments, e.g. [\"cargo\", \"test\"]."));
                }
                Ok(Targets {
                    argv: Some(t.argv),
                    preview: axiom_agent::secrets::describe(&withheld(a).await?),
                    ..Default::default()
                })
            }
            .boxed()
        },
        |a, c, i, _| task_run(a, c, i).boxed(),
    );
    add(
        "build.run",
        "Run a provider task (e.g. `build`, `test`; see workspace.info) and wait for it. Returns the outcome, the parsed diagnostics (file:line:column, message) and the output's last lines.",
        schema_of::<inputs::BuildRunInput>(),
        Effect::Execute,
        |a, c, i| prepare_build(a, c, i).boxed(),
        |a, c, i, t| build_run(a, c, i, t).boxed(),
    );
    add(
        "build.diagnostics",
        "The diagnostics of the newest build in this workspace (from build.run or the user's own build).",
        schema_of::<inputs::EmptyInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::EmptyInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, _, _| async move { last_diagnostics(a, c) }.boxed(),
    );
    add(
        "env.detect",
        "Which toolchain providers apply to the workspace and which of their tools are found (version, location) or missing.",
        schema_of::<inputs::EmptyInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::EmptyInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, _, _| env_detect(a, c).boxed(),
    );
    add(
        "env.planInstall",
        "Plan installing a toolchain version into Axiom's managed storage. Nothing runs; returns the plan (every download and command) and its planId for env.install.",
        schema_of::<inputs::PlanInstallInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::PlanInstallInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, i, _| env_plan(a, c, i).boxed(),
    );
    add(
        "env.install",
        "Execute an install plan from env.planInstall (its planId, within 10 minutes).",
        schema_of::<inputs::InstallInput>(),
        Effect::Install,
        |a, c, i| prepare_install(a, c, i).boxed(),
        |a, c, i, _| env_install(a, c, i).boxed(),
    );
    add(
        "editor.open",
        "Show a file to the user in Axiom's editor, optionally at a line and column.",
        schema_of::<inputs::OpenInput>(),
        Effect::Read,
        |_, _, i| {
            async move {
                let o: inputs::OpenInput = parse(i)?;
                Ok(Targets {
                    paths: vec![o.path],
                    ..Default::default()
                })
            }
            .boxed()
        },
        |a, c, i, _| editor_open(a, c, i).boxed(),
    );
    add(
        "preview.start",
        "Start the project's dev server with Axiom's inspector (Vite) and return its URL. The person opens the URL in their browser; ui.* tools work once the page is connected (`adapter` in the result). A second call returns the running preview.",
        schema_of::<inputs::PreviewStartInput>(),
        Effect::Execute,
        |a, c, i| prepare_preview(a, c, i).boxed(),
        |a, c, i, _| preview_start(a, c, i).boxed(),
    );
    add(
        "ui.snapshot",
        "The running page's element tree: per node its id, type, text, stable key, bounds (x, y, width x height in CSS px) and source location (file:line:column; `exact` from the build, else a guess with its confidence). Use stable keys, not ids, in ui.inspect and ui.verify: ids change on reload.",
        schema_of::<inputs::UiSnapshotInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::UiSnapshotInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, i, _| ui_snapshot(a, c, i).boxed(),
    );
    add(
        "ui.inspect",
        "One element of the running page in detail: bounds, computed layout properties (css.*), accessibility and every source candidate. Name it by `stableKey` (from ui.snapshot or ui.selection) or by `node` id.",
        schema_of::<inputs::UiInspectInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::UiInspectInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, i, _| ui_inspect(a, c, i).boxed(),
    );
    add(
        "ui.selection",
        "The element the person selected in the page (Axiom's inspect mode), with its ancestors and source location. Start here when they say \"this\" or \"the selected element\".",
        schema_of::<inputs::UiSelectionInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::UiSelectionInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, i, _| ui_selection(a, c, i).boxed(),
    );
    add(
        "ui.verify",
        "Verify a UI change objectively, after the patch. Decide the checks BEFORE patching: what the page must measure if the change worked (e.g. button.left == amount.right + 24; css.padding-left == \"16px\"). Call this after fs.applyPatch: it waits for the reload to settle and returns `verified` (a stated check passed, nothing failed), `failed` (a check failed, a build or runtime error) or `inconclusive` (the element was not found again, or nothing objective was stated). Report the verdict as it is; never claim success on `failed` or `inconclusive`.",
        schema_of::<inputs::UiVerifyInput>(),
        Effect::Read,
        |_, _, i| async move { parse::<inputs::UiVerifyInput>(i).map(|_| Targets::default()) }.boxed(),
        |a, c, i, _| ui_verify(a, c, i).boxed(),
    );
    register_acp(&mut r, app);
    r
}

// ---------------------------------------------------------------- helpers

fn rpc(e: RpcError) -> ToolError {
    ToolError::new(e.data.kind, e.message)
}

fn ws_err(e: WsError) -> ToolError {
    let hint = match &e {
        WsError::NotFound(_) => Some("Check the path with fs.list or workspace.search."),
        WsError::Escapes(_) | WsError::InvalidPath(_) => {
            Some("Use a workspace-relative path with `/` separators, no `..`.")
        }
        WsError::IsADirectory(_) => Some("Use fs.list for directories."),
        WsError::Changed(_) => {
            Some("The path changed after it was approved; read it again and retry.")
        }
        _ => None,
    };
    let mut t = rpc(e.into());
    t.hint = hint.map(String::from);
    t
}

fn ws(app: &App, ctx: &ToolCtx) -> Result<Arc<Workspace>, ToolError> {
    app.workspaces.get(&ctx.workspace_id).map_err(ws_err)
}

fn tail(text: &str) -> (String, bool) {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(TAIL_LINES);
    let mut t = lines[start..].join("\n");
    let mut cut = start > 0;
    if t.len() > TAIL_BYTES {
        let mut from = t.len() - TAIL_BYTES;
        while !t.is_char_boundary(from) {
            from += 1;
        }
        t = t[from..].to_string();
        cut = true;
    }
    (t, cut)
}

fn outcome_name(o: Outcome) -> &'static str {
    match o {
        Outcome::Succeeded => "succeeded",
        Outcome::Failed => "failed",
        Outcome::Cancelled => "cancelled",
    }
}

/// Starts `spec` and waits for it, collecting output; stops it on timeout or cancel.
async fn run_to_end(
    app: &App,
    ctx: &ToolCtx,
    spec: SpawnSpec,
    timeout: Duration,
    mut on_event: impl FnMut(&TaskEvent),
) -> Result<(Value, String, bool), ToolError> {
    let started = Instant::now();
    let (task_id, mut rx) = app
        .executor
        .start(ctx.workspace_id.clone(), spec)
        .await
        .map_err(|e| rpc(e.into()))?;
    let mut out = Tail::default();
    let mut timed_out = false;
    let deadline = tokio::time::Instant::now() + timeout;
    let status = loop {
        let ev = tokio::select! {
            ev = rx.recv() => ev,
            _ = tokio::time::sleep_until(deadline), if !timed_out => {
                timed_out = true;
                let _ = app.executor.cancel(&task_id).await;
                continue;
            }
            _ = ctx.cancel.cancelled() => {
                let _ = app.executor.cancel(&task_id).await;
                // Keep reading until it exits.
                rx.recv().await
            }
        };
        let Some(ev) = ev else {
            return Err(ToolError::new(
                ErrorKind::Internal,
                "The task ended without an exit status.",
            ));
        };
        on_event(&ev);
        match ev {
            TaskEvent::Output(o) => {
                if o.stream == Stream::Stderr || o.stream == Stream::Stdout {
                    out.push(&o.data);
                }
            }
            TaskEvent::Exited(x) => break x.status,
        }
    };
    let (t, cut) = out.tail();
    let v = json!({
        "outcome": outcome_name(status.outcome),
        "exitCode": status.exit_code,
        "durationMs": started.elapsed().as_millis() as u64,
        "timedOut": timed_out,
        "output": t,
    });
    Ok((v, out.text, cut))
}

/// The end of a command's output, bounded while it runs: the newest [`KEEP_BYTES`] at most
/// twice over, so a command that prints gigabytes costs the daemon kilobytes (AUD-035).
#[derive(Default)]
struct Tail {
    text: String,
    dropped: bool,
}

impl Tail {
    fn push(&mut self, data: &str) {
        self.text.push_str(data);
        if self.text.len() > 2 * KEEP_BYTES {
            let mut from = self.text.len() - KEEP_BYTES;
            while !self.text.is_char_boundary(from) {
                from += 1;
            }
            self.text.drain(..from);
            self.dropped = true;
        }
    }

    /// The last lines (as [`tail`]), and whether anything was cut.
    fn tail(&self) -> (String, bool) {
        let (t, cut) = tail(&self.text);
        (t, cut || self.dropped)
    }
}

// ---------------------------------------------------------------- tools

async fn workspace_info(app: &Arc<App>, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let w = ws(app, ctx)?;
    let entries = w.list(&WorkspacePath::root()).await.map_err(ws_err)?;
    let top: Vec<Value> = entries
        .iter()
        .filter(|e| !e.ignored && e.name != ".git")
        .map(|e| {
            json!({"name": e.name, "kind": match e.kind {
                EntryKind::Directory => "directory", EntryKind::File => "file", EntryKind::Symlink => "symlink" }})
        })
        .collect();
    let tasks = crate::handlers::build::tasks(
        app.clone(),
        WorkspaceRef {
            workspace_id: ctx.workspace_id.clone(),
        },
    )
    .await
    .map(|t| t.tasks)
    .unwrap_or_default();
    let tasks: Vec<Value> = tasks
        .iter()
        .map(|t| json!({"provider": t.provider_id, "task": t.id, "label": t.label}))
        .collect();
    let v = json!({"name": w.name(), "entries": top, "tasks": tasks});
    let summary = format!("{} entries, {} tasks", top.len(), tasks.len());
    Ok(ToolOutput::json(v, summary))
}

async fn search(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let s: inputs::SearchInput = parse(&input)?;
    if s.query.is_empty() {
        return Err(ToolError::invalid("The query is empty."));
    }
    let w = ws(app, ctx)?;
    let r = w
        .search(SearchQuery {
            pattern: s.query,
            regex: s.regex,
            case_sensitive: s.case_sensitive,
            glob: s.glob,
            offset: s.cursor.unwrap_or(0),
            limit: SEARCH_PAGE,
        })
        .await
        .map_err(|e| match e {
            WsError::InvalidPath(m) => ToolError::invalid(format!("{m}."))
                .hint("Fix the pattern or glob, or search literally (regex: false)."),
            other => ws_err(other),
        })?;
    let mut text: String = r
        .matches
        .iter()
        .map(|m| format!("{}:{}:{}: {}\n", m.path.0, m.line, m.column, m.text))
        .collect();
    if r.matches.is_empty() {
        text = format!("No matches in {} files.", r.files_searched);
    }
    if let Some(n) = r.next_offset {
        text.push_str(&format!("[more matches: cursor {n}]\n"));
    }
    let v = json!({
        "matches": r.matches.iter().map(|m| json!({"path": m.path, "line": m.line, "column": m.column, "text": m.text})).collect::<Vec<_>>(),
        "nextCursor": r.next_offset,
        "filesSearched": r.files_searched,
    });
    Ok(ToolOutput {
        summary: format!(
            "{}{} matches",
            r.matches.len(),
            if r.next_offset.is_some() { "+" } else { "" }
        ),
        output: v,
        text,
        truncated: false,
    })
}

async fn list(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let l: inputs::ListInput = parse(&input)?;
    let path = l.path.unwrap_or_else(WorkspacePath::root);
    let entries = ws(app, ctx)?.list(&path).await.map_err(ws_err)?;
    let mut text = String::new();
    for e in &entries {
        let suffix = match e.kind {
            EntryKind::Directory => "/",
            EntryKind::Symlink => "@",
            EntryKind::File => "",
        };
        let ign = if e.ignored { "  (ignored)" } else { "" };
        text.push_str(&format!("{}{suffix}{ign}\n", e.name));
    }
    let v = json!({"path": path, "entries": entries});
    Ok(ToolOutput {
        summary: format!("{} entries", entries.len()),
        output: v,
        text,
        truncated: false,
    })
}

async fn read(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let r: inputs::ReadInput = parse(&input)?;
    let file = ws(app, ctx)?.read_text(&r.path).await.map_err(ws_err)?;
    let lines: Vec<&str> = file.text.lines().collect();
    let total = lines.len();
    let start = r.start_line.unwrap_or(1).max(1) as usize;
    if start > total.max(1) {
        return Err(ToolError::invalid(format!(
            "startLine {start} is past the end ({total} lines)."
        )));
    }
    let last = r.end_line.map(|e| (e as usize).min(total)).unwrap_or(total);
    let mut text = String::new();
    let mut end = start.saturating_sub(1);
    for (i, line) in lines.iter().enumerate().take(last).skip(start - 1) {
        let row = format!("{:>5}\t{line}\n", i + 1);
        if end + 1 - start >= READ_MAX_LINES || text.len() + row.len() > READ_MAX_BYTES {
            break;
        }
        text.push_str(&row);
        end = i + 1;
    }
    let next = (end < last).then_some(end + 1);
    if let Some(n) = next {
        text.push_str(&format!(
            "[{} more lines: continue with startLine {n}]\n",
            last - end
        ));
    }
    let v = json!({
        "path": r.path, "version": file.version, "totalLines": total,
        "startLine": start, "endLine": end, "nextStartLine": next,
    });
    Ok(ToolOutput {
        summary: format!("lines {start}–{end} of {total}"),
        output: v,
        text,
        truncated: next.is_some(),
    })
}

/// The new text (`None` = delete) and the diff, from the file as it is now.
async fn plan_patch(
    w: &Workspace,
    p: &inputs::ApplyPatchInput,
) -> Result<(Option<FileVersion>, Option<String>, String, usize, usize), ToolError> {
    let modes = (!p.edits.is_empty()) as u8 + p.create.is_some() as u8 + p.delete as u8;
    if modes != 1 {
        return Err(ToolError::invalid(
            "Give exactly one of `edits`, `create` or `delete`.",
        ));
    }
    let current = match w.read_text(&p.path).await {
        Ok(f) => Some(f),
        Err(WsError::NotFound(_)) => None,
        Err(e) => return Err(ws_err(e)),
    };
    let (old, version) = match &current {
        Some(f) => (f.text.as_str(), Some(f.version.clone())),
        None => ("", None),
    };
    let new = if let Some(c) = &p.create {
        if current.is_some() {
            return Err(ToolError::new(
                ErrorKind::AlreadyExists,
                format!("`{}` already exists.", p.path.0),
            )
            .hint("Use `edits` to change an existing file."));
        }
        Some(c.clone())
    } else if p.delete {
        if current.is_none() {
            return Err(ToolError::new(
                ErrorKind::NotFound,
                format!("`{}` does not exist.", p.path.0),
            ));
        }
        None
    } else {
        if current.is_none() {
            return Err(ToolError::new(
                ErrorKind::NotFound,
                format!("`{}` does not exist.", p.path.0),
            )
            .hint("Use `create` for a new file."));
        }
        Some(patch::apply(old, &p.edits).map_err(|e| {
            ToolError::new(
                ErrorKind::Conflict,
                format!("{}.", capitalise(&e.to_string())),
            )
            .hint(e.hint())
        })?)
    };
    let (diff, added, removed) = patch::diff(&p.path.0, old, new.as_deref().unwrap_or(""));
    Ok((version, new, diff, added, removed))
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

async fn prepare_patch(app: &Arc<App>, ctx: &ToolCtx, input: &Value) -> Result<Targets, ToolError> {
    let p: inputs::ApplyPatchInput = parse(input)?;
    let w = ws(app, ctx)?;
    // Resolve first: a path outside the workspace never gets as far as the gate. The gate (and
    // the card) get the path the file really has, through any symlink: a protected path reached
    // through one still always asks (AUD-104, review B4).
    let real = w.canonical_path(&p.path).map_err(ws_err)?;
    let (version, new, diff, _, _) = plan_patch(&w, &p).await?;
    Ok(Targets {
        paths: vec![real],
        preview: Some(diff),
        versions: vec![(p.path, version)],
        effect: new.is_none().then_some(Effect::Delete),
        ..Default::default()
    })
}

async fn apply_patch(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
    targets: &Targets,
) -> Result<ToolOutput, ToolError> {
    let p: inputs::ApplyPatchInput = parse(&input)?;
    let w = ws(app, ctx)?;
    let (version, new, diff, added, removed) = plan_patch(&w, &p).await?;
    // The approval was for the file as it was when the call was prepared.
    let approved = targets
        .versions
        .iter()
        .find(|(path, _)| *path == p.path)
        .map(|(_, v)| v.clone());
    if approved.is_some_and(|a| a != version) {
        return Err(ToolError::new(
            ErrorKind::Conflict,
            format!("`{}` changed after the edit was approved.", p.path.0),
        )
        .hint("Read the file again and make the edit against its current content."));
    }
    // Written only where the gate judged it: a swap of a directory for a symlink since the call
    // was prepared is refused, not followed into, for example, `.git/hooks` (review R5).
    let judged = targets.paths.first().unwrap_or(&p.path);
    let new_version = match (&new, &version) {
        (Some(text), v) => Some(
            w.write_text_checked(&p.path, judged, text, v.as_ref(), false)
                .await
                .map_err(ws_err)?,
        ),
        (None, Some(v)) => {
            w.remove_file_checked(&p.path, judged, v)
                .await
                .map_err(ws_err)?;
            None
        }
        (None, None) => unreachable!("plan_patch refuses deleting a missing file"),
    };
    let action = if p.create.is_some() {
        "created"
    } else if p.delete {
        "deleted"
    } else {
        "edited"
    };
    let v = json!({
        "path": p.path, "action": action, "version": new_version,
        "added": added, "removed": removed, "diff": diff,
    });
    Ok(ToolOutput {
        text: format!("{action} {} (+{added} −{removed})\n{diff}", p.path.0),
        summary: format!("+{added} −{removed}"),
        output: v,
        truncated: false,
    })
}

/// The daemon's variables a process an agent starts does not get: those that look like
/// secrets and the model providers' `apiKeyEnv` variables, unless settings `agent.passEnv`
/// names them (`security.md` §Processes; AUD-010).
pub(crate) async fn withheld(app: &App) -> Result<Vec<String>, ToolError> {
    let configs = crate::handlers::agent::provider_configs(app)
        .await
        .map_err(rpc)?;
    let keys: Vec<String> = app
        .agent
        .providers
        .all(&configs)
        .into_iter()
        .filter_map(|c| c.api_key_env)
        .collect();
    let allowed: Vec<String> = app
        .store_call(|s| s.setting_get(PASS_ENV_KEY))
        .await
        .map_err(rpc)?
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let names = axiom_agent::secrets::daemon_names();
    Ok(axiom_agent::secrets::withheld(
        names.iter().map(String::as_str),
        &keys,
        &allowed,
    ))
}

/// Adds the removal of each withheld variable to a spawn environment.
fn withhold(env: &mut std::collections::BTreeMap<String, String>, names: &[String]) {
    for n in names {
        env.insert(SpawnSpec::remove_key(n), String::new());
    }
}

/// The environment of a process an agent starts: the project's over the daemon's, without the
/// withheld variables; and their names.
async fn spawn_env(
    app: &App,
    ctx: &ToolCtx,
) -> Result<(std::collections::BTreeMap<String, String>, Vec<String>), ToolError> {
    let wsinfo = workspace(app, &ctx.workspace_id).await.map_err(rpc)?;
    let r = env_registry(app, Some(wsinfo)).await.map_err(rpc)?;
    let environment = project_environment(app, &ctx.workspace_id, &r)
        .await
        .map_err(rpc)?;
    let mut env = axiom_env::project::process_env(&environment, std::env::var_os("PATH"));
    let hidden = withheld(app).await?;
    withhold(&mut env, &hidden);
    Ok((env, hidden))
}

/// A tool's result text with the line that says which variables the process it started did
/// not get, when any were withheld (review E5): a call the policy allows shows no card, so a
/// `git push` that fails for want of `SSH_AUTH_SOCK` is explained here, with how to pass it.
fn with_env_note(mut text: String, hidden: &[String]) -> String {
    if let Some(line) = axiom_agent::secrets::describe(hidden) {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&line);
    }
    text
}

async fn task_run(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let t: inputs::TaskRunInput = parse(&input)?;
    let timeout = t
        .timeout_secs
        .unwrap_or(DEFAULT_TIMEOUT)
        .clamp(1, MAX_TIMEOUT);
    let w = ws(app, ctx)?;
    let (env, hidden) = spawn_env(app, ctx).await?;
    let spec = SpawnSpec {
        command: Command::Argv(t.argv.clone()),
        cwd: w.root().to_path_buf(),
        env,
        label: Some(format!("agent: {}", t.argv.join(" "))),
    };
    let (v, _, cut) = run_to_end(app, ctx, spec, Duration::from_secs(timeout), |_| {}).await?;
    let summary = format!(
        "{} · {:.1} s",
        match v["exitCode"].as_i64() {
            Some(c) => format!("exit {c}"),
            None => v["outcome"].as_str().unwrap_or("").to_string(),
        },
        v["durationMs"].as_u64().unwrap_or(0) as f64 / 1000.0
    );
    let text = format!(
        "{} (exit code {}){}\n{}",
        v["outcome"].as_str().unwrap_or(""),
        v["exitCode"],
        if v["timedOut"] == json!(true) {
            ", timed out"
        } else {
            ""
        },
        v["output"].as_str().unwrap_or("")
    );
    Ok(ToolOutput {
        output: v,
        text: with_env_note(text, &hidden),
        summary,
        truncated: cut,
    })
}

async fn prepare_build(app: &Arc<App>, ctx: &ToolCtx, input: &Value) -> Result<Targets, ToolError> {
    let b: inputs::BuildRunInput = parse(input)?;
    let built = build_spec(app, ctx, &b).await?;
    Ok(Targets {
        argv: Some(built.argv),
        preview: axiom_agent::secrets::describe(&withheld(app).await?),
        conditions: built.conditions,
        ..Default::default()
    })
}

/// What `build.run` will run: the provider, the argv, and the conditions shown with it (the
/// provider, the environment's `PATH` prepend, which decides which program runs).
struct BuildSpec {
    provider: String,
    argv: Vec<String>,
    conditions: Vec<String>,
    spec: SpawnSpec,
    diagnostics: axiom_protocol::provider::DiagnosticsFormat,
    root: std::path::PathBuf,
}

/// The provider offering `task` and what it will run, in the project's environment.
async fn build_spec(
    app: &Arc<App>,
    ctx: &ToolCtx,
    b: &inputs::BuildRunInput,
) -> Result<BuildSpec, ToolError> {
    let tasks = crate::handlers::build::tasks(
        app.clone(),
        WorkspaceRef {
            workspace_id: ctx.workspace_id.clone(),
        },
    )
    .await
    .map_err(rpc)?
    .tasks;
    let t = tasks
        .iter()
        .find(|t| t.id == b.task && b.provider.as_ref().is_none_or(|p| *p == t.provider_id))
        .ok_or_else(|| {
            ToolError::new(
                ErrorKind::NotFound,
                format!("No provider offers a task `{}` here.", b.task),
            )
            .hint(format!(
                "Tasks in this workspace: {}.",
                tasks
                    .iter()
                    .map(|t| format!("{}:{}", t.provider_id, t.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
    let wsinfo = workspace(app, &ctx.workspace_id).await.map_err(rpc)?;
    let root = wsinfo.0.clone();
    let r = env_registry(app, Some(wsinfo)).await.map_err(rpc)?;
    let environment = project_environment(app, &ctx.workspace_id, &r)
        .await
        .map_err(rpc)?;
    let prepared = axiom_build::prepare(
        &r,
        &t.provider_id,
        &t.id,
        &root,
        &environment,
        std::env::var_os("PATH"),
    )
    .map_err(|e| rpc(e.into()))?;
    let argv = match &prepared.spec.command {
        Command::Argv(a) => a.clone(),
        Command::Shell(s) => vec![s.clone()],
    };
    let mut conditions = vec![format!("provider {}", t.provider_id)];
    if !environment.path_prepend.is_empty() {
        conditions.push(format!(
            "PATH starts with {}",
            environment.path_prepend.join(", ")
        ));
    }
    Ok(BuildSpec {
        provider: t.provider_id.clone(),
        argv,
        conditions,
        spec: prepared.spec,
        diagnostics: prepared.diagnostics,
        root,
    })
}

async fn build_run(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
    targets: &Targets,
) -> Result<ToolOutput, ToolError> {
    let b: inputs::BuildRunInput = parse(&input)?;
    let timeout = b
        .timeout_secs
        .unwrap_or(BUILD_DEFAULT_TIMEOUT)
        .clamp(1, BUILD_MAX_TIMEOUT);
    // What runs is what was approved: the task is resolved again (its provider, its manifest,
    // the environment may have changed while the approval waited) and must be the same
    // (AUD-103).
    let mut built = build_spec(app, ctx, &b).await?;
    if targets.argv.as_ref() != Some(&built.argv) || targets.conditions != built.conditions {
        return Err(ToolError::new(
            ErrorKind::Conflict,
            "The task changed after it was approved.",
        )
        .hint("Call build.run again: the person approves the command as it is now."));
    }
    let hidden = withheld(app).await?;
    withhold(&mut built.spec.env, &hidden);
    let provider = built.provider;
    let root = built.root;
    let started = Instant::now();
    let (task_id, rx) = app
        .executor
        .start(ctx.workspace_id.clone(), built.spec)
        .await
        .map_err(|e| rpc(e.into()))?;
    let mut events = axiom_build::drive(task_id.clone(), rx, &built.diagnostics, vec![root]);
    let mut out = Tail::default();
    let mut diags = Vec::new();
    let mut timed_out = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let status = loop {
        let ev = tokio::select! {
            ev = events.recv() => ev,
            _ = tokio::time::sleep_until(deadline), if !timed_out => {
                timed_out = true;
                let _ = app.executor.cancel(&task_id).await;
                continue;
            }
            _ = ctx.cancel.cancelled() => {
                let _ = app.executor.cancel(&task_id).await;
                events.recv().await
            }
        };
        match ev {
            Some(axiom_build::BuildEvent::Output(o)) => out.push(&o.data),
            Some(axiom_build::BuildEvent::Diagnostics(d)) => {
                app.record_diagnostics(&ctx.workspace_id, &d);
                diags.extend(d.diagnostics);
            }
            Some(axiom_build::BuildEvent::Exited(x)) => break x.status,
            None => {
                return Err(ToolError::new(
                    ErrorKind::Internal,
                    "The build ended without an exit status.",
                ))
            }
        }
    };
    let (t, cut) = out.tail();
    let errors = diags
        .iter()
        .filter(|d| d.severity == axiom_protocol::diagnostics::Severity::Error)
        .count();
    let shown: Vec<_> = diags.iter().take(MAX_DIAGNOSTICS).collect();
    let mut text = format!(
        "{}: {} (exit code {}){}, {} diagnostics ({errors} errors)\n",
        b.task,
        outcome_name(status.outcome),
        status.exit_code.map_or("none".into(), |c| c.to_string()),
        if timed_out {
            format!(", stopped after {timeout} s")
        } else {
            String::new()
        },
        diags.len()
    );
    for d in &shown {
        text.push_str(&render_diagnostic(d));
    }
    if shown.is_empty() {
        text.push_str(&t);
    }
    let v = json!({
        "task": b.task, "provider": provider,
        "outcome": outcome_name(status.outcome), "exitCode": status.exit_code,
        "timedOut": timed_out,
        "durationMs": started.elapsed().as_millis() as u64,
        "diagnostics": shown, "diagnosticCount": diags.len(), "output": t,
    });
    Ok(ToolOutput {
        summary: format!(
            "{} · {errors} errors · {:.1} s",
            outcome_name(status.outcome),
            started.elapsed().as_secs_f64()
        ),
        output: v,
        text: with_env_note(text, &hidden),
        truncated: cut || diags.len() > MAX_DIAGNOSTICS,
    })
}

fn render_diagnostic(d: &axiom_protocol::diagnostics::Diagnostic) -> String {
    let sev = serde_json::to_value(d.severity)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let loc = d
        .location
        .as_ref()
        .map(|l| match l.column {
            Some(c) => format!("{}:{}:{c}: ", l.file.0, l.line),
            None => format!("{}:{}: ", l.file.0, l.line),
        })
        .unwrap_or_default();
    let code = d
        .code
        .as_ref()
        .map(|c| format!(" [{c}]"))
        .unwrap_or_default();
    format!("{loc}{sev}{code}: {}\n", d.message)
}

fn last_diagnostics(app: &Arc<App>, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let m = app
        .last_diagnostics
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let diags = m
        .get(&ctx.workspace_id)
        .map(|(_, d)| d.clone())
        .unwrap_or_default();
    drop(m);
    let shown: Vec<_> = diags.iter().take(MAX_DIAGNOSTICS).collect();
    let mut text: String = shown.iter().map(|d| render_diagnostic(d)).collect();
    if diags.is_empty() {
        text = "No diagnostics from the newest build (or no build has run).".into();
    }
    Ok(ToolOutput {
        summary: format!("{} diagnostics", diags.len()),
        output: json!({"diagnostics": shown, "diagnosticCount": diags.len()}),
        text,
        truncated: diags.len() > MAX_DIAGNOSTICS,
    })
}

async fn env_detect(app: &Arc<App>, ctx: &ToolCtx) -> Result<ToolOutput, ToolError> {
    let r = crate::handlers::env::detect_tools(
        app.clone(),
        WorkspaceRef {
            workspace_id: ctx.workspace_id.clone(),
        },
    )
    .await
    .map_err(rpc)?;
    let v = serde_json::to_value(&r).unwrap_or_default();
    Ok(ToolOutput::json(v, "detected"))
}

async fn env_plan(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let p: inputs::PlanInstallInput = parse(&input)?;
    let version = match p.version {
        Some(v) => v,
        None => {
            // A read: the versions a manifest lists, never a program that would list them.
            let m = crate::handlers::env::provider(app, &p.provider)
                .await
                .map_err(rpc)?
                .ok_or_else(|| {
                    ToolError::new(
                        ErrorKind::NotFound,
                        format!("There is no provider \"{}\".", p.provider),
                    )
                })?;
            app.env
                .installer
                .listed_versions(&m)
                .await
                .map_err(|e| rpc(e.into()))?
                .ok_or_else(|| {
                    ToolError::invalid("This provider does not list its versions.")
                        .hint("Name the version to install (`version`).")
                })?
                .first()
                .cloned()
                .ok_or_else(|| {
                    ToolError::new(ErrorKind::NotFound, "No installable version is listed.")
                })?
        }
    };
    let plan = crate::handlers::env::plan_install(
        app.clone(),
        axiom_protocol::env::PlanInstallParams {
            provider_id: p.provider,
            version,
            method: None,
        },
    )
    .await
    .map_err(rpc)?;
    // Only this principal's env.install may run it (AUD-041).
    app.env.installer.set_owner(&plan.plan_id, &ctx.principal);
    let v = serde_json::to_value(&plan).unwrap_or_default();
    Ok(ToolOutput::json(v, format!("plan {}", plan.plan_id)))
}

/// The approval card of an install shows the plan: provider, version, scope, every download
/// (with how it is verified) and every command (AUD-041). Only a plan this principal made
/// with `env.planInstall` can be installed: not one a client issued, not another agent's.
async fn prepare_install(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: &Value,
) -> Result<Targets, ToolError> {
    use axiom_protocol::env::{Sha256Source, Step};
    use axiom_protocol::provider::InstallScope;
    let i: inputs::InstallInput = parse(input)?;
    let not_yours = || {
        ToolError::new(
            ErrorKind::NotFound,
            format!("There is no plan {} of yours.", i.plan_id),
        )
        .hint("Plan the install with env.planInstall (a plan lasts 10 minutes), then pass its planId.")
    };
    let (plan, owner) = app.env.installer.issued(&i.plan_id).ok_or_else(not_yours)?;
    if owner.as_deref() != Some(ctx.principal.as_str()) {
        return Err(not_yours());
    }
    if plan.scope == InstallScope::System {
        return Err(ToolError::new(
            ErrorKind::PermissionDenied,
            "An agent cannot install system packages.",
        ));
    }
    let mut preview = format!(
        "Install {} {} into Axiom's managed storage:\n",
        plan.provider_id, plan.version
    );
    for (n, step) in plan.steps.iter().enumerate() {
        let line = match step {
            Step::Download { url, sha256, .. } => format!(
                "download {url} ({})",
                match sha256 {
                    Sha256Source::Digest { .. } => "SHA-256 from the manifest".to_string(),
                    Sha256Source::Sha512Digest { .. } => "SHA-512 from the manifest".to_string(),
                    Sha256Source::Url { url } => format!("checksum from {url}"),
                }
            ),
            Step::Extract { .. } => "extract it".to_string(),
            Step::Run { argv, .. } => format!("run {}", axiom_agent::tools::display_argv(argv)),
            Step::SystemPackages { argv, .. } => {
                format!("run (elevated) {}", axiom_agent::tools::display_argv(argv))
            }
            Step::Verify { tool_id } => format!("verify {tool_id}"),
        };
        preview.push_str(&format!("{}. {line}\n", n + 1));
    }
    Ok(Targets {
        preview: Some(preview),
        conditions: vec![format!(
            "{} {} (managed install)",
            plan.provider_id, plan.version
        )],
        opaque: true,
        ..Default::default()
    })
}

async fn env_install(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let i: inputs::InstallInput = parse(&input)?;
    let current = match app.env.installer.plan_provider(&i.plan_id) {
        Some(id) => crate::handlers::env::provider(app, &id)
            .await
            .map_err(rpc)?,
        None => None,
    };
    let r = app
        .env
        .installer
        .install(&i.plan_id, current.as_ref(), &ctx.cancel, &|_| {})
        .await
        .map_err(|e| rpc(e.into()))?;
    let v = serde_json::to_value(&r).unwrap_or_default();
    Ok(ToolOutput::json(v, "installed"))
}

async fn editor_open(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let o: inputs::OpenInput = parse(&input)?;
    let w = ws(app, ctx)?;
    w.resolve(&o.path).map_err(ws_err)?;
    let params = RevealParams {
        workspace_id: ctx.workspace_id.clone(),
        path: o.path.clone(),
        line: o.line.unwrap_or(1).max(1),
        column: o.column.unwrap_or(1).max(1),
    };
    // Every connection at once; a stuck one is not waited on (AUD-030, review B6b).
    let shown = app
        .notify_clients::<Reveal>(&ctx.workspace_id, params.clone())
        .await;
    Ok(ToolOutput::json(
        json!({"path": o.path, "line": params.line, "column": params.column, "clients": shown}),
        if shown > 0 {
            "shown"
        } else {
            "no editor attached"
        },
    ))
}

// ---------------------------------------------------------------- preview.* and ui.* (M5)

const SNAPSHOT_NODES: u32 = 300;
const SNAPSHOT_MAX_NODES: u32 = 2000;

/// The command `preview/start` will run, for the gate (the same choice as the handler).
fn preview_argv(root: &std::path::Path, argv: Option<Vec<String>>) -> Vec<String> {
    match argv {
        Some(a) if !a.is_empty() => a,
        _ if root.join("node_modules/.bin/vite").is_file() => vec!["node_modules/.bin/vite".into()],
        _ => ["npm", "run", "dev"].map(String::from).to_vec(),
    }
}

async fn prepare_preview(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: &Value,
) -> Result<Targets, ToolError> {
    let p: inputs::PreviewStartInput = parse(input)?;
    let w = ws(app, ctx)?;
    Ok(Targets {
        argv: Some(preview_argv(w.root(), p.argv)),
        // The dev server does not get them either (review E5).
        preview: axiom_agent::secrets::describe(&withheld(app).await?),
        ..Default::default()
    })
}

fn preview_text(i: &axiom_protocol::preview::PreviewInfo) -> String {
    let mut t = format!("preview {} ({:?})", i.preview_id.0, i.state);
    if let Some(u) = &i.url {
        t.push_str(&format!("\nurl: {u}"));
    }
    t.push_str(match &i.adapter {
        Some(_) => "\npage: connected (ui.* tools are available)",
        None => "\npage: not connected; ask the person to open the URL in their browser",
    });
    if let Some(m) = &i.message {
        t.push_str(&format!("\n{m}"));
    }
    if let Some(o) = &i.output {
        t.push_str(&format!("\noutput:\n{o}"));
    }
    t
}

async fn preview_start(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
) -> Result<ToolOutput, ToolError> {
    let p: inputs::PreviewStartInput = parse(&input)?;
    let hidden = withheld(app).await?;
    let info = crate::handlers::preview::start_with(
        app.clone(),
        axiom_protocol::preview::StartParams {
            workspace_id: ctx.workspace_id.clone(),
            argv: p.argv,
        },
        &hidden,
    )
    .await
    .map_err(rpc)?;
    Ok(ToolOutput {
        output: serde_json::to_value(&info).unwrap_or_default(),
        text: with_env_note(preview_text(&info), &hidden),
        summary: info.url.clone().unwrap_or_else(|| "preview".into()),
        truncated: false,
    })
}

/// The preview a `ui.*` call means: the given one if it is this workspace's, else the
/// workspace's running preview.
fn preview_of(
    app: &App,
    ctx: &ToolCtx,
    id: Option<String>,
) -> Result<axiom_protocol::preview::PreviewId, ToolError> {
    use axiom_protocol::preview::PreviewState;
    let mine = app.previews.of_workspace(&ctx.workspace_id);
    match id {
        Some(id) => mine
            .iter()
            .find(|i| i.preview_id.0 == id)
            .map(|i| i.preview_id.clone())
            .ok_or_else(|| {
                ToolError::new(
                    ErrorKind::NotFound,
                    format!("This workspace has no preview {id}."),
                )
                .hint("Omit previewId to use the workspace's running preview.")
            }),
        None => mine
            .into_iter()
            .find(|i| i.state == PreviewState::Ready)
            .map(|i| i.preview_id)
            .ok_or_else(|| {
                ToolError::new(
                    ErrorKind::NotFound,
                    "No preview is running in this workspace.",
                )
                .hint("Start one with preview.start; the person then opens its URL.")
            }),
    }
}

fn ui_err(e: RpcError) -> ToolError {
    let kind = e.data.kind;
    let t = rpc(e);
    if kind == ErrorKind::AdapterUnavailable {
        t.hint("Ask the person to open the preview's URL (preview.start returns it) in their browser, then try again.")
    } else {
        t
    }
}

fn source_text(c: &axiom_protocol::inspection::SourceCandidate) -> String {
    use axiom_protocol::inspection::MappingMethod;
    let loc = format!(
        "{}:{}:{}",
        c.location.file.0,
        c.location.line,
        c.location.column.unwrap_or(1)
    );
    if c.method == MappingMethod::BuildInstrumentation && c.confidence >= 1.0 {
        format!("{loc} (exact)")
    } else {
        format!(
            "{loc} ({}, {:?}, confidence {:.2})",
            serde_json::to_value(c.relation)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_default(),
            c.method,
            c.confidence
        )
    }
}

fn node_line(n: &axiom_protocol::inspection::UiNode) -> String {
    let mut t = format!("[{}] {}", n.id.0, n.type_name);
    if let Some(name) = &n.name {
        t.push_str(&format!(" #{name}"));
    }
    if let Some(x) = &n.text {
        let x: String = x.chars().take(60).collect();
        t.push_str(&format!(" {x:?}"));
    }
    if let Some(k) = &n.stable_key {
        t.push_str(&format!(" key={k}"));
    }
    if let Some(b) = &n.bounds {
        t.push_str(&format!(
            " at {},{} {}x{}",
            round(b.x),
            round(b.y),
            round(b.width),
            round(b.height)
        ));
    }
    if let Some(c) = n.sources.first() {
        t.push_str(&format!(" src={}", source_text(c)));
    }
    t
}

fn round(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

async fn ui_snapshot(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let p: inputs::UiSnapshotInput = parse(&input)?;
    let preview = preview_of(app, ctx, p.preview_id)?;
    let max = p
        .max_nodes
        .unwrap_or(SNAPSHOT_NODES)
        .clamp(1, SNAPSHOT_MAX_NODES);
    let snap = crate::handlers::ui::snapshot(
        app.clone(),
        axiom_protocol::ui::SnapshotParams {
            preview_id: preview.clone(),
            max_nodes: Some(max),
            include_invisible: false,
        },
    )
    .await
    .map_err(ui_err)?;
    // Indent by depth (parents come before children).
    let mut depth = std::collections::HashMap::new();
    let mut text = String::new();
    for n in &snap.nodes {
        let d = n
            .parent
            .as_ref()
            .and_then(|p| depth.get(&p.0).copied())
            .map(|d: usize| d + 1)
            .unwrap_or(0);
        depth.insert(n.id.0.clone(), d);
        text.push_str(&"  ".repeat(d.min(20)));
        text.push_str(&node_line(n));
        text.push('\n');
    }
    if snap.truncated {
        text.push_str(&format!("[truncated at {max} nodes]\n"));
    }
    let (text, cut) = bound(&text, DEFAULT_MAX_RESULT_BYTES - 1024);
    Ok(ToolOutput {
        output: json!({"previewId": preview.0, "nodeCount": snap.nodes.len(), "truncated": snap.truncated || cut}),
        text,
        summary: format!("{} nodes", snap.nodes.len()),
        truncated: snap.truncated || cut,
    })
}

/// `text` cut at a line boundary within `max` bytes.
fn bound(text: &str, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text.to_string(), false);
    }
    let mut cut = max;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let head = &text[..cut];
    let head = head.rfind('\n').map(|i| &head[..=i]).unwrap_or(head);
    (format!("{head}[truncated]\n"), true)
}

fn node_detail_text(n: &axiom_protocol::inspection::UiNode) -> String {
    let mut t = node_line(n);
    if n.sources.len() > 1 {
        t.push_str("\nsources:");
        for c in &n.sources {
            t.push_str(&format!("\n  {}", source_text(c)));
        }
    }
    if let Some(a) = &n.accessibility {
        t.push_str(&format!(
            "\naccessibility: {}",
            serde_json::to_string(a).unwrap_or_default()
        ));
    }
    if !n.properties.is_empty() {
        t.push_str("\nproperties:");
        for (k, v) in &n.properties {
            let v = v
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| v.to_string());
            t.push_str(&format!("\n  {k}: {v}"));
        }
    }
    t
}

async fn ui_inspect(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    use axiom_protocol::inspection::NodeId;
    let p: inputs::UiInspectInput = parse(&input)?;
    let preview = preview_of(app, ctx, p.preview_id)?;
    let node = match (p.node, p.stable_key) {
        (Some(id), _) => NodeId(id),
        (None, Some(key)) => {
            let snap = crate::handlers::ui::snapshot(
                app.clone(),
                axiom_protocol::ui::SnapshotParams {
                    preview_id: preview.clone(),
                    max_nodes: None,
                    include_invisible: true,
                },
            )
            .await
            .map_err(ui_err)?;
            snap.nodes
                .into_iter()
                .find(|n| n.stable_key.as_deref() == Some(key.as_str()))
                .map(|n| n.id)
                .ok_or_else(|| {
                    ToolError::new(
                        ErrorKind::NotFound,
                        format!("No element has the key {key}."),
                    )
                    .hint("Take a ui.snapshot and use a key from it.")
                })?
        }
        (None, None) => {
            return Err(ToolError::invalid("Name the element.")
                .hint("Give `stableKey` (from ui.snapshot or ui.selection) or `node`."))
        }
    };
    let n = crate::handlers::ui::node_details(
        app.clone(),
        axiom_protocol::ui::NodeParams {
            preview_id: preview,
            node,
        },
    )
    .await
    .map_err(ui_err)?;
    let (text, cut) = bound(&node_detail_text(&n), DEFAULT_MAX_RESULT_BYTES - 1024);
    Ok(ToolOutput {
        output: serde_json::to_value(&n).unwrap_or_default(),
        text,
        summary: n.type_name.clone(),
        truncated: cut,
    })
}

async fn ui_selection(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
) -> Result<ToolOutput, ToolError> {
    let p: inputs::UiSelectionInput = parse(&input)?;
    let preview = preview_of(app, ctx, p.preview_id)?;
    let Some(chain) = app.previews.selection(&preview).filter(|c| !c.is_empty()) else {
        return Ok(ToolOutput {
            output: json!({"previewId": preview.0, "selected": null}),
            text: "Nothing is selected. The person selects an element with Axiom's inspect mode; or find it with ui.snapshot.".into(),
            summary: "nothing selected".into(),
            truncated: false,
        });
    };
    let mut text = format!("selected: {}", node_line(&chain[0]));
    if chain.len() > 1 {
        text.push_str("\nancestors:");
        for n in &chain[1..] {
            text.push_str(&format!("\n  {}", node_line(n)));
        }
    }
    let (text, cut) = bound(&text, DEFAULT_MAX_RESULT_BYTES - 1024);
    Ok(ToolOutput {
        output: json!({"previewId": preview.0, "chain": chain}),
        text,
        summary: chain[0].type_name.clone(),
        truncated: cut,
    })
}

async fn ui_verify(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let p: inputs::UiVerifyInput = parse(&input)?;
    if p.assertions.is_empty() {
        return Err(ToolError::invalid("No check is stated.").hint(
            "State what the page must measure if the change worked: a geometry assertion (an edge against another node's edge plus an offset, or a value) or a property assertion (e.g. css.margin-left equals \"24px\").",
        ));
    }
    let preview = preview_of(app, ctx, p.preview_id)?;
    let report = crate::handlers::ui::verify(
        app.clone(),
        axiom_protocol::ui::VerifyParams {
            preview_id: preview,
            assertions: p.assertions,
            target: p.target,
            timeout_ms: p.timeout_ms,
            since_ms: None,
        },
    )
    .await
    .map_err(ui_err)?;
    let verdict = serde_json::to_value(report.verdict)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    let mut text = format!("verdict: {verdict}");
    for c in &report.checks {
        let mark = if c.skipped {
            "skipped"
        } else if c.passed {
            "pass"
        } else {
            "FAIL"
        };
        let kind = serde_json::to_value(c.kind)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_default();
        text.push_str(&format!("\n{mark} {kind}: {}", c.summary));
    }
    let (text, cut) = bound(&text, DEFAULT_MAX_RESULT_BYTES - 1024);
    Ok(ToolOutput {
        output: serde_json::to_value(&report).unwrap_or_default(),
        text,
        summary: verdict,
        truncated: cut,
    })
}

// ---------------------------------------------------------------- ACP bridge tools (hidden)

/// The hidden tools an external ACP agent's file and terminal requests become (ADR-0016 §3):
/// gated and checkpointed like the others, never offered to a model.
pub fn register_acp(r: &mut Registry, app: Weak<App>) {
    let mut add = |name: &str, effect, prep: Prep, run: Run| {
        r.register_hidden(Arc::new(DaemonTool {
            spec: ToolSpec {
                name: name.into(),
                description: format!("ACP client request ({name})"),
                input_schema: json!({"type": "object"}),
                effect,
                max_result_bytes: 4 * 1024 * 1024,
            },
            app: app.clone(),
            prep,
            run,
        }))
    };
    add(
        "acp.readFile",
        Effect::Read,
        |a, c, i| acp_target(a, c, i).boxed(),
        |a, c, i, _| acp_read(a, c, i).boxed(),
    );
    add(
        "acp.writeFile",
        Effect::WriteWorkspace,
        |a, c, i| acp_prepare_write(a, c, i).boxed(),
        |a, c, i, t| acp_write(a, c, i, t).boxed(),
    );
    add(
        "acp.terminal",
        Effect::Execute,
        |a, c, i| acp_prepare_terminal(a, c, i).boxed(),
        |a, c, i, _| acp_terminal(a, c, i).boxed(),
    );
}

fn acp_rel(app: &App, ctx: &ToolCtx, abs: &Value) -> Result<WorkspacePath, ToolError> {
    let abs = abs
        .as_str()
        .ok_or_else(|| ToolError::invalid("`path` must be an absolute path."))?;
    let w = ws(app, ctx)?;
    let rel = axiom_agent::acp::workspace_path(w.root(), abs).ok_or_else(|| {
        ToolError::new(
            ErrorKind::PermissionDenied,
            format!("`{abs}` is outside the workspace."),
        )
    })?;
    // The path the file really has, through any symlink: what the gate judges (review B4).
    w.canonical_path(&WorkspacePath(rel)).map_err(ws_err)
}

async fn acp_target(app: &Arc<App>, ctx: &ToolCtx, input: &Value) -> Result<Targets, ToolError> {
    Ok(Targets {
        paths: vec![acp_rel(app, ctx, &input["path"])?],
        ..Default::default()
    })
}

async fn acp_read(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let p = acp_rel(app, ctx, &input["path"])?;
    let f = ws(app, ctx)?.read_text(&p).await.map_err(ws_err)?;
    let content = match (input["line"].as_u64(), input["limit"].as_u64()) {
        (None, None) => f.text,
        (line, limit) => {
            let start = line.unwrap_or(1).max(1) as usize - 1;
            let n = limit.map_or(usize::MAX, |l| l as usize);
            f.text.split_inclusive('\n').skip(start).take(n).collect()
        }
    };
    Ok(ToolOutput {
        summary: format!("read {}", p.0),
        text: String::new(),
        output: json!({"content": content}),
        truncated: false,
    })
}

async fn acp_prepare_write(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: &Value,
) -> Result<Targets, ToolError> {
    let p = acp_rel(app, ctx, &input["path"])?;
    let new = input["content"]
        .as_str()
        .ok_or_else(|| ToolError::invalid("`content` is required."))?;
    let w = ws(app, ctx)?;
    let (old, version) = match w.read_text(&p).await {
        Ok(f) => (f.text, Some(f.version)),
        Err(WsError::NotFound(_)) => (String::new(), None),
        Err(e) => return Err(ws_err(e)),
    };
    let (diff, _, _) = patch::diff(&p.0, &old, new);
    Ok(Targets {
        paths: vec![p.clone()],
        preview: Some(diff),
        versions: vec![(p, version)],
        ..Default::default()
    })
}

async fn acp_write(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
    targets: &Targets,
) -> Result<ToolOutput, ToolError> {
    let p = acp_rel(app, ctx, &input["path"])?;
    let new = input["content"].as_str().unwrap_or_default();
    let expected = targets
        .versions
        .iter()
        .find(|(q, _)| *q == p)
        .and_then(|(_, v)| v.clone());
    let w = ws(app, ctx)?;
    // Only where the gate judged it (review R5).
    let judged = targets.paths.first().unwrap_or(&p);
    let v = w
        .write_text_checked(&p, judged, new, expected.as_ref(), false)
        .await
        .map_err(ws_err)?;
    Ok(ToolOutput::json(
        json!({"path": p, "version": v}),
        format!("wrote {}", p.0),
    ))
}

fn acp_argv(input: &Value) -> Result<Vec<String>, ToolError> {
    let cmd = input["command"]
        .as_str()
        .filter(|c| !c.is_empty())
        .ok_or_else(|| ToolError::invalid("`command` is required."))?;
    let mut argv = vec![cmd.to_string()];
    for a in input["args"].as_array().into_iter().flatten() {
        // A non-string argument was run as "" (AUD-039): refuse it instead.
        argv.push(
            a.as_str()
                .ok_or_else(|| ToolError::invalid("Every argument must be a string."))?
                .to_string(),
        );
    }
    Ok(argv)
}

/// The environment variables an ACP `terminal/create` sets, validated.
fn acp_env(input: &Value) -> Result<Vec<(String, String)>, ToolError> {
    let mut out = Vec::new();
    for e in input["env"].as_array().into_iter().flatten() {
        let (Some(n), Some(v)) = (e["name"].as_str(), e["value"].as_str()) else {
            return Err(ToolError::invalid(
                "Every env entry needs a string name and value.",
            ));
        };
        if n.is_empty() || n.contains(['=', '\0']) || v.contains('\0') {
            return Err(ToolError::invalid(format!(
                "`{n}` is not a valid variable."
            )));
        }
        out.push((n.to_string(), v.to_string()));
    }
    Ok(out)
}

/// The working directory an ACP terminal asks for, as a workspace path (`None`: the root).
fn acp_cwd(app: &App, ctx: &ToolCtx, input: &Value) -> Result<Option<WorkspacePath>, ToolError> {
    match input["cwd"].as_str() {
        None => Ok(None),
        Some(c) => {
            let w = ws(app, ctx)?;
            if std::path::Path::new(c) == w.root() {
                return Ok(None);
            }
            acp_rel(app, ctx, &json!(c)).map(Some)
        }
    }
}

/// How many characters of a variable's value its condition line shows.
const ACP_ENV_SHOWN: usize = 300;

/// One condition line per variable the agent sets. A long value is shown cut, followed by the
/// SHA-256 of the whole value: the line is what a session grant compares, so two values that
/// share their first characters are two different calls (review N1). The card's preview shows
/// every value in full.
fn acp_env_conditions(env: &[(String, String)]) -> Vec<String> {
    use sha2::Digest;
    env.iter()
        .map(|(n, v)| {
            if v.chars().count() <= ACP_ENV_SHOWN {
                format!("env {n}={v}")
            } else {
                let shown: String = v.chars().take(ACP_ENV_SHOWN).collect();
                let hash = hex::encode(sha2::Sha256::digest(v.as_bytes()));
                format!("env {n}={shown}… (sha256 {hash})")
            }
        })
        .collect()
}

/// One line per variable the agent sets, for the card: a name or value with a line break,
/// whitespace, quotes or control characters is quoted with `\` escapes (as an argv is), so a
/// value cannot add lines that pass for the card's own or a terminal prompt's (review E6).
fn acp_env_lines(env: &[(String, String)]) -> Vec<String> {
    use axiom_agent::tools::display_argv;
    env.iter()
        .map(|(n, v)| {
            format!(
                "  {}={}",
                display_argv(std::slice::from_ref(n)),
                display_argv(std::slice::from_ref(v))
            )
        })
        .collect()
}

/// Everything that decides what the terminal runs is on the card: the argv, the variables the
/// agent sets and a working directory other than the root. Either of the latter makes it ask
/// even where a rule allows the argv (AUD-039).
async fn acp_prepare_terminal(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: &Value,
) -> Result<Targets, ToolError> {
    let argv = acp_argv(input)?;
    let env = acp_env(input)?;
    let cwd = acp_cwd(app, ctx, input)?;
    let mut conditions = acp_env_conditions(&env);
    if let Some(c) = &cwd {
        conditions.push(format!("cwd {}", c.0));
    }
    let mut preview: Vec<String> = Vec::new();
    if !env.is_empty() {
        preview.push("The agent sets these variables:".into());
        preview.extend(acp_env_lines(&env));
    }
    if let Some(c) = &cwd {
        preview.push(format!("in the folder {}", c.0));
    }
    preview.extend(axiom_agent::secrets::describe(&withheld(app).await?));
    Ok(Targets {
        argv: Some(argv),
        escalate: !conditions.is_empty(),
        conditions,
        preview: (!preview.is_empty()).then(|| preview.join("\n")),
        ..Default::default()
    })
}

async fn acp_terminal(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
) -> Result<ToolOutput, ToolError> {
    let argv = acp_argv(&input)?;
    let w = ws(app, ctx)?;
    let cwd = match acp_cwd(app, ctx, &input)? {
        Some(c) => w.resolve(&c).map_err(ws_err)?,
        None => w.root().to_path_buf(),
    };
    let (mut env, _) = spawn_env(app, ctx).await?;
    for (n, v) in acp_env(&input)? {
        env.insert(n, v);
    }
    let limit = input["outputByteLimit"]
        .as_u64()
        .unwrap_or(1024 * 1024)
        .min(MAX_ACP_OUTPUT) as usize;
    let (task_id, rx) = app
        .executor
        .start(
            ctx.workspace_id.clone(),
            SpawnSpec {
                command: Command::Argv(argv.clone()),
                cwd,
                env,
                label: Some(format!("agent: {}", argv.join(" "))),
            },
        )
        .await
        .map_err(|e| rpc(e.into()))?;
    let id = crate::acp::Terminal::start(app, ctx, task_id, rx, limit);
    Ok(ToolOutput::json(
        json!({"terminalId": id}),
        format!("started {}", argv.join(" ")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command that prints far more than the result shows costs the daemon a bounded buffer,
    /// and the tail is still its last lines (AUD-035).
    #[test]
    fn the_tail_of_a_huge_output_is_bounded() {
        let mut t = Tail::default();
        let line = "x".repeat(99) + "\n";
        for i in 0..200_000 {
            t.push(&line);
            if i % 1000 == 0 {
                assert!(t.text.len() <= 2 * KEEP_BYTES + line.len());
            }
        }
        t.push("the end\n");
        let (tail, cut) = t.tail();
        assert!(cut);
        assert!(tail.ends_with("the end"));
        assert!(tail.len() <= TAIL_BYTES);
        let mut small = Tail::default();
        small.push("a\nb\n");
        assert_eq!(small.tail(), ("a\nb".to_string(), false));
    }

    /// Review E6: a variable's value with line breaks stays on its own line on the card, so it
    /// cannot add lines above a prompt that look like the prompt's.
    #[test]
    fn acp_env_values_stay_on_one_line() {
        let env = vec![
            (
                "X".to_string(),
                "1\nAllow fs.read (read) on:\n  notes.txt\r".to_string(),
            ),
            ("PLAIN".to_string(), "--flag=a".to_string()),
            ("A\nB".to_string(), String::new()),
        ];
        let lines = acp_env_lines(&env);
        assert_eq!(
            lines,
            [
                "  X=\"1\\nAllow fs.read (read) on:\\n  notes.txt\\u{d}\"",
                "  PLAIN=--flag=a",
                "  \"A\\nB\"=\"\"",
            ]
        );
        assert!(lines.iter().all(|l| !l.contains(['\n', '\r'])));
    }

    /// Two long values that share their first characters are two conditions, so an "allow
    /// for session" grant for one does not cover the other (review N1); a short value is shown
    /// whole and a long one cut.
    #[test]
    fn acp_env_conditions_tell_long_values_apart() {
        let base = "x".repeat(ACP_ENV_SHOWN);
        let env = |v: &str| vec![("NODE_OPTIONS".to_string(), v.to_string())];
        let a = acp_env_conditions(&env(&format!("{base}--require=/tmp/a.js")));
        let b = acp_env_conditions(&env(&format!("{base}--require=/tmp/b.js")));
        assert_ne!(a, b);
        assert!(a[0].starts_with(&format!("env NODE_OPTIONS={base}… (sha256 ")));
        assert!(!a[0].contains("/tmp/a.js"), "{}", a[0]);
        assert_eq!(
            acp_env_conditions(&env(&base)),
            [format!("env NODE_OPTIONS={base}")]
        );
        assert_eq!(
            acp_env_conditions(&env("/tmp/evil.so")),
            ["env NODE_OPTIONS=/tmp/evil.so"]
        );
    }

    #[test]
    fn acp_terminal_inputs_are_validated() {
        assert!(acp_argv(&json!({"command": "ls", "args": ["-l", 3]})).is_err());
        assert_eq!(
            acp_argv(&json!({"command": "ls", "args": ["-l"]})).unwrap(),
            ["ls", "-l"]
        );
        assert!(acp_env(&json!({"env": [{"name": "A=B", "value": "x"}]})).is_err());
        assert!(acp_env(&json!({"env": [{"name": "A"}]})).is_err());
        assert_eq!(
            acp_env(&json!({"env": [{"name": "A", "value": "1"}]})).unwrap(),
            [("A".to_string(), "1".to_string())]
        );
    }
}
