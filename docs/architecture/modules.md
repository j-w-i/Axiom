# Modules

Every module has one owner directory, one responsibility and a stated set of
allowed dependencies. Adding a dependency edge that is not in this table is an
architecture change (see `governance.md`).

## Repository layout

```
Cargo.toml                 Rust workspace
crates/
  protocol/                wire contract (types only)                      EXISTS
  rpc/                     framing, dispatch, cancellation                 M1
  store/                   SQLite persistence                              M1
  workspace/               workspace registry, fs access, watching         M1
  exec/                    process supervision                             M1
  axiomd/                  daemon binary: wires services to methods        M1
  desktop-bridge/          daemon process + protocol 0.1 client for a GUI  M1.5 (no shell links it since FL-QA-001)
  env/                     provider registry, detection, installation      M2
  build/                   task templates, diagnostics parsing             M2
  agent/                   model providers, agent loop, tools, permissions M3
  lsp/                     language-server supervision and protocol client M4
  term/                    PTY terminals                                   M4
  inspect/                 inspection sessions, source resolution, verify  M5
apps/
  desktop/                 Qt shell (C++/QML) - retired by M1.5 task T-15  M1
  desktop-tauri/           Tauri shell: Rust host + React webview          M1.5 - retired by FL-QA-001 (tag tauri-shell-final)
  desktop-flutter/         Flutter shell (Dart, widgets layer only)        FL (ADR-0020) EXISTS
  cli/                     `axiom` command-line client (Rust)              M3
editor/                    editor surface (TypeScript, Monaco)             M1 - retired by FL-QA-001 (tag tauri-shell-final)
providers/<id>/            provider manifests (+ optional helper)          rust/ EXISTS
inspectors/<framework>/    inspection adapters                             M5
schemas/                   GENERATED JSON Schemas                          EXISTS
experiments/               isolated proofs of concept; never depended on   EXISTS
tests/fixtures/            small sample projects                           M1
docs/, agents/             documentation and agent continuity
scripts/                   gate, board, launch, packaging, headless runs   EXISTS
                           (check.sh, tasks.py, run.sh, bundle.sh, verify-package.sh,
                           headless.sh, sysroot.sh, env.sh, check_deps.py, check_dart_licences.py)
```

Directories marked with a milestone do not exist yet. **Do not create them
before the task that needs them** (no empty scaffolding).

## M1 modules in detail

### `crates/protocol` — `axiom-protocol`
- **Responsibility:** the wire contract. Message envelope, error model, method
  and notification definitions, shared value types, schema export.
- **Depends on:** `serde`, `serde_json`, `schemars`. Nothing else, ever. No
  I/O, no async, no `cfg(target_os)`.
- **Public interface:** `Method`/`Notification` traits; one module per domain;
  `registry::{methods, notifications, method_names, protocol_document}`.
- **Tests:** `tests/contract.rs` pins the 0.1 wire shapes, `tests/m2_contract.rs` the 0.2
  additions (`env`, `build`); `tests/drafts.rs` keeps the manifest and draft contracts
  parseable.

### `crates/rpc` — `axiom-rpc`
- **Responsibility:** turn a byte stream into typed request handling.
  NDJSON codec, connection read/write loops, a `Router` that maps method names
  to async handlers, per-request cancellation tokens, the `initialize` gate, a
  `Notifier` handle for sending notifications, and a typed test client.
- **Depends on:** `axiom-protocol`, `tokio`, `tokio-util`, `serde_json`,
  `tracing`, `thiserror`.
- **Must not know:** any domain concept.
- **Failure modes:** a line that is not JSON → `parse_error`, valid JSON that
  is not a valid message → `invalid_request` (both with `id: null`; the
  connection stays open); handler panic → caught, `internal` error response;
  peer closes → all in-flight handlers are cancelled.
- **Spec:** `docs/protocols/ipc.md`; task M1-RPC-001/002.

### `crates/store` — `axiom-store`
- **Responsibility:** durable application state: settings, recent workspaces,
  per-workspace key/value state. Owns the SQLite file and its migrations.
- **Depends on:** `rusqlite` (feature `bundled`), `serde_json`, `thiserror`,
  `directories`.
- **Public interface:** `Store::open(path)`, `Store::open_in_memory()`,
  `setting_get`, `settings_all`, `settings_set`, `recent_touch/list/remove`,
  `workspace_state_get/set` (exact signatures: task M1-STORE-001). All
  synchronous; callers wrap in `spawn_blocking`.
- **Failure modes:** corrupt database → rename to `axiom.db.corrupt-<ts>`,
  start fresh, report once via `$/log` at `error`. Schema newer than the
  binary → refuse to start with a clear message (never downgrade data).

### `crates/workspace` — `axiom-workspace`
- **Responsibility:** the set of open workspaces; path validation and
  containment; directory listing with ignore rules; text file read/write with
  version checks; file watching.
- **Depends on:** `axiom-protocol`, `tokio`, `ignore`, `notify`, `sha2`,
  `thiserror`, `tempfile`; from M3 (`search`, for the `workspace.search` tool) `regex`; the
  `ignore` walker does the traversal (M3-TOOLS-001 chose this over the grep crates ADR-0015 §3
  named: no new dependency).
- **Public interface:** `Workspaces` (registry) with `open`, `close`, `get`;
  `Workspace` with `resolve(&WorkspacePath) -> Result<PathBuf>`, `list`,
  `read_text`, `write_text`, `watch() -> WatchHandle` (exact signatures:
  tasks M1-WS-001, M1-WS-002).
- **Invariant:** `resolve` is the only way to turn a `WorkspacePath` into a
  host path, and it guarantees containment (see `security.md` §Paths). Every
  other crate that needs a host path for a workspace file calls it.

### `crates/exec` — `axiom-exec`
- **Responsibility:** spawn, stream, cancel and reap processes; kill whole
  process trees; bounded task history.
- **Depends on:** `axiom-protocol`, `tokio`, `thiserror`; `nix` on Unix,
  `windows-sys` on Windows (process-tree control only).
- **Public interface:** `Executor::start(workspace, SpawnSpec) -> (TaskId,
  Receiver<TaskEvent>)`, `cancel(TaskId)`, `list(WorkspaceId)`,
  `cancel_all(WorkspaceId)`, `shutdown()` (exact signatures: task
  M1-EXEC-001). It takes a *resolved* cwd; it does not know about
  workspaces' paths.
- **Platform abstraction:** `trait ProcessTree { fn terminate(&self); fn kill(&self); }`
  with a Unix implementation (own process group; SIGTERM then SIGKILL to the
  group) and a Windows implementation (Job Object with
  `KILL_ON_JOB_CLOSE`; `TerminateJobObject`). This is the pattern for all
  platform-specific code: a small trait in the crate that needs it, two
  implementations selected with `cfg`, tests that run on every OS in CI.

### `crates/axiomd` — binary `axiomd`
- **Responsibility:** process entry point. Parses arguments, sets up logging
  (to stderr), opens the store, builds the services, registers one handler
  per protocol method, runs the connection, shuts down cleanly.
- **Depends on:** all of the above, `clap`, `tracing-subscriber`.
- **Rule:** handlers are thin: deserialize → call one service function → map
  the service error to `RpcError` → serialize. No logic.

### `apps/desktop` — binary `axiom`
- **Responsibility:** windows, views, input, theming. Presents daemon state
  and forwards user intent.
- **Depends on:** Qt 6.8+ modules `Core Gui Qml Quick QuickControls2
  QuickDialogs2 WebEngineQuick WebChannel`, plus `Test` for tests — nothing
  else (all LGPL; the full allowed list is in ADR-0011).
- **Internal structure:**
  - `src/daemon/` — `DaemonClient` (process, framing, id correlation, typed
    notification signals). The only code that knows JSON-RPC.
  - `src/models/` — `FileTreeModel`, `OpenDocumentsModel`, `TaskOutputModel`,
    `SettingsStore`. QObject/QAbstractItemModel classes exposed to QML; they
    call `DaemonClient` and hold presentation state.
  - `src/editor/` — `EditorBridge` (QWebChannel object), `AssetSchemeHandler`
    (`axiom://`).
  - `qml/Axiom/Ui/` — design system: `Theme` singleton, tokens, base controls.
  - `qml/Axiom/App/` — application views composed from `Axiom.Ui`.
- **Rule:** QML never constructs JSON-RPC and never touches the filesystem.
  C++ models never contain layout or colours.
- **Status:** M1 implementation, superseded by ADR-0012. Kept building and
  tested until task T-15; no new features.

### `crates/desktop-bridge` — `axiom-desktop-bridge`
- **Responsibility:** act as the GUI's protocol-0.1 *client*: locate and spawn
  `axiomd`, one JSON message per line over its stdio, correlate ids, deliver
  notifications, enforce per-request timeouts (`$/cancel` on expiry), answer
  every request exactly once, detect process failure, restart, and shut down
  in bounded time (`docs/architecture/processes.md` §Daemon lifecycle).
- **Depends on:** `axiom-protocol` (envelope types and the method registry),
  `tokio`, `serde_json`. Nothing else.
- **Must not know:** Tauri, any GUI toolkit, or any domain service. It must not
  interpret payloads beyond the envelope.
- **Public interface:** `DaemonBridge` (`start`, `request`, `request_forward`,
  `cancel_forward`, `restart`, `shutdown`, `state`), `BridgeConfig`,
  `BridgeEvent`, `DaemonState`, `locate_daemon`, `ui_callable`.
- **Rule:** requests the UI may issue are forwarded as events into the same
  ordered stream as notifications, so a client sees the `task/start` response
  before that task's first `task/output`.
- **Tests:** `tests/bridge.rs` against the real `axiomd` binary (concurrency,
  ordering, cancel, timeout, crash, restart, shutdown, missing binary).
- **Status:** the Tauri host was its only user; since FL-QA-001 no shell links it. It stays in
  the Cargo workspace and `scripts/check.sh rust` still runs its tests, which pin the daemon's
  lifecycle from a client's side. The Flutter shell's Dart client keeps the same observable
  behaviour (`apps/desktop-flutter/lib/daemon/client.dart`).

### `apps/desktop-tauri` — Tauri 2 shell
- **Status:** retired by FL-QA-001 (2026-11-22, ADR-0020): removed from the tree and from the
  Cargo workspace; recover it from the tag `tauri-shell-final`. The record below is how it was
  built (M1.5–M4).
- **Responsibility:** the desktop shell. Presents daemon state and forwards
  user intent. Two halves:
  - `src-tauri/` (Rust, binary `axiom-tauri`): owns the `DaemonBridge`,
    delivers one ordered event channel to the webview, exposes exactly the
    commands in `security.md` §Host commands, persists window geometry,
    intercepts window close so the UI can confirm unsaved work, and shuts the
    daemon down in bounded time.
  - `src/` (React + TypeScript): views, the design system, and the stores
    that hold presentation state and protocol sequencing (ports of the Qt
    models: `WorkspaceStore`, `DocumentsStore`, `TasksStore`, `SettingsStore`).
- **Depends on:** `axiom-desktop-bridge`, `tauri`, `tauri-plugin-dialog`
  (folder picker only); npm: React, Radix primitives, `@tauri-apps/api`,
  Monaco (pinned exact version, shared with `editor/`).
- **Rule:** the webview never touches the filesystem, never spawns a process
  and never speaks the wire protocol itself; it calls the host's commands.
  No business logic in either half.

### `apps/desktop-flutter` — Flutter shell (ADR-0020)
- **Responsibility:** the desktop shell (the app since FL-QA-001; binary `axiom`, app id
  `dev.axiom.Axiom`). One Dart process that presents daemon state and forwards user intent.
- **Parts:** `lib/daemon/` (`client.dart` spawns and supervises `axiomd` and speaks JSON-RPC
  over its stdio; `protocol.g.dart`, the wire types, generated from `schemas/` by
  `tool/gen_types.dart`), `lib/app/` (command line, app state, the shell-local layout file),
  `lib/ui/` (the design system; colours only in `tokens.dart`), `lib/state/` (stores), and one
  folder per surface (`shell`, `files`, `editor`, `console`, `scm`, `search`, `env`, `chat`).
  Tests in `test/`, end-to-end suites in `integration_test/`, packaging icons in `packaging/`.
  `third_party/re_editor/` is re_editor 0.10.0 with one patch, a configurable caret blink
  (FL-PERF-001). It is used through `dependency_overrides`; `PATCHES.md` records the patch and
  how to drop it.
- **Depends on:** Flutter's `widgets` layer and Dart packages that are MIT, BSD or Apache-2.0
  (`scripts/check_dart_licences.py`): `re_editor`, `re_highlight` (editor), `xterm`
  (terminal), `markdown` (Chat), `file_selector` (folder and image pickers), `path_parsing`
  (icons). No Material or Cupertino (`test/no_material_test.dart`).
- **Rule:** it never touches workspace files and never runs processes other than `axiomd`,
  except the system's URL opener for an http(s) link the person clicked
  (`lib/app/open_link.dart`); no business logic. It has no Rust dependency: the daemon is the
  only edge, over the process boundary.

### `editor/` — package `@axiom/editor`
- **Status:** retired with the Tauri shell by FL-QA-001 (tag `tauri-shell-final`); the editor is
  now `re_editor` inside `apps/desktop-flutter` (ADR-0020 §3). The record below is how it was
  built.
- **Responsibility:** the editor surface: Monaco setup, workers, document
  models and the page side of the bridge. Its framework-neutral modules
  (`documents.ts`, `monaco-setup.ts`, `bridge.ts`, `version.ts`) are imported
  directly by the Tauri shell's webview; the Qt shell loads the built bundle.
- **Depends on:** `monaco-editor` (pinned exact version), `vite`, `typescript`.
- **Public interface:** the bridge contract in `docs/protocols/editor-bridge.md`.
- **Rule:** no network access, no direct file access; it only knows documents
  the host gave it.

## M2 modules

### `crates/env` — `axiom-env` (M2-ENV-001…005)
- **Status:** complete for M2: registry, validation and detection (ENV-001), archive installs
  (ENV-002), delegate installs (ENV-003), system packages (ENV-004), project environments and
  the lock (ENV-005).
- **Responsibility:** providers (load, validate, trust), tool detection, version listing,
  install plans and their execution into managed storage, uninstall, and resolving a
  project's `Environment` from `.axiom/project.toml` / `.axiom/project.lock`.
- **Depends on:** `protocol`, `store` (installed-toolchain index, workspace trust); `reqwest`
  (rustls), `sha2`, `tar`, `flate2`, `xz2`, `zip`, `toml`, `regex`, `semver` (ADR-0014 §3).
- **Rule:** never executes anything not in an issued `InstallPlan`; never writes outside its
  managed root except the workspace's `.axiom/project.lock`.

### `crates/build` — `axiom-build` (M2-BUILD-001)
- **Status:** built (M2-BUILD-001).
- **Responsibility:** provider tasks: list them, start them through `exec` in the resolved
  environment, and parse their output into `Diagnostic`s (`build/diagnostics`).
- **Depends on:** `protocol`, `exec`, `env`; `regex`.

## M3 modules

### `crates/agent` — `axiom-agent` (M3-MODEL-001…, M3-LOOP-001)
- **Status:** model layer, providers and credentials (MODEL-001/002), tools (TOOLS-001), gate (GATE-001), checkpoints (CKPT-001), sessions and the loop (LOOP-001) built; MCP and ACP to come.
- **Responsibility:** the agent runtime: message model, `ModelProvider` and its adapters
  (Anthropic Messages, OpenAI Chat, scripted for tests), credentials, the tool registry types
  and result bounding, the permission gate and audit log, checkpoints (shadow git), the session
  journal and compaction, the loop, and the MCP codec (server and client halves).
- **Depends on:** `protocol`, `store`; `reqwest` (rustls), `keyring`, `tokio`, `serde_json`,
  `futures-util` (ADR-0015 §3). Runs the `git` program for checkpoints.
- **Must not know:** the workspace, exec, env or build crates. Tools reach services only through
  the closures `axiomd` registers (`modules.md` rule "agent → none").
- **Rule:** no tool executes without a gate decision; no write without a checkpoint.

### `apps/cli` — `axiom-cli`, binary `axiom` (M3-DAEMON-001, M3-MCP-001)
- **Status:** built: `task run`, `build`, `agent sessions|send` (M3-DAEMON-001); `mcp` follows
  (M3-MCP-001).
- **Responsibility:** the command-line client: `task run`, `build`, `agent send|sessions`,
  `mcp` (the MCP server front end). Connects to a running daemon's local socket, else spawns
  its own `axiomd` over stdio.
- **Depends on:** `protocol`, `rpc` (codec and typed client); `clap`, `tokio`, `serde_json`,
  `directories` (the data dir, as the store finds it).
- **Rule:** no business logic; everything is a protocol call.

### `crates/lsp` — `axiom-lsp` (M4-LSP-001; ADR-0018 §2)
- **Responsibility:** language servers as daemon children, one per (workspace, server
  definition): LSP base-protocol framing, `initialize` with one fixed client-capability set, a
  shared id space with a pending map (many clients, one server), answers to server→client
  requests, a mirror of open documents, diagnostics per URI, a bounded restart policy.
- **Depends on:** `protocol`; `tokio`, `tokio-util` (cancellation), `serde_json`, `url`,
  `tracing`, `thiserror`.
- **Status:** built (M4-LSP-001). `axiomd` serves `lsp/*`, resolves each server's command in the
  workspace's environment (the daemon's plus the project's, as builds), and relays events to the
  workspace's clients. Tests: `crates/axiomd/tests/{lsp_server,lsp}.rs` against
  `tests/helpers/fake-lsp`, plus the real rust-analyzer where installed.
- **Rule:** the daemon owns the conversation with each server; clients use `lsp/request` and
  `lsp/notify`, never the server's own ids.

### `crates/term` — `axiom-term` (M4-TERM-001; ADR-0018 §4)
- **Responsibility:** PTY terminals: spawn the person's shell (`portable-pty`, behind a `Pty`
  trait), stream UTF-8 output without splitting characters, coalesce it, keep a bounded backlog,
  resize, end the process group.
- **Depends on:** `protocol`; `portable-pty`, `tokio` (events), `nix` (signals, Unix), `thiserror`.
- **Status:** built (M4-TERM-001). Two threads per terminal: one reads the PTY, the other
  coalesces. A bounded channel between them makes a flooding shell wait instead of growing
  memory. Close is SIGHUP to the process group, then SIGKILL after 2 s; a signal ends it without
  an exit code. `axiomd` serves `terminal/*` and relays events to the workspace's clients. Tests:
  `crates/term/tests/pty.rs`, `crates/axiomd/tests/terminal.rs` (Unix; ConPTY is owed with
  Windows). `portable-pty` is used directly rather than behind a `Pty` trait: the tests run
  real PTYs on Linux and macOS.
- **Rule:** not an agent tool and not gated: a terminal is the person's own shell.

`crates/workspace` gains a `git` module (M4-GIT-001) that runs the `git` program read-only;
no new edge.
- **Isolation:** the inherited `GIT_*` variables are removed; no pager, hooks or fsmonitor;
  `GIT_OPTIONAL_LOCKS=0`; `LC_ALL=C`. The person's own configuration (ignore files,
  `safe.directory`) applies.
- **Operations:** discovery with `rev-parse` (a workspace may be a subdirectory); status with
  porcelain v2, made relative to the workspace; show at HEAD or in the index, with binary
  detection; a watch on the repository's HEAD, index and refs.
- **In `axiomd`:** `git/status` and `git/show`, and `git/didChange` (the workspace watcher plus
  that watch, debounced 300 ms). The watch starts with a workspace's first status. Status on
  this repository takes 15 ms.

## M5 modules

### `crates/inspect` — `axiom-inspect` (M5-INSPECT-001…; ADR-0019 §4)
- The adapter WebSocket server (loopback, one port per daemon, a one-time token per preview),
  adapter sessions and their latest snapshot, normalisation, source resolution (stamps to
  workspace paths, owner and heuristic candidates, the confidence caps), and the verification
  engine (quiescence, re-identification, checks, verdict).
- Depends on `axiom-protocol` only; `axiomd` serves `preview/*` and `ui/*` through it and runs
  the dev server through `axiom-exec`.
- `inspectors/web/` (not a crate): the Vite plugin and the in-page adapter script, JavaScript
  shipped with the daemon's data.

## Dependency matrix (allowed edges)

| From ↓ / To → | protocol | rpc | store | workspace | exec | env | build | agent | lsp | term | inspect | desktop-bridge | axiomd |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| protocol  | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| rpc       | ✓ | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| store     | ✗ | ✗ | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| workspace | ✓ | ✗ | ✗ | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| exec      | ✓ | ✗ | ✗ | ✗ | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| env       | ✓ | ✗ | ✓ | ✗ | ✗ | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| build     | ✓ | ✗ | ✗ | ✗ | ✓ | ✓ | – | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| agent     | ✓ | ✗ | ✓ | ✗ | ✗ | ✗ | ✗ | – | ✗ | ✗ | ✗ | ✗ | ✗ |
| lsp       | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – | ✗ | ✗ | ✗ | ✗ |
| term      | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – | ✗ | ✗ | ✗ |
| inspect   | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – | ✗ | ✗ |
| desktop-bridge | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | – | ✗ |
| axiomd    | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | – |
| cli       | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |

The `env` and `build` rows are ADR-0014 §4; the `agent` and `cli` rows are ADR-0015 §2; the
`lsp` and `term` rows and `axiomd → lsp, term` are ADR-0018 §7; the `inspect` row and
`axiomd → inspect` are ADR-0019 §4.

Later service crates follow the same rule: they may depend on `protocol` and
on *lower* service crates listed in their task spec (e.g. `build` → `exec`,
`env`; `agent` → none: it calls services through the tool registry that
`axiomd` populates). `store` deliberately does not depend on `protocol`: it
stores JSON values and plain rows, which keeps persistence independent of
wire-format changes.

The Flutter shell is not a Cargo package and has no row: its only edge is the wire protocol
(its types are generated from `schemas/`). The `desktop-tauri` row (→ `desktop-bridge` only)
went with the Tauri shell.

`desktop-bridge` is a GUI-side client and deliberately does not depend on
`axiom-rpc` (which serves the *server* side of the same framing); a future task
may share the codec, which would add one edge and is then an architecture
change to record here.

`scripts/check.sh` enforces this matrix (`scripts/check_deps.py`, via
`cargo metadata`).

## Later modules (interfaces drafted, internals deferred)

| Module | Contract today | Detailed design when |
|--------|----------------|----------------------|
| `inspect` | `crates/protocol/src/inspection.rs`, `docs/protocols/inspection.md` | start of M5 |
