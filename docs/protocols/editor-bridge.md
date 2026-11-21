# Editor bridge 1 (shell ↔ editor surface)

**State: retired (FL-QA-001, 2026-11-22; ADR-0020).** The Flutter shell has no web view and no
editor bridge: the editor (`re_editor`) is a widget in the same Dart process, and the open
documents live in `apps/desktop-flutter/lib/state/documents.dart`. `editor/` (Monaco) and the
Tauri shell that implemented this contract were removed; both are at the tag
`tauri-shell-final`. The rest of this document is the historical record of the contract as
frozen for M1 and kept through M1.5–M4.

Before that: frozen for M1, validated in outline by experiment E1.

The editor surface is a web page (`editor/`, loaded from
`axiom://editor/index.html`) inside a Qt WebEngine view. It communicates with
the shell through one QWebChannel object named **`host`**, implemented by the
C++ class `EditorBridge`.

QWebChannel semantics used here:
- **Signals** on `host` are shell → page messages. The page subscribes with
  `host.<signal>.connect(fn)`.
- **Slots** on `host` are page → shell messages. The page calls
  `host.<slot>(args…)`.
- All calls are asynchronous and unordered relative to DOM events but ordered
  relative to each other.
- Arguments are JSON-compatible values. Every message below takes **one
  object argument**, so fields can be added without changing arity.

The TypeScript declaration of this contract is `editor/src/bridge.ts` (created
by M1-EDITOR-001); it must match this document exactly. The C++ side is
`apps/desktop/src/editor/EditorBridge.h`.

## Handshake

1. Page loads, constructs `QWebChannel(qt.webChannelTransport, …)`.
2. Page creates the Monaco editor, subscribes to all signals, then calls
   `host.ready({ bridgeVersion: 1, monacoVersion: "<x.y.z>" })`.
3. Shell answers with signal `configure` and then re-opens any documents that
   should be open (`openDocument` per document, then `activateDocument`).

The shell must not emit any signal before `ready`. If `bridgeVersion` is not
1 the shell shows an error instead of the editor. After a renderer crash or
reload the handshake simply happens again.

## Shell → page (signals)

| Signal | Payload | Meaning |
|--------|---------|---------|
| `configure` | `{ theme: "light"\|"dark", fontSize: number, fontFamily: string, wordWrap: boolean }` | Apply appearance. Sent after `ready` and whenever a relevant setting changes. `theme` is already resolved (never `"system"`). |
| `openDocument` | `{ path: string, text: string, version: string, activate: boolean, dirty: boolean }` | Create the model for `path` (a `WorkspacePath`) with this content. If a model for `path` exists, replace its content and keep view state. `dirty: false` (the normal case): `text` is what is on disk at `version`; mark the document clean. `dirty: true` (crash recovery only): `text` is unsaved content that differs from disk; mark the document dirty (see Dirty tracking). If `activate`, show it. Language is inferred by the page from the file name. |
| `activateDocument` | `{ path: string }` | Show an already-open document and focus the editor. |
| `closeDocument` | `{ path: string }` | Dispose the model. If it was active, show nothing (empty state is the shell's job: it hides the web view when no document is open). |
| `requestSave` | `{ path: string }` | Ask the page to emit `saveRequested` for this document (used by the File ▸ Save menu item and Save All). |
| `documentSaved` | `{ path: string, version: string }` | The pending save succeeded: the content sent in `saveRequested` is now on disk (see Dirty tracking). |
| `saveFailed` | `{ path: string }` | The pending save did not happen (error, or the user cancelled a conflict dialog). The page clears the pending save and leaves the document dirty. The shell shows the reason; the page shows nothing. |
| `revealPosition` | `{ path: string, line: number, column: number }` | 1-based. Move the cursor there and centre it. Activates the document. |
| `runCommand` | `{ command: "undo"\|"redo"\|"find"\|"replace"\|"selectAll"\|"format" }` | Trigger a Monaco action from a shell menu item. |

## Page → shell (slots)

| Slot | Payload | Meaning |
|------|---------|---------|
| `ready` | `{ bridgeVersion: 1, monacoVersion: string }` | Handshake. |
| `saveRequested` | `{ path: string, text: string }` | The user pressed Ctrl/Cmd+S in the editor, or the shell sent `requestSave`. The shell performs `fs/writeFile` and must answer every `saveRequested` with exactly one `documentSaved` or `saveFailed` for that path. |
| `dirtyChanged` | `{ path: string, dirty: boolean }` | The document's dirty state changed. |
| `cursorChanged` | `{ path: string, line: number, column: number, selectionLength: number }` | Throttled to at most one per 50 ms. For the status bar. |
| `textSnapshot` | `{ path: string, text: string }` | Unsaved content, sent at most once per second per dirty document while it is changing. The shell keeps the latest snapshot in memory only, to restore after a renderer crash. |
| `openLink` | `{ url: string }` | The user activated a link; the shell opens it externally after checking the scheme is `http`/`https`. |
| `reportError` | `{ message: string, detail: string }` | An unexpected page error; the shell logs it. |

There is deliberately no slot for reading files, listing directories, running
processes or sending arbitrary RPC.

## Dirty tracking

Per document the page keeps `savedId` and `pendingId`.

- `openDocument` with `dirty: false`, and `documentSaved`: see below; the
  document is clean when Monaco's `model.getAlternativeVersionId()` equals
  `savedId`. Using the alternative version id means undoing back to the
  saved state makes the document clean again.
- `openDocument` with `dirty: true`: `savedId = null`, so the document is
  dirty until a save succeeds (undo cannot make it clean, because the page
  does not know the on-disk text).
- `dirty = (savedId === null) || (currentId !== savedId)`; `dirtyChanged` is
  sent whenever that value changes, and once after every `openDocument`.

Saves are serialised per document:

1. On a save trigger, if `pendingId` is set for that document, the trigger
   is **ignored** (a save is already in flight).
2. Otherwise the page sets `pendingId = currentId` and sends
   `saveRequested` with the current text.
3. On `documentSaved`: `savedId = pendingId`, `pendingId = null` — not the
   current id, because the user may have typed during the save, in which
   case the document stays dirty.
4. On `saveFailed`: `pendingId = null`; nothing else changes.
5. `openDocument` and `closeDocument` for the path clear `pendingId`.

## Keyboard ownership

- While the editor has focus, Monaco handles its own shortcuts, including
  Ctrl/Cmd+S (bound in the page to emit `saveRequested`), find, undo, etc.
- Application shortcuts that must work everywhere (Open Folder, Quick switch,
  toggle panels, run) are QML `Shortcut`s with
  `context: Qt.ApplicationShortcut`. They are chosen not to collide with
  Monaco's defaults; the list lives in
  `apps/desktop/qml/Axiom/App/Shortcuts.qml`.
- E1 verified that synthetic Qt key events reach Monaco (typing and Ctrl+S)
  on Linux, both offscreen and under Wayland. The M1-SHELL-003 smoke test
  (`tst_smoke_editor`) repeats this with the real bundle offscreen on Linux:
  typing, Ctrl+S, conflict and renderer-crash recovery pass 5/5 runs. macOS
  and Windows are unverified until CI runs the same smoke test (M1-CI-002).

## Failure modes

| Failure | Detection | Behaviour |
|---------|-----------|-----------|
| Page fails to load / asset missing | `loadingChanged` with error | Shell replaces the editor area with an error panel naming the asset directory |
| `ready` not received within 10 s | timer | Same panel, with *Reload* |
| Renderer process crash | `renderProcessTerminated` | Panel with *Reload editor*; on reload, each document that had a `textSnapshot` reopens with `openDocument { text: <snapshot>, dirty: true }`, the others from disk with `dirty: false` |
| `saveRequested` for a path the shell does not consider open | lookup fails | Ignored and logged |

## Tests

- `editor/` unit tests (Vitest) run the bridge logic against a fake `host`
  object: dirty tracking, pendingId handling, snapshot throttling.
- The desktop smoke test (`apps/desktop/tests/smoke`) drives the real page:
  open → host edit → key events → Ctrl+S → file on disk (as E1 does).
- Vitest cases required for the save rules: second trigger while pending is
  ignored; `saveFailed` re-enables saving and keeps dirty; `dirty: true`
  open stays dirty across undo and becomes clean after `documentSaved`.
