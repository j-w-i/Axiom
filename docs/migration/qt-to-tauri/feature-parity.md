# Qt → Tauri: feature-parity matrix

_Status: evaluation. Every "Tauri" cell states what the **prototype** in
`apps/desktop-tauri/` actually does and the evidence; "planned" means the
mechanism is designed but not built. Verified on Fedora 44 / x86_64 /
WebKitGTK 2.54.2 under a private Xvfb only. macOS and Windows: **unverified
for both columns' Tauri side**._

Evidence keys:
- **QT:** `apps/desktop/tests/<suite>::<function>` (from `verification/M1.md`).
- **B:** `apps/desktop-tauri/bridge/tests/bridge.rs::<test>` (Rust, real daemon).
- **S:** `apps/desktop-tauri/tests/stores.test.ts` "<test>" (UI stores, real daemon, no webview).
- **E:** `apps/desktop-tauri/e2e/smoke.mjs` check number (real Tauri app, real daemon, WebDriver, real key events).

Status: ✅ parity (tested) · 🟡 parity by design, not tested in the prototype · 🟠 partial / different · ❌ missing.

## M1 exit criteria

| # | M1 criterion | Qt evidence | Tauri implementation | Tauri evidence | Status |
|---|--------------|-------------|----------------------|----------------|--------|
| 1 | Launches, shows "Connected" | QT: tst_smoke_launch::launchesConnectsAndShowsStatus | Rust bridge `start()` → `State{ready}` → status bar | B: connects_and_reports_ready; E: 1 | ✅ |
| 2 | File ▸ Open Folder opens a workspace | QT: smoke_launch (controller call; dialog not clicked) | `tauri-plugin-dialog` → `documents.requestOpenFolder` | E: 2 (store call; dialog not clicked — same limitation as Qt) | 🟡 dialog itself unverified in both |
| 3 | Explorer lazy, ignored dimmed, updates on disk change | QT: tst_workspace (16 tests), smoke_launch keyboard nav | `WorkspaceStore` (port of FileTreeModel) + virtualised ARIA tree | S: "opens a workspace…", "re-lists an expanded directory…"; E: 3a, 3b, 6c | ✅ |
| 4 | Activating a file opens it with syntax colouring | QT: tst_smoke_editor::editTypeSaveAndConflict | `DocumentsStore` + `EditorSurface` (Monaco 0.57.0, same `documents.ts`) | E: 4a (≥ 3 token classes rendered) | ✅ |
| 5 | Dirty tab; Ctrl/Cmd+S saves; bytes match; external change of dirty file → conflict dialog | QT: smoke_editor, tst_documents (30 tests) | same save/conflict rules; Radix dialog | S: "edits, saves…", "reloads a clean document…Overwrite wins"; E: 4b, 5, 6a, 6b (real keystrokes, real Ctrl+S in Monaco) | ✅ (Linux) |
| 6 | Everything through `axiomd` | QT: no file API in shell | host has no fs/process command; capability lists 7 commands + `dialog:allow-open` | architecture.md §2.3; `build.rs` | ✅ |
| 7 | Streams stdout/stderr distinctly; exit status; Stop kills children | QT: tst_tasks (20), smoke_launch | `TasksStore` + `OutputBuffer` (port of TaskOutputModel) | S: "runs a command…", "stops a long command…"; E: 7a, 7b | ✅ |
| 8 | Failing command / missing program → structured errors | QT: tst_tasks::spawnFailedOfAnArgvShowsADangerRow | same rendering of `spawn_failed` details | S: "reports spawn_failed…"; E: 7c | ✅ |
| 9 | Restart restores theme/editor settings, last workspace, tabs, expanded folders, command history | QT: tst_settings, tst_workspace, tst_documents, tst_tasks | same state keys in the same store | S: "persists expanded folders, tabs…"; E: 8 | ✅ |

## Detailed feature matrix

### Daemon connection (R1–R3)

| ID | Feature | Qt | Tauri | Evidence | Status |
|----|---------|----|-------|----------|--------|
| D-1 | Daemon lookup order (flag, env, sibling, PATH) | `locateDaemon` | `bridge::locate_daemon`, same order | `locate::precedence_is_explicit_then_env_then_sibling_then_path`, `locate::a_source_that_does_not_exist_is_skipped_not_returned` (Rust unit tests) | ✅ |
| D-2 | `initialize` within 10 s or fail with message | DaemonClient | same | B: missing_program_fails_clearly | ✅ |
| D-3 | Request timeout 30 s → `$/cancel` + `timeout` | tst_daemonclient::callTimesOutAndSendsCancel | same | B: timeout_answers_with_timeout_and_sends_cancel | ✅ |
| D-4 | Crash → all pending answered "Backend stopped", banner with Restart | tst_daemonclient, smoke_launch::failedBackendShowsBannerAndRestartRecovers | same; banner in UI | B: crash_fails_pending_and_restart_recovers; S: "a crashed daemon…" | ✅ (banner not screenshot-tested) |
| D-5 | Bounded shutdown (3 s / 2 s / kill) | tst_daemonclient::shutdownIsOrderlyAndStopsTheProcess | same | B: shutdown_is_bounded_and_the_process_exits | ✅ |
| D-6 | stderr forwarded to shell log | tst_daemonclient::stderrLinesAreForwardedWithPrefix | `[axiomd]` lines on host stderr | B: `daemon_stderr_is_forwarded_as_log_events` | ✅ |
| D-7 | Results/ids correlated under concurrency | tst_daemonclient::resultsAndIdsAreCorrelated | same | B: concurrent_requests_each_answered_once (200) | ✅ |

### Workspace and explorer (R4, R5)

| ID | Feature | Qt | Tauri | Evidence | Status |
|----|---------|----|-------|----------|--------|
| W-1 | Protocol order of root entries; kinds; ignored flag | tst_workspace | same data; dimmed via `--text-faint` | S, E: 3a | ✅ |
| W-2 | Lazy expansion; re-list on re-expand | tst_workspace::expandingListsChildrenLazily, ::collapsedDirectoriesAreRelistedWhenExpandedAgain | same | S; E: 3b | ✅ |
| W-3 | Minimal updates on `fs/didChange`, expansion preserved | ::newFileAppearsWithoutCollapsingOtherDirectories, ::changesUseMinimalRowSignals | node map keeps unchanged nodes; React re-renders visible rows only | S: "re-lists an expanded directory…" | ✅ (no "minimal signal" equivalent needed) |
| W-4 | Inline error row for an unreadable directory | ::unreadableDirectoryShowsInlineErrorRow | `node.error` rendered under the directory | S: "an unreadable directory shows an inline error under it" (POSIX only: chmod cannot deny reads on Windows) | ✅ |
| W-5 | Missing path / file → error message | ::openingAMissingPathEmitsError, ::openingAFileEmitsError | same messages in banner | S: "opening a folder that does not exist reports it", "opening a file instead of a folder reports an error" | ✅ |
| W-6 | Recent list newest first; Open Recent menu | ::recentListIsPopulatedAndNewestFirst | File menu "Recent" section + Welcome list + palette | E screenshots | ✅ |
| W-7 | Reopen last on startup | ::reopenLastOpensTheLastWorkspaceWhenEnabled | `createApp` once-per-launch | S, E: 8 | ✅ |
| W-8 | Daemon restart drops the workspace | ::daemonRestartDropsTheWorkspace | same | S: "a crashed daemon…" | ✅ |
| W-9 | Keyboard: arrows, Home/End, Enter, Left to parent | smoke_launch::explorerShowsTreeAndKeyboardNavigationWorks | same + PageUp/PageDown | E: 3b (ArrowDown/ArrowRight) | ✅ partial (not every key tested) |
| W-10 | Switching workspaces while a close is in flight | WorkspaceController deferred open | ported (`pendingClose`, `deferredOpen`) | S: "a folder change requested while a close is in flight is deferred, not lost" (the calls are adjacent, so the race is not luck) | ✅ |

### Documents and editor (R6–R8, R14)

| ID | Feature | Qt | Tauri | Evidence | Status |
|----|---------|----|-------|----------|--------|
| E-1 | Bridge-1 handshake: configure → documents → activate | tst_documents::handshakeSends… | `DocumentsStore.ready()` same order | S (FakeEditor runs the real `DocumentStore`) | ✅ |
| E-2 | Wrong bridge version refused | ::wrongBridgeVersionIsRefused… | same check | S: "a page speaking the wrong bridge version is refused and gets no messages" | ✅ |
| E-3 | CRLF round-trip byte-exact | ::crlfSurvivesTheRoundTrip | same path (text passed verbatim) | S: "CRLF line endings survive a round trip" | ✅ |
| E-4 | Non-UTF-8 / too-large banners | ::unsupportedEncoding…, ::tooLargeFiles… | same messages | S: "refuses a non-UTF-8 file…", "a file over 16 MiB is refused…" | ✅ |
| E-5 | Save answers exactly once; unknown path ignored | ::saveWritesWithExpectedVersionAndAnswersExactlyOnce | same | S, E: 5 | ✅ |
| E-6 | Conflict: Overwrite / Reload / Cancel | ::conflictOverwrite…, ::conflictReload…, ::conflictCancel… | same | S: "…a dirty one conflicts and Overwrite wins", "conflict → Reload…", "conflict → Cancel…"; E: 6b/6b' | ✅ |
| E-7 | Clean doc reloads on disk change; dirty doc flagged | ::cleanDocumentReloads…, ::dirtyDocumentIsMarkedChangedOnDisk… | same + "changed on disk" tab badge | S; E: 6a, 6b | ✅ |
| E-8 | Unsaved prompt on close tab / folder / quit, each choice | ::closingADirtyDocumentPrompts…, ::quitAndFolderChanges… | same; window close intercepted in Rust and routed to the prompt | S: "closing a dirty tab prompts…" (Discard), "unsaved-work prompt: Save writes then proceeds; Cancel keeps the tab", "opening another folder … Cancel stays, Discard switches"; E quit: 1b/1c (the window's close button), 2–6 (quit: Cancel, Save) | ✅ |
| E-9 | Tab cycle (Ctrl+Tab / Ctrl+PgUp/PgDn), close (Ctrl+W) | EditorArea.qml Shortcuts | all three implemented in the window key handler (`App.tsx`) | inspected; E 5/10 exercise the same handler | ✅ |
| E-10 | Edit menu: undo/redo/find/replace/select all | Main.qml | Edit menu → `runCommand` | E parity: five checks driving the real menu (Undo reverts, Redo restores, Select All reports the selection, Find and Replace open Monaco's widget) | ✅ |
| E-11 | Cursor position in status bar | ::dirtyAndCursorAreForwardedToTheModel | same | screenshots show it | ✅ |
| E-12 | Renderer crash → reload with unsaved text from snapshot | smoke_editor::unsavedTextSurvivesARendererCrash | snapshots held in the **host** (`snapshot_put`); engine signal on Linux + heartbeat lease elsewhere; reload replays them through the restore path | S: "unsaved text comes back from the host snapshot after a webview crash", "a clean document leaves nothing to recover"; E: 11a/11b/11c (real web process killed, host detects, reloads, app+daemon survive) | ✅ (text restoration at store level; engine hook verified on Linux only) |
| E-13 | Links open externally, http/https only | ::linksOnlyOpenForHttpSchemes | `open_external` host command with the same check | S: "links open externally only for http and https" | ✅ |
| E-14 | Tab state restored, missing files skipped | ::tabsAndActiveTabAreRestoredSkippingMissingFiles | same | S, E: 8 | ✅ |
| E-15 | Switching workspaces closes documents, ignores late answers | ::switchingWorkspacesClosesDocuments… | generation counter ported | S: "switching workspaces closes the documents of the old one" | ✅ (late-answer half not driven) |
| E-16 | Editor theme follows app theme; font size/family/wrap live | smoke_launch::settingsApplyLive… | Monaco themes generated from CSS tokens | E: 9 | ✅ |
| E-17 | Asset scheme with path-escape checks | tst_assethandler (7) | Tauri's built-in asset protocol serves only `dist/`; no scheme handler of ours (see PARITY.md) | Tauri-provided | 🟡 (no Axiom test; N/A by design) |

### Tasks and output (R9)

| ID | Feature | Qt | Tauri | Evidence | Status |
|----|---------|----|-------|----------|--------|
| T-1 | seq ordering, duplicates ignored, line split per stream | tst_tasks model tests | `OutputBuffer` | S: "output assembly (pure)" | ✅ |
| T-2 | ANSI stripped, CR keeps last segment | ::ansiSequencesAreStripped, ::carriageReturnProgress… | same regexes | S (pure) | ✅ |
| T-3 | 10 000-line cap with truncation row | ::modelCapsAtTenThousandLines… | same cap, notice row | S: "output is capped at 10 000 lines and reports how many were dropped" (12 050 lines → 10 000 kept, 2 051 dropped, newest kept); bench `output` | ✅ |
| T-4 | 50 000 lines without stalling | ::fiftyThousandLinesAreCapped… | rAF-batched publishing + virtualised rows | bench `output` (performance.md) | see performance |
| T-5 | Command passed verbatim to shell | ::commandLineIsPassedVerbatimToTheShell | same | S | ✅ |
| T-6 | History: most-recent-first, deduped, capped 20, persisted | ::historyIs… | same; ↑/↓ in the field instead of a combo-box | S, E: 8 | ✅ (interaction differs) |
| T-7 | Rerun last (F5), Stop (Shift+F5), Run Command (Ctrl+Shift+R) | Main.qml | same shortcuts | E parity: F5 reruns the recorded command, Shift+F5 stops a run, Ctrl/Cmd+Shift+R focuses the command field | ✅ |
| T-8 | Closing the workspace cancels a running task | ::closingTheWorkspaceCancelsARunningTask | daemon does it; store resets | S: "closing the workspace cancels a running command" (same three assertions as Qt) | ✅ |
| T-9 | Copy selection / Copy all / right-click menu on output | OutputPanel.qml | context menu (Copy selection / Copy all) + "Copy output" button + native text selection | inspected | ✅ |
| T-10 | stderr marked without colour alone | gutter marker | gutter "▌" + visually-hidden "stderr:" prefix | screenshot | ✅ |

### Settings, chrome, platform (R10–R18)

| ID | Feature | Qt | Tauri | Evidence | Status |
|----|---------|----|-------|----------|--------|
| S-1 | Settings change only on daemon confirmation; invalid rejected | tst_settings | same | S: "settings change only when…" | ✅ |
| S-2 | Theme system/light/dark; follows OS | Theme singleton + `Qt.styleHints` | `prefers-color-scheme` + `data-theme` | E: 9 | ✅ (OS-follow untested) |
| S-3 | About: app, backend, protocol, toolkit versions, data dir | smoke_launch::settingsApplyLiveAndAboutShowsDetails | About sheet (Tauri + webview versions) | E parity: the Help ▸ About sheet shows the backend, protocol, Tauri and webview versions and the data directory | ✅ |
| S-4 | Window geometry persisted, never restored off-screen | Shell::restoreGeometry | host `window.json` in the app config dir, same rules | E (geometry suite) 1, 4: resize → quit through the app's path → relaunch restores 1000×700 | ✅ |
| S-5 | Side bar / panel toggles (Ctrl+B / Ctrl+J), resizable | Main.qml SplitView | splitters + toggles; **sizes persisted** (Qt does not) | screenshots | ✅ (+) |
| S-6 | Minimum window 720×480; side bar collapsed < 900 px | Main.qml | `minWidth/minHeight`; initial collapse rule | E (geometry suite) 2: a 400×300 request yields 720×480 | ✅ (size); collapse rule inspected |
| S-7 | Menu bar File/Edit/View/Run/Help | QML MenuBar (in-window) | In-window Radix menus File/Edit/View/Run/Help with the same items and shortcuts; a **native** macOS menu (T-07) dispatches the same action ids | screenshots; macOS menu compiled only in CI | ✅ (macOS menu unverified) |
| S-8 | Accessibility: names/roles on every control, focus ring, keyboard reachability | QML `Accessible.*` | ARIA roles (tree, tablist, log, dialog, menu, radiogroup), `aria-label` on icon buttons, `:focus-visible` ring, Radix focus traps | **no screen-reader test in either shell** | 🟡 |
| S-9 | Same data directory readable by both shells | — | same `axiom.db`, same keys | E: 8 reads state written via the same protocol; store shared by design | ✅ |
| S-10 | Quit confirms unsaved work from window close | Main.qml `onClosing` | Rust `CloseRequested` → UI prompt → `app_quit` | E (quit suite) 1–6: prompt lists the document, Cancel keeps window+text and writes nothing, Save writes and the app exits; the window-button → `CloseRequested` hop itself is 6 lines and is covered by the manual protocol (3.3) | ✅ (UI half end-to-end) |
| S-11 | `--version` without a display | main.cpp | `axiom-tauri --version` / `--help` answered before Tauri starts | R `meta_arguments_answer_without_a_display` | ✅ |
| S-12 | Native folder dialog | FolderDialog | tauri-plugin-dialog (xdg portal / GTK on Linux) | not driven in either | 🟡 |

### Wire types

Payload types are generated from `schemas/ipc/protocol.schema.json` (ADR-0005)
into `apps/desktop-tauri/src/daemon/protocol.gen.ts`; `pnpm run check:types`
fails when the committed file is stale (T-04).

### New in the prototype (not in Qt M1)

| Feature | Notes |
|---------|-------|
| Command palette (Ctrl/Cmd+K) | all menu actions, recent folders, theme switch, restart backend |
| Quick open of listed files (Ctrl/Cmd+P) | only files already listed by the explorer (a workspace index is M4 scope) |
| Panel sizes persisted | per machine, like window geometry |
| Monaco minimap, sticky scroll, smooth caret, tokenised theme | presentation only |

## Summary of gaps

| Gap | Severity | Plan |
|-----|----------|------|
| ~~E-12 renderer-crash recovery~~ | closed by T-08 (Linux verified; Windows/macOS use the heartbeat fallback until T-09) | — |
| E-6/E-8 untested branches (Reload, Cancel, Save-in-prompt, window close) | Medium | port the remaining tst_documents cases to `stores.test.ts` (T-05) |
| ~~E-9, T-9, S-11~~ | closed by T-06 | — |
| ~~T-9 output menu~~ | closed by T-06 | — |
| ~~S-7 menu structure~~ | closed by T-06/T-07 | — |
| ~~S-11 `--version`~~ | closed by T-03 | — |
| Everything on macOS and Windows | **High** (unknown) | T-09 CI matrix before any decision to remove Qt |
| Screen readers, IME, HiDPI, real GPU | **High** (unknown in both shells) | T-10 manual verification protocol |
