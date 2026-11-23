# Process model

## Processes

| Process | Language | Started by | Lifetime | Talks to | If it dies |
|---------|----------|-----------|----------|----------|------------|
| `axiom` (shell) | Dart (Flutter, `apps/desktop-flutter`) | user | app session | `axiomd` (stdio, via `lib/daemon/client.dart`) | daemon sees stdin EOF, cancels tasks, exits; unsaved text is lost with the process (there is no separate renderer process to survive it) |
| `axiom` (Tauri shell, retired) | Rust host (Tauri) + React webview | user | app session | `axiomd` (stdio, via `crates/desktop-bridge`, removed with AUD-102), its webview processes | until FL-QA-001 (tag `tauri-shell-final`); its webview-crash recovery (T-08) has no Flutter counterpart because there is no webview |
| `axiom` (Qt shell, retired) | C++/QML | user | app session | `axiomd` (stdio), Qt WebEngine renderer (QWebChannel) | until task T-15 (tag `qt-shell-final`) |
| `axiomd` (daemon) | Rust | shell (M1); may be pre-existing from M3 | while ≥1 client is connected | children below | shell shows a blocking banner with *Restart backend*; editor buffers are untouched; on restart the shell re-opens the workspace and re-reads state |
| task processes | any | daemon `exec` | until exit/cancel | daemon (pipes) | `task/exited` |
| language servers (M4) | any | daemon | per workspace + language | daemon (LSP over stdio) | restart with backoff (max 5 in 3 min), then surface an error |
| provider helpers (M2) | any | daemon | on demand, idle timeout | daemon (JSON-RPC over stdio) | operation fails with `internal`; next call restarts |
| preview targets (M5) | any | daemon | until stopped | inspection adapter → daemon | preview shows "stopped" |

## Daemon lifecycle (M1)

1. Shell resolves the daemon binary: `--daemon <path>` argument, then
   `$AXIOMD`, then a sibling of the shell executable, then `PATH`
   (`locateDaemon` in `apps/desktop-flutter/lib/daemon/client.dart`, which is the
   one specification of this order). The packages put `axiomd`
   beside the shell (`scripts/bundle.sh`: `/usr/lib/axiom/` on Linux,
   `Axiom.app/Contents/MacOS/` on macOS).
2. Shell spawns it with piped stdin/stdout. **stdout carries protocol
   messages only.** stderr is free-form log text which the shell appends to
   its own log; nothing parses it.
3. Shell sends `initialize`. If no response arrives within 10 s, or the major
   protocol version differs, the shell shows a fatal error dialog with the
   daemon's stderr tail.
4. Normal operation.
5. Shutdown: shell sends `shutdown`, waits up to 3 s for the response, closes
   stdin, waits up to 2 s, then kills. The daemon also treats stdin EOF as
   `shutdown`.

The daemon holds no state that is not either persisted in SQLite before the
response to the mutating request (settings, workspace state) or trivially
re-creatable (open workspace handles, watcher). Running tasks are lost on
daemon exit by design: they are killed.

**As built (audit AUD-027, AUD-007):**
- **Shutdown** cancels every task (bounded: a task whose own process has exited is reported
  after a 2 s drain of its pipes even if a background descendant still holds them; that
  descendant is killed with the task's process group), stops the language servers, the external
  agents (ACP) and MCP servers, the change watches, and closes the terminals *waiting* for their
  hang-up → SIGKILL escalation (2 s) before the process exits.
- **A killed or crashed daemon** (SIGKILL, OOM): `axiomd` starts a small watchdog, `axiomd
  --watchdog`, in its own process group with a pipe from the daemon as its stdin, and registers
  the process group of every task, terminal, language server, external agent (ACP) and MCP
  server it starts: each is started as the leader of a group of its own. When the pipe reaches
  end of file (the daemon is gone, however it ended) the watchdog sends SIGHUP and SIGTERM to
  every group still registered, SIGKILL a second later, and exits. Unix; Windows tasks are in
  kill-on-close job objects.
  - `axiom-lsp` and `axiom-agent` do not depend on `axiom-exec`: the daemon passes them the
    watchdog's register and unregister functions at startup.
  - A group is unregistered when its leader has ended, so the watchdog does not signal a
    number that a later process may have reused. A terminal's group is unregistered once its
    process has been reaped, also when the `terminal/exited` relay lagged and dropped the
    event. Registrations are counted per owner: one owner's unregister does not drop
    another's (review D5).
  - Registering never blocks the daemon: the lines go through a channel to a thread of their
    own, which writes the pipe. `terminal/create` and a task's start answer only once their
    registration is in the pipe (`guard::flushed`, at most 1 s, off the async threads), so a
    daemon killed right after the answer still takes the process along.
  - Stopping an external agent or an MCP server (session deleted, idle reap, MCP restart,
    daemon shutdown) SIGKILLs its whole process group while the leader is still unreaped, then
    reaps the leader and unregisters the group. One whose leader exited by itself stays
    registered while its group still has members, and the group is killed with its handle
    (review N13, D2). A process that leaves the group (`setsid`, a daemonising launcher) is
    not covered.

## Threads and async

- **Shell:** one Dart isolate on the UI thread; the daemon client reads the
  child's stdout asynchronously and delivers responses and notifications in
  wire order. Nothing in the UI calls a blocking wait on the daemon. (Tauri
  shell, retired: the host ran `crates/desktop-bridge` on a Tokio runtime and
  forwarded ordered events to the webview. Qt shell: one GUI thread,
  `DaemonClient` on `QProcess` signals.)
- **Daemon:** Tokio multi-thread runtime. One reader task and one writer task
  per connection; each request runs in its own task so a slow request never
  blocks the connection. Blocking filesystem and SQLite calls go through
  `tokio::task::spawn_blocking`. SQLite is accessed through a single
  dedicated connection behind a mutex (state volume is tiny).
- **Editor surface:** `re_editor` lays out and paints in the shell's UI
  thread; there are no editor workers (Monaco's web workers went with the
  Tauri shell).

## Multi-client (M3 onwards)

From M3 the daemon can also listen on a local socket (Unix domain socket in
the runtime dir with mode 0600; Windows named pipe with an owner-only ACL) so
the CLI, MCP clients and additional windows can attach. The protocol is
identical; only the transport differs (ADR-0003). Events are broadcast to all
connections that subscribed. Workspace handles are reference-counted across
connections (as built, AUD-029: each connection that opened a workspace holds it; its
`workspace/close` releases only that connection, and the workspace's tasks, servers, terminals
and watches stop when the last holder closes it or disconnects). This is deliberately not built in M1, but M1 code must not
assume a single connection in service crates — only `axiomd`'s `main` may.

**As built (M3-DAEMON-001, Unix):**
- **Where.** The socket is `<data dir>/run/axiomd.sock` (0600) in `<data dir>/run/` (0700),
  with the token in `run/axiomd.token` (0600), regenerated at every start. Using the data dir
  rather than `$XDG_RUNTIME_DIR` ties the socket to the store it serves.
- **Who listens.** The daemon started by the desktop app listens unless `--no-listen`. A
  second daemon on the same data dir finds the live socket and does not listen. A stale
  socket file (no listener) is replaced. The owner removes both files when it exits.
- **What a socket connection shares and does not.** Socket connections share the workspace
  registry, executor, sessions and gate. They do not run the daemon's shutdown hooks when they
  end (`ServeOptions.run_hooks = false`). The daemon ends with its stdio client.
- **Who listens, exactly.** The daemon holding an exclusive lock on `run/axiomd.lock` (for
  its lifetime; the OS releases it when the process ends) is the only one that may listen; the
  token is written after the bind, to a temporary file renamed into place (AUD-095). A failing
  `accept` (descriptor exhaustion) backs off from 50 ms to 1 s instead of spinning (AUD-094).
- **Per connection.** `fs/watch` is per connection: each one that enables it gets every batch
  (AUD-028). Daemon-wide relays (terminal output, `git/didChange`, previews, inspection) send to
  each connection without waiting on a slow one: a connection whose queue stays full for 500 ms
  misses notifications until it reads again (AUD-030); terminal output it missed shows as a
  `seq` gap, and `terminal/list` redraws it.
- **Closed clients.** Each connection's notifications go only to it. Session events go to
  attached connections; approvals go to connections that declared `capabilities.approvals`.
  `axiom_rpc::Client` now fails a request sent after its connection closed instead of waiting.
- **The CLI.** `axiom` uses the socket when a daemon listens, else it starts `axiomd
  --no-listen` over stdio for the command.
- **Windows (deferred):** no listener; every client starts its own daemon.

## Remote execution (M6 onwards)

`axiomd` runs on the remote host; the shell connects through an SSH-forwarded
stream. Because clients never send host paths except in `workspace/open` and
never touch the filesystem themselves, nothing else changes. This is why the
shell must not read or write workspace files directly, even though it could.
