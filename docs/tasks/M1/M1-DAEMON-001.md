---
id: M1-DAEMON-001
title: Create the axiomd binary and wire all protocol 0.1 methods
status: planned
status_note:
depends_on: [M1-RPC-001, M1-STORE-001, M1-WS-001, M1-EXEC-001]
order: 30
scope: crates/axiomd/, Cargo.toml (workspace members and dependencies only)
---

# M1-DAEMON-001: The `axiomd` daemon

## Objective
A runnable daemon that serves every protocol 0.1 method over stdio by
delegating to the service crates.

## Context
Read: `docs/architecture/processes.md` §Daemon lifecycle,
`docs/architecture/modules.md` §`crates/axiomd`, `docs/protocols/ipc.md`.

## Implementation requirements
1. CLI (`clap`): `axiomd [--data-dir <dir>] [--log-level error|warn|info|debug]`
   plus `--version`. `--data-dir` overrides `AXIOM_DATA_DIR`.
2. Logging: `tracing-subscriber` to **stderr only**. Nothing but protocol
   messages may ever be written to stdout (add a test that greps stdout
   lines are all valid `Message`s).
3. Startup: open the `Store` (log once at `error` if it recovered from
   corruption; exit with code 2 and a one-line stderr message on
   `SchemaTooNew`), build `Workspaces` and `Executor`, build the `Router`,
   `serve(stdin, stdout, …)` with `ServerInfo { name: "axiomd", version:
   CARGO_PKG_VERSION }`.
4. Handlers — one small function each in `src/handlers/{workspace,fs,task,settings}.rs`:
   - `workspace/open`: `Workspaces::open`; then `store.recent_touch`; set
     setting `workspace.lastRoot`.
   - `workspace/close`: `executor.cancel_all(id)`; drop its watcher;
     `Workspaces::close`.
   - `workspace/recent`: `store.recent_list(20)` filtered to roots that are
     existing directories.
   - `workspace/getState` / `setState`: store, keyed by the workspace's
     canonical root. `StoreError::TooLarge` → `too_large`.
   - `fs/list`, `fs/readFile`, `fs/writeFile`: delegate.
   - `fs/watch`: `enabled: true` starts `Workspace::watch` (when
     M1-WS-002 is merged; until then return `unsupported` — see note) and
     forwards each batch as `fs/didChange`; `false` stops it. Idempotent.
   - `task/start`: resolve `cwd` (default root) through `Workspace::resolve`,
     check it is a directory (`not_found` otherwise), `Executor::start`,
     **respond first**, then forward events as `task/output` / `task/exited`
     notifications from a spawned task.
   - `task/cancel`, `task/list`: delegate.
   - `settings/get`: merge stored values over the defaults of the known keys
     (table in `ipc.md` §Settings); `keys` empty = all known + stored.
   - `settings/set`: validate known keys' types and ranges
     (`invalid_params`, details `{"key": …}`); `null` resets; store; then
     send `settings/didChange` with the new effective values of the changed
     keys.
   Known settings live in one table in `src/settings.rs`
   (key, default, validator).
5. Shutdown hook: `executor.shutdown()`, drop watchers.
6. Blocking calls (`Store`) go through `spawn_blocking`.

Note on ordering with M1-WS-002: this task does not depend on it. If
`Workspace::watch` does not exist yet when you implement this task, make
`fs/watch` return `unsupported` and say so in the completion report;
M1-DAEMON-002 (which depends on M1-WS-002) then completes the wiring. If it
does exist, wire it now.

## Constraints
- Handlers contain no logic beyond what is listed (governance rule 4).
- Dependencies: the five crates, `tokio` (full), `clap` (derive),
  `tracing`, `tracing-subscriber`, `serde_json`.

## Acceptance criteria
- `cargo run -p axiomd -- --version` prints the version.
- Manual session works:
  `printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"t","version":"0"}}}' '{"jsonrpc":"2.0","id":2,"method":"shutdown"}' | AXIOM_DATA_DIR=$(mktemp -d) cargo run -q -p axiomd`
  prints two response lines and exits 0; `methods` in the first equals
  `axiom_protocol::registry::method_names()` as a set.
- A unit test asserts that the router's method names equal the registry's
  (so a protocol method can never be left unwired).
- Closing stdin without `shutdown` exits 0 within 2 s.

## Tests
`scripts/check.sh rust`. Behavioural coverage is M1-DAEMON-002; this task
needs the wiring test, the stdout-purity test and a smoke test of the manual
session above as an integration test.

## Failure conditions
A handler needs behaviour a service crate does not offer → add it to that
crate only if its task is already tested **and** the change is additive;
otherwise CR.

## Documentation updates
`docs/roadmap/current.md`; `docs/contributing/dev-setup.md` if the run
command differs.

## Handoff
Paste the output of the manual session.
