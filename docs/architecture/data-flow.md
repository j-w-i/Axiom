# Data flow

Concrete walkthroughs of the M1 paths. Method names are wire names from
`crates/protocol`; bridge message names are from
`docs/protocols/editor-bridge.md`.

## Open a workspace

```
user: File ▸ Open Folder…            (QML FolderDialog)
shell → daemon   workspace/open { path }
daemon           canonicalize, register, store.recent_touch, start nothing else
daemon → shell   { workspaceId, root, name }
shell → daemon   fs/watch { workspaceId, enabled: true }
shell → daemon   fs/list { workspaceId, path: "" }          → root entries
shell → daemon   workspace/getState { workspaceId, keys: [] } → restore tabs, expanded dirs
shell            FileTreeModel populated; title bar shows name
```
Directories are listed lazily: `fs/list` for a directory is sent when the
user first expands it. On `fs/didChange`, the tree re-lists only the
*expanded* directories that are parents of changed paths.

## Open, edit and save a file

```
user: activates a file in the tree
shell → daemon   fs/readFile { workspaceId, path }
daemon → shell   { text, version }
shell → editor   openDocument { path, text, version, activate: true }
editor           creates (or reuses) a Monaco model for the path, shows it
editor → shell   dirtyChanged { path, dirty: true }              (on first edit)
user: Ctrl/Cmd+S (handled inside Monaco)  — or File ▸ Save (shell → editor requestSave)
editor → shell   saveRequested { path, text }
shell → daemon   fs/writeFile { workspaceId, path, text, expectedVersion }
daemon → shell   { version }                         → shell → editor documentSaved { path, version }
   or error conflict                                  → shell shows Overwrite / Reload / Cancel
```
The *editor surface owns unsaved text*; the daemon owns the file on disk; the
shell owns which documents are open and their saved `version`. Language id is
chosen by the editor surface from the file extension (Monaco's registry).

External change to an open file: `fs/didChange` arrives → if the document is
clean, the shell re-reads and sends `openDocument` again (editor replaces the
model content, preserving the view state); if dirty, the shell marks the tab
"changed on disk" and the next save hits `conflict`.

## Run a command

```
user: types `cargo build` in the Run bar, presses Enter
shell → daemon   task/start { workspaceId, command: { shell: "cargo build" } }
daemon → shell   { taskId }          (or error spawn_failed → shown inline in the panel)
daemon → shell   task/output { taskId, seq, stream, data }   × N
daemon → shell   task/exited { taskId, outcome, exitCode, durationMs }
user: Stop       shell → daemon task/cancel { taskId }   → … task/exited { outcome: "cancelled" }
```
`TaskOutputModel` appends chunks in `seq` order, tags stderr chunks for
styling, and shows a final status line from `task/exited`.

## Persisted state

| What | Where | Written when |
|------|-------|--------------|
| Theme, font size, last workspace | `settings/set` → SQLite `settings` | on change |
| Recent workspaces | daemon, on `workspace/open` | automatically |
| Open tabs, active tab, expanded directories, panel sizes | `workspace/setState` → SQLite `workspace_state` | debounced 500 ms after change, and on close |
| Window geometry | shell-local `QSettings` | on close (it is per-machine UI state, not platform state) |

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
