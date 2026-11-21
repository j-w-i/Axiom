---
id: M3-ARCH-001
title: Freeze the M3 contracts: protocol 0.3 agent/* types, agents.md, module edges
status: planned
status_note:
depends_on: []
order: 401
scope: crates/protocol/, schemas/ (regenerated), apps/desktop-tauri/src/protocol.gen.ts (regenerated), docs/protocols/{ipc,agents}.md, docs/architecture/{modules,governance}.md, scripts/check_deps.py
---

# M3-ARCH-001: Freeze the M3 contracts: protocol 0.3 agent/* types, agents.md, module edges

## Objective
`crates/protocol/src/agent.rs` defines every method, notification, value type and error kind of
ADR-0015 §1 as typed Rust; the registry lists them; `InitializeResult.protocolVersion` is `0.3`;
schemas and the shell's generated types are regenerated; `agents.md` is reconciled with ADR-0015
and frozen; `modules.md` and `check_deps.py` carry the new edges. No behaviour is implemented:
the daemon lists the new methods in a `NOT_YET_SERVED` set until each task wires them (the M2
pattern).

## Context
Epic 3.0. ADR-0015 (decision record), ADR-0007, `docs/protocols/agents.md` (draft),
`crates/protocol/src/{env,build}.rs` (style), `tests/m2_contract.rs`.

## Required interfaces
- `crates/protocol` rules: serde/serde_json/schemars only, no I/O.
- `tests/contract.rs`, `tests/m2_contract.rs`: unchanged expectations.

## Implementation requirements
1. Types: `SessionId`, `SessionInfo`, `ProviderConfig` (`kind: anthropic|openaiChat`, `id`,
   `name`, `baseUrl?`, `apiKeyEnv?`, `hasKey`, `models?`, capability overrides), `ModelInfo`,
   `Capabilities` (`yes|no|unknown` per capability), `AgentEvent` (seq, ts, `kind` =
   user | assistantText | assistantDelta | toolCall | toolResult | approval | checkpoint |
   compaction | usage | turnEnd | error), `ToolInfo` (name, description, inputSchema, effect),
   `Effect`, `ApprovalRequest` (requestId, sessionId?, tool, effect, targets, argv?, patch
   preview?, rule?, checkpointPlanned), `ApprovalDecision` (`once|session|deny`),
   `CheckpointInfo`, `RestoreResult` (restored, conflicts[{path, reason}]), `Policy`, `PolicyRule`.
2. Methods and notifications exactly as ADR-0015 §1 with params/results and registry entries.
3. Error kinds `approval_required`, `checkpoint_conflict`, `model_error` (next free codes).
4. `InitializeParams` gains optional `authToken` and `capabilities.approvals` (additive,
   `skip_serializing_if`), with a wire test that a 0.1 initialize still parses.
5. `agents.md`: reconciled with ADR-0015 (tool table final for M3, MCP section, sockets,
   checkpoints via git CLI); state **frozen for M3**; governance contract table updated.
6. `modules.md`: `agent` and `cli` sections and matrix rows (`agent → protocol, store`;
   `axiomd → agent`; `cli → protocol, rpc`); `check_deps.py` allows exactly these.

## Constraints
ADR-0015 decisions are not reopened here. Additive changes only.

## Acceptance criteria
1. Every ADR-0015 §1 method/notification is typed and in `registry::method_names`.
2. Existing contract tests pass unchanged; `tests/m3_contract.rs` pins each new wire shape.
3. `scripts/check.sh` green (schema freshness included).

## Tests
`cargo test -p axiom-protocol`; `scripts/check.sh`.

## Failure conditions
An existing contract expectation would need editing.

## Documentation updates
`ipc.md` §Protocol 0.3, `agents.md`, `governance.md`, `modules.md`, `current.md`.

## Handoff
The final type list and any deviation from ADR-0015 with the reason.
