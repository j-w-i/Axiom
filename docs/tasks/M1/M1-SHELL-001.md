---
id: M1-SHELL-001
title: Desktop shell skeleton — window, design system, daemon client
status: tested
status_note: 2026-11-11; see handoff
depends_on: [M1-DAEMON-001]
order: 40
scope: apps/desktop/, scripts/check.sh (desktop section only)
---

# M1-SHELL-001: Shell skeleton

## Objective
A launchable Qt application that starts the daemon, completes `initialize`,
shows the main window frame built from the design system, and reports
backend status. No explorer, editor or output yet.

## Context
Read: `docs/architecture/processes.md`, `docs/architecture/modules.md`
§`apps/desktop`, `docs/architecture/design-system.md`,
`docs/decisions/ADR-0011-licensing.md` (allowed Qt modules),
`docs/contributing/dev-setup.md`. Working reference for process + JSON
handling: `experiments/e1-qt-rust-monaco/shell/main.cpp` (class `Daemon`).

## Implementation requirements
1. CMake project `apps/desktop/CMakeLists.txt`: C++17, `AUTOMOC`,
   `find_package(Qt6 6.8 REQUIRED COMPONENTS Core Gui Qml Quick QuickControls2 Test)`
   (WebEngine arrives in M1-SHELL-003). Target `axiom`. Warnings on
   (`-Wall -Wextra` / `/W4`). QML modules via `qt_add_qml_module`:
   `Axiom.Ui` and `Axiom.App`. Tests registered with `add_test`.
2. `src/daemon/DaemonClient.{h,cpp}` (QObject):
   - `start(program, args)`; spawns with `QProcess`, forwards daemon stderr
     lines to `qInfo` with a `[axiomd]` prefix.
   - `call(method, params, callback)` where callback receives
     `(QJsonObject result, RpcError error)`; `RpcError { kind, message,
     details; bool ok() }`. Ids are incrementing integers.
   - Per-call timeout (default 30 s; `initialize` 10 s): on expiry send
     `$/cancel` and invoke the callback with kind `timeout`.
   - `notification(QString method, QJsonObject params)` signal.
   - `state` property: `Starting | Ready | Failed | Stopped`, with
     `errorText`. Daemon exit while not stopping → `Failed`, all pending
     callbacks invoked with kind `internal` and message "Backend stopped".
   - `restart()`; `shutdown()` per the lifecycle in `processes.md` (3 s + 2 s).
   - Line framing: buffer until `\n`; never assume one read = one message.
3. Daemon binary lookup order: `--daemon` argument, `$AXIOMD`, sibling of
   the executable (`axiomd[.exe]`), `PATH`.
4. `qml/Axiom/Ui/`: `Theme.qml` singleton with every token from
   `design-system.md`, and the controls needed by this task: `AxLabel`,
   `AxButton`, `AxIconButton`, `AxSeparator`, `AxBanner`, `AxPanelHeader`,
   `AxSplitHandle`.
5. `qml/Axiom/App/Main.qml`: `ApplicationWindow` (min 720×480) with menu bar
   (File ▸ Quit; View ▸ Toggle Side Bar (Ctrl+B), Toggle Panel (Ctrl+J);
   Help ▸ About), a horizontal `SplitView` (side bar placeholder area titled
   "Explorer" with the text "No folder open", content area) and a vertical
   split with the bottom panel area titled "Output", and a status bar showing
   backend state ("Connecting…", "Connected — axiomd <version>",
   "Backend stopped").
   When state is `Failed`: an `AxBanner` (danger) across the top with the
   error text and a **Restart backend** button that calls `restart()`.
   Only controls that work are present: do not add menu items for features
   of later tasks.
6. Theme: follows `Qt.styleHints.colorScheme` for now (settings arrive in
   M1-SHELL-005).
7. Window geometry persisted with `QSettings` (organisation "axiom",
   application "Axiom").
8. `src/main.cpp`: argument parsing (`--daemon <path>`, `--data-dir <dir>`
   forwarded to the daemon, `--version`), creates `DaemonClient`, exposes it
   and nothing else to QML as a context property `backend` (a small
   `Backend` façade QObject with `state`, `errorText`, `serverVersion`,
   `restart()`), loads `Axiom.App/Main`.
9. Tests (Qt Test, run with `QT_QPA_PLATFORM=offscreen`):
   - `tst_daemonclient`: against the real `axiomd` (path from env
     `AXIOMD_UNDER_TEST`, set by CMake to `${CMAKE_SOURCE_DIR}/../../target/debug/axiomd`):
     initialize succeeds; a call to an unknown method yields kind
     `method_not_found`; split-line delivery is handled (feed a fake process
     via a test seam or unit-test the line buffer class separately); killing
     the daemon → `Failed` and pending callbacks fire; `restart()` → `Ready`.
   - `tst_smoke_launch`: starts the application object graph offscreen,
     waits for `backend.state == Ready`, checks the status bar text, quits.
10. `scripts/check.sh desktop`: builds `axiomd` (debug) first, then
    configure/build/ctest as already scripted.

## Constraints
- Allowed Qt modules only. No Qt Widgets. No WebEngine yet.
- QML contains no JSON-RPC and no file access. No literal colours/sizes
  outside `Axiom.Ui`.
- GUI thread never blocks on the daemon (no `waitForReadyRead` outside
  tests and `shutdown()`'s bounded waits).

## Acceptance criteria
- `scripts/check.sh` passes including the desktop section.
- Launching `build/desktop/axiom --daemon target/debug/axiomd` shows the
  window with "Connected — axiomd 0.1.0" (state how this was verified:
  on-screen, or offscreen via the smoke test only).
- Killing the daemon process externally shows the banner; Restart recovers.
- Light and dark appearance both render (switch the OS scheme or set
  `Theme.dark` in a test and grab the window to PNG; attach paths).

## Tests
`scripts/check.sh`.

## Failure conditions
Qt is not installed / cannot be bootstrapped in the environment → mark the
task blocked with the exact error; do not write untested QML.

## Documentation updates
`docs/roadmap/current.md`, `docs/contributing/dev-setup.md` (verify the run
instructions).

## Handoff
Screenshots (paths) of light and dark; Qt version used.
