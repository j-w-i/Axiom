---
id: M3-TOOLS-001
title: Tool registry and the M3 tools (workspace, fs, patch, task, build, env, editor)
status: tested
status_note: 2026-11-21; see handoff
depends_on: [M3-ARCH-001]
order: 405
scope: crates/agent/, crates/axiomd/, crates/workspace/ (search)
---

# M3-TOOLS-001: Tool registry and the M3 tools

## Objective
`agent::tools` defines `ToolSpec`, `Effect`, `ToolRegistry`, bounded results (`max_result_bytes`,
`truncated`, continuation cursor) and structured errors (`kind`, message, `hint`); `axiomd`
registers the M3 tools of `agents.md` as thin adapters over the services, and serves
`agent/tools`. `workspace` gains `search`.

## Context
Epic 3.2. `agents.md` §Tools; ADR-0015 §2.

## Implementation requirements
1. Tools: `workspace.info`, `workspace.search` (regex or literal, glob filter, paged by cursor),
   `fs.list`, `fs.read` (line range, paged, line numbers), `fs.applyPatch` (search/replace
   blocks; each search must match exactly once; optional create/delete file; fails with a hint
   showing the closest region when no match), `task.run` (argv; waits up to a timeout; exit code,
   last 200 lines, diagnostics if a provider matcher applies), `build.run` (provider task, waits,
   returns status + diagnostics), `build.diagnostics` (last run's), `env.detect`,
   `env.planInstall`, `env.install` (approved plan id only), `editor.open` (emits a notification
   for attached clients; no file access).
2. Every path goes through `Workspace::resolve`; writes go through `write_text` with versions.
3. Input validated against the JSON schema before anything else (`invalid_params` + hint).
4. A tool never runs without the gate (M3-GATE-001 wires it; until then the registry's `call`
   requires a `Decision::Allow` token so it cannot be bypassed by construction).

## Acceptance criteria
1. Each tool has a test through the registry against a temp workspace (no model).
2. `fs.applyPatch`: zero matches, two matches, stale version, create, delete each behave as
   specified; results include the new version and a unified diff.
3. Results over the bound are truncated with a working cursor.

## Tests
`cargo test -p axiom-agent tools_`; `cargo test -p axiomd tools`; `cargo test -p axiom-workspace search`.

## Failure conditions
A tool can reach a path outside the workspace or run without a gate decision.

## Documentation updates
`agents.md` tool table (final schemas summary).

## Handoff
The tool list with effects and bounds.
