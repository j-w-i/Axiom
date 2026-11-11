# Process model

## Processes

| Process | Language | Started by | Lifetime | Talks to | If it dies |
|---------|----------|-----------|----------|----------|------------|
| `axiom` (shell) | C++/QML | user | app session | `axiomd` (stdio), editor surface (QWebChannel) | daemon sees stdin EOF, cancels tasks, exits |
| WebEngine renderer | Chromium | Qt, per profile | while a web view exists | shell only | shell shows "editor crashed — reload"; reload re-opens documents from last saved content plus the shell's unsaved-text cache (M1-SHELL-003) |
| `axiomd` (daemon) | Rust | shell (M1); may be pre-existing from M3 | while ≥1 client is connected | children below | shell shows a blocking banner with *Restart backend*; editor buffers are untouched; on restart the shell re-opens the workspace and re-reads state |
| task processes | any | daemon `exec` | until exit/cancel | daemon (pipes) | `task/exited` |
| language servers (M4) | any | daemon | per workspace + language | daemon (LSP over stdio) | restart with backoff (max 5 in 3 min), then surface an error |
| provider helpers (M2) | any | daemon | on demand, idle timeout | daemon (JSON-RPC over stdio) | operation fails with `internal`; next call restarts |
| preview targets (M5) | any | daemon | until stopped | inspection adapter → daemon | preview shows "stopped" |

## Daemon lifecycle (M1)

1. Shell resolves the daemon binary: `--daemon <path>` argument, then
   `$AXIOMD`, then a sibling of the shell executable, then `PATH`.
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

## Threads and async

- **Shell:** one GUI thread. `DaemonClient` uses `QProcess` signals on the GUI
  thread; parsing one line of JSON per message is cheap. Nothing in the shell
  calls a blocking wait on the daemon. No worker threads in M1.
- **Daemon:** Tokio multi-thread runtime. One reader task and one writer task
  per connection; each request runs in its own task so a slow request never
  blocks the connection. Blocking filesystem and SQLite calls go through
  `tokio::task::spawn_blocking`. SQLite is accessed through a single
  dedicated connection behind a mutex (state volume is tiny).
- **Editor surface:** Monaco's own web workers (editor worker, TS/JSON/CSS/HTML
  workers). Verified working under the `axiom://` scheme in E1.

## Multi-client (M3 onwards)

From M3 the daemon can also listen on a local socket (Unix domain socket in
the runtime dir with mode 0600; Windows named pipe with an owner-only ACL) so
the CLI, MCP clients and additional windows can attach. The protocol is
identical; only the transport differs (ADR-0003). Events are broadcast to all
connections that subscribed. Workspace handles are reference-counted across
connections. This is deliberately not built in M1, but M1 code must not
assume a single connection in service crates — only `axiomd`'s `main` may.

## Remote execution (M6 onwards)

`axiomd` runs on the remote host; the shell connects through an SSH-forwarded
stream. Because clients never send host paths except in `workspace/open` and
never touch the filesystem themselves, nothing else changes. This is why the
shell must not read or write workspace files directly, even though it could.
