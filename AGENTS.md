# AGENTS.md

Entry point for any agent or contributor starting cold. Read this file
completely, then follow the links for the task at hand. Do not rely on any
conversation history: the repository is the source of truth.

## What this is

**Axiom** — an open-source, cross-platform, LLM-native development platform:
a Tauri 2 desktop shell (React + Monaco) over a Rust daemon that manages
workspaces, toolchains, builds, agents and (its defining feature) inspection
of running UIs with element→source mapping and verified AI edits.
Requirements: `docs/product/requirements.md`.

## Current status

**M1 complete; M1.5 (Qt → Tauri migration) in progress.** Always check
`docs/roadmap/current.md` and `scripts/tasks.py` — they are kept current; this
paragraph may lag. The Qt shell is still the release artifact until task T-14.

## Architecture in ten lines

- Two processes we own: `axiom` (Tauri 2 shell: Rust host + React webview)
  and `axiomd` (Rust daemon). No Qt in the daemon, no business logic in the
  shell (ADR-0002, ADR-0012).
- They speak JSON-RPC 2.0, one JSON document per line, over the daemon's
  stdio (ADR-0003). Contract: `crates/protocol` + `docs/protocols/ipc.md`.
- `crates/protocol` is the single source of truth for wire types; `schemas/`
  is generated from it (ADR-0005).
- The editor is Monaco in the shell's webview, behind the frozen
  editor-bridge-1 message set (`docs/protocols/editor-bridge.md`, ADR-0004).
  `editor/src/documents.ts` is shared by both shells.
- All platform logic lives in daemon service crates. The shell presents; it
  never touches workspace files or runs processes itself.
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
| `apps/desktop-tauri/` | Tauri shell (M1.5): Rust host (`src-tauri/`), React UI (`src/`), bridge tests, E2E, benchmarks |
| `crates/desktop-bridge/` | `axiom-desktop-bridge`: spawns/supervises `axiomd`, protocol 0.1 client for the GUI |
| `apps/desktop/` | Qt shell (M1) — kept building and tested until task T-15 |
| `editor/` | Monaco editor surface (M1) |
| `providers/` | Toolchain provider manifests |
| `schemas/` | **Generated** JSON Schemas — never edit by hand |
| `experiments/` | Phase 0 proofs of concept and their results; reference only |
| `docs/architecture/` | How the system is built and the rules (`governance.md`) |
| `docs/decisions/` | ADRs; `proposed/` for change requests |
| `docs/protocols/` | Behavioural specs of each contract |
| `docs/research/` | Sourced technology research behind the decisions |
| `docs/roadmap/` | `current.md`, `milestones.md`, `dependency-graph.md` |
| `docs/tasks/` | One file per task; status lives in its front matter |
| `agents/` | Guides, templates, handoffs, next-session prompt |
| `scripts/` | `check.sh` (the gate), `tasks.py` (the board), `dev-env.sh`, `bootstrap-qt.sh` |

## Commands

```sh
scripts/tasks.py                  # task board
scripts/tasks.py next             # what to work on
scripts/check.sh                  # THE quality gate: fmt, clippy, tests, schema freshness (+ editor, desktop, tauri when present)
scripts/check.sh rust             # Rust only
cargo run -p axiom-protocol --example export_schema   # regenerate schemas/ after protocol changes
source scripts/dev-env.sh         # Qt + cargo on PATH; defines axiom_cmake
```
Setup from scratch: `docs/contributing/dev-setup.md`.

## How to work

1. `agents/IMPLEMENTATION_GUIDE.md` — the procedure for implementing a task.
2. `agents/REVIEW_GUIDE.md` — the procedure for verifying someone else's task.
3. `docs/architecture/governance.md` — what you may not change, and how to
   raise a change request when the plan is wrong.

## Hard rules (the short list)

- Frozen contracts (IPC 0.1, editor bridge 1) are not modified by
  implementation tasks. Wrong contract → change request, not a quiet edit.
- Stay inside your task's `scope`. No drive-by refactors.
- No new frameworks or module dependency edges beyond `modules.md`.
- Never delete, skip or weaken a test to get green.
- No placeholder UI, no empty scaffolding, no fake data.
- Never claim something works without having run it. Report what was
  verified, on which OS, and what was not.
- Only LGPL-available Qt modules (ADR-0011). No GPL code copied in.
- Leave `scripts/check.sh` passing.

## Coding standards

- **Rust:** edition 2021, `rustfmt` (max width 100), `clippy -D warnings`,
  `unsafe` forbidden except where a task explicitly allows a scoped module.
  Errors: `thiserror` enums in libraries, mapped to `RpcError` at the
  boundary; no `unwrap`/`expect` outside tests and provably-infallible cases
  with a comment. Async: Tokio; blocking work in `spawn_blocking`.
- **C++ (Qt shell only):** C++17, Qt naming conventions, `QObject` ownership
  by parent, no raw `new` without a parent or smart pointer, no blocking calls
  on the GUI thread, warnings clean.
- **QML (Qt shell only):** views use `Axiom.Ui` tokens/controls only; no
  literal colours or font sizes in `Axiom.App`; every interactive item has
  `Accessible.name`.
- **CSS/React (Tauri shell):** views and components use design tokens only —
  no literal colours outside `src/styles/tokens.css` (a guard test enforces
  it); every interactive element has an accessible name and role.
- **TypeScript:** `strict`, ES modules, no `any` in exported APIs.
- **Tests** live next to what they test; integration tests use real
  processes and temp directories, never the user's profile
  (`AXIOM_DATA_DIR`).
- **Commits:** `<TASK-ID>: <imperative summary>`; one task per branch.

## Common pitfalls (learned in Phase 0)

- `cargo` may not be on `PATH`: use the scripts or `source scripts/dev-env.sh`.
- Qt from aqtinstall needs OpenGL/xkbcommon development headers to configure
  (`find_package(Qt6Gui)` fails otherwise) — see `dev-setup.md`.
- Building the Tauri shell on Linux needs the WebKitGTK 4.1 / GTK 3
  development packages; without root, `apps/desktop-tauri/scripts/sysroot.sh`
  extracts them locally.
- **Never open a window on a developer's desktop from a test or benchmark.**
  Linux GUI runs go through `apps/desktop-tauri/scripts/headless.sh` (private
  Xvfb, Wayland removed); the E2E and benchmark scripts refuse to run without
  it.
- WebEngine: register the custom URL scheme and call
  `QtWebEngineQuick::initialize()` before creating the application object.
- Monaco 0.57 worker entry points changed from older guides — see
  `experiments/README.md` §E1.
- A stale `editor/dist` loads and fails at runtime; tests must fail closed
  on timeouts.
- The daemon's stdout is the protocol channel: never print to it.
