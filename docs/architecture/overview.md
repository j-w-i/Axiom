# Architecture overview

Status: **decided** for the desktop shell (ADR-0020: Flutter, `apps/desktop-flutter`, the app
since FL-QA-001); **directional** for later milestones, with contracts drafted in
`crates/protocol`. History: M1's shell was Qt (`apps/desktop`); milestone M1.5 replaced it with
Tauri 2 + React (`apps/desktop-tauri`), the default from task T-14, Qt retired by T-15 (tag
`qt-shell-final`). ADR-0020 then moved the shell to Flutter, gated the same way; FL-QA-001
retired the Tauri shell and the Monaco `editor/` package (tag `tauri-shell-final`). The daemon
side of the picture did not change in either migration.

## The shape in one picture

```
 ┌──────────────────── axiom (desktop shell: Flutter, Dart, widgets layer only) ────────────────┐
 │  surfaces (shell, files, editor, console, scm, search, env, chat) on lib/ui (design system)  │
 │                    ←→ stores lib/state (workspace, documents, tasks, lsp, git, agent, …)     │
 │  editor: re_editor + re_highlight · terminal: xterm · Markdown: markdown → own widgets       │
 │  lib/daemon/client.dart: spawn, NDJSON framing, ids, timeouts, $/cancel, restart, shutdown   │
 │  lib/daemon/protocol.g.dart: wire types generated from schemas/                              │
 └──────────────────────────────────────────────────────────────┬───────────────────────────────┘
                                                                │ JSON-RPC 2.0, NDJSON, stdio
 ┌──────────────────────────── axiomd (daemon, Rust) ───────────▼──────────────────────────────┐
 │  rpc router ─ workspace ─ exec ─ store ─ (M2: env, build) ─ (M3: agent, tools) ─ (M5: inspect)│
 └───────┬───────────────┬──────────────────┬───────────────────────┬───────────────────────────┘
         │               │                  │                       │
   user processes   language servers   provider processes    inspection adapters
   (tasks, builds)  (LSP, M4)          (JSON-RPC, M2)        (in the previewed app, M5)
```

Two processes we own (`axiom`, `axiomd`) and any number of child processes the
daemon supervises. The shell renders its whole UI itself; there is no web view.

The Qt shell (`apps/desktop`: C++/QML + Qt WebEngine + QWebChannel, M1) and the
Tauri shell (`apps/desktop-tauri`: Rust host + React + Monaco, M1.5–M4) were
earlier implementations of the same architecture; their suites were the
specification for the Flutter ones (ADR-0020 Consequences).

## Decisions that define the architecture

| # | Decision | Record |
|---|----------|--------|
| 1 | Flutter shell (Dart, widgets layer only, no Material or Cupertino); `axiomd` is a separate Rust process; the shell contains no business logic | ADR-0020, ADR-0002 |
| 2 | The shell and daemon are separate processes speaking JSON-RPC 2.0 as newline-delimited JSON; the daemon is a stdio child in M1 and additionally listens on a local socket from M3 | ADR-0002, ADR-0003 |
| 3 | `crates/protocol` (Rust types) is the single source of truth for the wire contract; JSON Schema is generated from it and the shell's Dart types are generated from the schema | ADR-0005, ADR-0020 §2 |
| 4 | The editor is `re_editor` (with `re_highlight`) inside the shell, with Axiom's tabs, split, find, LSP client, Git marks and diff on top; no web view, no editor bridge. Language features come from language servers supervised by the daemon | ADR-0020 §3 (supersedes ADR-0004's Monaco) |
| 5 | Toolchain providers are declarative manifests interpreted by the daemon, optionally backed by an out-of-process helper speaking JSON-RPC. No in-process third-party code | ADR-0006 |
| 6 | The agent loop is in-house, in the daemon. Agent tools are the same service operations the GUI calls, wrapped in a permission gate. The same registry is exposed over MCP | ADR-0007 |
| 7 | Visual inspection uses build-time source stamping as the primary mapping, a common adapter protocol, and verification by geometry + pixels + diagnostics | ADR-0008 |
| 8 | SQLite for application state, human-readable TOML for project configuration, JSONL for agent transcripts | ADR-0009 |
| 9 | Isolation is opt-in, per-OS, capability-probed and never overstated | ADR-0010 |
| 10 | Licence `MIT OR Apache-2.0`; Rust and Dart dependencies restricted to permissive licences (`cargo deny`, `scripts/check_dart_licences.py`); Flutter and Dart are BSD-3 | ADR-0011, ADR-0020 §10 |

## Why a daemon instead of a library

The brief requires that the GUI, CLI, agents, tests and future remote clients
use *the same* services. A process boundary with a typed protocol gives that
for free, and also:

- crash isolation in both directions (N3);
- the Rust side is testable end-to-end without any GUI toolkit — the shell's
  client and end-to-end tests spawn `axiomd` and speak the protocol;
- no FFI build coupling: Cargo builds the daemon and Flutter builds the shell
  independently;
- remote execution later is "run `axiomd` over SSH", not a redesign.

The cost is that every GUI interaction with platform state is asynchronous
and serialised. For an IDE that is the correct default anyway (N2). The hot
path that must not cross the boundary per keystroke — text editing — stays
inside the editor surface.

Experiment E1 measured the full path with the Qt shell; the Flutter shell
repeats it end to end in `apps/desktop-flutter/integration_test/` (`editor_test.dart`:
open → type → save → daemon → disk; `console_test.dart`: run command → streamed
output).

## Layering rule

```
apps/desktop-flutter ──▶ (wire protocol only, types generated from schemas/) ──▶ axiomd

crates/desktop-bridge ──▶ axiom-protocol      (a client library; no shell links it today)

axiomd ──▶ service crates ──▶ axiom-protocol
           (workspace, exec, store, ...)
axiomd ──▶ axiom-rpc ──▶ axiom-protocol
```

- Service crates never depend on each other's internals, on `axiom-rpc`, or on
  `axiomd`. They are plain async Rust libraries whose public functions take
  and return `axiom-protocol` types and report events on a channel.
- `axiom-rpc` knows framing, dispatch and cancellation. It knows nothing about
  workspaces or tasks.
- `axiom-desktop-bridge` knows the daemon process and protocol 0.1 as a
  client. It knows nothing about any UI. It served the Tauri host; the
  Flutter shell's Dart client (`lib/daemon/client.dart`) keeps its observable
  behaviour, and its tests still pin the daemon's lifecycle from a client's
  side.
- `axiomd` is the only place where services are wired to methods.
- The shell contains no business logic: if a behaviour would be needed by a
  CLI or an agent too, it belongs in the daemon.

Full module list and dependency table: `modules.md`. Processes and lifecycle:
`processes.md`. Request walkthroughs: `data-flow.md`. Threats and controls:
`security.md`. Rules that keep this intact: `governance.md`.

## What exists today

Read `docs/roadmap/current.md` for status. In short: the protocol crate
(0.1–0.4), generated schemas, the M1–M4 service crates and `axiomd`, the `axiom`
CLI, and the Flutter shell (`apps/desktop-flutter`), which `scripts/run.sh`
launches and `scripts/bundle.sh` packages (deb, rpm, macOS app). Windows has no
desktop build yet (deferred by the owner).
