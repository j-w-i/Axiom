# Architecture overview

Status: **decided** for M1 (and validated by experiments E1–E3); **directional**
for later milestones, with contracts drafted in `crates/protocol`.

## The shape in one picture

```
 ┌──────────────────────────── axiom (desktop shell, C++/QML, Qt 6) ───────────────────────────┐
 │  QML views  ←→  C++ controllers/models  ←→  DaemonClient                                    │
 │                         │                        │ JSON-RPC 2.0, NDJSON, stdio              │
 │                 EditorBridge (QWebChannel)       │                                          │
 │                         │                        │                                          │
 │   ┌─ Qt WebEngine renderer (separate OS process) ┐                                          │
 │   │  editor surface: Monaco + bridge.ts          │                                          │
 │   └──────────────────────────────────────────────┘                                          │
 └──────────────────────────────────────────────────┼──────────────────────────────────────────┘
                                                    │
 ┌──────────────────────────── axiomd (daemon, Rust) ▼──────────────────────────────────────────┐
 │  rpc router ─ workspace ─ exec ─ store ─ (M2: env, build) ─ (M3: agent, tools) ─ (M5: inspect)│
 └───────┬───────────────┬──────────────────┬───────────────────────┬───────────────────────────┘
         │               │                  │                       │
   user processes   language servers   provider processes    inspection adapters
   (tasks, builds)  (LSP, M4)          (JSON-RPC, M2)        (in the previewed app, M5)
```

Two processes we own (`axiom`, `axiomd`), one we host (the WebEngine
renderer), and any number of child processes the daemon supervises.

## Decisions that define the architecture

| # | Decision | Record |
|---|----------|--------|
| 1 | Qt 6 C++/QML shell; Rust daemon; no Rust in the GUI process | ADR-0001, ADR-0002 |
| 2 | The shell and daemon are separate processes speaking JSON-RPC 2.0 as newline-delimited JSON; the daemon is a stdio child in M1 and additionally listens on a local socket from M3 | ADR-0002, ADR-0003 |
| 3 | `crates/protocol` (Rust types) is the single source of truth for the wire contract; JSON Schema and TypeScript are generated from it | ADR-0005 |
| 4 | Editor is Monaco in Qt WebEngine, served from an `axiom://` secure custom scheme, bridged with QWebChannel. Language features come from language servers supervised by the daemon. VS Code extension hosting is a later, measured step | ADR-0004 |
| 5 | Toolchain providers are declarative manifests interpreted by the daemon, optionally backed by an out-of-process helper speaking JSON-RPC. No in-process third-party code | ADR-0006 |
| 6 | The agent loop is in-house, in the daemon. Agent tools are the same service operations the GUI calls, wrapped in a permission gate. The same registry is exposed over MCP | ADR-0007 |
| 7 | Visual inspection uses build-time source stamping as the primary mapping, a common adapter protocol, and verification by geometry + pixels + diagnostics | ADR-0008 |
| 8 | SQLite for application state, human-readable TOML for project configuration, JSONL for agent transcripts | ADR-0009 |
| 9 | Isolation is opt-in, per-OS, capability-probed and never overstated | ADR-0010 |
| 10 | Licence `MIT OR Apache-2.0`; Qt used under LGPL-3.0 by dynamic linking; GPL-only Qt modules are forbidden | ADR-0011 |

## Why a daemon instead of a library

The brief requires that the GUI, CLI, agents, tests and future remote clients
use *the same* services. A process boundary with a typed protocol gives that
for free, and also:

- crash isolation in both directions (N3);
- the Rust side is testable end-to-end without Qt (integration tests spawn
  `axiomd` and speak the protocol);
- no C++/Rust FFI build coupling: Cargo builds the daemon, CMake builds the
  shell, CI can cache them independently;
- remote execution later is "run `axiomd` over SSH", not a redesign.

The cost is that every GUI interaction with platform state is asynchronous
and serialised. For an IDE that is the correct default anyway (N2). The hot
path that must not cross the boundary per keystroke — text editing — stays
inside the editor surface.

Experiment E1 measured the full path (shell → daemon → file → Monaco →
keystrokes → save → daemon → disk → run command → streamed output) on Qt
6.11.2 and 6.12.0; see `experiments/README.md`.

## Layering rule

```
apps/desktop ──▶ (wire protocol only) ──▶ axiomd
editor/      ──▶ (bridge contract only) ──▶ apps/desktop

axiomd ──▶ service crates ──▶ axiom-protocol
           (workspace, exec, store, ...)
axiomd ──▶ axiom-rpc ──▶ axiom-protocol
```

- Service crates never depend on each other's internals, on `axiom-rpc`, or on
  `axiomd`. They are plain async Rust libraries whose public functions take
  and return `axiom-protocol` types and report events on a channel.
- `axiom-rpc` knows framing, dispatch and cancellation. It knows nothing about
  workspaces or tasks.
- `axiomd` is the only place where services are wired to methods.
- The shell contains no business logic: if a behaviour would be needed by a
  CLI or an agent too, it belongs in the daemon.

Full module list and dependency table: `modules.md`. Processes and lifecycle:
`processes.md`. Request walkthroughs: `data-flow.md`. Threats and controls:
`security.md`. Rules that keep this intact: `governance.md`.

## What exists today

Read `docs/roadmap/current.md` for status. In short: the protocol crate,
generated schemas, a reference provider manifest, three passing experiments
and this documentation exist. The daemon, the shell and the editor package do
not exist yet; they are milestone M1.
