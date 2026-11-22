# AGENTS.md

Entry point for any agent or contributor starting cold. Read this file
completely, then follow the links for the task at hand. Do not rely on any
conversation history: the repository is the source of truth.

## What this is

**Axiom** — an open-source, cross-platform, LLM-native development platform:
a Flutter desktop shell (Dart, widgets layer only) over a Rust daemon that manages
workspaces, toolchains, builds, agents and (its defining feature) inspection
of running UIs with element→source mapping and verified AI edits.
Requirements: `docs/product/requirements.md`.

## Current status

**M1–M5 complete** (M5, visual intelligence for React + Vite, on branch `task/m5`). Always check
`docs/roadmap/current.md` and `scripts/tasks.py` — they are kept current; this
paragraph may lag. The Tauri shell became the default on 2026-11-17 (T-14) and the Qt shell was retired the same day (T-15; recover it from tag `qt-shell-final`).
ADR-0020 then moved the shell to Flutter: FL-QA-001 made `apps/desktop-flutter` the app and
removed the Tauri shell and `editor/` (Monaco) on 2026-11-22 (recover them from tag
`tauri-shell-final`). Linux and macOS are built and tested; a Windows desktop build is owed
(deferred by the owner).

## Architecture in ten lines

- Two processes we own: `axiom` (Flutter shell, Dart, `apps/desktop-flutter`)
  and `axiomd` (Rust daemon). No UI toolkit in the daemon, no business logic
  in the shell (ADR-0002, ADR-0020).
- They speak JSON-RPC 2.0, one JSON document per line, over the daemon's
  stdio (ADR-0003). Contract: `crates/protocol` + `docs/protocols/ipc.md`.
- `crates/protocol` is the single source of truth for wire types; `schemas/`
  is generated from it (ADR-0005); the shell's Dart types are generated from
  `schemas/` (`apps/desktop-flutter/tool/gen_types.dart` → `lib/daemon/protocol.g.dart`).
- The editor is `re_editor` + `re_highlight` in the shell's own process; the
  terminal is `xterm` (Dart). There is no web view and no editor bridge
  (editor-bridge-1 retired with the Tauri shell; ADR-0020 §3).
- All platform logic lives in daemon service crates. The shell presents; it
  never touches workspace files or runs processes itself (besides spawning
  `axiomd`).
- Later: declarative toolchain providers (ADR-0006), in-house agent loop over
  a gated tool registry (ADR-0007), inspection adapters with build-time
  source stamping and objective verification (ADR-0008).

Start with `docs/architecture/overview.md`; details in `modules.md`,
`processes.md`, `data-flow.md`, `security.md`, `testing.md`.

## Repository map

| Path | What |
|------|------|
| `crates/protocol/` | Wire contract (exists) |
| `crates/{rpc,store,workspace,exec,axiomd}/` | M1 backend (complete) |
| `crates/{env,build,agent,lsp,term}/`, `apps/cli/` | M2–M4 services and the `axiom` CLI |
| `apps/desktop-flutter/` | The desktop shell (ADR-0020): `lib/` (daemon client, design system `lib/ui/`, stores `lib/state/`, one folder per surface), `test/`, end-to-end suites `integration_test/`, `scripts/{e2e,frames}.sh` |
| `crates/desktop-bridge/` | `axiom-desktop-bridge`: protocol 0.1 client that spawns/supervises `axiomd`; no shell links it since the Tauri shell's removal, its tests still pin the daemon lifecycle from a client's side |
| `providers/` | Toolchain provider manifests |
| `schemas/` | **Generated** JSON Schemas — never edit by hand |
| `experiments/` | Phase 0 proofs of concept and their results; reference only |
| `docs/architecture/` | How the system is built and the rules (`governance.md`) |
| `docs/decisions/` | ADRs; `proposed/` for change requests |
| `docs/protocols/` | Behavioural specs of each contract |
| `docs/research/` | Sourced technology research behind the decisions |
| `docs/design/` | M1.6 visual design (approved, ADR-0013): concepts, design system, motion, surfaces; `flutter/` the Flutter design system (ADR-0020 §4) |
| `docs/roadmap/` | `current.md`, `milestones.md`, `dependency-graph.md` |
| `docs/tasks/` | One file per task; status lives in its front matter |
| `agents/` | Guides, templates, handoffs, next-session prompt |
| `scripts/` | `check.sh` (the gate), `tasks.py` (the board), `run.sh` (launch the app), `headless.sh` (private Xvfb), `sysroot.sh` + `env.sh` (GTK 3 without root), `bundle.sh` + `verify-package.sh` (packages) |

## Commands

```sh
scripts/tasks.py                  # task board
scripts/tasks.py next             # what to work on
scripts/check.sh                  # THE quality gate: fmt, clippy, tests, schema freshness, Flutter shell
scripts/run.sh                    # launch the desktop app (release build of the shell, debug axiomd)
scripts/check.sh rust             # Rust only
scripts/check.sh flutter          # the shell only: wire types, format, analyze, tests, licences
scripts/check.sh e2e              # the shell's end-to-end suites, in a private compositor
scripts/check.sh package          # build the deb/rpm (macOS: Axiom.app) and verify them installed
cargo run -p axiom-protocol --example export_schema   # regenerate schemas/ after protocol changes
```
Setup from scratch: `docs/contributing/dev-setup.md`.

## How to work

1. `agents/IMPLEMENTATION_GUIDE.md` — the procedure for implementing a task.
2. `agents/REVIEW_GUIDE.md` — the procedure for verifying someone else's task.
3. `docs/architecture/governance.md` — what you may not change, and how to
   raise a change request when the plan is wrong.

## Hard rules (the short list)

- Frozen contracts (IPC 0.1–0.4, provider manifest, agent tools) are not
  modified by implementation tasks. Wrong contract → change request, not a
  quiet edit.
- Stay inside your task's `scope`. No drive-by refactors.
- No new frameworks or module dependency edges beyond `modules.md`.
- Never delete, skip or weaken a test to get green.
- No placeholder UI, no empty scaffolding, no fake data.
- Never claim something works without having run it. Report what was
  verified, on which OS, and what was not.
- No GPL code copied in; keep the dependency licences ADR-0011/ADR-0020 allow
  (`cargo deny`; Dart packages MIT, BSD or Apache-2.0, `scripts/check_dart_licences.py`).
- Leave `scripts/check.sh` passing.

## Coding standards

- **Rust:** edition 2021, `rustfmt` (max width 100), `clippy -D warnings`,
  `unsafe` forbidden except where a task explicitly allows a scoped module.
  Errors: `thiserror` enums in libraries, mapped to `RpcError` at the
  boundary; no `unwrap`/`expect` outside tests and provably-infallible cases
  with a comment. Async: Tokio; blocking work in `spawn_blocking`.
- **Dart/Flutter (the shell):** `dart format`, `flutter analyze --fatal-infos`.
  Widgets layer only: no Material or Cupertino import (`test/no_material_test.dart`).
  No literal colours outside `apps/desktop-flutter/lib/ui/tokens.dart`
  (`test/ui/tokens_test.dart`, which also checks contrast); every interactive
  element has an accessible name and role. `lib/daemon/protocol.g.dart` is
  generated — never edit it by hand.
- **Tests** live next to what they test; integration tests use real
  processes and temp directories, never the user's profile
  (`AXIOM_DATA_DIR`).
- **Commits:** `<TASK-ID>: <imperative summary>`; one task per branch.

## Common pitfalls (learned in Phase 0)

- `cargo` may not be on `PATH`: use the scripts (they add `~/.cargo/bin`) or add it yourself.
- Flutter is looked for on `PATH`, then in `~/flutter` (dev-setup.md §Flutter).
- Building the shell on Linux needs clang, CMake, Ninja, pkg-config and the
  GTK 3 development packages; without root (Fedora), `scripts/sysroot.sh` extracts
  them (and Xvfb) into `.toolchains/gtk-sysroot`; `source scripts/env.sh`
  before building.
- **Never open a window on a developer's desktop from a test or benchmark.**
  Linux GUI runs go through `apps/desktop-flutter/scripts/e2e.sh` (a private
  `kwin_wayland --virtual`, else `scripts/headless.sh`: a private Xvfb,
  Wayland removed, fails closed) or `apps/desktop-flutter/scripts/frames.sh`
  (a private virtual KWin). Only `scripts/run.sh` opens a real window.
- Tests must fail closed on timeouts (the end-to-end suites' waits are bounded).
- The daemon's stdout is the protocol channel: never print to it.
