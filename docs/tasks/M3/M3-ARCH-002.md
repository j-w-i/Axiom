---
id: M3-ARCH-002
title: Chat additions to protocol 0.3: workspace-free sessions, rename/delete, edit and regenerate, images
status: tested
status_note: 2026-11-21; see handoff
depends_on: [M3-ARCH-001]
order: 404
scope: crates/protocol/, schemas/ (regenerated), apps/desktop-tauri/src/daemon/protocol.gen.ts (regenerated), crates/agent/src/model/ (image blocks), crates/axiomd/src/router.rs (NOT_YET_SERVED), docs/protocols/{ipc,agents}.md
---

# M3-ARCH-002: Chat additions to protocol 0.3

## Objective
ADR-0017 §2–3 and ADR-0016 §1 in the wire types:
- **Workspace optional:** `workspaceId` optional in `SessionCreateParams`, `SessionRef`,
  `SendParams`, `RestoreParams`, and `agent/sessionList` (new `SessionListParams`).
- **New methods:** `agent/sessionRename`, `agent/sessionDelete`.
- **`SendParams`:** `fromTurn` and `images`.
- **Events:** `rewind`.
- **ACP providers:** `ProviderKind::Acp`, `ProviderConfig.command`/`env`.
- **Image blocks:** `Block::Image` in the model layer, sent by both adapters.

## Context
ADR-0016, ADR-0017; `crates/protocol/src/agent.rs`; `tests/m3_contract.rs`.

## Implementation requirements
1. Every change is additive or required→optional. Existing `m3_contract.rs` cases still pass
   unchanged; new cases pin the new shapes, and old shapes (with `workspaceId`) still parse.
2. `agent/sessionList` params become `SessionListParams { workspaceId? }`, a superset of
   `WorkspaceRef`'s wire shape (the same JSON still parses).
3. `Block::Image { media_type, data_ref }`:
   - Anthropic: `{"type":"image","source":{"type":"base64",…}}`.
   - OpenAI: `{"type":"image_url","image_url":{"url":"data:…"}}` in a content array.
   - Golden tests for both.
4. The new methods join `NOT_YET_SERVED` until LOOP-001 serves them.

## Acceptance criteria
1. `cargo test -p axiom-protocol` passes; existing cases unchanged; new wire tests.
2. Adapter golden tests for images.
3. `scripts/check.sh` green.

## Documentation updates
`ipc.md` §Protocol 0.3, `agents.md`.

## Handoff
The exact additions.
