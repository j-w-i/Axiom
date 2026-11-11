---
id: M1-EDITOR-001
title: Build the editor surface package (Monaco + bridge)
status: planned
status_note:
depends_on: []
order: 14
scope: editor/, scripts/check.sh (editor section only), .github/workflows/ci.yml (add editor job)
---

# M1-EDITOR-001: `@axiom/editor`

## Objective
A static web bundle (`editor/dist/`) that implements the page side of the
editor bridge with Monaco, plus unit tests of the bridge logic that run
without Qt.

## Context
Read: `docs/protocols/editor-bridge.md` (the contract; all of it),
`docs/decisions/ADR-0004-editor-strategy.md`, `experiments/README.md` §E1
(lessons), and the working prototype
`experiments/e1-qt-rust-monaco/editor/` (start from it).

## Implementation requirements
1. Package: `editor/package.json` (`"name": "@axiom/editor"`, private,
   `"type": "module"`), pnpm lockfile committed. Exact-pinned dependencies:
   `monaco-editor` 0.57.0; dev: `vite`, `typescript`, `vitest`.
   Scripts: `build` (`vite build`), `check` (`tsc --noEmit`), `test`
   (`vitest run`), `dev` (`vite`, for the dev harness below).
2. `vite.config.ts`: `base: "./"`, output `dist/`, no external network
   references in the output (assert in a test: no `http://`/`https://` URL
   in `dist/index.html`).
3. Source layout:
   - `src/bridge.ts` — TypeScript interfaces `HostSignals`, `HostSlots`,
     payload types, exactly mirroring the tables in `editor-bridge.md`;
     `BRIDGE_VERSION = 1`.
   - `src/documents.ts` — `DocumentStore`: models by path, dirty tracking
     with `savedId`/`pendingId` as specified, snapshot throttling (1 s per
     dirty document), cursor throttling (50 ms). Pure logic over a minimal
     `ModelLike` interface so it is testable without Monaco.
   - `src/monaco-setup.ts` — worker wiring (`MonacoEnvironment.getWorker`
     with `?worker` imports: editor, json, css, html, ts), theme mapping
     (`light` → `vs`, `dark` → `vs-dark`), option application from
     `configure`.
   - `src/main.ts` — creates the editor, connects `QWebChannel` when
     `window.qt` exists, wires signals/slots, sends `ready`.
   - `src/dev-host.ts` — when `window.qt` is absent (plain browser via
     `pnpm dev`), installs a fake `host` that logs slot calls and opens a
     sample document, so the page can be developed in a browser.
4. Language id: `monaco.languages.getLanguages()` lookup by file extension /
   filename; fallback `plaintext`.
5. Keybinding: Ctrl/Cmd+S → `saveRequested`. `runCommand` maps to Monaco
   actions (`undo`, `redo`, `actions.find`,
   `editor.action.startFindReplaceAction`, `editor.action.selectAll`,
   `editor.action.formatDocument`).
6. `index.html` loads `/qwebchannel.js` with a classic script tag placed
   before the module script; in the dev harness the file is absent and the
   page must tolerate that (script `onerror` ignored).
7. Limit shipped languages to keep the bundle reasonable is **not** required
   in this task; record the `dist/` size in the report.
8. Uncaught errors and unhandled rejections → `reportError`.

## Constraints
- No runtime dependency other than `monaco-editor`.
- No network access, no `localStorage` for document content.
- The bridge file is hand-written from the document (ADR-0005 notes this
  exception).

## Acceptance criteria
- `pnpm install --frozen-lockfile && pnpm run check && pnpm run build &&
  pnpm run test` succeeds in `editor/`.
- Unit tests (Vitest, fake host + fake models) cover: open → clean;
  edit → `dirtyChanged(true)`; undo to saved state → `dirtyChanged(false)`;
  `saveRequested` carries current text; typing during a save keeps the
  document dirty after `documentSaved` (pendingId rule); a second save
  trigger while one is pending is ignored; `saveFailed` clears the pending
  save and keeps the document dirty; `openDocument` with `dirty: true`
  stays dirty across undo and becomes clean after a successful save; re-`openDocument`
  of an existing path replaces content and marks clean; `closeDocument`
  disposes; snapshot throttling; cursor throttling; language id inference
  for `.rs`, `.ts`, `.json`, `Makefile`, unknown.
- `scripts/check.sh editor` runs the four commands; `scripts/check.sh`
  (all) includes them.
- CI has an `editor` job (ubuntu only is enough) running the same.
- Opening `pnpm dev` in a normal browser shows a working Monaco editor with
  the sample document (state this was checked, or that no browser was
  available).

## Tests
`scripts/check.sh` passes (rust + editor).

## Failure conditions
Monaco 0.57.0's worker entry points differ from those recorded in
`experiments/README.md` §E1: use the ones that work, record them in a
comment in `monaco-setup.ts` and in the completion report. Do not change the
pinned Monaco version without a CR.

## Documentation updates
`docs/roadmap/current.md`; `docs/contributing/dev-setup.md` (editor
commands) if they differ.

## Handoff
`dist/` size; list of bundled language workers.
