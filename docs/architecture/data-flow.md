# Data flow

Concrete walkthroughs of the M1 paths. Method names are wire names from
`crates/protocol`. "shell → daemon" means a store in
`apps/desktop-flutter/lib/state/` calling the Dart daemon client
(`lib/daemon/client.dart`), which writes one protocol request to `axiomd`'s
stdin. There is no editor bridge any more: the open documents and their
editors live in the same Dart process (`DocumentsStore`,
`lib/state/documents.dart`), so the editor steps below are method calls, not
messages. (The Qt and Tauri shells exchanged the editor-bridge-1 messages of
`docs/protocols/editor-bridge.md`, retired with the Tauri shell.)

## Open a workspace

```
user: File ▸ Open Folder…            (the system folder picker, file_selector)
shell → daemon   workspace/open { path }
daemon           canonicalize, register, store.recent_touch, start nothing else
daemon → shell   { workspaceId, root, name }
shell → daemon   fs/watch { workspaceId, enabled: true }
shell → daemon   fs/list { workspaceId, path: "" }          → root entries
shell → daemon   workspace/getState { workspaceId, keys: [] } → restore tabs, expanded dirs
shell            explorer populated (WorkspaceStore); title row shows name
```
Directories are listed lazily: `fs/list` for a directory is sent when the
user first expands it. On `fs/didChange`, the tree re-lists only the
*expanded* directories that are parents of changed paths.

## Open, edit and save a file

```
user: activates a file in the tree
shell → daemon   fs/readFile { workspaceId, path }
daemon → shell   { text, version }
shell            DocumentsStore creates (or reuses) a Doc for the path: a re_editor
                 controller holding the text, the saved version; the tab shows it
user: edits      the Doc is dirty while its text differs from the saved text
user: Mod+S (the window's shortcut) — or File ▸ Save
shell → daemon   fs/writeFile { workspaceId, path, text, expectedVersion }
daemon → shell   { version }                         → the Doc's saved version and text move on
   or error conflict                                  → shell shows Overwrite / Reload / Cancel
                                                        (Cancel or any other error → the Doc stays dirty)
```
The *editor's controller owns unsaved text*; the daemon owns the file on disk;
`DocumentsStore` owns which documents are open and their saved `version`. The
highlighting language is chosen from the file name (`languageOf` in
`lib/editor/code_view.dart`, `re_highlight`'s grammars); the LSP language id
separately (`lspLanguageId`, `lib/editor/lsp/lsp_editor.dart`).

External change to an open file: `fs/didChange` arrives → if the document is
clean, the shell re-reads it and replaces the controller's text; if dirty, the
shell marks the tab "changed on disk" and the next save hits `conflict`.

## Run a command

```
user: types `cargo build` in the Run bar, presses Enter
shell → daemon   task/start { workspaceId, command: { shell: "cargo build" } }
daemon → shell   { taskId }          (or error spawn_failed → shown inline in the panel)
daemon → shell   task/output { taskId, seq, stream, data }   × N
daemon → shell   task/exited { taskId, outcome, exitCode, durationMs }
user: Stop       shell → daemon task/cancel { taskId }   → … task/exited { outcome: "cancelled" }
```
`TasksStore` (`lib/state/tasks.dart`) appends chunks in `seq` order, keeps stderr
apart for styling, and the Output view shows a final status line from
`task/exited`.

## Persisted state

| What | Where | Written when |
|------|-------|--------------|
| Theme, font size, last workspace | `settings/set` → SQLite `settings` | on change |
| Recent workspaces | daemon, on `workspace/open` | automatically |
| Open tabs, active tab, split, expanded directories, run history | `workspace/setState` → SQLite `workspace_state` (`editor.*`, `explorer.expanded`, `run.history`) | debounced 500 ms after change, and on close |
| Window geometry | not persisted by the Flutter shell: the window opens at its default size (Qt kept it in `QSettings`, Tauri in `window.json`) | — |
| Side-bar, console and dock sizes and visibility, the rail page | shell-local `shell-local.json` (`lib/app/local_prefs.dart`; in the data dir when `--data-dir`/`AXIOM_DATA_DIR` is given, else the per-user config dir) | debounced on change, flushed on quit |

## The closed loop (M5, for orientation)

```
preview/launch ─▶ adapter connects ─▶ inspect/hello (capabilities)
user clicks     ─▶ inspect/hitTest ─▶ UiNode chain with SourceCandidates
agent           ─▶ context = selection + source excerpts + screenshot + diagnostics
agent           ─▶ states assertions (Check[] it expects to hold) + proposes patch
user approves   ─▶ checkpoint ─▶ fs/writeFile ─▶ rebuild or hot reload
adapter         ─▶ inspect/didMutate ─▶ re-identify node by stableKey
daemon          ─▶ evaluate checks, screenshot diff ─▶ VerificationReport
```
Specified in `docs/protocols/inspection.md`.
