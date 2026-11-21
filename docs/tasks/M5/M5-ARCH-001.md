---
id: M5-ARCH-001
title: Freeze the M5 contracts: protocol 0.5 (preview/*, ui/*), the inspection adapter protocol, module edges
status: planned
status_note:
depends_on: []
order: 601
scope: crates/protocol/, schemas/ (regenerated), apps/desktop-tauri/src/daemon/protocol.gen.ts (regenerated), crates/axiomd/src/router.rs (NOT_YET_SERVED), crates/desktop-bridge/ (version), docs/protocols/{ipc,inspection}.md, docs/architecture/{modules,governance,security}.md, scripts/check_deps.py
---

# M5-ARCH-001: Freeze the M5 contracts

## Objective
`crates/protocol/src/{preview,ui}.rs` define ADR-0019 §3's methods and notifications; the
inspection draft (`inspection.rs`, `inspection.md`) is reconciled with ADR-0019 (injected adapter,
`inspect/hovered` and `inspect/selected`, `screenshot: false` and `skipped` pixel checks) and
frozen; `PROTOCOL_VERSION` is `0.5`; schemas and the shell's types regenerated; `inspect` crate
edges in `modules.md` and `check_deps.py`; the new methods listed in `NOT_YET_SERVED`.

## Acceptance criteria
1. `m5_contract.rs` pins the shapes; earlier contract tests pass (0.4 pin relaxed to "0.4 or later").
2. Schemas and `protocol.gen.ts` fresh; the bridge speaks 0.5.
3. `inspection.md` says **frozen for M5**; security.md covers the adapter socket and the token.

## Tests
`cargo test -p axiom-protocol`; schema check.
