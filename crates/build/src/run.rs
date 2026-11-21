//! Provider tasks: listing, the process spec, and the run driver that turns `exec`'s events
//! into output, batched diagnostics and the exit (M2-BUILD-001).

use crate::parse::Parser;
use axiom_env::registry::Registry;
use axiom_exec::{SpawnSpec, TaskEvent};
use axiom_protocol::build::{BuildDiagnostics, TaskInfo};
use axiom_protocol::diagnostics::{Diagnostic, Severity};
use axiom_protocol::env::Environment;
use axiom_protocol::provider::{DiagnosticsFormat, TaskTemplate};
use axiom_protocol::task::{Command, ExitedParams, OutputParams, Stream};
use axiom_protocol::{ErrorKind, RpcError, TaskId};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// At most this often a `build/diagnostics` batch is sent while a task runs (10 Hz).
pub const BATCH_INTERVAL: Duration = Duration::from_millis(100);

#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    InvalidTemplate(String),
}

impl From<BuildError> for RpcError {
    fn from(e: BuildError) -> RpcError {
        let kind = match e {
            BuildError::NotFound(_) => ErrorKind::NotFound,
            BuildError::InvalidTemplate(_) => ErrorKind::InvalidParams,
        };
        RpcError::new(kind, e.to_string())
    }
}

/// The tasks of every provider that applies to the workspace, in registry order.
pub fn tasks(registry: &Registry, root: &Path) -> Vec<TaskInfo> {
    registry
        .active()
        .filter(|(_, m)| axiom_env::detect::applies(m, root))
        .flat_map(|(_, m)| {
            m.tasks.iter().map(|t| TaskInfo {
                provider_id: m.id.clone(),
                id: t.id.clone(),
                kind: t.kind,
                label: t.label.clone(),
            })
        })
        .collect()
}

/// A task template resolved for running.
pub struct Prepared {
    pub spec: SpawnSpec,
    pub diagnostics: DiagnosticsFormat,
}

/// The process for `provider`/`task` in `root`: argv with `${workspace}` expanded (never a
/// shell), working directory the workspace root, and the environment's variables and `PATH`
/// prepend over `inherited_path`.
pub fn prepare(
    registry: &Registry,
    provider: &str,
    task: &str,
    root: &Path,
    env: &Environment,
    inherited_path: Option<OsString>,
) -> Result<Prepared, BuildError> {
    let m = registry
        .get(provider)
        .ok_or_else(|| BuildError::NotFound(format!("There is no provider \"{provider}\".")))?;
    let t: &TaskTemplate = m
        .tasks
        .iter()
        .find(|t| t.id == task)
        .ok_or_else(|| BuildError::NotFound(format!("{} has no task \"{task}\".", m.name)))?;
    let ws = root.to_string_lossy();
    let argv: Vec<String> = t
        .command
        .iter()
        .map(|a| a.replace("${workspace}", &ws))
        .collect();
    if argv.iter().any(|a| a.contains("${")) {
        return Err(BuildError::InvalidTemplate(format!(
            "Task \"{task}\" uses a variable other than ${{workspace}}."
        )));
    }
    let vars: BTreeMap<String, String> = axiom_env::project::process_env(env, inherited_path);
    Ok(Prepared {
        spec: SpawnSpec {
            command: Command::Argv(argv),
            cwd: root.to_path_buf(),
            env: vars,
            label: Some(t.label.clone()),
        },
        diagnostics: t.diagnostics.clone(),
    })
}

/// What a running build emits, in order: output and diagnostics batches interleaved, then
/// the final batch (`complete: true`), then the exit.
#[derive(Debug, Clone, PartialEq)]
pub enum BuildEvent {
    Output(OutputParams),
    Diagnostics(BuildDiagnostics),
    Exited(ExitedParams),
}

/// Totals of a finished run (for the build history).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub errors: u32,
    pub warnings: u32,
}

impl Counts {
    pub fn add(&mut self, d: &[Diagnostic]) {
        for x in d {
            match x.severity {
                Severity::Error => self.errors += 1,
                Severity::Warning => self.warnings += 1,
                _ => {}
            }
        }
    }
}

/// Splits chunks into lines per stream; a line may span chunks.
#[derive(Default)]
struct Lines {
    out: String,
    err: String,
}

impl Lines {
    fn push(&mut self, stream: Stream, data: &str) -> Vec<String> {
        let buf = match stream {
            Stream::Stdout => &mut self.out,
            Stream::Stderr => &mut self.err,
        };
        buf.push_str(data);
        let mut lines = Vec::new();
        while let Some(i) = buf.find('\n') {
            lines.push(buf[..i].to_string());
            buf.drain(..=i);
        }
        // A pathological line without a newline is cut, not grown without bound.
        if buf.len() > 4 << 20 {
            lines.push(std::mem::take(buf));
        }
        lines
    }

    fn rest(&mut self) -> Vec<(Stream, String)> {
        [
            (Stream::Stdout, std::mem::take(&mut self.out)),
            (Stream::Stderr, std::mem::take(&mut self.err)),
        ]
        .into_iter()
        .filter(|(_, s)| !s.is_empty())
        .collect()
    }
}

/// Consumes a task's events, parses its output and returns the build's events. Diagnostics go
/// out at most every [`BATCH_INTERVAL`], each batch holding only new ones; the last batch
/// (`complete: true`) comes before `Exited`.
pub fn drive(
    task_id: TaskId,
    mut events: mpsc::Receiver<TaskEvent>,
    format: &DiagnosticsFormat,
    roots: Vec<PathBuf>,
) -> mpsc::Receiver<BuildEvent> {
    let (tx, rx) = mpsc::channel(256);
    let mut parser = Parser::new(format, roots);
    tokio::spawn(async move {
        let mut lines = Lines::default();
        let mut pending: Vec<Diagnostic> = Vec::new();
        let mut tick = tokio::time::interval(BATCH_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                ev = events.recv() => match ev {
                    Some(TaskEvent::Output(o)) => {
                        for l in lines.push(o.stream, &o.data) {
                            pending.extend(parser.line(o.stream, &l));
                        }
                        if tx.send(BuildEvent::Output(o)).await.is_err() {
                            // Receiver gone: keep draining so the task is not stalled.
                            while events.recv().await.is_some() {}
                            return;
                        }
                    }
                    Some(TaskEvent::Exited(x)) => {
                        for (s, l) in lines.rest() {
                            pending.extend(parser.line(s, &l));
                        }
                        pending.extend(parser.finish());
                        let _ = tx
                            .send(BuildEvent::Diagnostics(BuildDiagnostics {
                                task_id: task_id.clone(),
                                diagnostics: std::mem::take(&mut pending),
                                complete: true,
                            }))
                            .await;
                        let _ = tx.send(BuildEvent::Exited(x)).await;
                        return;
                    }
                    None => return,
                },
                _ = tick.tick(), if !pending.is_empty() => {
                    let batch = BuildDiagnostics {
                        task_id: task_id.clone(),
                        diagnostics: std::mem::take(&mut pending),
                        complete: false,
                    };
                    if tx.send(BuildEvent::Diagnostics(batch)).await.is_err() {
                        while events.recv().await.is_some() {}
                        return;
                    }
                }
            }
        }
    });
    rx
}
