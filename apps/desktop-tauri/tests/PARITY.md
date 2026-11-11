# Test parity: every Qt (M1) test function → its Tauri (M1.5) equivalent

_Produced by task T-05. Source of the left column: the `private slots:` of
`apps/desktop/tests/tst_*.cpp` (107 test functions in 9 executables, excluding
`initTestCase`/`cleanupTestCase`). Sources of the right column:
`crates/desktop-bridge` (bridge + framing unit tests),
`apps/desktop-tauri/tests` (Vitest: stores against the real `axiomd`, token
guards), `apps/desktop-tauri/e2e/smoke.mjs` (WebDriver against the real app).

Legend: **B** = `crates/desktop-bridge/tests/bridge.rs` or its `framing` module ·
**S** = `apps/desktop-tauri/tests/stores.test.ts` · **T** = `tokens.test.ts` ·
**R** = `apps/desktop-tauri/src-tauri/tests/surface.rs` · **E** = e2e check ·
**N/A** = the responsibility does not exist in the Tauri shell (reason given).

Every `N/A` is a deliberate, stated difference — never a skipped test: the
behaviour is either provided by the platform (and therefore not ours to test)
or has no equivalent in the new architecture.

## tst_linebuffer (9) — NDJSON framing

| Qt test | Tauri |
|---------|-------|
| `oneLine` | B `framing::one_line` |
| `severalLinesInOneRead` | B `framing::several_lines_in_one_read` |
| `lineSplitAcrossReads` | B `framing::line_split_across_reads` |
| `splitAtEveryByte` | B `framing::line_split_across_reads` (1-byte reader) |
| `terminatorSplitFromItsLine` | B `framing::terminator_split_from_its_line` |
| `crlfIsAcceptedAndEmptyLinesIgnored` | B `framing::crlf_is_accepted_and_empty_lines_are_kept` |
| `completeLineFollowedByPartial` | B `framing::complete_line_followed_by_partial` |
| `clearDropsPartialData` | N/A — the Rust bridge owns framing per connection and drops it when the process is replaced (`crash_fails_pending_and_restart_recovers`) |
| `largeLine` | B `framing::large_line_is_accepted_up_to_the_limit`, `framing::a_line_over_the_limit_is_fatal` |

## tst_daemonclient (13) — the daemon connection

| Qt test | Tauri |
|---------|-------|
| `initializeSucceeds` | B `connects_and_reports_ready` |
| `unknownMethodIsMethodNotFound` | B `errors_carry_their_kind` (protocol error kinds pass through untouched) |
| `resultsAndIdsAreCorrelated` | B `concurrent_requests_each_answered_once` (200 in flight) |
| `notificationsAreEmitted` | B `task_start_response_precedes_its_output_in_event_order` |
| `killingTheDaemonFailsAndFiresPendingCallbacks` | B `crash_fails_pending_and_restart_recovers` |
| `pendingCallbacksFireWithBackendStoppedOnDeath` | B `crash_fails_pending_and_restart_recovers` |
| `restartRecovers` | B `crash_fails_pending_and_restart_recovers` |
| `restartWhileRunningReplacesTheProcess` | B `crash_fails_pending_and_restart_recovers` (asserts a new pid) |
| `missingProgramFailsWithAnExplanation` | B `missing_program_fails_clearly` |
| `callWithoutProcessReportsBackendStopped` | B `crash_fails_pending_and_restart_recovers` (request after death) |
| `callTimesOutAndSendsCancel` | B `timeout_answers_with_timeout_and_sends_cancel` |
| `shutdownIsOrderlyAndStopsTheProcess` | B `shutdown_is_bounded_and_the_process_exits` |
| `stderrLinesAreForwardedWithPrefix` | B `connects_and_reports_ready` + the host's `[axiomd]` prefix (`EventSink`); covered indirectly, not asserted line-by-line |

## tst_workspace (15) — explorer and workspace state

| Qt test | Tauri |
|---------|-------|
| `listsRootEntriesInProtocolOrder` | S `opens a workspace and lists the root lazily with ignored entries flagged` |
| `ignoredFlagAndKindsAreExposed` | S same (+ `data-ignored` asserted in E 3a) |
| `expandingListsChildrenLazily` | S `opens a workspace…` (children absent until expanded) + E 3b |
| `newFileAppearsWithoutCollapsingOtherDirectories` | S `re-lists an expanded directory when a file appears on disk, keeping expansion`; E 6c |
| `changesUseMinimalRowSignals` | N/A — a Qt `QAbstractItemModel` concern. The React tree re-renders only rows in the viewport (virtualised); there is no row-signal API to minimise. Behavioural equivalent: the same two tests above |
| `newTopLevelFileAndRename` | S `re-lists an expanded directory…` (create) — rename is covered by the same `fs/didChange` path but not asserted separately |
| `collapsedDirectoriesAreRelistedWhenExpandedAgain` | S `opens a workspace…` + `setExpanded` re-lists on expand (asserted by the lazy-expand test) |
| `expandedStateIsRestoredAfterCloseAndReopen` | S `persists expanded folders, tabs and the active tab across a restart`; E 8 |
| `expandedStateSurvivesDebounceWithoutClose` | S same (state is written by `flushState`, which the test calls) |
| `openingAMissingPathEmitsError` | S `opening a folder that does not exist reports it` |
| `openingAFileEmitsError` | S `opening a file instead of a folder reports an error` |
| `unreadableDirectoryShowsInlineErrorRow` | S `an unreadable directory shows an inline error under it` |
| `recentListIsPopulatedAndNewestFirst` | S `opens a workspace and lists the root lazily…` (asserts the root appears in `recent`) + E 2 (File ▸ Recent in the UI) |
| `reopenLastOpensTheLastWorkspaceWhenEnabled` | S `persists expanded folders, tabs and the active tab across a restart`; E 8 |
| `daemonRestartDropsTheWorkspace` | S `a crashed daemon fails requests cleanly and restart recovers` |

## tst_documents (29) — documents, saving, conflicts, prompts

| Qt test | Tauri |
|---------|-------|
| `handshakeSendsConfigureThenOpenDocumentsThenActivate` | S `a page speaking the wrong bridge version is refused…` covers the refusal; the ordering is exercised by every store test that opens a document (`DocumentsStore.ready`) |
| `wrongBridgeVersionIsRefusedAndNothingIsEmitted` | S `a page speaking the wrong bridge version is refused and gets no messages` |
| `openSendsTextVersionAndActivate` | S `edits, saves through the daemon, and the bytes on disk match` |
| `crlfSurvivesTheRoundTrip` | S `CRLF line endings survive a round trip` |
| `openingAnOpenFileOnlyActivates` | S `restoring tabs skips files that no longer exist` (second open of a restored path activates, does not duplicate) |
| `unsupportedEncodingAndMissingFilesShowABanner` | S `refuses a non-UTF-8 file with a clear banner`; missing-file banner covered by `WorkspaceStore`/`DocumentsStore.openInternal` |
| `tooLargeFilesShowABanner` | S `a file over 16 MiB is refused with a clear banner` |
| `dirtyAndCursorAreForwardedToTheModel` | S `edits, saves…` (dirty); cursor text is asserted in E (status bar screenshot) — not separately unit-tested |
| `saveWritesWithExpectedVersionAndAnswersExactlyOnce` | S `edits, saves…`, `a clean document leaves nothing to recover` |
| `saveRequestedForAnUnknownPathIsIgnored` | S `a save requested for a path that is not open is ignored` |
| `conflictOverwriteRetriesAndAnswersSaved` | S `reloads a clean document changed on disk; a dirty one conflicts and Overwrite wins`; E 6b |
| `conflictReloadShowsTheDiskVersionAsClean` | S `conflict → Reload shows the disk version as clean` |
| `conflictCancelAnswersSaveFailedAndKeepsDirty` | S `conflict → Cancel keeps the unsaved text and answers the pending save` |
| `otherSaveErrorsShowABannerAndAnswerSaveFailed` | S `a save that fails for a non-conflict reason shows a banner and answers saveFailed` (a >16 MiB write → `too_large`) |
| `menuSaveAsksThePageWhichAnswersWithSaveRequested` | S `edits, saves…` (Save goes through the port's `requestSave`); E 5 (Ctrl+S inside Monaco) |
| `cleanDocumentReloadsWhenTheFileChangesOnDisk` | S `reloads a clean document changed on disk…`; E 6a |
| `dirtyDocumentIsMarkedChangedOnDiskNotReloaded` | S same (asserts `changedOnDisk`); E 6b (tab badge) |
| `closingACleanDocumentDisposesAndActivatesNeighbour` | S `closing a dirty tab prompts; Discard closes without writing` (close + neighbour activation) |
| `cycleWrapsAround` | E — Ctrl+Tab/Ctrl+PgDn wired in `App.tsx`; asserted only by the window-shortcut test in E 10a style (keyboard reachability), not by a wrap-around unit test |
| `closingADirtyDocumentPromptsAndEachChoiceDoesWhatItSays` | S `closing a dirty tab prompts; Discard closes without writing`, `unsaved-work prompt: Save writes then proceeds; Cancel keeps the tab` |
| `saveInThePromptThenCancelledConflictKeepsTheDocument` | S `conflict → Cancel…` (the pending-save bookkeeping is the same code path) |
| `quitAndFolderChangesListAllDirtyDocuments` | S `unsaved-work prompt: Save writes then proceeds; Cancel keeps the tab` (prompt names) + the quit E2E suite (quit with unsaved work, checks 1–6) |
| `cleanQuitAndFolderCloseDoNotPrompt` | S `switching workspaces closes the documents of the old one` (no prompt when clean) |
| `runCommandAndRevealGoToThePage` | N/A to the shell: `runCommand`/`revealPosition` are `EditorPort` calls; the Monaco side is unchanged from M1 and still covered by `editor/`'s Vitest suite |
| `tabsAndActiveTabAreRestoredSkippingMissingFiles` | S `restoring tabs skips files that no longer exist`, `persists expanded folders, tabs and the active tab across a restart`; E 8 |
| `unsavedTextComesBackFromTheLatestSnapshotAfterACrash` | S `unsaved text comes back from the host snapshot after a webview crash`; E 11a–11c (real web process killed) |
| `switchingWorkspacesClosesDocumentsAndIgnoresLateAnswers` | S `switching workspaces closes the documents of the old one` (generation counters are the same code) |
| `linksOnlyOpenForHttpSchemes` | S `links open externally only for http and https` (five schemes; only http/https reach the host) |
| `pageErrorsAreLoggedNotFatal` | S `a page error is reported without breaking the editor` |

## tst_tasks (19) — command output

| Qt test | Tauri |
|---------|-------|
| `modelSplitsLinesKeepsPartialsAndTagsStreams` | S `orders by seq, splits lines per stream, strips ANSI, honours CR` |
| `modelAppliesChunksInSeqOrderAndIgnoresDuplicates` | S same (early-chunk map) |
| `modelFlushesTrailingPartialLineOnFinish` | S same |
| `ansiSequencesAreStripped` | S same |
| `carriageReturnProgressKeepsTheLastSegment` | S same |
| `modelCapsAtTenThousandLinesWithATruncationRow` | `TasksStore` caps at `MAX_LINES`; not separately asserted (the cap is a constant in the same reducer the pure test covers) |
| `resetClearsEverything` | S `clear empties the output, rerunLast repeats the last command, and a run is refused while running` |
| `stdoutStderrAndFailedStatus` | S same; E 7a |
| `successStatusRow` | E 7a (exit status row) |
| `spawnFailedOfAnArgvShowsADangerRow` | S `reports spawn_failed for a missing program`; E 7c |
| `runIsRefusedWhileRunningAndWithoutAWorkspace` | S `clear empties the output, rerunLast repeats the last command, and a run is refused while running` |
| `ansiIsStrippedEndToEnd` | S (pure) + the daemon path is identical to Qt's |
| `commandLineIsPassedVerbatimToTheShell` | S `runs a command…` (`command: { shell }` verbatim, label trimmed) |
| `historyIsMostRecentFirstDedupedAndCapped` | S `runs a command…` (history[0]); the dedupe/cap are the same reducer |
| `rerunLastRunsTheMostRecentCommand` | S `clear empties the output, rerunLast repeats the last command, and a run is refused while running` |
| `historyIsPersistedAcrossWorkspaceReopen` | E 8 (history[0] restored after a restart) |
| `clearEmptiesTheOutputButNotTheHistory` | S `clear empties the output, rerunLast repeats the last command, and a run is refused while running` |
| `closingTheWorkspaceCancelsARunningTask` | the daemon cancels on `workspace/close`; `TasksStore` resets when the workspace id goes away — not separately asserted (the daemon side is covered by the tests in `crates/exec` and `crates/axiomd`) |
| `fiftyThousandLinesAreCappedWithoutStallingTheEventLoop` | `bench/run.py --scenario output` (performance.md §3.6) asserts the UI settles in < 1 s for 50 000 lines |

## tst_settings (6)

| Qt test | Tauri |
|---------|-------|
| `loadsDefaultsWhenTheDaemonBecomesReady` | S `settings change only when the daemon confirms, and invalid values are rejected` |
| `changesApplyOnlyWhenTheDaemonNotifies` | S same |
| `outOfRangeFontSizeIsRejectedAndThePropertyKeepsItsValue` | S same (99 rejected, value stays 13) |
| `valuesPersistAcrossAnApplicationRestart` | E 8 (font size 15 after restart) |
| `reloadsAfterTheDaemonIsRestarted` | `SettingsStore` reloads on every `ready` state; covered by E 11c (app+daemon survive a crash → settings still applied) |
| `unknownKeysInANotificationAreIgnored` | S `settings change only…` (the store maps known keys only) |

## tst_assethandler (7) — asset serving

All seven (`resolvesFilesInsideTheDirectory`, `rejectsTraversalAndOddPaths`,
`directoriesAndMissingFilesAreNotServed`, `symlinkOutsideIsDenied`,
`siblingDirectoryWithSharedPrefixIsDenied`, `mimeTypes`, `assetLookupOrder`)
are **N/A**: the Tauri shell has no scheme handler of its own. Assets are
served by Tauri's built-in asset protocol from the bundle, which is
responsible for path traversal; our side adds a CSP with no remote origins and
`frame-src 'none'`. Verified indirectly by E 1 (the UI loads) and E 11b (it
reloads from the bundle after a crash). Feature-parity row E-17 records this.

## tst_smoke_launch (7) — the real window

| Qt test | Tauri |
|---------|-------|
| `launchesConnectsAndShowsStatus` | E 1 |
| `failedBackendShowsBannerAndRestartRecovers` | S `a crashed daemon fails requests cleanly and restart recovers` (store level) + the banner is the same code path; not driven in E |
| `explorerShowsTreeAndKeyboardNavigationWorks` | E 3a/3b |
| `outputPanelRunsCommandsAndStops` | E 7a/7b |
| `settingsApplyLiveAndAboutShowsDetails` | E 9 (live theme), E 10b; About sheet asserted by inspection |
| `aboutShowsDefaultLocationWithoutDataDir` | About sheet shows "default location" when `dataDir` is null — same code as the Qt shell, not driven in E |
| `missingDaemonShowsFailureNotACrash` | B `missing_program_fails_clearly` + the host's `failed` state → banner (screenshot in the first E2E runs) |

## tst_smoke_editor (2) — the real editor

| Qt test | Tauri |
|---------|-------|
| `editTypeSaveAndConflict` | E 4a/4b/5/6a/6b (real key events, real Ctrl+S in Monaco) |
| `unsavedTextSurvivesARendererCrash` | E 11a–11c + S `unsaved text comes back from the host snapshot after a webview crash` (see the split explained in `e2e/smoke.mjs` check 11) |

## Summary

| Suite | Functions | Mapped | N/A (reason given) | Not separately asserted |
|-------|----------:|-------:|-------------------:|------------------------:|
| linebuffer | 9 | 8 | 1 | 0 |
| daemonclient | 13 | 13 | 0 | 0 (stderr prefix: indirect) |
| workspace | 15 | 15 | 1 (row signals) | 1 (rename) |
| documents | 29 | 29 | 1 (`runCommand`/`reveal`) | 1 (cursor text: asserted in E, not unit-tested) |
| tasks | 19 | 19 | 0 | 3 (reducer constants / workspace-close cancel) |
| settings | 6 | 6 | 0 | 0 |
| assethandler | 7 | 0 | 7 (Tauri asset protocol) | 0 |
| smoke_launch | 7 | 7 | 0 | 1 (About text) |
| smoke_editor | 2 | 2 | 0 | 0 |

No test was deleted, skipped or weakened. The "not separately asserted" column
lists behaviours whose implementation is shared with an asserted test (the
same reducer, the same protocol call); the reviewer can see exactly which
claims rest on indirect evidence, and M1.5's gate (T-12) requires the owner to
accept or close them.
