---
id: M1-SHELL-004
title: Run commands and show streamed output
status: planned
status_note:
depends_on: [M1-SHELL-003]
order: 43
scope: apps/desktop/
---

# M1-SHELL-004: Output panel

## Objective
The user can run a command in the workspace and watch its output, errors and
exit status; stop it; and rerun previous commands.

## Context
Read: `docs/architecture/data-flow.md` §Run a command,
`crates/protocol/src/task.rs`, `docs/protocols/ipc.md` §Tasks.

## Implementation requirements
1. `src/models/TaskController` (exposed as `tasks`): `running: bool`,
   `currentLabel`, `lastStatus` (`{outcome, exitCode, durationMs}` or null);
   invokables `run(commandLine)`, `stop()`, `clear()`; `history` (string
   list, most recent first, max 20, de-duplicated; persisted in workspace
   state key `run.history`).
   `run` sends `task/start` with `command: { shell: commandLine }`. One task
   at a time in M1: Run is disabled while `running`.
2. `src/models/TaskOutputModel` (list model of lines): consumes
   `task/output` in `seq` order (buffer out-of-order chunks; they should not
   occur), splits into lines, keeps a partial trailing line per stream,
   roles `text`, `stream`. Strips ANSI escape sequences (CSI) in M1 (colour
   rendering is not required). Caps at 10 000 lines (drops the oldest and
   shows a first row "… earlier output truncated").
   On `task/exited`: appends a status row, e.g. `✔ Succeeded in 1.2 s`,
   `✖ Failed with exit code 3 in 0.4 s`, `■ Cancelled`.
   `spawn_failed` and other `task/start` errors are shown as a `danger` row
   with the error message.
3. QML `OutputPanel.qml`: header with an editable `AxComboBox` (history) —
   Enter runs —, **Run** (Ctrl+Enter when the field has focus; F5 anywhere),
   **Stop** (enabled only while running), **Clear**. Body: `ListView` with
   the mono font, stderr rows in `danger` colour plus a gutter marker,
   auto-scroll to the end unless the user scrolled up; text is selectable
   and copyable (Ctrl+C copies the selection; a "Copy all" context action).
4. Status bar: task state (`Running: <label>`, then the final status).
5. Closing the workspace or quitting while a task runs: the daemon cancels;
   the shell need not ask.
6. Run menu: **Run Command** (focuses the field), **Rerun Last** (F5),
   **Stop** (Shift+F5).

## Constraints
No terminal emulation, no stdin. The command line is passed verbatim; the
shell does not parse or quote it.

## Acceptance criteria
Tests (Qt Test, real daemon, fixture workspace; unix commands, with
`QSKIP` on Windows until Windows equivalents are added by M1-CI-002):
- `echo out; echo err >&2; exit 3` → two rows with the right streams, status
  row "Failed with exit code 3", `lastStatus.exitCode == 3`.
- a command that does not exist → the shell (`sh`) reports it on stderr and
  exit code 127 is shown; `task/start` with an **argv** of a missing program
  (exercise through a test-only invokable) → `danger` row from
  `spawn_failed`.
- `sleep 30` then `stop()` → "Cancelled" within 5 s; `running` false.
- 50 000 lines of output → model capped, truncation row present, UI model
  updates complete without the event loop stalling > 200 ms (measure the
  longest gap between timer ticks during the run and report it).
- ANSI sequences stripped (`printf '\033[31mred\033[0m'` → `red`).
- history persisted across workspace reopen.

## Tests
`scripts/check.sh`.

## Failure conditions
The event loop stalls on large outputs → batch model updates (append in
chunks on a 16 ms timer) rather than raising the threshold.

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
The measured longest event-loop gap.
