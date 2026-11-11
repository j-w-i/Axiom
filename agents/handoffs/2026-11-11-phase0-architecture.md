# Handoff — 2026-11-11 — Phase 0: architecture and planning

Session: principal-architect session (planning model). Branch: `main`.

## Result
Phase 0 is complete. The repository contains the research, architecture,
decisions, contracts, validated experiments, roadmap, M1 task specifications
and agent infrastructure. No product code exists beyond the protocol crate.
M1 is ready to start: five tasks are Ready in parallel.

## What exists
- **Research** (`docs/research/`, 7 files, sourced): qt-rust, editor,
  toolchains, xtool, visual-inspection, agents, prior-art. Written by
  research sub-agents with web access; each lists what it could not verify.
- **Architecture** (`docs/architecture/`): overview, processes, modules,
  data-flow, security, design-system, governance, open-questions.
- **ADRs 0001–0011** (`docs/decisions/`).
- **Contracts**:
  - `crates/protocol` — IPC 0.1 (frozen): envelope, error model, 16 methods,
    6 notifications; draft modules `diagnostics`, `provider`, `inspection`.
  - `schemas/` — generated from the crate (ipc, provider manifest, inspection).
  - `docs/protocols/` — `ipc.md`, `editor-bridge.md` (frozen);
    `providers.md`, `agents.md`, `inspection.md` (draft).
  - `providers/rust/provider.toml` — reference manifest, parsed by a test.
- **Experiments** (`experiments/`, results in its README): E1 Qt+WebEngine+
  Monaco+Rust daemon; E2 QML probe with source locations; E3 React
  element→source→patch→verified.
- **Roadmap**: `milestones.md` (M1–M7), `dependency-graph.md`, `current.md`.
- **Tasks**: 15 M1 task files in `docs/tasks/M1/`, template, board script.
- **Agent infrastructure**: `AGENTS.md`, `agents/IMPLEMENTATION_GUIDE.md`,
  `REVIEW_GUIDE.md`, `HANDOFF_TEMPLATE.md`, `START_NEXT_SESSION.md`.
- **Tooling**: `scripts/check.sh`, `tasks.py`, `dev-env.sh`,
  `bootstrap-qt.sh`; `.github/workflows/ci.yml` (Rust job).

## Commands run and actual results (this machine: Fedora 44, x86_64)
```
scripts/check.sh
  cargo fmt --check                      ok
  cargo clippy -D warnings               ok
  cargo test --workspace                 contract.rs: 9 passed; drafts.rs: 6 passed
  export_schema --check                  ok
scripts/tasks.py check                   15 tasks, 0 problems
E1 self-test (Qt 6.11.2 offscreen, Qt 6.11.2 Wayland, Qt 6.12.0 offscreen)   allPassed: true, exit 0 (12 checks)
E2 probe (Qt 6.11.2, 6.12.0)             4/4 source locations exact
E3 driver (Chrome 155 headless, React 19.3, Vite 8.3)   verified: true, exit 0
```

## Not verified
- Anything on macOS or Windows (no such machine; CI has never run — there
  is no git remote).
- CDP against Qt WebEngine (E3 used Chrome). Scheduled: M5 epic 5.0.
- IME, clipboard, drag-and-drop, accessibility in the embedded editor.
- The task specs themselves have not been executed by an implementation
  agent yet; expect small corrections in the first tasks.

## Environment changes made on this machine
- Installed Rust stable via rustup into `~/.cargo`, `~/.rustup` (PATH not
  modified).
- Installed Qt 6.11.2 and 6.12.0 (aqtinstall) and extracted GL/xkbcommon
  development headers into `.toolchains/` (git-ignored, ≈ 4 GB). Safe to
  delete; `scripts/bootstrap-qt.sh [--gl-headers]` recreates it.
- No system packages installed; no sudo used.

## Deviations and decisions worth knowing
- ADR-0003 chooses newline-delimited JSON although the research doc
  recommended LSP-style length framing; reasons are in the ADR.
- Minimum Qt is 6.8, development target 6.12 LTS; experiments ran on both
  6.11.2 and 6.12.0.
- Dependencies at `tested` unblock dependants (single-agent flow);
  `verified` still requires an independent review.

## Known limitations of the plan
- Only M1 has task-level specs. M2+ are epics with exit criteria; each
  milestone starts with a contract-freeze + task-writing task.
- The Windows process-tree code in M1-EXEC-001 cannot be tested on this
  machine.

## Next steps
`scripts/tasks.py next` → **M1-RPC-001**. Wave A (RPC-001, STORE-001,
WS-001, EXEC-001, EDITOR-001) may run in parallel. Start a new session with
`agents/START_NEXT_SESSION.md`.
