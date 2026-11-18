---
id: M2-BUILD-001
title: crates/build: provider tasks and diagnostics parsers
status: tested
status_note: 
depends_on: [M2-ENV-005]
order: 307
scope: crates/build/ (new), crates/axiomd/ (build/tasks, build/run, build/diagnostics), Cargo.toml
---

# M2-BUILD-001: crates/build: provider tasks and diagnostics parsers

## Objective
`crates/build` turns a provider's task templates into processes run through `exec` inside the project's environment, parses their output into normalised `Diagnostic`s (`cargo_json`, `gcc_text`, `regex`, `sarif`), keeps a short build history, and `axiomd` serves `build/tasks`, `build/run` and the `build/diagnostics` notification.

## Context
Epic 2.6. `crates/protocol/src/diagnostics.rs`; `TaskTemplate`/`DiagnosticsFormat` (provider.rs); `docs/research/toolchains.md` §Cargo JSON, §Clang and GCC.

## Required interfaces
- `exec::Executor` (unchanged semantics); M2-ENV-005 `Environment`; `Diagnostic` types.

## Implementation requirements
1. `build/tasks(workspace)`: tasks from applying providers (id, kind, label, provider).
2. `build/run(workspace, providerId, taskId)`: resolves the environment, starts the argv through `exec` (no shell), returns the `TaskId`; output flows through the existing `task/output`/`task/exited`.
3. Parsers: `cargo_json` (compiler messages → Diagnostic with spans, codes, rendered text), `gcc_text`, `regex` (named groups), `sarif` (2.1.0 results); paths made workspace-relative or dropped if outside.
4. `build/diagnostics` notifications batched (≤ 10 Hz) and a final complete set on exit.
5. History: last 20 runs per workspace in the store (task, start, duration, outcome, counts).

## Constraints
- ADR-0014 (accepted) is the decision record; do not reopen its decisions in this task.
- Frozen contracts (IPC 0.1 meanings, editor bridge 1) unchanged; additive protocol changes only where this spec says so.
- `scripts/check.sh` green; no test deleted, weakened or skipped.
- Parsers are pure functions over bytes; no I/O.
- A diagnostic never points outside the workspace.

## Acceptance criteria
1. `cargo build` of a fixture with a type error yields one `error` diagnostic at the right file/line/column with code `E0308`.
2. gcc/clang text, a regex matcher and a SARIF file parse to the expected diagnostics (golden tests).
3. Notifications are batched and the final set equals the parser's full output.

## Tests
- `cargo test -p axiom-build` (golden: `cargo_json_e0308`, `gcc_text_multiline`, `regex_named_groups`, `sarif_minimal`, `paths_outside_workspace_dropped`).
- daemon integration test for `build/run`.
- `scripts/check.sh`.

## Failure conditions
- A parser panics on malformed input (fuzz a few hundred random inputs in a test).

## Documentation updates
`docs/protocols/ipc.md` (build as built), `docs/roadmap/current.md`.

## Handoff
Parser coverage and the fixture outputs used.
