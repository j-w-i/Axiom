---
id: M4-ARCH-001
title: Freeze the M4 contracts: protocol 0.4 (lsp, terminal, git, workspace/search), module edges
status: tested
status_note: 2026-11-22; see handoff
depends_on: []
order: 501
scope: crates/protocol/, schemas/ (regenerated), apps/desktop-tauri/src/daemon/protocol.gen.ts (regenerated), crates/axiomd/src/router.rs (NOT_YET_SERVED), crates/desktop-bridge/ (version), docs/protocols/ipc.md, docs/architecture/{modules,governance,security}.md, scripts/check_deps.py
---

# M4-ARCH-001: Freeze the M4 contracts

## Objective
`crates/protocol/src/{lsp,terminal,git}.rs` and `workspace.rs` define every method, notification,
value type and error kind of ADR-0018 §1; `InitializeResult.protocolVersion` is `0.4`; schemas and
the shell's generated types are regenerated; `ipc.md` gains §0.4; `modules.md` and
`check_deps.py` carry `lsp → protocol`, `term → protocol`, `axiomd → lsp, term`. The daemon lists
the new methods in `NOT_YET_SERVED` until each task wires them (the M2/M3 pattern).

## Context
Epic 4.0. ADR-0018 (decision record), ADR-0004 §Language intelligence, `crates/protocol/src/agent.rs`
(style), `tests/m3_contract.rs`.

## Implementation requirements
1. `lsp`: `ServerInfo` (id, languages, state, detail, installHint?, capabilities?: JSON),
   `ServerState`, `LspRequestParams {workspaceId, serverId, method, params}` → `{result}`,
   `LspNotifyParams`, `LspServerRef`, notifications `lsp/notification {workspaceId, serverId,
   method, params}` and `lsp/serverState {workspaceId, server: ServerInfo}`. LSP payloads stay
   `serde_json::Value` (the LSP spec is the contract for them).
2. `terminal`: `TerminalId`, `TerminalCreateParams {workspaceId, argv?, cols, rows}` →
   `TerminalInfo {terminalId, title, pid?, running}`; `write {terminalId, data}`, `resize`,
   `close`, `list`; notifications `terminal/output {terminalId, seq, data}` and
   `terminal/exited {terminalId, code?}`.
3. `git`: `GitStatus {repo: bool, reason?, branch?, upstream?, ahead, behind, files[{path,
   index, worktree, origPath?}]}`, `GitFileState`, `GitShowParams {workspaceId, path, rev:
   head|index}` → `{text?: string, binary: bool}`; notification `git/didChange {workspaceId}`.
4. `workspace/search {workspaceId, query, regex, caseSensitive, wholeWord, include?, exclude?,
   mode: text|files, limit?}` → `{matches[{path, line, column, endColumn, preview}], files?[],
   truncated}`.
5. Error kinds `lsp_error`, `server_unavailable`, `git_unavailable` at the next free codes.
6. Contract test `tests/m4_contract.rs` pins the shapes; 0.1–0.3 tests unchanged.
7. `ipc.md` §0.4, governance contract table, `security.md` (terminal ungated: the person's own
   shell; LSP out-of-workspace reads limited to paths a server returned).

## Acceptance criteria
1. Schemas regenerate clean (`export_schema --check`), `protocol.gen.ts` regenerated, contract tests
   pass.
2. The desktop bridge initializes with 0.4; older clients (0.1–0.3 initialize) still work.
3. `check_deps.py` accepts exactly the new edges.

## Tests
`cargo test -p axiom-protocol`; `cargo run -p axiom-protocol --example export_schema -- --check`.

## Failure conditions
A 0.1–0.3 shape changes; a new edge beyond ADR-0018 §7.

## Documentation updates
`ipc.md`, `modules.md`, `governance.md`, `security.md`.

## Handoff
The 0.4 method table.
