# Static review: Flutter shell non-visual logic

Reviewer scope: `apps/desktop-flutter/lib/daemon/{client,protocol_runtime}.dart`, `lib/state/*.dart`,
`lib/app/*.dart`, `lib/main.dart`, `lib/editor/lsp/lsp_editor.dart`, `lib/editor/{languages,snippets}.dart`,
`lib/chat/models.dart`, `linux/runner/*.cc`. Read-only static review on 2026-11-22 against `main` @ 751ffc2.
Nothing was built or run. Where a finding depends on daemon behaviour, the daemon code was read too
(paths given). Line numbers are of the files as checked out.

Priority: P0 data loss / crash of a core flow, P1 significant, P2 moderate, P3 minor.
Confidence: Confirmed (code path followed end to end), Suspected (code path real, impact or timing
not proven), Unverified.

---

## F1. A daemon crash, or "Restart the Backend", silently throws away every unsaved edit

- Category: data loss / daemon restart handling
- Priority: **P0**
- Confidence: Confirmed

`lib/state/workspace.dart:96-105`
```dart
void _onDaemonState() {
  if (client.state.value is DaemonReady) { ... } else {
    // A new daemon process knows no workspaces.
    _pendingClose = 0; _deferredOpen = null;
    if (isOpen || opening) _clearOpenState();   // fires every onClosed listener
  }
}
```
`lib/state/documents.dart:365` registers `workspace.onClosed(_clearAll)`, and `_clearAll`
(`documents.dart:651-672`) does `docs.clear()` and disposes every controller, dirty or not, with no
prompt and no copy kept. `lib/app/app_state.dart:171-177` (`restartBackend`) calls
`client.restart()` with no `_guarded(...)` check, and `restart()` → `start()` sets
`DaemonStarting` (`client.dart:189`), which is "not ready" → `_clearOpenState()`. The same happens on
`DaemonFailed` from `_exited` (`client.dart:476-483`) and `_protocolFailure` (`client.dart:468-474`).
After the restart, `reopenLast()` reopens the folder and `_restoreTabs` reads the files from disk: the
tabs come back with the saved text, so the loss is not even visible.

Entry points: palette "Restart the Backend" (`lib/shell/shell.dart:561-566`, always enabled), the
"Restart the Backend" button on the "The backend stopped" page (`shell.dart:1412`), and any crash,
OOM kill or protocol failure (a line > 64 MiB, a non-JSON line) of `axiomd`.

Repro (GUI):
1. Open a folder, open `README.md`, type some text (tab shows the modified mark). Do not save.
2. Ctrl+K → "Restart the Backend" → Enter. No "unsaved changes" question appears.
3. The folder reopens; `README.md` shows the on-disk text; the typed text is gone.
4. Variant: instead of step 2, in a terminal run `kill -9 $(pgrep -P $(pgrep -x axiom) axiomd)`;
   the editor disappears behind "The backend stopped"; click Restart → same loss.

Proposed fix:
- Keep documents across a daemon loss: give `WorkspaceStore` a distinct "daemon lost" event (or a
  reason argument on `onClosed`) and have `DocumentsStore` retain its `docs` (marked detached,
  read-only while detached) instead of `_clearAll()`. When the same root reopens, re-attach by path and
  run `_recheck` per document (the version decides between reload and "changed on disk").
- Guard `restartBackend()` with `documents._guarded('restart', dirtyPaths, ...)` (Save / Don't Save /
  Cancel) as folder switch and quit are.
- Until then, at minimum do not dispose dirty documents on a daemon loss.

Verification: an integration test in `integration_test/editor_test.dart`: open a folder, edit a file,
`app.restartBackend()`, expect a prompt (or, with the retention fix, the doc still dirty with the
typed text after reopen). A second case kills the child (`Process.killPid(app.client.pid!)`) and
expects the dirty text to survive the Restart button.

---

## F2. Opening another folder while one is open keeps the old folder's documents, LSP sync and diagnostics

- Category: stale state applied to a new workspace / data integrity
- Priority: **P1**
- Confidence: Confirmed

`lib/state/workspace.dart:146-156` (the switch branch of `openFolder`):
```dart
if (isOpen) {
  final oldId = id;
  await flushState();
  _resetTree('');
  id = '';
  try { await client.call(Methods.workspaceClose, WorkspaceRef(workspaceId: oldId)); } ...
}
```
Unlike `close()` → `_clearOpenState()`, this branch never runs the `_closed` callbacks. Only two stores
use them: `DocumentsStore` (`documents.dart:365`, `_clearAll`) and `LspStore` (`lsp.dart:96`, `_clear`).
`requestOpenFolder` (`documents.dart:839-840`) → `workspace.openFolder(path)` takes exactly this
branch; it is reached from Open Folder… (Mod+O, `shell.dart:178-181,249-253`) and every
"Open Recent: …" palette command (`shell.dart:262-268`) while a folder is open.

Consequences, all following from the documents and LSP state surviving the switch:
- Folder A's `Doc`s stay in `docs` with A's text and A's `savedVersion`. `_restoreTabs` for B skips any
  path already open (`_openInternal`: `doc(path) != null` return, `documents.dart:460`), so a file that
  exists in both (`README.md`, `Cargo.toml`, `src/main.rs`, `package.json`) keeps showing **A's
  content under B's name**. `_generation` is not bumped, so an `_openInternal` of A in flight also lands
  in B.
- Saving such a tab sends `fs/writeFile(workspaceId: B, path, A's text, expectedVersion: A's
  version)`; the daemon answers `conflict` (`crates/workspace/src/workspace.rs:361-369`), the
  conflict bar offers Overwrite, and Overwrite writes A's text over B's file.
- With "Don't Save" at the switch prompt, `resolvePrompt('discard')` only sets `dirty = false`
  (`documents.dart:831-835`): the unsaved text stays open in B, marked clean.
- The debounced tab save (`_scheduleTabSave`, 500 ms) and every later one write A's paths into B's
  `editor.openPaths` (`documents.dart:852-875`).
- `LspSync._open` keeps A's documents: their `didChange`/`didSave` go to workspace B with B-root URIs
  (`lsp_editor.dart:117-135`, `lsp.dart:129-132`) for documents B's servers never opened.
- `LspStore.diagnostics` keeps A's entries keyed by server id and relative path; B's Problems and
  underlines show A's diagnostics until those servers publish again (`lsp.dart:135-144` never runs).
- If opening B fails (folder gone), A is closed in the daemon but its documents stay open with
  `workspace.id == ''`; saving fails with a banner.

Repro (GUI):
1. Make two folders, both with a `README.md` of different content ("A" and "B").
2. Open folder A, open `README.md`.
3. Mod+O → choose folder B (or Ctrl+K → "Open Recent: B").
4. Observe the `README.md` tab still shows "A". Problems still lists A's diagnostics (use a Rust file
   with an error in A to make this visible).
5. Type a character in that tab, Ctrl+S → "changed on disk" bar → Overwrite → B's `README.md` on
   disk now contains A's text.

Proposed fix: in the switch branch call the same teardown as `close()` (fire `_closed` after
`_resetTree('')`/`id = ''`, i.e. reuse `_clearOpenState()` minus the generation reset that would
abort this open), so documents, LSP sync and diagnostics are cleared before B's `_opened` runs.

Verification: integration test: open A, open `README.md`, `requestOpenFolder(B)`; expect
`documents.docs` to contain only B's restored tabs, `doc('README.md')!.text` equal to B's content,
and `lsp.diagnostics` empty until B publishes.

---

## F3. A file with mixed line breaks or a stray CR is marked modified without an edit, and saving rewrites it

- Category: CRLF / encoding, silent content change
- Priority: **P2**
- Confidence: Confirmed

`lib/state/documents.dart:93-97`: the document's line break is the *first* one in the file.
re_editor splits on every CRLF, CR and LF (`third_party/re_editor/lib/src/code_line.dart:1175`:
`replaceAll('\r\n', '\n').replaceAll('\r', '\n').split('\n')`). `Doc.text` joins all lines with the
one detected break. `refreshDirty` (`documents.dart:321-332`) compares that join with `_savedText`
(the bytes as read) on the first controller notification, which a caret move or focus produces:
```dart
final now = (length != null && length != _savedText.length) || controller.text != _savedText;
```
So for `a\r\nb\nc\r\n` (CRLF first, one LF line) or an LF file containing a bare `\r` inside a line,
the tab turns "modified" as soon as it is clicked; closing it asks Save / Don't Save; Save (or
Save All, or Save at quit) rewrites every break to the first kind and turns the stray CR into a line
break. The LSP mirror is consistent with the editor (both see the split), so only the file changes.
Requirement E-3 (line endings preserved) is met only for uniform files.

Repro (GUI):
1. In a folder: `printf 'one\r\ntwo\nthree\r\n' > mixed.txt` and `printf 'x = "a\rb"\n' > cr.py`.
2. Open `mixed.txt`, click in the text: the tab shows the modified mark with no edit.
3. Close the tab: the unsaved-changes prompt appears. Choose Save; `od -c mixed.txt` shows `\r\n` after
   `two` now.

Proposed fix: keep `dirty` false until an edit: set `_dirtyLines = controller.codeLines` and compute
the normalised baseline (`_savedText = controller.text` after construction) for comparison, while
keeping the original bytes for a "line endings differ" status note; or detect the mixed/stray-CR case
at open and show it (as VS Code does) rather than silently normalising on save.

Verification: unit test next to `test/state/doc_line_break_test.dart`: `Doc(text: 'a\r\nb\nc')`,
move the selection, expect `dirty == false`; a CR-inside-line case likewise.

---

## F4. Two saves of one document in flight race on `expectedVersion`: a spurious "changed on disk" for your own save

- Category: race / save path
- Priority: **P2**
- Confidence: Confirmed (code); timing window depends on format-on-save latency

`lib/state/documents.dart:679-725`: `save()` has no per-document in-flight guard. Each call awaits
`beforeSave` (format on save, up to 2 s, `lsp_editor.dart:154-177`) and then sends
`expectedVersion: d.savedVersion`; `savedVersion` is only updated when a write's response arrives
(`markSaved`, line 711). A second Ctrl+S while the first is formatting or writing sends the *same*
old version; the daemon compares it with the file the first save just wrote
(`crates/workspace/src/workspace.rs:361-369`) and answers `conflict`. The conflict bar
(`resolveConflict`) then offers Overwrite / Reload; Reload (`_reloadFromDisk` → `replaceText`)
replaces the editor text with the first save's text, discarding anything typed between the two
presses.

Repro (GUI): Settings → enable Format on save; open a Rust file in a folder with rust-analyzer ready;
type, then press Ctrl+S twice quickly (or hold Ctrl+S). The "changed on disk" bar appears although
nothing else touched the file. Type one more character between the two presses and choose Reload: the
character is gone.

Proposed fix: serialise saves per `Doc` (a `Future<void>? saving` on the doc; a save while one is in
flight waits for it, then writes with the updated `savedVersion`, or is coalesced into one trailing
save). `saveAll` and the prompt's save path should use the same queue.

Verification: integration test with format-on-save on a fake server that delays formatting 300 ms:
call `save(path)` twice without awaiting; expect no entry in the conflict queue
(`conflictName == null`) and the file holding the latest text.

---

## F5. After a daemon crash or restart mid-reply, the open conversation stays "writing" for good

- Category: daemon restart handling / stale state
- Priority: **P2**
- Confidence: Confirmed

`lib/state/agent.dart`: `Conversation.running` is set true on send (`_patch(id, running: true)`,
line 519) and cleared only by a `turnEnd` event (lines 872-875) or a failed `agentSend` call. Nothing
resets it when the daemon goes away: `AgentStore` does not listen to `client.state`, and
`_onWorkspace` (lines 368-378) only refreshes the list. The cached conversation (`_cache`) keeps
`running: true`; the Chat page renders Stop instead of Send and disables image attach, Edit and
Regenerate while `active.running` (`lib/chat/chat_page.dart:419-420,445,565,732-735`). `stop()` sends
`agent/cancel` to the new daemon, whose error is swallowed (line 551), so the state never clears until
the person opens another conversation and comes back (`open()` takes `running` from the daemon).

Repro (GUI): start a long answer in Chat; while it streams, Ctrl+K → "Restart the Backend" (or kill
`axiomd`, then Restart). The conversation still shows the running indicator and Stop; pressing Stop
or Esc does nothing; Send is unavailable.

Proposed fix: listen to `client.state` in `AgentStore`; on a non-ready state mark every cached
conversation `running = false`, clear `streaming` and `_pendingDelta`, and drop `approvals`; on the
next ready state re-`open(activeId)` to resync from the daemon.

Verification: unit test with a fake `DaemonClient` state flip: `running` true → state `DaemonFailed`
→ expect `active.running == false`.

---

## F6. Task output without newlines grows one string without bound, re-split on every chunk

- Category: unbounded buffer / O(n²) on the UI isolate
- Priority: **P2**
- Confidence: Confirmed (algorithm); impact depends on the command

`lib/state/tasks.dart:83-91`
```dart
void _apply(StreamKind stream, String data) {
  final text = _partial[stream]! + stripAnsi(data);
  final parts = text.split('\n');
  _partial[stream] = parts.removeLast();
  ...
```
The 10 000-line cap (`maxLines`) bounds rows, not a line. A command that writes a long line (a minified
bundle, a single-line JSON response, `\r`-only progress, rspec/pytest dot rows) keeps
`_partial` growing; every `task/output` chunk copies and re-splits the whole partial on the UI isolate,
which is quadratic in the line length, and the final row holds the whole string.

Repro (GUI): open a folder; in the run prompt run `head -c 30000000 /dev/zero | tr '\0' 'a'` (or
`cat` a large minified `.js`). The UI stalls for seconds while the output arrives; memory grows by
hundreds of MB.

Proposed fix: in `_apply`, split only the new data and append to the partial with a `StringBuffer`;
collapse `\r` progress immediately (keep text after the last `\r`); cap a partial line (e.g. 64 KiB)
by emitting it as a row with a "line truncated" marker.

Verification: unit test on `OutputBuffer`: append 2 000 chunks of 64 KiB without `\n`; expect bounded
time and `take()` rows with a bounded length.

---

## F7. Every keystroke does several whole-document passes on the UI isolate; full-sync servers get the whole text per key

- Category: per-keystroke O(n) work
- Priority: **P2**
- Confidence: Suspected (cost not measured here)

Per edit of a document with a server:
- `LspSync` `onChange` (`lib/editor/lsp/lsp_editor.dart:117-135`): `_lines(now)` builds a list of every
  line's text, `contentChange` scans from both ends, and for a server with `textDocumentSync: 1`
  `after.join('\n')` builds the whole text, which `client.call` then `jsonEncode`s and UTF-8 encodes
  on the UI isolate (`client.dart:153`).
- `Doc._followLines` (`documents.dart:138-166`) compares lines from the top when the line count
  changes and rebuilds the semantic-token map.
- `Doc.refreshDirty` (`documents.dart:321-332`) walks every line (`textLength`) and, when the length
  is unchanged (replacing a character, a paste of equal length), joins and compares the whole text.

For a multi-megabyte file this is several MB of allocation per key, and with a full-sync server a
JSON-encoded copy of the file per key, plus the same in the daemon.

Repro (GUI): open a ~5 MB source file (e.g. a generated `.json`/`.yaml` handled by a full-sync server,
or a large `.rs`); hold a letter key; watch frame times in `scripts/frames.sh` / DevTools for
>16 ms frames attributable to `LspSync`/`refreshDirty`.

Proposed fix: compute the change from re_editor's edit (the controller's previous and new
`CodeLines` share unchanged segments; diff only touched segments) instead of materialising all lines;
for full-sync servers debounce `didChange` (e.g. one per frame) since the text supersedes; make
`refreshDirty` track an edit counter / undo-stack position and compare only when it returns to the
saved position.

Verification: a frame benchmark case in `integration_test/editor_frames_test.dart` typing into a
5 MB document with a fake full-sync server; budget per-frame build time.

---

## F8. After a restart, the old daemon's notifications are still dispatched, and ids are reused

- Category: race between daemon generations
- Priority: **P3**
- Confidence: Confirmed (code); impact Suspected (timing)

`lib/daemon/client.dart:420-440` dispatches every notification read from *any* connection to the
global `_listeners`; `_pump` keeps running for the old `_Connection` after `restart()` (lines 525-543:
it closes stdin and waits up to 2 s for exit). The daemon numbers ids from a counter per process
(`crates/workspace/src/registry.rs:57` `w{n}`, `crates/exec/src/executor.rs:150` `t{n}`,
`crates/term/src/lib.rs:359` `t{n}`), so the new daemon's first workspace is again `w1`. Notifications
the old daemon emits while shutting down (e.g. `lsp/serverState` stopped, `fs/didChange`,
`task/exited`) pass the stores' `workspaceId == workspace.id` checks for the new `w1`, e.g.
`LspStore._onServerState` replaces the new server's state and drops its diagnostics.

Repro (GUI): open a Rust folder, wait for rust-analyzer "ready"; "Restart the Backend"; on fast
machines the folder reopens within the old daemon's 2 s exit window; the status bar may show the
server as stopped although the new one runs (intermittent).

Proposed fix: in `_message`, ignore notifications when `!identical(conn, _conn)` or `conn.stopping`.

Verification: client unit test with a fake process: restart, then emit a notification on the old
process's stdout; expect no listener call.

---

## F9. Terminal keystrokes are sent as independent requests that the daemon may apply out of order

- Category: ordering
- Priority: **P3**
- Confidence: Suspected

`lib/state/terminals.dart:154-158` sends one `terminal/write` per xterm `onOutput` call, fire and
forget. The daemon runs every request in its own task (`crates/rpc/src/server.rs:359-360`) and the
write itself in `spawn_blocking` (`crates/axiomd/src/handlers/terminal.rs:34-41`), so two writes can
reach the PTY in either order. The same problem was fixed for `lsp/notify` with a per-server queue
(`lsp.dart:176-215`, "audit finding 1"), but not here. A full PTY input buffer (a foreground program
not reading stdin while the person types ahead) or a loaded machine widens the window.

Repro (GUI, intermittent): in a terminal run `sleep 5`, then type `echo abcdefghijklmnop` quickly
with key repeat or under CPU load (`stress -c $(nproc)`); after `sleep` returns, compare what the
shell received with what was typed.

Proposed fix: a per-terminal write queue in `TerminalsStore` (await the previous write; coalesce data
queued meanwhile into one write), as `LspStore.notify` does.

Verification: unit test with a fake client whose `terminal/write` answers in reverse order: expect
the second write to be sent only after the first answered.

---

## F10. First open of a conversation drops events that arrive before `agent/sessionOpen` answers

- Category: race between notification and response
- Priority: **P3**
- Confidence: Suspected

`lib/state/agent.dart:461-487`: `open(id)` sets `active = _cache[id] ?? Conversation()` but does not
put that conversation in `_cache`; `_onEvent` (lines 851-854) drops any event whose session is not
in `_cache`. Events the daemon emits after taking its snapshot and before the response line are on
the wire before the response and are dropped; the merge of `extra` (lines 471-476) only works when
the conversation was already cached. A dropped `toolResult` leaves a tool row spinning; a dropped
`turnEnd` leaves the conversation "running" (see F5).

Repro (GUI, intermittent): start an agent task that runs many tools in conversation X; switch to
another conversation; reopen X while tools are running; occasionally a tool row stays "running"
after the reply finished. Reopening again fixes it.

Proposed fix: before the call, `_cache.putIfAbsent(id, Conversation.new)` so events are buffered,
then merge buffered events with `seq > last` as the code already does for `prev`.

Verification: unit test: start `open(id)` with a fake client, deliver an `agent/event` (seq above
the snapshot) before completing the response; expect the event in `active.events`.

---

## F11. Two overlapping flushes of `shell-local.json` can leave it corrupt, and a corrupt file silently resets the layout

- Category: persistence atomicity
- Priority: **P3**
- Confidence: Suspected (narrow window)

`lib/app/local_prefs.dart:40-61`: the debounce timer calls `flush()` without awaiting; quit calls
`flush()` again (`app_state.dart:180-185`). `_save?.cancel()` cancels a pending timer, not a flush in
progress. Both write the same `'$path.tmp'` with truncate-and-write; if the later JSON is shorter, the
file can be the later text plus the earlier one's tail, which the first `rename` then installs.
`load()` (lines 17-28) swallows the parse error and starts from defaults, so splitter sizes, the open
destination and panel visibility are lost without a word.

Repro: hard to hit by hand (drag a splitter, release, quit within ~300 ms + write time). Simulate in a
unit test (see below).

Proposed fix: serialise flushes (chain on a `Future? _writing`, re-flush if values changed meanwhile)
and use a unique temp name per write; optionally keep `shell-local.json.bak` and fall back to it on a
parse error.

Verification: unit test: call `flush()` twice without awaiting with different-length values; reload;
expect valid JSON equal to the later values.

---

## F12. "The backend exited unexpectedly" can lack the daemon's last stderr lines (its panic)

- Category: error reporting
- Priority: **P3**
- Confidence: Suspected

`lib/daemon/client.dart:207-213,476-483`: `_exited` builds the message from `stderrTail` as soon as
`process.exitCode` completes; Dart may complete `exitCode` before the stderr stream has delivered its
last chunks, so the panic text (`thread 'main' panicked at …`) is often not yet in the tail. Likewise
responses still unread on stdout are failed as "Backend stopped" by `_failPending`.

Repro (GUI): start with `--daemon` pointing at a wrapper script that prints a line to stderr and
exits 101 immediately after `initialize`; the "backend stopped" page shows only the exit code on some
runs.

Proposed fix: in `_exited`, await the stderr subscription's done (and stdout's, then the pump) with a
short bound (e.g. 500 ms) before failing pending requests and composing the message.

Verification: client test with a fake daemon that writes a stderr line and exits; expect the line in
`DaemonFailed.message`.

---

## F13. A result that does not decode escapes as `TypeError`/`FormatException`; callers only catch `DaemonException`

- Category: error handling
- Priority: **P3**
- Confidence: Confirmed (code); needs a daemon/shell protocol mismatch to trigger

`lib/daemon/client.dart:264-282`: `call()` returns `method.decodeResult(json)`; the generated decoders
cast (`m['text'] as String`, `protocol.g.dart:3450-3453`) and throw `TypeError`. Nearly every store
catches only `on DaemonException`, so a mismatched answer leaves states stuck, e.g.
`WorkspaceStore.openFolder` keeps `opening = true` (`workspace.dart:158-169`) and
`TasksStore._start` keeps `running = true` (`tasks.dart:361-393`), with the error only in the log.
This matters when a shell runs against a newer/older `axiomd` (the bridge's version check covers the
protocol version, not every field).

Repro: run the shell against a daemon build whose `fs/list` entry lacks a required field; the
explorer spinner never ends.

Proposed fix: wrap `decodeResult` in `call()` and rethrow as `DaemonException(ErrorKind.internal,
"The backend's answer to <method> was not understood")`.

Verification: client test with a fake daemon returning `{}` for a method with required fields; expect
`DaemonException`.

---

## F14. Session-long caches that are never pruned

- Category: unbounded growth
- Priority: **P3**
- Confidence: Confirmed

- `AgentStore._cache` (`agent.dart:360`) keeps the full event list of every conversation opened in the
  session; `images` (`agent.dart:317`) keeps every decoded image's data. Neither is evicted (only
  `remove()` of a conversation drops its entry).
- `TerminalsStore._early` (`terminals.dart:55,73-77`) buffers output for any terminal id without a tab
  and drops it only when a tab for that id is made or the workspace changes; output for a closed tab
  (`close()` removes the tab first, trailing output follows) accumulates.
- `ProblemsStore._byTask` (`problems.dart:14-18`) keeps `build/diagnostics` for task ids that never
  went through `begin()` until the next provider run.

Repro: open many long conversations with images in one session; RSS of the shell process grows and
does not return.

Proposed fix: LRU-bound `_cache` (keep the active and recently used few), evict `images` with their
conversation; drop `_early` entries for ids not created within a few seconds, and ignore output for ids
just closed; prune `_byTask` on workspace change and for ids not in `_runs`.

Verification: unit tests per store asserting the bound.

---

## F15. Problems is rebuilt and sorted three times per console rebuild, and the console rebuilds every frame a task prints

- Category: per-notification O(n log n) on the UI isolate / rebuild storm
- Priority: **P3**
- Confidence: Suspected (cost not measured)

`lib/state/problems.dart:60-78`: `count(s)` calls `all()`, which concatenates build diagnostics with
`lsp.problems()` (allocating a `Diagnostic` per LSP diagnostic) and sorts the lot. The console header
calls `count` twice and the Problems list calls `all()` (`lib/console/console.dart:30-35,123`), in a
`ListenableBuilder` over `[prefs, problems, tasks, terminals]`; `TasksStore` notifies once per frame
while a task prints (`tasks.dart:201-232`). With thousands of diagnostics (a large Rust workspace)
and a chatty build, that is three full rebuild-and-sorts per frame. Related: `WorkspaceStore` notifies
for every tree re-list (`workspace.dart:313-316`), and `shell.dart:695` rebuilds the whole main row
(rail, side bar, editor, console) on any workspace or prefs notification, including every splitter
drag step.

Repro (GUI): a Rust folder with ~2 000 warnings visible in Problems; run `cargo build -vv` from the run
prompt; watch frame build times.

Proposed fix: cache the sorted list and per-severity counts in `ProblemsStore`, invalidated on
`diagnosticsVersion`/build changes; give the tree its own `ValueListenable` (`treeVersion`) so open/close
listeners do not rebuild on re-lists.

Verification: unit test that `all()` is computed once per change (counter); a frames-suite case with
a large Problems list and a printing task.

---

## F16. Switching folders flips Chat to General scope

- Category: transient state notification
- Priority: **P3**
- Confidence: Confirmed

`lib/state/workspace.dart:171-173` notifies (via `_resetTree(r.workspaceId)` → `_bump()`) while
`id` is still `''`; `AgentStore._onWorkspace` (`agent.dart:368-378`) sees "no folder" and calls
`newChat(Scope.general)`. After the switch the Chat page is in General scope with a new empty chat;
the dock puts itself back to workspace scope only when it is shown (`chat_page.dart:309`).

Repro (GUI): open folder A, Chat → scope "This folder", open a conversation; Mod+O → folder B; Chat
shows General and an empty chat.

Proposed fix: set `id`/`root`/`name` before `_resetTree(r.workspaceId)` notifies (or notify once after
both), so listeners never observe the transient closed state during a switch; or let `AgentStore`
keep workspace scope across a switch.

Verification: unit test: scope workspace, switch folders, expect `scope == Scope.workspace`.

---

## F17. Search results of a cancelled request can be applied after the folder changed

- Category: stale response
- Priority: **P3**
- Confidence: Suspected

`lib/state/search.dart:252-264` cancels the token on a workspace change but does not bump `_seq`;
`_run` applies a response when `seq == _seq` (line 323). If the daemon answers the old search before
it sees `$/cancel`, A's matches are shown in B (and a click opens a path that may not exist there).

Repro (GUI, intermittent): in a large folder start a slow regex search; immediately Mod+O to another
folder; occasionally the old results appear.

Proposed fix: `_seq++` in `_onWorkspace`, and check the workspace id captured at request time.

Verification: unit test with a fake client completing the old request after the workspace change.

---

## Areas reviewed with no finding

- `client.dart` lifecycle: initialize timeout, shutdown sequence (bounded 3 s + 2 s + 1 s), per-request
  timeout with `$/cancel`, line framing and the 64 MiB bound, off-isolate decode of large lines keeping
  order, response-before-later-notification ordering in `_pump`. `CancelToken` listeners are per
  request (no leak).
- LSP document sync ordering (`lsp_editor.dart`, `lsp.dart`): versions increase monotonically per
  open; `didOpen` → `didChange` → `didSave` → `didClose` go through the per-server queue; language
  change closes and reopens; the daemon mirrors documents even when the server is not ready
  (`crates/lsp/src/server.rs:777-817`) and drops stale `didChange`s. `contentChange` ranges checked by
  hand on insert/delete/last-line cases. UTF-16 offsets match LSP's default.
- Quit path (`main.dart`): window close reaches `onExitRequested` through the Linux embedder
  (`fl_view.cc` → `fl_engine_request_app_exit`); dirty documents prompt; a failed save or conflict
  aborts the quit; `_exit` in `main.cc` happens after the Dart shutdown completed.
- Tasks: seq reordering of output, early events before `task/start` answers, the 10 000-row cap,
  per-frame publishing, reset on daemon loss.
- Git, Inspector, Environments, ServerInstalls, Settings stores: generation/sequence checks on
  workspace change are present; gutter diff runs on an isolate for large inputs.
- `snippets.dart` (nesting, transforms, choices, escapes; terminates on malformed input) and
  `languages.dart` (detection order; built once).
- `chat/models.dart`: async handlers check `mounted`; the key field is cleared before the call.
- Linux runner: `gtk_widget_queue_draw` hook is called on the GTK thread (the embedder's
  `redraw_cb` via `g_idle_add`, `fl_view_renderer_subsurface.cc:197-219,430`, engine
  4d35714800b9 as pinned), so the direct `wl_surface_commit` does not race GTK; the size/scale guard
  (`painted_as_now`) falls back to GTK after any configure that changes size or scale; per-window
  `Painted` data is freed with the `GdkWindow` and the `after-paint` handler is connected with
  `g_signal_connect_object`; on X11/XWayland `GDK_IS_WAYLAND_WINDOW` is false and the call goes to GTK.
  Residual risk (not a finding): a commit outside GTK's cycle also commits any other pending surface
  state GTK set without a size change (e.g. opaque/input region); no such path was found in GTK 3, not
  verified at runtime. Build note: `wayland-client` is `REQUIRED` even for X11-only build hosts.

## Not reviewed

- Widgets and visual code (another reviewer), including how editor views consume `Doc` and the
  console's row rendering of very long lines.
- `protocol.g.dart` contents beyond how results are decoded.
- Runtime behaviour: nothing was built, run or measured; all costs in F6, F7, F15 are by reading.
- macOS/Windows runners.
