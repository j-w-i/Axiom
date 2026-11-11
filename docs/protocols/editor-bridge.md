# Editor bridge 1 (shell ↔ editor surface)

**State: frozen for M1.** Validated in outline by experiment E1.

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
| `openDocument` | `{ path: string, text: string, version: string, activate: boolean }` | Create the model for `path` (a `WorkspacePath`) with this content and mark it clean. If a model for `path` exists, replace its content, keep view state, mark clean. If `activate`, show it. Language is inferred by the page from the file name. |
| `activateDocument` | `{ path: string }` | Show an already-open document and focus the editor. |
| `closeDocument` | `{ path: string }` | Dispose the model. If it was active, show nothing (empty state is the shell's job: it hides the web view when no document is open). |
| `requestSave` | `{ path: string }` | Ask the page to emit `saveRequested` for this document (used by the File ▸ Save menu item and Save All). |
| `documentSaved` | `{ path: string, version: string }` | The save succeeded: mark the model clean at its *current* alternative version id captured when `saveRequested` was sent (see Dirty tracking). |
| `revealPosition` | `{ path: string, line: number, column: number }` | 1-based. Move the cursor there and centre it. Activates the document. |
| `runCommand` | `{ command: "undo"\|"redo"\|"find"\|"replace"\|"selectAll"\|"format" }` | Trigger a Monaco action from a shell menu item. |

## Page → shell (slots)

| Slot | Payload | Meaning |
|------|---------|---------|
| `ready` | `{ bridgeVersion: 1, monacoVersion: string }` | Handshake. |
| `saveRequested` | `{ path: string, text: string }` | The user pressed Ctrl/Cmd+S in the editor, or the shell sent `requestSave`. The shell performs `fs/writeFile`. |
| `dirtyChanged` | `{ path: string, dirty: boolean }` | The document's dirty state changed. |
| `cursorChanged` | `{ path: string, line: number, column: number, selectionLength: number }` | Throttled to at most one per 50 ms. For the status bar. |
| `textSnapshot` | `{ path: string, text: string }` | Unsaved content, sent at most once per second per dirty document while it is changing. The shell keeps the latest snapshot in memory only, to restore after a renderer crash. |
| `openLink` | `{ url: string }` | The user activated a link; the shell opens it externally after checking the scheme is `http`/`https`. |
| `reportError` | `{ message: string, detail: string }` | An unexpected page error; the shell logs it. |

There is deliberately no slot for reading files, listing directories, running
processes or sending arbitrary RPC.

## Dirty tracking

The page records Monaco's `model.getAlternativeVersionId()` as `savedId` when
a document is opened or reported saved. `dirty = (currentId !== savedId)`.
Using the alternative version id means undoing back to the saved state makes
the document clean again. On `saveRequested` the page remembers the id it is
sending as `pendingId`; on `documentSaved` it sets `savedId = pendingId` (not
the current id, because the user may have typed during the save).

## Keyboard ownership

- While the editor has focus, Monaco handles its own shortcuts, including
  Ctrl/Cmd+S (bound in the page to emit `saveRequested`), find, undo, etc.
- Application shortcuts that must work everywhere (Open Folder, Quick switch,
  toggle panels, run) are QML `Shortcut`s with
  `context: Qt.ApplicationShortcut`. They are chosen not to collide with
  Monaco's defaults; the list lives in
  `apps/desktop/qml/Axiom/App/Shortcuts.qml`.
- E1 verified that synthetic Qt key events reach Monaco (typing and Ctrl+S)
  on Linux, both offscreen and under Wayland. macOS and Windows are
  unverified until CI runs the same smoke test (M1-CI-002).

## Failure modes

| Failure | Detection | Behaviour |
|---------|-----------|-----------|
| Page fails to load / asset missing | `loadingChanged` with error | Shell replaces the editor area with an error panel naming the asset directory |
| `ready` not received within 10 s | timer | Same panel, with *Reload* |
| Renderer process crash | `renderProcessTerminated` | Panel with *Reload editor*; on reload, documents reopen with `textSnapshot` content if newer than saved, and are marked dirty |
| `saveRequested` for a path the shell does not consider open | lookup fails | Ignored and logged |

## Tests

- `editor/` unit tests (Vitest) run the bridge logic against a fake `host`
  object: dirty tracking, pendingId handling, snapshot throttling.
- The desktop smoke test (`apps/desktop/tests/smoke`) drives the real page:
  open → host edit → key events → Ctrl+S → file on disk (as E1 does).
