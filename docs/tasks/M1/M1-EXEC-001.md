---
id: M1-EXEC-001
title: Implement axiom-exec (process supervision with streamed output)
status: planned
status_note:
depends_on: []
order: 13
scope: crates/exec/, Cargo.toml (workspace members and dependencies only)
---

# M1-EXEC-001: Implement the `axiom-exec` crate

## Objective
Spawn processes, stream their output as ordered chunks, cancel whole process
trees, and keep a bounded history — the engine behind `task/*`.

## Context
Read: `crates/protocol/src/task.rs`, `docs/protocols/ipc.md` §Tasks,
`docs/architecture/modules.md` §`crates/exec`,
`docs/decisions/ADR-0010-isolation.md` (for the `ExecBackend` seam).

## Implementation requirements
```rust
pub struct SpawnSpec {
    pub command: axiom_protocol::task::Command,
    pub cwd: PathBuf,                       // already resolved by the caller
    pub env: BTreeMap<String, String>,      // added to the inherited environment
    pub label: Option<String>,
}
pub enum TaskEvent { Output(OutputParams), Exited(ExitedParams) }

pub struct Executor { /* cheap to clone */ }
impl Executor {
    pub fn new() -> Self;
    /// Spawns and returns immediately. The receiver yields Output* then exactly one Exited.
    pub async fn start(&self, workspace: WorkspaceId, spec: SpawnSpec)
        -> Result<(TaskId, tokio::sync::mpsc::Receiver<TaskEvent>), ExecError>;
    pub async fn cancel(&self, id: &TaskId) -> Result<(), ExecError>;      // NotFound for unknown ids
    pub fn list(&self, workspace: &WorkspaceId) -> Vec<TaskInfo>;          // running + last 50 finished, newest first
    /// Cancels every running task of a workspace and waits for them to exit.
    pub async fn cancel_all(&self, workspace: &WorkspaceId);
    pub async fn shutdown(&self);                                          // cancel_all for every workspace
}
pub enum ExecError { SpawnFailed { program: String, os_error: String }, EmptyCommand, CwdNotFound, NotFound }
// impl Into<RpcError>: SpawnFailed→spawn_failed (details {program, osError}); EmptyCommand→invalid_params;
// CwdNotFound→not_found; NotFound→not_found.
```

1. Task ids: `t1`, `t2`, … per `Executor`.
2. `Command::Argv`: spawn directly; empty → `EmptyCommand`.
   `Command::Shell(s)`: Unix `sh -c s`; Windows `cmd /C s`.
3. stdin = null; stdout and stderr piped and read concurrently in chunks of
   up to 8 KiB. `seq` starts at 0 and increases by 1 across both streams in
   the order chunks were read. Decode UTF-8 incrementally: carry an
   incomplete trailing sequence into the next chunk of the same stream;
   replace invalid bytes with U+FFFD.
4. The event channel has capacity 256; when it is full the reader awaits
   (backpressure reaches the child through the pipe).
5. `Exited` is sent after both pipes reached EOF **and** the process was
   reaped. `outcome`: `cancelled` if cancellation was requested for this
   task before exit; else `succeeded` for exit code 0; else `failed`.
   `signal` set on Unix when killed by a signal. `duration_ms` measured from
   spawn.
6. Process tree control behind
   `trait ProcessTree: Send { fn terminate(&mut self); fn kill(&mut self); }`:
   - Unix: spawn with `process_group(0)`; `terminate` = `killpg(SIGTERM)`,
     `kill` = `killpg(SIGKILL)` (use the `nix` crate).
   - Windows: create a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
     assign the child; `terminate` and `kill` both `TerminateJobObject`
     (use `windows-sys`). This file may use `unsafe`: put it in
     `src/tree_windows.rs` with `#![allow(unsafe_code)]` scoped to that
     module and a `// SAFETY:` comment per block.
7. `cancel`: mark the task cancelled, `terminate`, and after 3 s `kill` if it
   has not exited. Cancelling a finished task is `Ok`.
8. Dropping the `Executor` (last clone) kills all running tasks
   (`kill_on_drop` plus tree kill).
9. Define the isolation seam now, with one implementation:
   `trait ExecBackend { fn prepare(&self, spec: &SpawnSpec) -> tokio::process::Command; }`
   and `HostBackend`. `Executor::new()` uses `HostBackend`. Do not add other
   backends.

## Constraints
- Dependencies: `axiom-protocol`, `tokio` (process, io-util, sync, time, rt),
  `thiserror`, `tracing`; `nix` (feature `signal`) under `cfg(unix)`;
  `windows-sys` under `cfg(windows)`.
- No knowledge of workspaces' paths or of RPC.

## Acceptance criteria
Tests (`crates/exec/tests/`; Unix commands under `#[cfg(unix)]`, with
Windows equivalents using `cmd /C` under `#[cfg(windows)]`):
- stdout and stderr are both captured and tagged; exit code 3 → `failed`,
  `exit_code: Some(3)`.
- `seq` is contiguous from 0; `Exited` is last.
- missing program → `SpawnFailed` naming the program.
- `cwd` and `env` are applied (`pwd`, `echo $VAR`).
- a 1 MiB output is received completely and in order.
- a multi-byte UTF-8 character split across the 8 KiB boundary arrives intact.
- cancel a `sleep 60` → `cancelled` within 1 s.
- cancel kills grandchildren: `sh -c 'sleep 60 & echo $!; wait'` → after
  cancel, the printed pid no longer exists.
- a process that ignores SIGTERM (`trap '' TERM; sleep 60`) is killed after
  the 3 s grace → `cancelled`, `signal: Some(9)`.
- `list` shows running then finished tasks; history is capped at 50.

## Tests
`scripts/check.sh rust`. `cargo test -p axiom-exec` ≥ 12 tests.

## Failure conditions
Cannot verify the Windows implementation locally: that is expected on
Linux/macOS — it must still compile under `cargo check --target
x86_64-pc-windows-msvc` if that target is installed, and the report must
say "Windows implementation untested locally; CI is the verifier".

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
State explicitly which OSes the tests ran on.
