# apps/desktop-tauri — Tauri 2 shell prototype (evaluation only)

Isolated prototype for the Qt → Tauri evaluation. **Not the product shell**:
`apps/desktop/` (Qt) is. Not part of the root Cargo workspace or
`scripts/check.sh`.

Read `docs/migration/qt-to-tauri/prototype-plan.md` §4 for how to build,
test, run the E2E suite and the Qt-vs-Tauri benchmarks. All GUI runs are
headless (`scripts/headless.sh`); nothing opens on the desktop.

Screenshots: `scripts/e2e.sh shots.mjs` captures every visual-QA state in both themes into `e2e/shots/<os>/`; compare two sets with `node e2e/shots-compare.mjs <before> <after>` (see `e2e/shots/README.md`).

Component gallery (dev only, never in `dist`): `pnpm run gallery`, then open `http://localhost:5174/gallery.html` (`?theme=light&motion=off` also work); `scripts/e2e.sh shots.mjs` captures it in the app's own webview as state S17.
