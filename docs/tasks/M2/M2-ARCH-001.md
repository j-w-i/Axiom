---
id: M2-ARCH-001
title: Freeze the M2 contracts: env/* and build/* protocol additions
status: planned
status_note:
depends_on: []
order: 301
scope: crates/protocol/, schemas/ (regenerated), docs/protocols/{ipc,providers}.md, docs/architecture/modules.md
---

# M2-ARCH-001: Freeze the M2 contracts: env/* and build/* protocol additions

## Objective
The protocol defines every M2 method, notification, value type and error kind from ADR-0014 §1 as typed Rust in `crates/protocol`, the schemas are regenerated, `ipc.md` documents them (protocol 0.2, additive), and `modules.md` records the new crates' dependency edges. No behaviour is implemented.

## Context
Epic 2.0. ADR-0014; `docs/protocols/providers.md` §Core data and §Errors; `crates/protocol/src/{provider,diagnostics}.rs` (drafts); `ipc.md` §Versioning.

## Required interfaces
- `crates/protocol` rules (`modules.md`): serde/serde_json/schemars only, no I/O.
- `tests/contract.rs`: existing expectations must not change.
- `cargo run -p axiom-protocol --example export_schema` regenerates `schemas/`.

## Implementation requirements
1. New modules `env.rs` and `build.rs` in `crates/protocol`: `Toolchain`, `ToolLocation` (`Managed{root}` | `System{path}`), `Environment`, `InstallPlan` (with `planId`, `providerId`, `version`, `scope`, `steps`, `downloadBytes`, `effects`), `Step` (`Download{url, bytes?, sha256Source}` | `Extract{format, stripComponents}` | `Run{argv, env}` | `SystemPackages{manager, packages}` | `Verify{toolId}`), `ProviderInfo` (id, name, source: builtin/user/workspace, trusted, errors), `DetectResult`, `TaskInfo` for provider tasks, `InstallProgress`, `BuildDiagnostics` (task id, diagnostics, final flag).
2. Methods and notifications exactly as ADR-0014 §1, each with params/result types and a registry entry; `InitializeResult.protocolVersion` = `"0.2"`.
3. Error kinds `checksum_mismatch`, `unsupported_platform`, `approval_denied`, `prerequisite_missing` (details `{missing: [toolId]}`).
4. Review the provider and diagnostics drafts against ADR-0014 and amend them (any change recorded in the handoff); mark both **frozen for M2** in `governance.md`'s contract table.
5. `ipc.md`: a 0.2 section listing the additions; versioning note (0.1 clients keep working).
6. `modules.md`: `env` and `build` rows, edges `env → protocol, store`; `build → protocol, exec, env`; `axiomd → env, build`; `scripts/check_deps.py` allows exactly these.

## Constraints
- ADR-0014 (accepted) is the decision record; do not reopen its decisions in this task.
- Frozen contracts (IPC 0.1 meanings, editor bridge 1) unchanged; additive protocol changes only where this spec says so.
- `scripts/check.sh` green; no test deleted, weakened or skipped.
- Types only: no crate other than `protocol` changes behaviour.

## Acceptance criteria
1. Every ADR-0014 §1 method and notification exists with typed params/results and appears in `registry::method_names`.
2. `tests/contract.rs` passes unchanged; new wire-shape tests pin each new type.
3. `schemas/` regenerated (check.sh freshness step green).
4. `ipc.md`, `providers.md`, `governance.md`, `modules.md` updated as above.

## Tests
- `cargo test -p axiom-protocol` (new: `env_wire_shapes`, `build_wire_shapes`, `install_plan_roundtrip`).
- `scripts/check.sh`.

## Failure conditions
- Any existing contract expectation would need editing.
- A decision in ADR-0014 turns out to be unimplementable as typed: stop, write a CR.

## Documentation updates
`docs/protocols/ipc.md`, `docs/protocols/providers.md` (state: frozen), `docs/architecture/governance.md`, `docs/architecture/modules.md`, `docs/roadmap/current.md`.

## Handoff
The final type list with any draft amendments and why.
