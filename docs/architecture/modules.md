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
  env/                     provider registry, detection, installation      M2
  build/                   task templates, diagnostics parsing             M2
  agent/                   model providers, agent loop, tools, permissions M3
  lsp/                     language-server supervision and relay           M4
  inspect/                 inspection sessions, source resolution, verify  M5
apps/
  desktop/                 Qt shell (C++/QML)                              M1
  cli/                     `axiom` command-line client (Rust)              M3
editor/                    editor surface (TypeScript, Monaco)             M1
providers/<id>/            provider manifests (+ optional helper)          rust/ EXISTS
inspectors/<framework>/    inspection adapters                             M5
schemas/                   GENERATED JSON Schemas                          EXISTS
experiments/               isolated proofs of concept; never depended on   EXISTS
tests/fixtures/            small sample projects                           M1
docs/, agents/             documentation and agent continuity
scripts/                   check.sh, dev-env.sh, bootstrap-qt.sh           EXISTS
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
- **Tests:** `tests/contract.rs` pins wire shapes; `tests/drafts.rs` keeps
  draft contracts parseable.

### `crates/rpc` — `axiom-rpc`
- **Responsibility:** turn a byte stream into typed request handling.
  NDJSON codec, connection read/write loops, a `Router` that maps method names
  to async handlers, per-request cancellation tokens, the `initialize` gate, a
  `Notifier` handle for sending notifications, and a typed test client.
- **Depends on:** `axiom-protocol`, `tokio`, `tokio-util`, `serde_json`,
  `tracing`, `thiserror`.
- **Must not know:** any domain concept.
- **Failure modes:** malformed line → `parse_error` response with `id: null`,
  connection stays open; handler panic → caught, `internal` error response;
  peer closes → all in-flight handlers are cancelled.
- **Spec:** `docs/protocols/ipc.md`; task M1-RPC-001/002.

### `crates/store` — `axiom-store`
- **Responsibility:** durable application state: settings, recent workspaces,
  per-workspace key/value state. Owns the SQLite file and its migrations.
- **Depends on:** `rusqlite` (feature `bundled`), `serde_json`, `thiserror`,
  `directories`.
- **Public interface:** `Store::open(path)`, `Store::open_in_memory()`,
  `settings_get/set`, `recent_list/touch`, `workspace_state_get/set`. All
  synchronous; callers wrap in `spawn_blocking`.
- **Failure modes:** corrupt database → rename to `axiom.db.corrupt-<ts>`,
  start fresh, report once via `$/log` at `error`. Schema newer than the
  binary → refuse to start with a clear message (never downgrade data).

### `crates/workspace` — `axiom-workspace`
- **Responsibility:** the set of open workspaces; path validation and
  containment; directory listing with ignore rules; text file read/write with
  version checks; file watching.
- **Depends on:** `axiom-protocol`, `tokio`, `ignore`, `notify`, `sha2`,
  `thiserror`, `tempfile`.
- **Public interface:** `Workspaces` (registry) with `open`, `close`, `get`;
  `Workspace` with `resolve(&WorkspacePath) -> Result<PathBuf>`, `list`,
  `read_text`, `write_text`, `watch(sender)`.
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
  WebEngineQuick WebChannel` only (all LGPL; see ADR-0011).
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

### `editor/` — package `@axiom/editor`
- **Responsibility:** the web page loaded in the WebEngine view: Monaco setup,
  workers, document models, and the page side of the bridge.
- **Depends on:** `monaco-editor` (pinned exact version), `vite`, `typescript`.
- **Public interface:** the bridge contract in `docs/protocols/editor-bridge.md`.
- **Rule:** no network access, no direct file access; it only knows documents
  the host gave it.

## Dependency matrix (allowed edges)

| From ↓ / To → | protocol | rpc | store | workspace | exec | axiomd |
|---|---|---|---|---|---|---|
| protocol  | – | ✗ | ✗ | ✗ | ✗ | ✗ |
| rpc       | ✓ | – | ✗ | ✗ | ✗ | ✗ |
| store     | ✗ | ✗ | – | ✗ | ✗ | ✗ |
| workspace | ✓ | ✗ | ✗ | – | ✗ | ✗ |
| exec      | ✓ | ✗ | ✗ | ✗ | – | ✗ |
| axiomd    | ✓ | ✓ | ✓ | ✓ | ✓ | – |

Later service crates follow the same rule: they may depend on `protocol` and
on *lower* service crates listed in their task spec (e.g. `build` → `exec`,
`env`; `agent` → none: it calls services through the tool registry that
`axiomd` populates). `store` deliberately does not depend on `protocol`: it
stores JSON values and plain rows, which keeps persistence independent of
wire-format changes.

`scripts/check.sh` will enforce this matrix once task M1-CI-001 adds the
`cargo metadata` check.

## Later modules (interfaces drafted, internals deferred)

| Module | Contract today | Detailed design when |
|--------|----------------|----------------------|
| `env` | `crates/protocol/src/provider.rs`, `docs/protocols/providers.md`, `providers/rust/provider.toml` | start of M2 |
| `build` | `crates/protocol/src/diagnostics.rs`, `TaskTemplate` in provider.rs | start of M2 |
| `agent` | `docs/protocols/agents.md` | start of M3 |
| `lsp` | ADR-0004 §Language intelligence | start of M4 |
| `inspect` | `crates/protocol/src/inspection.rs`, `docs/protocols/inspection.md` | start of M5 |
