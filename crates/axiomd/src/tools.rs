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
const MAX_DIAGNOSTICS: usize = 50;

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
        |_, _, i| {
            async move {
                let t: inputs::TaskRunInput = parse(i)?;
                if t.argv.is_empty() || t.argv[0].is_empty() {
                    return Err(ToolError::invalid("argv is empty.").hint("Give the program and its arguments, e.g. [\"cargo\", \"test\"]."));
                }
                Ok(Targets {
                    argv: Some(t.argv),
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
        |a, _, i, _| env_plan(a, i).boxed(),
    );
    add(
        "env.install",
        "Execute an install plan from env.planInstall (its planId, within 10 minutes).",
        schema_of::<inputs::InstallInput>(),
        Effect::Install,
        |_, _, i| {
            async move { parse::<inputs::InstallInput>(i).map(|_| Targets::default()) }.boxed()
        },
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
    let mut out = String::new();
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
                    out.push_str(&o.data);
                }
            }
            TaskEvent::Exited(x) => break x.status,
        }
    };
    let (t, cut) = tail(&out);
    let v = json!({
        "outcome": outcome_name(status.outcome),
        "exitCode": status.exit_code,
        "durationMs": started.elapsed().as_millis() as u64,
        "timedOut": timed_out,
        "output": t,
    });
    Ok((v, out, cut))
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
    // Resolve first: a path outside the workspace never gets as far as the gate.
    w.resolve(&p.path).map_err(ws_err)?;
    let (version, new, diff, _, _) = plan_patch(&w, &p).await?;
    Ok(Targets {
        paths: vec![p.path.clone()],
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
    let new_version = match (&new, &version) {
        (Some(text), v) => Some(
            w.write_text(&p.path, text, v.as_ref(), false)
                .await
                .map_err(ws_err)?,
        ),
        (None, Some(v)) => {
            w.remove_file(&p.path, v).await.map_err(ws_err)?;
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

async fn spawn_env(
    app: &App,
    ctx: &ToolCtx,
) -> Result<std::collections::BTreeMap<String, String>, ToolError> {
    let wsinfo = workspace(app, &ctx.workspace_id).await.map_err(rpc)?;
    let r = env_registry(app, Some(wsinfo)).await.map_err(rpc)?;
    let environment = project_environment(app, &ctx.workspace_id, &r)
        .await
        .map_err(rpc)?;
    Ok(axiom_env::project::process_env(
        &environment,
        std::env::var_os("PATH"),
    ))
}

async fn task_run(app: &Arc<App>, ctx: &ToolCtx, input: Value) -> Result<ToolOutput, ToolError> {
    let t: inputs::TaskRunInput = parse(&input)?;
    let timeout = t
        .timeout_secs
        .unwrap_or(DEFAULT_TIMEOUT)
        .clamp(1, MAX_TIMEOUT);
    let w = ws(app, ctx)?;
    let spec = SpawnSpec {
        command: Command::Argv(t.argv.clone()),
        cwd: w.root().to_path_buf(),
        env: spawn_env(app, ctx).await?,
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
        text,
        summary,
        truncated: cut,
    })
}

async fn prepare_build(app: &Arc<App>, ctx: &ToolCtx, input: &Value) -> Result<Targets, ToolError> {
    let b: inputs::BuildRunInput = parse(input)?;
    let (provider, argv) = build_spec(app, ctx, &b).await?;
    let _ = provider;
    Ok(Targets {
        argv: Some(argv),
        ..Default::default()
    })
}

/// The provider offering `task` and the argv it will run.
async fn build_spec(
    app: &Arc<App>,
    ctx: &ToolCtx,
    b: &inputs::BuildRunInput,
) -> Result<(String, Vec<String>), ToolError> {
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
    let prepared =
        axiom_build::prepare(&r, &t.provider_id, &t.id, &root, &Default::default(), None)
            .map_err(|e| rpc(e.into()))?;
    let argv = match prepared.spec.command {
        Command::Argv(a) => a,
        Command::Shell(s) => vec![s],
    };
    Ok((t.provider_id.clone(), argv))
}

async fn build_run(
    app: &Arc<App>,
    ctx: &ToolCtx,
    input: Value,
    _targets: &Targets,
) -> Result<ToolOutput, ToolError> {
    let b: inputs::BuildRunInput = parse(&input)?;
    let (provider, _) = build_spec(app, ctx, &b).await?;
    let wsinfo = workspace(app, &ctx.workspace_id).await.map_err(rpc)?;
    let root = wsinfo.0.clone();
    let r = env_registry(app, Some(wsinfo)).await.map_err(rpc)?;
    let environment = project_environment(app, &ctx.workspace_id, &r)
        .await
        .map_err(rpc)?;
    let prepared = axiom_build::prepare(
        &r,
        &provider,
        &b.task,
        &root,
        &environment,
        std::env::var_os("PATH"),
    )
    .map_err(|e| rpc(e.into()))?;
    let started = Instant::now();
    let (task_id, rx) = app
        .executor
        .start(ctx.workspace_id.clone(), prepared.spec)
        .await
        .map_err(|e| rpc(e.into()))?;
    let mut events = axiom_build::drive(task_id.clone(), rx, &prepared.diagnostics, vec![root]);
    let mut out = String::new();
    let mut diags = Vec::new();
    let status = loop {
        let ev = tokio::select! {
            ev = events.recv() => ev,
            _ = ctx.cancel.cancelled() => {
                let _ = app.executor.cancel(&task_id).await;
                events.recv().await
            }
        };
        match ev {
            Some(axiom_build::BuildEvent::Output(o)) => out.push_str(&o.data),
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
    let (t, cut) = tail(&out);
    let errors = diags
        .iter()
        .filter(|d| d.severity == axiom_protocol::diagnostics::Severity::Error)
        .count();
    let shown: Vec<_> = diags.iter().take(MAX_DIAGNOSTICS).collect();
    let mut text = format!(
        "{}: {} (exit code {}), {} diagnostics ({errors} errors)\n",
        b.task,
        outcome_name(status.outcome),
        status.exit_code.map_or("none".into(), |c| c.to_string()),
        diags.len()
    );
    for d in &shown {
        text.push_str(&render_diagnostic(d));
    }
    if shown.is_empty() {
        let (t2, _) = tail(&out);
        text.push_str(&t2);
    }
    let v = json!({
        "task": b.task, "provider": provider,
        "outcome": outcome_name(status.outcome), "exitCode": status.exit_code,
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
        text,
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

async fn env_plan(app: &Arc<App>, input: Value) -> Result<ToolOutput, ToolError> {
    let p: inputs::PlanInstallInput = parse(&input)?;
    let version = match p.version {
        Some(v) => v,
        None => crate::handlers::env::versions(
            app.clone(),
            axiom_protocol::env::VersionsParams {
                provider_id: p.provider.clone(),
            },
        )
        .await
        .map_err(rpc)?
        .available
        .first()
        .cloned()
        .ok_or_else(|| ToolError::new(ErrorKind::NotFound, "No installable version is listed."))?,
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
    let v = serde_json::to_value(&plan).unwrap_or_default();
    Ok(ToolOutput::json(v, format!("plan {}", plan.plan_id)))
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
    let mut shown = 0;
    for n in app.clients_of(&ctx.workspace_id) {
        if n.notify::<Reveal>(params.clone()).await.is_ok() {
            shown += 1;
        }
    }
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
    let info = crate::handlers::preview::start(
        app.clone(),
        axiom_protocol::preview::StartParams {
            workspace_id: ctx.workspace_id.clone(),
            argv: p.argv,
        },
    )
    .await
    .map_err(rpc)?;
    Ok(ToolOutput {
        output: serde_json::to_value(&info).unwrap_or_default(),
        text: preview_text(&info),
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
    let p = WorkspacePath(rel);
    w.resolve(&p).map_err(ws_err)?;
    Ok(p)
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
    let v = w
        .write_text(&p, new, expected.as_ref(), false)
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
        argv.push(a.as_str().unwrap_or_default().to_string());
    }
    Ok(argv)
}

async fn acp_prepare_terminal(
    _app: &Arc<App>,
    _ctx: &ToolCtx,
    input: &Value,
) -> Result<Targets, ToolError> {
    Ok(Targets {
        argv: Some(acp_argv(input)?),
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
    let cwd = match input["cwd"].as_str() {
        Some(c) => w.resolve(&acp_rel(app, ctx, &json!(c))?).map_err(ws_err)?,
        None => w.root().to_path_buf(),
    };
    let mut env = spawn_env(app, ctx).await?;
    for e in input["env"].as_array().into_iter().flatten() {
        if let (Some(n), Some(v)) = (e["name"].as_str(), e["value"].as_str()) {
            env.insert(n.into(), v.into());
        }
    }
    let limit = input["outputByteLimit"].as_u64().unwrap_or(1024 * 1024) as usize;
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
    let id = crate::acp::Terminal::start(app, task_id, rx, limit);
    Ok(ToolOutput::json(
        json!({"terminalId": id}),
        format!("started {}", argv.join(" ")),
    ))
}
