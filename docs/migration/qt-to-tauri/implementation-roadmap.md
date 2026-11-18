# Qt → Tauri: implementation roadmap (milestone M1.5)

_Status: **proposed, not approved.** Nothing in this file may start until
the decision in [decision.md](decision.md) is accepted by the project owner
and recorded as ADR-0012. No task here deletes or modifies the Qt shell
except T-14, which has its own gate._

## Milestone M1.5 — Desktop shell on Tauri (between M1 and M2)

**Outcome.** A Tauri 2 + React desktop shell at full M1 parity on Linux,
macOS and Windows, built and tested by `scripts/check.sh` and CI, shipped
side by side with the Qt shell, with Qt removed only after the removal gate
(§5) passes.

**Why between M1 and M2.** M2 adds the Environment Manager and Problems
panel; M3 adds the AI workspace. Building those UIs twice (QML and React) is
the cost the migration exists to avoid, so M1.5 must finish its parity gate
(T-12) before M2's UI epic (2.7) starts. M2's backend epics (2.0–2.6, 2.8)
do not touch the shell and **may run in parallel** with M1.5.

### Exit criteria
1. All nine M1 exit criteria pass on the Tauri shell on Linux, macOS and
   Windows, in CI, using the same daemon protocol (no daemon changes).
2. Every row of feature-parity.md is ✅ or explicitly waived by the owner.
3. performance.md budgets met on all three OSes (CI runners + one manual
   run on real hardware with a GPU per OS).
4. `scripts/check.sh` runs the Tauri gates; CI green on all three OSes.
5. Manual verification (IME, clipboard, screen reader, HiDPI) recorded in
   `docs/roadmap/verification/M1.5.md`.
6. Release packages for each OS build in CI and install/launch/connect.

## 1. Task list

Each task gets a file in `docs/tasks/M1.5/` (front matter as in
`docs/tasks/TEMPLATE.md`) when the milestone is approved. Contracts below are
complete enough to write those files mechanically.

| ID | Title | Depends on | Scope | Wave |
|----|-------|-----------|-------|------|
| T-01 | ADR-0012 + governance updates (supersede ADR-0001/0002 in part, amend 0004, 0011) | approval | `docs/decisions/`, `docs/architecture/` | 0 |
| T-02 | Promote the bridge to `crates/desktop-bridge` (root workspace, dependency matrix, cargo-deny) | T-01 | `crates/desktop-bridge/`, `Cargo.toml`, `scripts/check_deps.py`, `docs/architecture/modules.md` | A |
| T-03 | Promote the host to `apps/desktop-tauri/src-tauri` in the root workspace; CLI flags `--daemon`, `--data-dir`, `--version` (headless) | T-02 | `apps/desktop-tauri/src-tauri/` | B |
| T-04 | Generated TypeScript wire types from `schemas/ipc/protocol.schema.json` (ADR-0005), replacing `src/daemon/types.ts` | T-01 | `apps/desktop-tauri/src/daemon/`, `scripts/` | A |
| T-05 | Port the remaining Qt model test cases to `stores.test.ts` (all 115 Qt test functions mapped or waived) | T-04 | `apps/desktop-tauri/tests/` | B |
| T-06 | Parity UI gaps: Ctrl+W, Ctrl+PgUp/PgDn, output context menu + copy selection, View/Help menus, too-large/CRLF cases | T-05 | `apps/desktop-tauri/src/` | C |
| T-07 | Native application menu on macOS (Tauri menu API) mapped to the same actions; in-window menus on Linux/Windows | T-03 | `src-tauri/`, `src/` | C |
| T-08 | Renderer-crash recovery: snapshots held in the host, web-process termination detection, reload + replay via the `ready` handshake | T-03 | `src-tauri/`, `src/state/documents.ts` | C |
| T-09 | CI: `tauri` job on ubuntu/macos/windows — bridge tests, Vitest, E2E (tauri-driver on Linux/Windows; WebdriverIO embedded driver on macOS) | T-03, T-05 | `.github/workflows/ci.yml`, `scripts/check.sh` | C |
| T-10 | Manual verification protocol + run (IME, clipboard, DnD, HiDPI, Orca/NVDA/VoiceOver, real GPU) | T-06, T-08 | `docs/roadmap/verification/M1.5.md` | D |
| T-11 | Editor isolation spike: Monaco in an iframe/secondary webview with a narrower capability; measure memory/latency; decide | T-08 | `apps/desktop-tauri/` (spike branch) | D |
| T-12 | **Parity gate**: re-run feature-parity.md and performance.md on 3 OSes; owner sign-off | T-06…T-10 | docs | E |
| T-13 | Packaging: AppImage/deb/rpm, MSI/NSIS, dmg; daemon as `externalBin`; signing hooks; licence bundle (WebKitGTK LGPL notice on Linux, WebView2 bootstrapper terms) | T-03 | `apps/desktop-tauri/src-tauri/`, CI | D |
| T-14 | Make Tauri the default shell; keep Qt buildable behind a flag for one milestone | T-12, T-13 | `scripts/`, CI, docs | F |
| T-15 | Remove the Qt shell (after §5 gate) | T-14 + gate | `apps/desktop/`, `scripts/bootstrap-qt.sh`, CI, docs | G |
| M5-pre | **Preview-engine spike** (architecture.md §4.1): external Chromium over CDP rendered into the app, or WebView2 CDP; must succeed before M5 is planned on Tauri | T-01 | `experiments/e4-preview-cdp/` | A (parallel) |

### Parallel strategy

```
wave 0  T-01
wave A  T-02 · T-04 · M5-pre                         (disjoint scopes)
wave B  T-03 · T-05                                  (host vs tests)
wave C  T-06 · T-07 · T-08 · T-09                    (T-06/T-07/T-08 share src/: integrator merges T-08 first)
wave D  T-10 · T-11 · T-13
wave E  T-12  (gate)
wave F  T-14
wave G  T-15  (gate §5)
```
M2 backend epics run alongside waves A–E. M2 epic 2.7 (UI) starts after T-12
and is built **only** in the Tauri shell.

## 2. Task contracts (implementation-ready)

### T-02 Promote the bridge to `crates/desktop-bridge`
- **Objective.** `apps/desktop-tauri/bridge` becomes the root-workspace
  crate `axiom-desktop-bridge` with the identical public API
  (`DaemonBridge`, `BridgeConfig`, `BridgeEvent`, `DaemonState`,
  `locate_daemon`, `ui_callable`).
- **Requirements.** (1) dependency matrix row: `desktop-bridge → protocol`
  only; (2) `thiserror` for any new error type; no `unwrap`/`expect` outside
  tests; (3) tests move with it and run in `cargo test --workspace`;
  (4) Windows: crash test uses `taskkill`; timeout test is `#[cfg(unix)]`
  with a Windows equivalent using a never-answering fake daemon (tests/helpers).
- **Acceptance.** `scripts/check.sh rust` green on Linux; CI rust job green on
  3 OSes; `scripts/check_deps.py` lists the new edge; 12+ bridge tests pass,
  looped 20× without failure.
- **Must not.** Change protocol types; depend on Tauri.

### T-03 Host in the root workspace
- **Objective.** `src-tauri` builds from the root `Cargo.toml`; the app
  starts with `--daemon/--data-dir`, answers `--version` without a display,
  and exits non-zero with a message when the webview cannot start.
- **Acceptance.** `cargo build -p axiom-desktop-tauri --release` on 3 OSes in
  CI; capability file grants exactly the 7 commands + `dialog:allow-open`
  (a test parses `capabilities/*.json` and `build.rs` and fails on any
  addition not listed in architecture.md §2.3).
- **Must not.** Add any command that reads/writes files or starts
  processes; enable `tauri-plugin-shell` or `fs`.

### T-04 Generated TS wire types
- **Objective.** `pnpm run gen:types` produces `src/daemon/protocol.gen.ts`
  from `schemas/ipc/protocol.schema.json`; `scripts/check.sh` fails if it is
  stale (same pattern as the Rust schema check).
- **Acceptance.** `types.ts` reduced to re-exports; `tsc --noEmit` clean;
  freshness check fails when a schema changes without regeneration (test
  by temporary edit in CI fixture).

### T-05 Test parity port
- **Objective.** Every Qt test function in `apps/desktop/tests` is mapped to a
  Tauri test (bridge, stores, or E2E) or waived with a reason in a table in
  the task handoff.
- **Acceptance.** Mapping table complete (115 rows); all mapped tests pass
  20× consecutively; no `skip`.

### T-06 Parity UI gaps
- **Acceptance.** Each gap row in feature-parity.md (E-9, T-9, S-7, E-3, E-4
  too-large) turns ✅ with a named test.

### T-07 macOS native menu
- **Acceptance.** On macOS the app menu shows File/Edit/View/Run/Help with
  accelerators; actions dispatch to the same store methods (E2E on macOS
  triggers at least Save and Run via the menu API).

### T-08 Renderer-crash recovery
- **Requirements.** (1) `textSnapshot`s are forwarded to and held by the
  host (memory only, per document); (2) host detects web-process
  termination (WebKitGTK `web-process-terminated`, WebView2
  `ProcessFailed`, WKWebView `webViewWebContentProcessDidTerminate`) and
  reloads the page; (3) on `ready`, documents with snapshots reopen dirty
  (bridge-1 crash rule) — identical outcome to Qt's
  `tst_smoke_editor::unsavedTextSurvivesARendererCrash`.
- **Acceptance.** E2E test kills the web process and asserts the unsaved
  text returns marked dirty, on Linux and Windows (macOS: manual).

### T-09 CI
- **Acceptance.** New `tauri` job matrix green on ubuntu/macos/windows (or
  each red cell explained as a known defect with an owner, as KD-WIN-1 was);
  job timeout ≤ 45 min; screenshots uploaded as artifacts.

### T-10 Manual verification
- **Acceptance.** Checklist per OS with who/when/what: IME (Japanese/Chinese
  input in Monaco), clipboard to/from other apps, drag and drop of a folder
  onto the window, 150 %/200 % scaling, screen-reader navigation of explorer,
  tabs, output, dialogs; real-GPU run of the benchmark suite.

### T-11 Editor isolation spike
- **Decision rule.** Adopt isolation if it costs ≤ 40 MiB PSS and ≤ 5 ms p95
  typing latency over the in-document editor; otherwise record why not.

### T-12 Parity gate
- **Acceptance.** feature-parity.md all ✅/waived; performance.md budget
  table filled for 3 OSes; owner sign-off recorded in decision.md.

### T-13 Packaging
- **Acceptance.** CI artifacts for each OS install and pass a launch+connect
  smoke test; installed size recorded; licence notices present.

### T-14 Tauri as default
- **Acceptance.** Release artifacts and docs point to the Tauri shell; Qt
  shell still builds with `scripts/check.sh desktop-qt` and its tests pass.

### T-15 Remove Qt
- Only after §5. Deletes `apps/desktop/`, Qt CI jobs, `bootstrap-qt.sh`,
  ADR-0011's Qt clauses (superseded), and updates AGENTS.md.

## 3. Preserving M1 behaviour

- The daemon and protocol 0.1 do not change in M1.5. Any change discovered
  to be necessary goes through a CR (governance.md).
- Both shells share one data directory format; a user can switch back and
  forth (verified for the persisted keys in the prototype).
- `editor/src/documents.ts` stays the single implementation of bridge-1
  rules; its 43 Vitest tests keep running for both shells.
- The Qt shell remains the release artifact until T-14.

## 4. Rollback

| Point | Rollback |
|-------|----------|
| Before T-14 | Nothing to roll back: Qt is still the shipped shell. Abandoning M1.5 = delete `apps/desktop-tauri/` and `crates/desktop-bridge/`, revert ADR-0012 to "Rejected". |
| After T-14, before T-15 | Flip the default back to Qt (one script/CI change); Qt is still built and tested every commit. M2 UI built only in Tauri would have to be ported to QML — this is why T-14 waits for T-12. |
| After T-15 | Restore `apps/desktop/` from the tag `qt-shell-final` (created by T-15), re-add CI jobs. Costly; hence the gate. |

## 5. When Qt can safely be removed (all must hold)

1. T-14 has been the default for at least one milestone (M2 complete) with
   no open P1 defect attributable to the Tauri shell.
2. CI has been green on all three OSes for the Tauri shell for 30
   consecutive days of commits.
3. Renderer-crash recovery (T-08) passes on Linux and Windows.
4. The M5 preview-engine spike has a working answer on Tauri, **or** the
   owner accepts that M5 will host previews in an external browser.
5. Manual accessibility verification (T-10) found no blocker.
6. A tag `qt-shell-final` exists.

## 6. Documentation to update (by the task named)

| Document | Change | Task |
|----------|--------|------|
| `docs/decisions/ADR-0012-desktop-shell-tauri.md` | new | T-01 |
| ADR-0001, ADR-0002, ADR-0004 | status → "Superseded in part by ADR-0012" | T-01 |
| ADR-0011 | add Tauri/WebKitGTK/WebView2 licensing; Qt clauses retained until T-15 | T-01 |
| `docs/architecture/overview.md`, `modules.md`, `processes.md`, `security.md`, `testing.md`, `design-system.md` | shell description, module table, process model (webview processes), page privileges, test layers, web design system | T-01 (text), T-02/T-03 (modules) |
| `docs/protocols/editor-bridge.md` | transport section: QWebChannel (Qt) or in-process interfaces (Tauri); message set unchanged | T-01 |
| `docs/roadmap/milestones.md`, `dependency-graph.md`, `current.md` | insert M1.5 | T-01 |
| `AGENTS.md` | architecture-in-ten-lines, repository map, commands | T-01, T-15 |
| `docs/contributing/dev-setup.md` | WebKitGTK dev packages / WebView2 / Xcode CLT; `tauri-driver` | T-03 |
