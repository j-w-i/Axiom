---
id: M1-SHELL-003
title: Embed the editor — open, edit, save, tabs
status: tested
status_note: 2026-11-11; see handoff
depends_on: [M1-SHELL-002, M1-EDITOR-001]
order: 42
scope: apps/desktop/, scripts/check.sh (desktop section), .github/workflows/ci.yml (only if needed for editor assets)
---

# M1-SHELL-003: Editor integration

## Objective
Activating a file opens it in the embedded Monaco editor; the user can edit,
save (with conflict handling), switch between tabs and close documents.

## Context
Read: `docs/protocols/editor-bridge.md` (contract),
`docs/architecture/data-flow.md` §Open, edit and save,
`docs/architecture/security.md` §Editor surface, `experiments/README.md` §E1
and its `shell/main.cpp` (scheme handler, bridge object, self-test).

## Implementation requirements
1. Qt components: add `WebEngineQuick WebChannel`. In `main`: register the
   `axiom` scheme (`SecureScheme | CorsEnabled | FetchApiAllowed`, host
   syntax) and call `QtWebEngineQuick::initialize()` **before**
   constructing `QGuiApplication`.
2. `src/editor/AssetSchemeHandler`: serves `axiom://editor/<path>` from the
   editor asset directory; `/qwebchannel.js` from
   `:/qtwebchannel/qwebchannel.js`; canonicalizes and rejects paths outside
   the directory (`RequestDenied`); correct MIME types (html, js, css, json,
   ttf, woff2, svg, map, wasm).
   Asset directory lookup: `--editor-assets <dir>`, `$AXIOM_EDITOR_ASSETS`,
   then `<app dir>/editor`. CMake copies `editor/dist` to
   `<build>/editor` when it exists (a custom target that fails with a clear
   message if `editor/dist/index.html` is missing).
3. `src/editor/EditorBridge` (QObject registered on the channel as `host`):
   signals and slots exactly as in `editor-bridge.md`, each with one
   `QJsonObject` argument. Bridge version check on `ready`.
4. `src/models/OpenDocumentsModel` (list model + controller, exposed as
   `documents`): entries `{path, title, dirty, savedVersion,
   changedOnDisk}`; `activePath`; invokables `open(path)`, `activate(path)`,
   `close(path)`, `save(path)`, `saveAll()`. Implements the flows of
   `data-flow.md`:
   - open: `fs/readFile` → `openDocument`; already open → `activateDocument`.
     `unsupported_encoding` / `too_large` → banner "Cannot open <name>: …".
   - save: on `saveRequested` → `fs/writeFile` with `expectedVersion` →
     `documentSaved`. On `conflict` → `AxDialog` "<name> changed on disk"
     with **Overwrite** (retry with `overwrite: true` → `documentSaved`),
     **Reload** (re-read, `openDocument` with `dirty: false`), **Cancel**
     (`saveFailed`). Any other error → banner with the message and
     `saveFailed`. Every `saveRequested` is answered exactly once.
   - external change (`fs/didChange` for an open path): clean → re-read and
     `openDocument`; dirty → set `changedOnDisk` (tab shows the `warning`
     colour and tooltip).
   - close dirty → dialog **Save / Discard / Cancel**. Quitting or closing
     the folder with dirty documents → the same dialog listing them.
5. QML: `EditorArea.qml` — tab strip (`AxTabButton` with title, dirty dot,
   close button; middle-click closes; Ctrl+W closes active; Ctrl+Tab /
   Ctrl+Shift+Tab cycle) above a single `WebEngineView` (hidden, with the
   Welcome/empty view shown, when no document is open). Navigation requests
   to anything other than `axiom://editor/` are rejected; context menu
   limited to Monaco's own.
6. Menus: File ▸ Save (Ctrl+S when focus is outside the editor; inside it
   Monaco handles it), Save All; Edit ▸ Undo, Redo, Find, Replace, Select
   All (send `runCommand`). Items are disabled when no document is open.
7. Status bar: `Ln x, Col y` from `cursorChanged`.
8. Persist open tabs and the active tab in workspace state
   (`editor.openPaths`, `editor.activePath`); restore on workspace open,
   skipping files that no longer exist.
9. Renderer crash: on `renderProcessTerminated`, show the reload panel
   (see contract §Failure modes) and restore documents from the latest
   `textSnapshot`s on reload using `openDocument` with `dirty: true`.
   Test it: `tst_smoke_editor` kills the renderer
   (`WebEngineView` loads `chrome://crash`, or `QWebEnginePage::killRenderProcess`
   equivalent available to Qt Quick) after typing, reloads, and asserts the
   typed text is back and the tab is dirty. If no way to kill the renderer
   works offscreen, say so and cover the restore path with a model-level
   test instead.
10. Smoke test `tst_smoke_editor` (offscreen, real daemon, real bundle;
    modelled on E1's self-test, **fails closed on timeout**): open the
    fixture's `src/main.rs`; wait for `ready` and document open; send key
    events typing `//K`; assert `dirty`; send Ctrl+S; assert the file on
    disk contains `//K` and `dirty` is false; modify the file externally
    while clean → model reloads; make it dirty, modify externally, save →
    conflict dialog state is raised.

## Constraints
- The page receives only the `host` object. No other objects on the channel.
- `QTWEBENGINE_REMOTE_DEBUGGING` is not set by application code.
- The shell does not parse or transform document text.

## Acceptance criteria
- `scripts/check.sh` passes; `tst_smoke_editor` passes offscreen.
- On-screen manual check if a display is available: open, type, save,
  tab switching, conflict dialog. State what was and was not checked
  (IME, clipboard, drag-and-drop are explicitly *not* required here but
  note any observations).

## Tests
`scripts/check.sh`.

## Failure conditions
- Key events do not reach Monaco on the test platform → investigate focus
  (`forceActiveFocus` on the view; E1 delivers to
  `window->activeFocusItem()`); if unresolved, report with details rather
  than replacing the test with a host-initiated edit.
- WebEngine cannot start in the environment (sandbox/GPU) → document the
  exact error and the flags tried.

## Documentation updates
`docs/roadmap/current.md`; `docs/protocols/editor-bridge.md` §Keyboard
ownership "unverified" note if new platforms were verified.

## Handoff
Platform(s) verified; WebEngine/Chromium version from the user agent.
