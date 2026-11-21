# Architecture overview

Status: **migrating**: ADR-0020 moves the desktop shell to Flutter (`apps/desktop-flutter`),
gated so that the Tauri shell below stays the app `scripts/run.sh` launches until FL-QA-001.
The daemon side of this picture does not change. Earlier status: **decided** for the desktop
shell (ADR-0012); **directional** for later
milestones, with contracts drafted in `crates/protocol`. M1's shell was Qt
(`apps/desktop`); milestone M1.5 replaces it with Tauri 2 + React
(`apps/desktop-tauri`), gated so that Qt remains the release artifact until
task T-14.

## The shape in one picture

```
 ┌───────────────────── axiom (desktop shell: Tauri 2, Rust host + React) ─────────────────────┐
 │  React views ←→ stores (workspace, documents, tasks, settings) ←→ daemon client (TS)         │
 │                                  │                          │                                │
 │                 EditorPort/Listener (bridge 1, in-process)   │ JSON-RPC 2.0, NDJSON, stdio    │
 │                                  │                          │                                │
 │   ┌─ system webview (WebKitGTK · WebView2 · WKWebView) ──┐  │                                │
 │   │  Monaco + editor/src/documents.ts (bridge.ts API)     │  │                                │
 │   └───────────────────────────────────────────────────────┘  │                                │
 │  Tauri host (Rust): capabilities, one ordered event channel  │                                │
 │  crates/desktop-bridge: spawn, framing, ids, timeouts, cancel, restart, shutdown              │
 └──────────────────────────────────────────────────────────────┼───────────────────────────────┘
                                                                │
 ┌──────────────────────────── axiomd (daemon, Rust) ───────────▼──────────────────────────────┐
 │  rpc router ─ workspace ─ exec ─ store ─ (M2: env, build) ─ (M3: agent, tools) ─ (M5: inspect)│
 └───────┬───────────────┬──────────────────┬───────────────────────┬───────────────────────────┘
         │               │                  │                       │
   user processes   language servers   provider processes    inspection adapters
   (tasks, builds)  (LSP, M4)          (JSON-RPC, M2)        (in the previewed app, M5)
```

Two processes we own (`axiom`, `axiomd`), the OS-provided webview processes,
and any number of child processes the daemon supervises.

The Qt shell (`apps/desktop`: C++/QML + Qt WebEngine + QWebChannel) is the M1
implementation of the same architecture. It stays buildable and tested until
T-15 and is the reference for behaviour during the migration.

## Decisions that define the architecture

| # | Decision | Record |
|---|----------|--------|
| 1 | Tauri 2 shell: Rust host + React/TypeScript webview; `axiomd` is a separate Rust process; the shell contains no business logic | ADR-0012, ADR-0002 |
| 2 | The shell and daemon are separate processes speaking JSON-RPC 2.0 as newline-delimited JSON; the daemon is a stdio child in M1 and additionally listens on a local socket from M3 | ADR-0002, ADR-0003 |
| 3 | `crates/protocol` (Rust types) is the single source of truth for the wire contract; JSON Schema is generated from it and TypeScript types are generated from the schema | ADR-0005 |
| 4 | Editor is Monaco hosted in the application webview, behind the editor-bridge-1 message set (an in-process TypeScript interface). Language features come from language servers supervised by the daemon. VS Code extension hosting is a later, measured step | ADR-0004, ADR-0012 |
| 5 | Toolchain providers are declarative manifests interpreted by the daemon, optionally backed by an out-of-process helper speaking JSON-RPC. No in-process third-party code | ADR-0006 |
| 6 | The agent loop is in-house, in the daemon. Agent tools are the same service operations the GUI calls, wrapped in a permission gate. The same registry is exposed over MCP | ADR-0007 |
| 7 | Visual inspection uses build-time source stamping as the primary mapping, a common adapter protocol, and verification by geometry + pixels + diagnostics | ADR-0008 |
| 8 | SQLite for application state, human-readable TOML for project configuration, JSONL for agent transcripts | ADR-0009 |
| 9 | Isolation is opt-in, per-OS, capability-probed and never overstated | ADR-0010 |
| 10 | Licence `MIT OR Apache-2.0`; Rust and npm dependencies restricted to permissive licences; the system webview is used under its own terms (WebKitGTK LGPL-2.1 on Linux) | ADR-0011 (amended by ADR-0012) |

## Why a daemon instead of a library

The brief requires that the GUI, CLI, agents, tests and future remote clients
use *the same* services. A process boundary with a typed protocol gives that
for free, and also:

- crash isolation in both directions (N3);
- the Rust side is testable end-to-end without any GUI toolkit — both shells'
  integration tests spawn `axiomd` and speak the protocol;
- no FFI build coupling: Cargo builds the daemon and the Tauri host
  independently of the webview bundle;
- remote execution later is "run `axiomd` over SSH", not a redesign.

The cost is that every GUI interaction with platform state is asynchronous
and serialised. For an IDE that is the correct default anyway (N2). The hot
path that must not cross the boundary per keystroke — text editing — stays
inside the editor surface.

Experiment E1 measured the full path with the Qt shell; the Tauri shell
repeats it end to end in `apps/desktop-tauri/e2e/` (open → type → save →
daemon → disk → run command → streamed output).

## Layering rule

```
apps/desktop-tauri ──▶ (wire protocol only) ──▶ axiomd
      │  src-tauri (Rust host)
      └────────────▶ crates/desktop-bridge ──▶ axiom-protocol

editor/      ──▶ (bridge contract only) ──▶ webview of apps/desktop-tauri

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
  client. It knows nothing about Tauri or the UI.
- `axiomd` is the only place where services are wired to methods.
- The shell contains no business logic: if a behaviour would be needed by a
  CLI or an agent too, it belongs in the daemon.

Full module list and dependency table: `modules.md`. Processes and lifecycle:
`processes.md`. Request walkthroughs: `data-flow.md`. Threats and controls:
`security.md`. Rules that keep this intact: `governance.md`.

## What exists today

Read `docs/roadmap/current.md` for status. In short: the protocol crate,
generated schemas, every M1 backend crate and `axiomd` (227 Rust tests), the
Monaco editor package, the Qt shell (M1) and the Tauri shell (M1.5, being
brought to parity). The Qt shell is the release artifact until task T-14.
