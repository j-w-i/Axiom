# apps/desktop-tauri — Tauri 2 shell prototype (evaluation only)

Isolated prototype for the Qt → Tauri evaluation. **Not the product shell**:
`apps/desktop/` (Qt) is. Not part of the root Cargo workspace or
`scripts/check.sh`.

Read `docs/migration/qt-to-tauri/prototype-plan.md` §4 for how to build,
test, run the E2E suite and the Qt-vs-Tauri benchmarks. All GUI runs are
headless (`scripts/headless.sh`); nothing opens on the desktop.
