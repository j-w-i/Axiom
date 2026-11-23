# Issue register

Audit of `main` at `751ffc2` (2026-11-22). Every issue has a stable id. Implementation was
authorised by the owner on 2026-11-23 for every issue (task AUD-REM-001, branch
`task/audit-remediation`); the current status of each issue is in **Remediation status** below
(each entry's Status line repeats it).

**Confidence**
- **Confirmed (runtime)** — reproduced in the running app or daemon during this audit; the
  evidence names the screenshot, log or measurement.
- **Confirmed (code)** — the code path is unambiguous on reading; not reproduced at runtime.
- **Suspected** — likely, but depends on runtime conditions not exercised.
- **Unverified** — plausible risk, not established.

**Evidence shorthand.** Raw static findings, with code excerpts, failure scenarios and the test
each reviewer proposed, are in `evidence/static/`:
`RC-n` = `rust-core.md` §n · `RS-Fn` = `rust-services.md` Fn · `FS-Fn` = `flutter-state.md` Fn ·
`FUI-nn` = `flutter-ui.md` FUI-nn. Screenshots are `evidence/screenshots/<name>.png`. Paths below
are relative to the repository root; `app/` means `apps/desktop-flutter/`.

Priority counts: **P0 1 · P1 12 · P2 49 · P3 50 · total 112.**


## Remediation status (2026-11-23)

Verification for every issue: the test or measurement named in its entry, plus the final gate
(`scripts/check.sh`, `scripts/check.sh e2e`) recorded in `AUDIT_STATE.md` §Implementation log.

| Issue | Status | Note |
|-------|--------|------|
| AUD-001 | Fixed | documents detach on daemon loss and re-attach on restart; recovery folder for shell crashes; data_safety_test.dart; a reload of a clean document keeps no recovery copy, and recovered text over a newer file asks before Save (review B1, B2; test/state/documents_safety_test.dart) |
| AUD-002 | Fixed | folder switch closes the old folder's documents; data_safety_test.dart step 6; a save held up by format-on-save, or queued behind another, is dropped when the folder changed (review B3, test/state/documents_safety_test.dart) |
| AUD-003 | Fixed | gutter strip full height; layout_test.dart checks it |
| AUD-004 | Fixed | split bounds never cross; layout_test.dart at 900×560 with dock and split |
| AUD-005 | Fixed | quadratic line loops removed (re_editor and shell), no highlight round trip above the size limit, far backward jumps no longer lay out every line between (re_editor `_code_field.dart`); 100k-line file: typing at line 1 max build 3938 → 5.7 ms, PSS after editing ~940 → 284 MiB (`evidence/perf/after-remediation-probe.json`) |
| AUD-006 | Fixed |  |
| AUD-007 | Fixed |  |
| AUD-008 | Fixed | built-in servers that run project code are refused or restricted until the folder is trusted; Trust Folder line above the editor |
| AUD-009 | Fixed |  |
| AUD-010 | Fixed |  |
| AUD-011 | Fixed |  |
| AUD-012 | Fixed | no spin; a 10 s poll fallback; `GitStatus.polling` (0.6, additive) and a "watch limited" note in the status bar |
| AUD-013 | Fixed |  |
| AUD-014 | Fixed | fitPanels() bounds side bar, dock and console by the window; layout_test.dart |
| AUD-015 | Fixed |  |
| AUD-016 | Fixed |  |
| AUD-017 | Fixed | the active tab is shown again when the strip itself narrows (dock, split, side bar; review B6d); layout_test.dart fails when the tab or strip is missing or the tab is outside the strip |
| AUD-018 | Fixed |  |
| AUD-019 | Fixed |  |
| AUD-020 | Fixed |  |
| AUD-021 | Fixed |  |
| AUD-022 | Fixed | protocol 0.6 fs/createFile, createDirectory, rename, trash (CR-AUD-022); explorer menu, F2, Delete, Shift+F10, right-click; File → New File…, New Folder…, Save As… |
| AUD-023 | Fixed | line breaks follow their lines when lines are added or removed (review B6a); test/state/doc_line_break_test.dart |
| AUD-024 | Fixed |  |
| AUD-025 | Fixed |  |
| AUD-026 | Fixed |  |
| AUD-027 | Fixed |  |
| AUD-028 | Fixed |  |
| AUD-029 | Fixed | an open during the last holder's teardown waits and opens afresh (review B5); multi_client.rs an_open_during_the_last_close_gets_a_workspace_that_stays_open |
| AUD-030 | Fixed | the LSP relay, approval prompts and agent/reveal go to every connection at once, bounded (review B6b); lsp.rs a_connection_that_stopped_reading_does_not_stop_diagnostics |
| AUD-031 | Fixed |  |
| AUD-032 | Fixed | no daemon-wide lock across start; dead agents restarted; idle MCP servers and resumable agents stopped after 15 min (`acp_idle_agents_that_can_resume_are_stopped`, `…_cannot_resume_are_kept`) |
| AUD-033 | Fixed |  |
| AUD-034 | Fixed |  |
| AUD-035 | Fixed |  |
| AUD-036 | Fixed |  |
| AUD-037 | Fixed |  |
| AUD-038 | Fixed |  |
| AUD-039 | Fixed |  |
| AUD-040 | Fixed |  |
| AUD-041 | Fixed |  |
| AUD-042 | Fixed |  |
| AUD-043 | Fixed with a deviation | Fixed with a deviation: Origin checked, a live page is never replaced; the token stays reusable for a reload |
| AUD-044 | Fixed |  |
| AUD-045 | Fixed |  |
| AUD-046 | Fixed |  |
| AUD-047 | Fixed |  |
| AUD-048 | Fixed |  |
| AUD-049 | Fixed |  |
| AUD-050 | Fixed |  |
| AUD-051 | Fixed |  |
| AUD-052 | Fixed |  |
| AUD-053 | Fixed |  |
| AUD-054 | Fixed |  |
| AUD-055 | Fixed |  |
| AUD-056 | Fixed |  |
| AUD-057 | Fixed |  |
| AUD-058 | Fixed |  |
| AUD-059 | Fixed |  |
| AUD-060 | Fixed | skips reported by e2e.sh and check.sh; layout_test.dart; data_safety_test.dart |
| AUD-061 | Fixed | scripts/perf.sh, AXIOM_STARTUP_TRACE, integration_test/perf_probe_test.dart; performance.md §4.1 |
| AUD-062 | Fixed | palette ranking memoised (AUD-049) and shell rebuilds narrowed (AUD-072): `frames.sh` on the final tree has no frame over 16.7 ms in any interaction (palette raster p99 24.6 → 6.8 ms) |
| AUD-063 | Fixed |  |
| AUD-064 | Fixed |  |
| AUD-065 | Fixed |  |
| AUD-066 | Fixed |  |
| AUD-067 | Fixed |  |
| AUD-068 | Fixed |  |
| AUD-069 | Fixed |  |
| AUD-070 | Fixed |  |
| AUD-071 | Fixed |  |
| AUD-072 | Fixed |  |
| AUD-073 | Fixed |  |
| AUD-074 | Fixed |  |
| AUD-075 | Fixed |  |
| AUD-076 | Fixed |  |
| AUD-077 | Fixed |  |
| AUD-078 | Fixed |  |
| AUD-079 | Fixed |  |
| AUD-080 | Fixed |  |
| AUD-081 | Fixed |  |
| AUD-082 | Fixed |  |
| AUD-083 | Fixed |  |
| AUD-084 | Fixed |  |
| AUD-085 | Fixed |  |
| AUD-086 | Fixed |  |
| AUD-087 | Fixed |  |
| AUD-088 | Fixed |  |
| AUD-089 | Fixed |  |
| AUD-090 | Fixed |  |
| AUD-091 | Fixed |  |
| AUD-092 | Fixed |  |
| AUD-093 | Fixed |  |
| AUD-094 | Fixed |  |
| AUD-095 | Fixed |  |
| AUD-096 | Fixed |  |
| AUD-097 | Fixed |  |
| AUD-098 | Fixed |  |
| AUD-099 | Fixed |  |
| AUD-100 | Fixed |  |
| AUD-101 | Fixed |  |
| AUD-102 | Fixed | crates/desktop-bridge removed; its daemon-side coverage moved to crates/axiomd/tests/lifecycle.rs |
| AUD-103 | Fixed |  |
| AUD-104 | Fixed | the gate judges the path through symlinks, and .git at any depth (review B4); approval.rs protected_paths_through_a_symlink_or_nested_still_ask |
| AUD-105 | Fixed |  |
| AUD-106 | Fixed |  |
| AUD-107 | Fixed with a residual risk | Fixed with a residual risk: pinned uv 0.9.0 and rustup 1.28.2 (not verified online: confirm before release); the first install of a checksum URL trusts its origin, later changes are refused |
| AUD-108 | Fixed |  |
| AUD-109 | Fixed (contract-limited) | line caps, 0600 credential file, MCP principals per connection, env.planInstall runs no program; its declared effect stays `read` because the agent-tool contract is frozen |
| AUD-110 | Fixed |  |
| AUD-111 | Fixed (owner binding kept) | Edit menu; palette ordered by recent use then category; New File/New Folder/Save As in File and the explorer; Ctrl+N stays New Chat, as the owner set it |
| AUD-112 | Fixed (review hand-off) | status docs, performance.md, security.md updated; each of the 44 `implemented` tasks now records the audit and the remediation in its status note. The formal reviews must be done by a different session: `REVIEW_GUIDE.md` forbids the session that fixed the findings from marking tasks verified |

---

## P0 — Critical

### AUD-001 — A daemon crash or "Restart the Backend" silently discards every unsaved edit
- **Category:** Functional / Data integrity
- **Priority:** P0
- **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/state/workspace.dart:96-105` (`_onDaemonState` → `_clearOpenState`), `app/lib/state/documents.dart:365,651-672` (`onClosed(_clearAll)`), `app/lib/app/app_state.dart:171-177` (`restartBackend`)
- **Description:** When `axiomd` exits for any reason (crash, OOM kill, `kill -9`) the workspace store treats it as a folder close and the documents store drops every document, dirty or not, without a prompt. "Restart the Backend" (palette and failure page) does the same with no confirmation. Requirement N3 says a daemon crash must not lose unsaved editor buffers.
- **Evidence:** Repro R2 (`evidence/logs/repro-results.json`): `dirtyBefore: true`, `docsAfterKill: []`, after restart `libOpenAfterRestart: true` but `textPreserved: false` (the file reopened from disk). Screenshots `repro-R2-after-daemon-kill.png`, `repro-R2-after-restart.png`, `49-backend-died.png`. Static: FS-F1.
- **Reproduction:** 1. Open a folder, open a file, type without saving. 2. `kill -9 $(pgrep -x axiomd)` (or palette → Restart the Backend). 3. The editor area shows "The backend stopped"; tabs are gone. 4. Click Restart the Backend. The folder and tab come back with the on-disk text; the edit is lost.
- **Root cause:** The shell couples document lifetime to the daemon's workspace id. `_onDaemonState` calls `_clearOpenState()` on any non-ready state, which fires the `_closed` callbacks; `DocumentsStore._clearAll` disposes all `Doc`s. Nothing preserves dirty text across a daemon generation.
- **Proposed fix:** Separate "the daemon lost its workspace" from "the person closed the folder". On a daemon loss keep `DocumentsStore.docs` (mark them detached, read-only until reconnected), and after `restartBackend()`/reconnect re-open the same root and re-attach each doc: for dirty docs keep the buffer and re-read the disk version for the conflict check (version mismatch → the existing "changed on disk" flow). Make "Restart the Backend" go through the same path, and prompt only if re-attach fails. Add a crash-recovery snapshot of dirty buffers to the data dir (also closes the owed "no recovery after shell crash", `verification/FL.md` §Owed).
- **Dependencies:** AUD-002 (same open/close plumbing), AUD-025 and AUD-086 (other state that must survive a restart).
- **Risk:** Medium: document identity is keyed by workspace-relative path; re-attach must handle the folder having disappeared (AUD-066) and LSP re-sync (`LspSync`) for every surviving doc.
- **Verification:** New e2e step (extend `integration_test/editor_test.dart`): type into a file, SIGKILL the daemon (`app.client.pid`), restart, assert the buffer text and dirty flag are unchanged and Save writes it. Same for palette "Restart the Backend".
- **Status:** Fixed (see Remediation status, 2026-11-23)

---

## P1 — High

### AUD-002 — Switching folders keeps the previous folder's documents; saving can write folder A's text into folder B
- **Category:** Functional / Data integrity
- **Priority:** P1
- **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/state/workspace.dart:136-206` (`openFolder` switch branch), `app/lib/state/documents.dart:839-840`, `app/lib/state/lsp.dart:95-96`
- **Description:** `openFolder` on an open workspace closes the old workspace on the daemon but never fires the `_closed` callbacks, so the documents store, LSP sync and Problems keep folder A's documents. Their paths are workspace-relative, so they now address folder B's files.
- **Evidence:** Screenshot `47-switch-folder-dirty-prompt.png`: explorer shows the new folder (`a.txt` only) while the editor still shows the old folder's tabs, including the dirty `lib.rs`. Static: FS-F2 (shows Overwrite on the resulting conflict writes A's text into B's `README.md`).
- **Reproduction:** 1. Folders A and B both contain `README.md` with different text. 2. Open A, open `README.md`. 3. File → Open Folder… → B. 4. The tab still shows A's text. 5. Type, Ctrl+S, choose Overwrite on the conflict: B's `README.md` now contains A's text.
- **Root cause:** The switch path resets the tree and id but skips `_clearOpenState()`/`_closed`; only `close()` and daemon loss notify listeners.
- **Proposed fix:** In `openFolder`, after the dirty-work prompt (already done by `requestOpenFolder`) fire the closed callbacks for the old workspace before opening the new one (call a shared `_closeCurrent()` used by `close()` as well). Add an assertion that every `Doc` belongs to the current workspace id.
- **Dependencies:** AUD-001 (same plumbing; fix together), AUD-092 (Chat scope flip on switch has the same transient state).
- **Risk:** Low–medium: tab restore for the new folder must not race the clear.
- **Verification:** e2e: open A + file, switch to B, assert `documents.docs` is empty or B's, Problems empty, LSP `didClose` sent for A's docs.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-003 — Git gutter marks are drawn half an editor-height below the lines they describe
- **Category:** GUI / Visual (functional indicator wrong)
- **Priority:** P1
- **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/editor/code_view.dart:206-230` (gutter `Row`), `:253-320` (`GitMarkStrip`, `_MarksPainter`)
- **Description:** Added/modified/deleted marks appear far below their lines (in a 760 px window, line 1's mark is ~300 px under line 1). In any file taller than half the view, marks for the lower half are drawn off-screen, so the gutter shows changes on the wrong lines or not at all.
- **Evidence:** `repro-R1-gutter-fresh.png` (fresh session, a modified file opened first: text lines at y≈145–265, marks at y≈438 and y≈518), `repro-R1c-gutter-after-typing.png` (marks move with edits but keep the offset), `14-dirty.png`, `18-split.png` (both groups, different offsets). The offset equals half the editor's height at 1180×760 and at 1440×900.
- **Reproduction:** Open a Git repository, modify a committed file on disk, open it in Axiom. Compare the green/brass bars in the gutter with the changed lines.
- **Root cause:** `GitMarkStrip` is `SizedBox(width: 3, child: CustomPaint(...))` with no height, inside a `Row` whose default `crossAxisAlignment` is `center`. The `CustomPaint` has no child and lays out at height 0, so it is placed at the Row's vertical centre; the painter draws each paragraph at `p.offset.dy` relative to that centre.
- **Proposed fix:** Give the strip the gutter's full height: `SizedBox(width: 3, height: double.infinity, …)` or `crossAxisAlignment: CrossAxisAlignment.stretch` on the indicator `Row` (check `DefaultCodeLineNumber` still aligns). One-line change.
- **Dependencies:** None.
- **Risk:** Low.
- **Verification:** Widget or e2e test that reads the strip's `RenderBox` global rect and asserts its top equals the editor viewport's top and its height equals the viewport's; screenshot comparison of a known diff (mark rects align with line rects from `CodeIndicatorValueNotifier.paragraphs`). The existing `git_test.dart` only checks mark values, which is why this shipped.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-004 — Splitting the editor throws during build when the editor column is narrower than 480 px
- **Category:** GUI / Functional
- **Priority:** P1
- **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/editor/editor_area.dart:76-90` (`clamp(240.0, box.maxWidth - 240)`), `app/lib/ui/splitter.dart:62-63`
- **Description:** With a split open, `double.clamp(240, maxWidth-240)` throws `ArgumentError` when `maxWidth < 480`; the whole editor area is replaced by Flutter's error widget (grey in release) until the split is closed. With the default side bar (260) and AI dock (380) this happens in any window narrower than ~1175 px, including a half-screen tiled window on a 1920 px display and the 900×560 minimum window.
- **Evidence:** `repro-R5-split-min-window.png` (red error box, "Invalid argument(s): 240.0"); exploratory run at 640×480 logged the same exception (`evidence/logs/explore-results.json`, step "window 640x480"). Static: FUI-01.
- **Reproduction:** Window ≈ 1000 px wide, side bar visible, open a folder and a file, Mod+L (AI dock), Mod+\ (split).
- **Root cause:** Bounds computed from available width without guarding `min > max`; the splitter repeats the same clamp.
- **Proposed fix:** Compute `minPane = min(240, maxWidth / 2)` and clamp to `[minPane, maxWidth - minPane]`; when `maxWidth < 2 * 160`, render the groups stacked or show only the active group with a "split hidden — window too narrow" affordance. Apply the same guard in `AxSplitter`.
- **Dependencies:** AUD-014 (panel sizes not bounded by the window) shares the root cause "layout bounds ignore available space"; fix in one batch.
- **Risk:** Low.
- **Verification:** Widget test pumping `EditorArea` with a split at widths 300, 479, 480, 900 asserts no exception; e2e at 900×560 with dock + split.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-005 — Typing near the top of a very large file freezes the UI for seconds and multiplies memory
- **Category:** Performance
- **Priority:** P1
- **Confidence:** Confirmed (runtime, measured twice)
- **Affected components:** editor edit path: `app/lib/state/documents.dart:138-166,321-332`, `app/lib/editor/lsp/lsp_editor.dart:117-135`, vendored `app/third_party/re_editor` (controller/undo), highlight workers
- **Description:** In a 100 000-line Rust file (≈6 MB), typing 13 characters at line 1 produced one frame whose build took **4.05 s** (run 2) and **4.47 s** (run 3); typing at the end gave frame spans p99 of ~2 s. Shell PSS rose from **240 to 939 MiB** (run 2) / **257 → 947 MiB** (run 3) after ~34 keystrokes in that file and did not come back.
- **Evidence:** `evidence/perf/audit-perf-*.json` (`huge_typing_at_start`, `huge_typing_at_end`, `mem_2_huge_open` vs `mem_3_huge_edited`), profile build, release `axiomd`, private KWin at 125 %. Static FS-F7 lists the whole-document passes per keystroke (dirty check, line-break join, LSP full-text sync for full-sync servers).
- **Reproduction:** `audit_perf_test.dart` (harness), or manually: open a 100k-line file, put the caret on line 1, type.
- **Root cause:** Unknown in detail (no CPU profile captured). Suspects, in order: per-keystroke whole-document work on the UI isolate (FS-F7: `Doc.text` joins every line for the dirty/length check and LSP sync), re_editor re-indexing lines after an insert at the top, and undo history retaining whole-document snapshots (the ~+700 MiB is consistent with ~20 MiB retained per keystroke).
- **Proposed fix:** First profile with DevTools (CPU profile + memory snapshot) on the same workload to attribute the 4 s and the 700 MB. Then: keep a running length/hash instead of joining text per key; send incremental `didChange` (and only build full text for full-sync servers, off the UI isolate); cap or diff-encode undo snapshots in the vendored re_editor; debounce git-gutter and dirty recomputation for documents above a size threshold.
- **Dependencies:** AUD-015 (open latency of the same file).
- **Risk:** Medium: re_editor is vendored with a patch (`third_party/re_editor/PATCHES.md`); undo changes touch editing correctness.
- **Verification:** `audit_perf_test.dart` scenario as a committed frame benchmark: typing at line 1 of a 100k-line file, budget p99 build ≤ 16.7 ms and PSS growth ≤ 50 MiB over 50 keystrokes; compare before/after with interleaved runs.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-006 — Workspace search is quadratic in line length; one minified file stalls search for minutes
- **Category:** Performance / Reliability
- **Priority:** P1
- **Confidence:** Confirmed (runtime, measured)
- **Affected components:** `crates/workspace/src/search.rs:366-407`
- **Description:** For each match the line is rescanned from its start, and all of a file's matches are collected before the result limit is applied; cancellation is checked only between files.
- **Evidence:** `evidence/perf/search-scaling.txt` (release `axiomd`, one file of N × `a`, query `a`): 50 KB 1.8 s, 100 KB 7.1 s, 200 KB 33 s, 400 KB 134 s — doubling the size quadruples the time. Extrapolated, a 2 MB minified bundle takes about an hour. Harness: `evidence/harness/search_probe.py`. Static: RC-4 (also ~1 GB allocation).
- **Reproduction:** Put a minified `bundle.min.js` (not git-ignored) in a folder; search for `e`.
- **Root cause:** Column/preview computation per match restarts from the beginning of the line; no per-file match cap.
- **Proposed fix:** Iterate matches with a single forward scan (carry byte/char offset), stop collecting a file's matches at the remaining limit, check cancellation per N matches, and cap preview length; consider skipping lines longer than e.g. 10 000 chars for preview (report the match count only).
- **Dependencies:** None.
- **Risk:** Low.
- **Verification:** Rust test: a 2 MiB single-line file searched with a 1-char query completes in < 1 s and respects `limit`; cancel test with a slow file.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-007 — A task stays "running" until every background grandchild exits; cancel cannot interrupt the wait
- **Category:** Functional / Reliability
- **Priority:** P1
- **Confidence:** Confirmed (runtime) for the hang; Confirmed (code) for cancel and close
- **Affected components:** `crates/exec/src/executor.rs:214-233,339-379`
- **Description:** The supervisor waits for EOF on stdout/stderr after the child exits. A grandchild that inherits the pipes (`npm run dev &`, `nohup …`, daemonising tools) keeps the task running indefinitely; `task/cancel`, `workspace/close` (which the shell awaits during a folder switch) and the shutdown hook wait with it.
- **Evidence:** `axiom task run --workspace <tmp> -- sh -c 'sleep 20 & echo started'` printed `started` immediately and returned after **20 043 ms** (shell exited at once). Static: RC-1.
- **Reproduction:** Output panel → run `sh -c 'sleep 600 & echo hi'`. The task shows running for 10 minutes; Cancel does not end it.
- **Root cause:** EOF on the pipes is used as the task's end; the process group is not signalled once the direct child has exited and reaping is not bounded.
- **Proposed fix:** When the direct child exits, wait a bounded drain period (e.g. 2 s, as already done for delegates in `78eb407`) and then close the pipes and report the exit; on cancel, kill the task's process group and stop reading immediately.
- **Dependencies:** AUD-027 (orphaned children) — same process-group policy.
- **Risk:** Low–medium: output from a lingering grandchild after the drain is lost (document it).
- **Verification:** Rust test in `crates/exec`: `sh -c 'sleep 30 & echo x'` completes in < 3 s with exit 0; cancel of a task blocked on a grandchild returns in < 2 s.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-008 — Language servers start in untrusted workspaces and execute the workspace's code
- **Category:** Security
- **Priority:** P1
- **Confidence:** Confirmed (code) — gate missing; server behaviour is upstream default, not exercised
- **Affected components:** `crates/axiomd/src/handlers/lsp.rs:25-60`, `crates/axiomd/src/handlers/env.rs:19-36,145`
- **Description:** Opening a folder and a file starts its language server automatically. rust-analyzer runs `build.rs` and proc macros, jdtls runs Gradle build scripts, the TypeScript server loads the workspace's `typescript`. Workspace trust exists (`workspace_trusted`) but only gates `.axiom/providers`. Opening a cloned repository therefore runs its code with no prompt.
- **Evidence:** RS-F1; `grep trust crates/axiomd/src/handlers/lsp.rs` finds no check.
- **Reproduction:** (Not executed — would run untrusted code.) A repo whose `build.rs` writes a file; open it, open `src/main.rs` with rust-analyzer installed.
- **Root cause:** Trust model applied to providers, not to servers, despite the same exposure.
- **Proposed fix:** Gate `lsp` start on workspace trust: in an untrusted folder start servers in a restricted mode where the server supports it (rust-analyzer `cargo.buildScripts.enable=false`, `procMacro.enable=false`; jdtls without Gradle import) or not at all, and show the existing trust prompt ("Trust this folder to enable language features that run its build scripts"). Record in `security.md`.
- **Dependencies:** none; touches the M5.5 auto-start flow (`lsp.autoInstall`).
- **Risk:** Medium: UX change to first-open; needs owner decision on the default.
- **Verification:** Daemon test: untrusted workspace → `lsp` start refused or started with restricted init options (assert the options sent to fake-lsp); trusted → normal.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-009 — "Allow for session" grants are wider than what the person approved
- **Category:** Security (agent permission gate)
- **Priority:** P1
- **Confidence:** Confirmed (code)
- **Affected components:** `crates/agent/src/gate/mod.rs:275-288,347`, `crates/agent/src/gate/policy.rs:79-84`
- **Description:** A session grant matches argv as a prefix (approving `rm x` also allows `rm x -rf …`), grants zero-target tools (`env.install`, MCP tools) outright, is never cleared (`clear_grants` has no caller), is not scoped to a workspace, and MCP principals share grants by a self-declared name.
- **Evidence:** RS-F2 (code excerpts).
- **Reproduction:** Agent requests `task.run ["rm","x"]`; approve for session; agent then runs `["rm","x","-rf","/home/…"]` without a prompt.
- **Root cause:** Grant key is too coarse; lifetime not tied to session/workspace end.
- **Proposed fix:** Match grants on exact argv (or an explicit pattern the person sees), key them by (session, workspace, tool, target), call `clear_grants` on session end and workspace close, never grant zero-target tools for the session without showing what future calls may do, and bind MCP principals to the connection, not a declared name.
- **Dependencies:** AUD-040 (ACP answers → grants), AUD-104 (policy file writable by the agent).
- **Risk:** Medium: more prompts; the owner should confirm the intended UX.
- **Verification:** Gate unit tests for each widening case; e2e approval flow in `agent_test.dart`.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-010 — Processes started by agents inherit the daemon's full environment; the secret stripping `security.md` describes is not implemented
- **Category:** Security
- **Priority:** P1
- **Confidence:** Confirmed (code; `grep` finds no stripping in `crates/exec` or `tools.rs`)
- **Affected components:** `crates/axiomd/src/tools.rs:669-724`, `crates/exec/src/backend.rs:58-63`, `crates/agent/src/credentials.rs:99-131`, `docs/architecture/security.md:169-171`
- **Description:** `task.run`, `acp.terminal` and agent harness processes get every variable of the daemon (`*_TOKEN`, `*_KEY`, cloud credentials, `GITHUB_TOKEN`); API keys configured through `api_key_env` are also inherited by every child process, contradicting `credentials.rs`'s own header. A prompt-injected agent can read and exfiltrate them with one approved command (or one that the session grant already covers, AUD-009).
- **Evidence:** RC-5; RS-F18 (api_key_env); `security.md` lines 169-171 claim the stripping.
- **Reproduction:** `export GITHUB_TOKEN=x` before launching; ask the agent to run `env`; output shows the token.
- **Root cause:** The documented control was never built.
- **Proposed fix:** Implement the documented filter in one place (`exec` spawn for agent-initiated work): remove variables matching `*_TOKEN`, `*_KEY`, `*_SECRET`, `*PASSWORD*` and known provider names unless allowed by workspace policy; never export `api_key_env` values to children. Add the rule to the approval card ("environment: 12 variables removed").
- **Dependencies:** AUD-039 (agent-set env in ACP terminals).
- **Risk:** Low–medium: some builds legitimately need tokens; the allow-list covers them.
- **Verification:** Daemon test: agent `task.run ["env"]` with `FOO_TOKEN` set → absent from output; GUI-initiated task → present.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-011 — `agent/send` leaves a session stuck "running" when it fails after `live.begin()`
- **Category:** Reliability (agent)
- **Priority:** P1
- **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/sessions.rs:387-424,512-538`
- **Description:** Any error after `live.begin()` (e.g. a malformed `[agent.policy]` in `.axiom/project.toml`) returns without ending the live turn; every later send gets `conflict` until the daemon restarts.
- **Evidence:** RC-2.
- **Reproduction:** Put an invalid `[agent.policy]` table in `.axiom/project.toml`, send a chat message (error), send again → "conflict".
- **Root cause:** Missing guard/RAII for the live-turn state.
- **Proposed fix:** Wrap the turn in a guard that calls `live.end()` on every exit path (drop guard), and validate the project policy before `begin()`.
- **Dependencies:** AUD-025 (shell side stays "writing").
- **Risk:** Low.
- **Verification:** Daemon test with a malformed policy: first send errors, second send is accepted.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-012 — Git change watch busy-loops when the `.git` watch cannot be created
- **Category:** Performance / Reliability
- **Priority:** P1
- **Confidence:** Suspected (code path confirmed; trigger — inotify limit — not exercised)
- **Affected components:** `crates/axiomd/src/handlers/git.rs:51-83`
- **Description:** If creating the `.git` watcher fails (e.g. `fs.inotify.max_user_watches` exhausted, common with large `node_modules` trees or several IDEs open), the loop retries without backoff, pins a core and emits `git/didChange` every 300 ms, so the shell runs `git status` continuously.
- **Evidence:** RC-3.
- **Reproduction:** `sudo sysctl fs.inotify.max_user_watches=<low>`; open a repository; watch CPU and `git/didChange` traffic.
- **Root cause:** Error path loops back into setup immediately and signals a change each time.
- **Proposed fix:** On failure, fall back to a single polling timer (e.g. 5 s, only while the window is focused), log once, and surface "file watching limited" in the status bar.
- **Dependencies:** AUD-031 (blocking watch setup on async threads).
- **Risk:** Low.
- **Verification:** Unit test with an injected watcher factory that fails: no more than one `didChange` per poll interval; CPU-time assertion.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-013 — Menus and pickers always open downward/rightward with no flip or clamp, so they run off the window
- **Category:** GUI
- **Priority:** P1
- **Confidence:** Confirmed (code); not reproduced at runtime (the exploratory run's menus at 1440×900 fit)
- **Affected components:** `app/lib/ui/overlays.dart:187-203,292-322` (`AxMenu`)
- **Description:** The model picker in the chat composer (≈40 px above the window's bottom) opens off-screen, and past the right edge in the dock; the 22-item View menu (~580 px) is cut off in short windows (the minimum is 560 px).
- **Evidence:** FUI-02.
- **Reproduction:** Chat with no conversation → click the model name in the composer. Window 900×560 → open View.
- **Root cause:** Fixed anchor placement; no available-space measurement, no scroll container.
- **Proposed fix:** Measure the overlay against the window: open upward/leftward when there is more room there, clamp to the window's edges, and make the menu scroll when taller than the available height. Reuse for tooltips (AUD-051) and hover cards (AUD-052).
- **Dependencies:** AUD-051, AUD-052 (one positioning helper).
- **Risk:** Low.
- **Verification:** Widget tests placing an anchor near each edge assert the menu rect is inside the window; screenshot check at 900×560.
- **Status:** Fixed (see Remediation status, 2026-11-23)

---
## P2 — Medium

Entries for static findings are kept short; the code excerpt, full scenario and proposed test are
in the referenced `evidence/static/` section, which is part of each entry's evidence.

### AUD-014 — Side bar, AI dock and console sizes are not bounded by the window; the editor collapses or the layout overflows
- **Category:** GUI / Layout · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/shell/shell.dart:721-788` (splitters: side 180–520, console 90–640, dock 300–720, all absolute)
- **Description:** Persisted sizes ignore the window. At the default 1180×760 window with the side bar at 520 and the dock at 720 the editor disappears and the dock overflows 115 px past the window; at the 900×560 minimum a 640 px console pushes the main area to a 149 px overflow.
- **Evidence:** `repro-R4-side-dock-max.png` (15 framework errors), `repro-R6-console-max-min-window.png` (31 errors: "BOTTOM OVERFLOWED BY 149 PIXELS"). Static FUI-05.
- **Reproduction:** Drag the side-bar splitter and the dock splitter to their maxima in a 1180×760 window; or make the console tall then shrink the window to its minimum.
- **Root cause:** Splitter bounds are constants; `SizedBox` widths from prefs are applied without considering the remaining space.
- **Proposed fix:** Lay out the shell row with a resolver that gives the editor a minimum (e.g. 320 px) and shrinks side bar → dock → console proportionally when the window is too small (keep the stored preference, apply a clamped effective size); derive splitter `max` from the current constraints.
- **Dependencies:** AUD-004 (same root cause).
- **Risk:** Low.
- **Verification:** Widget test over a grid of window sizes × stored sizes asserting no overflow and editor width ≥ minimum.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-015 — Opening a 100 000-line file blocks the UI thread for about a second
- **Category:** Performance · **Priority:** P2 · **Confidence:** Confirmed (runtime, measured twice)
- **Affected components:** document open path (`app/lib/state/documents.dart` open, re_editor `CodeLineEditingController` construction)
- **Description:** `open_100k_lines_ms` 1152 / 1238 ms; the first second after opening contained one frame with a 1005 / 1091 ms build (`huge_first_second`).
- **Evidence:** `evidence/perf/audit-perf-*.json`.
- **Reproduction:** Open a 6 MB, 100k-line source file.
- **Root cause:** Unknown in detail; text split into lines and controller/highlight setup run synchronously on the UI isolate.
- **Proposed fix:** Profile; split lines on a background isolate (as the git diff already does) and show a progress state; defer highlighting of off-screen lines.
- **Dependencies:** AUD-005.
- **Risk:** Low.
- **Verification:** Frame benchmark: open 100k-line file, max build ≤ 50 ms, no frame > 100 ms.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-016 — Deeply nested folders overflow explorer rows; the leaf row renders blank
- **Category:** GUI / Layout · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/files/explorer.dart:243-260` (row `Row`: indent `depth * _indent + s4`, fixed 16+4+16+4 px before the `Expanded` name)
- **Description:** Indentation is unbounded; at about 15 levels in the default 260 px side bar the fixed-width parts exceed the row, the name's `Expanded` gets zero width and the row overflows (12–26 px reported). Deep trees are common (Java packages, `node_modules`, generated code).
- **Evidence:** `07-explorer-deep.png` (leaf row blank, overflow stripes), framework errors at `explorer.dart:243` in `explore-results.json`.
- **Reproduction:** Folder `deep/a/b/…/p/leaf.txt`, expand every level.
- **Root cause:** No cap or compression of indentation and no horizontal scroll.
- **Proposed fix:** Cap the indent (e.g. compress per-level indent after depth 8, or limit to 40 % of row width) and keep a minimum name width; show the full path in the hint/tooltip; optionally horizontal scroll for the tree.
- **Dependencies:** None.
- **Risk:** Low.
- **Verification:** Widget test with depth 30 at width 180: no overflow, name visible.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-017 — The active editor tab is not scrolled into view; the tab strip cannot be scrolled with a mouse wheel
- **Category:** GUI / Usability · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/editor/editor_area.dart:160-175` (horizontal tab list)
- **Description:** After opening 16 files in a 1180 px window, the active tab sits at x≈3084 (off-screen); nothing indicates more tabs exist. Only 6 tabs fit (tabs are ~170 px wide).
- **Evidence:** `repro-R3-tabs-16.png`; R3 result `activeTabRects` (x 3083–3260, view width 1180). Static FUI-11.
- **Reproduction:** Open 16 files; look for the active tab.
- **Root cause:** No `ensureVisible` on activation; vertical wheel not mapped to horizontal scroll.
- **Proposed fix:** On active-tab change, `Scrollable.ensureVisible` the tab; map vertical wheel deltas to horizontal scroll on the strip; add edge fade + an overflow menu listing all open tabs.
- **Dependencies:** AUD-059 (`AxTabs` is the same pattern elsewhere).
- **Risk:** Low.
- **Verification:** e2e: open 20 files, assert active tab rect within the strip; wheel test.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-018 — The "changed on disk" dialog focuses the destructive "Overwrite" by default
- **Category:** Usability / Data safety · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/shell/shell.dart:125-140` (`AxQuestion` actions order; first action focused)
- **Description:** Pressing Enter on the conflict dialog overwrites another program's changes. The safe default for a conflict is Cancel (or a diff).
- **Evidence:** `17-conflict-dialog.png` (focus ring on Overwrite), `repro-R8-save-after-delete.png`.
- **Reproduction:** Edit a file, change it on disk, Ctrl+S, press Enter.
- **Root cause:** `AxQuestion` focuses its first action; the conflict lists Overwrite first.
- **Proposed fix:** Give `AxQuestion` an explicit `defaultAction`; use Cancel for the conflict dialog; offer "Compare" (the diff view exists) as a fourth action.
- **Dependencies:** AUD-019.
- **Risk:** Low.
- **Verification:** Widget test: dialog opens with focus on Cancel; Enter resolves `cancel`.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-019 — A file deleted on disk is presented as "changed on disk"; Save does not recreate it and "Reload" would discard the buffer
- **Category:** Functional / Usability · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** `app/lib/state/documents.dart:679-725` (save/conflict), `app/lib/shell/shell.dart:125-140`
- **Description:** With a dirty document whose file was deleted, Ctrl+S shows "file_number_3.txt changed on disk … reload the version on disk" — there is no version on disk. After the save attempt the file still does not exist and the doc stays dirty (R8: `existsAfterSave: false`, `dirtyAfterSave: true`). The tab gives no "deleted" indication.
- **Evidence:** `repro-R8-dirty-file-deleted.png`, `repro-R8-save-after-delete.png`, `repro-results.json` R8.
- **Reproduction:** Open a file, type, delete it from a terminal, Ctrl+S.
- **Root cause:** The deleted case is folded into the version-mismatch conflict.
- **Proposed fix:** Distinguish `not_found` on save: dialog "X was deleted on disk — Save (recreate) / Close / Cancel"; mark deleted docs on their tab (strikethrough or "deleted" chip) from `fs/didChange`.
- **Dependencies:** AUD-018.
- **Risk:** Low.
- **Verification:** e2e R8 scenario asserts the dialog text and that Save recreates the file with the buffer.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-020 — System text scaling clips text in fixed-height rows
- **Category:** Accessibility / Layout · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** fixed row heights in `app/lib/files/explorer.dart`, `app/lib/scm/scm_page.dart` (diff rows), tabs, menus (`Metrics.*Height` in `app/lib/ui/tokens.dart`)
- **Description:** At text scale 1.5 (GNOME "Large Text" sets this through GTK), diff lines are cut off at the descenders, explorer labels collide, icons stay at 1×.
- **Evidence:** `42-textscale-1.5.png`, `43-settings-textscale-1.5.png`.
- **Reproduction:** Set text scaling 1.5 in the desktop's accessibility settings; open a folder and a diff.
- **Root cause:** Row heights are constants instead of derived from the scaled line height.
- **Proposed fix:** Derive row heights from `MediaQuery.textScalerOf(context).scale(lineHeight)` (or cap the honoured scale with a documented maximum, e.g. 1.3, and scale rows accordingly).
- **Dependencies:** None.
- **Risk:** Low–medium (many surfaces).
- **Verification:** Golden/screenshot tests at text scale 1.0/1.3/1.5 for explorer, diff, tabs, menus with no clipping (assert text painter height ≤ row height).
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-021 — Every list row is its own Tab stop (no roving focus); keyboard users cannot Tab past a long tree
- **Category:** Accessibility · **Priority:** P2 · **Confidence:** Confirmed (runtime) for the explorer; Confirmed (code) for other lists
- **Affected components:** explorer, Problems, Search results, Source Control, conversations, palette rows; inactive tabs' hidden close buttons (FUI-19)
- **Description:** In the exploratory run 25 Tab presses moved through the rail and then through 20 explorer rows one by one; with 3 000 rows Tab cannot reach the editor. Requirement N7: keyboard access to every action.
- **Evidence:** `explore-results.json` step "keyboard focus traversal" note (sequence of `row deep/a/...`). FUI-19.
- **Reproduction:** Open a folder, click the explorer, press Tab repeatedly.
- **Root cause:** Each row's `Pressable` has its own focus node in traversal order.
- **Proposed fix:** One Tab stop per list (`FocusTraversalGroup` + roving tabindex: arrows move within, Tab leaves); exclude invisible close buttons from traversal.
- **Dependencies:** AUD-048 (palette focus trap).
- **Risk:** Low.
- **Verification:** `a11y_test.dart`: from the explorer, one Tab reaches the next region.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-022 — No way to create, rename or delete files or folders, no Save As, and no Edit menu
- **Category:** Usability / Functional gap · **Priority:** P2 · **Confidence:** Confirmed (runtime + code)
- **Affected components:** `app/lib/files/explorer.dart` (no context menu), `app/lib/shell/shell.dart` menus (File, View, Preferences, Help), protocol `fs/*` (no create/rename/remove methods)
- **Description:** The only file operations are open/edit/save. A new file can only be made from the terminal or by the agent. Ctrl+N opens a new chat. There is no Edit menu (undo, redo, cut, copy, paste, find); the retired Tauri shell had Edit and Run menus. Not listed in requirements (R1 says "browse, open, edit and save"), so this is a product gap rather than a regression of a requirement, but first-time users will expect it.
- **Evidence:** `03-file-menu.png`; `grep` finds no fs create/rename/delete method in `protocol.g.dart`; `docs/assets/tauri/09-workbench-dark.png` shows Edit and Run menus.
- **Reproduction:** Try to add a file in the explorer.
- **Root cause:** Not scoped yet.
- **Proposed fix:** Owner decision. If approved: protocol additions (`fs/create`, `fs/rename`, `fs/remove` to trash) through a change request (frozen contracts), explorer context menu, File → New File / Save As, an Edit menu bound to the editor's existing actions.
- **Dependencies:** governance change request for the protocol.
- **Risk:** Medium (contract change).
- **Verification:** e2e: create, rename, delete (to trash) from the explorer; Save As writes a new path.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-023 — A file with mixed line breaks is marked modified without an edit, and saving rewrites its line breaks
- **Category:** Functional / Data integrity · **Priority:** P2 · **Confidence:** Confirmed (code). (Pure CRLF is fine at runtime: exploratory step "CRLF file not dirty" passed.)
- **Affected components:** `app/lib/state/documents.dart:93-97,321-332`; `app/third_party/re_editor/lib/src/code_line.dart:1175`
- **Description / repro:** `printf 'one\r\ntwo\nthree\r\n' > mixed.txt`, open, click → tab modified; Save rewrites `two\n` as `two\r\n`. FS-F3.
- **Root cause:** One line-break style chosen per document; the dirty check compares the re-joined text.
- **Proposed fix:** Keep per-line terminators (or the original text) and compare against it; on save preserve existing terminators and use the dominant style only for new lines.
- **Dependencies:** AUD-005 (same dirty-check path). **Risk:** Low.
- **Verification:** Extend `test/state/doc_line_break_test.dart` with mixed and stray-CR files.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-024 — Two saves of one document in flight race on `expectedVersion`, producing a false conflict for your own save
- **Category:** Functional · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/state/documents.dart:679-725`
- **Description:** Ctrl+S twice quickly (or format-on-save) sends two writes with the same expected version; the second is rejected as "changed on disk", and Reload drops text typed in between. FS-F4.
- **Proposed fix:** Serialise saves per document (queue; coalesce a pending save) and update the expected version from the first write's result before the second.
- **Risk:** Low. **Verification:** Unit test with a fake client delaying the first write.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-025 — After a daemon crash or restart mid-reply, the conversation stays "writing" forever
- **Category:** Functional (chat) · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/state/agent.dart:368-378,519,821-880`
- **Description:** Stop does nothing and Send stays unavailable. FS-F5.
- **Proposed fix:** On daemon loss end every running turn locally with an "interrupted" marker; resync session state after reconnect.
- **Dependencies:** AUD-001, AUD-011. **Risk:** Low.
- **Verification:** `test/state/agent_test.dart`: running turn + daemon state change → not running, error shown.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-026 — Task output without newlines grows one string without bound and is re-split per chunk (quadratic on the UI isolate)
- **Category:** Performance / Memory · **Priority:** P2 · **Confidence:** Confirmed (code); impact Suspected
- **Affected components:** `app/lib/state/tasks.dart:83-91` (the 10 000-line cap does not apply to a single long line)
- **Description / repro:** Run `head -c 30000000 /dev/zero | tr '\0' a` in Output. FS-F6. (Line-oriented floods are fine: 200 000-line `seq` measured with no frame over budget, `console_flood` in perf JSON.)
- **Proposed fix:** Keep the pending partial line in a `StringBuffer`, split only the new chunk, and cap a single line's length (truncate with a marker).
- **Risk:** Low. **Verification:** Unit test feeding 30 MB without newline: bounded memory, linear time.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-027 — Child processes outlive a daemon that is killed or crashes
- **Category:** Reliability / Resource lifetime · **Priority:** P2 · **Confidence:** Confirmed (terminal) / Suspected (others)
- **Affected components:** `crates/axiomd/src/main.rs:107-117`, `crates/axiomd/src/router.rs:449-456`, `crates/term/src/lib.rs:199-222`
- **Description:** No `PR_SET_PDEATHSIG`; tasks, language servers and PTYs run in their own process groups; `process::exit` cuts the terminal SIGKILL escalation; MCP/ACP processes are not stopped explicitly at shutdown. RC-6. (Observed: after the repro's daemon SIGKILL, `daemon_children` bookkeeping restarted from zero; orphan count not measured.)
- **Proposed fix:** Set `PR_SET_PDEATHSIG(SIGKILL)` in children on Linux (and a parent-pipe watchdog on macOS); at shutdown stop MCP/ACP explicitly and finish escalation before exit.
- **Dependencies:** AUD-007. **Risk:** Low.
- **Verification:** Test: start a task and a terminal, SIGKILL the daemon, assert the children are gone within 2 s.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-028 — `fs/watch` keeps one forwarder per workspace bound to the first connection
- **Category:** Reliability (multi-client) · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/fs.rs:233-266`
- **Description:** A second client (CLI, MCP, a second window) gets OK but no events; the entry goes stale when the first connection ends; concurrent enables leak a forwarder. RC-7.
- **Proposed fix:** Subscribe per connection over a shared broadcast per workspace; remove on connection close.
- **Risk:** Low. **Verification:** Two-connection daemon test.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-029 — `workspace/close` is not reference-counted across connections
- **Category:** Reliability (multi-client) · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/workspace.rs:36-53` (contradicts `docs/architecture/processes.md`)
- **Description:** One client's close cancels every client's tasks, servers and terminals. RC-8.
- **Proposed fix:** Count opens per connection; tear down when the count reaches zero or the last connection drops.
- **Risk:** Low. **Verification:** Two-connection test: A closes, B's task survives.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-030 — `App.clients` is never de-duplicated or pruned; daemon-wide forwarders await each client in turn
- **Category:** Reliability / Performance · **Priority:** P2 · **Confidence:** Confirmed (code) / Suspected (impact)
- **Affected components:** `crates/axiomd/src/app.rs:239-256`, `crates/axiomd/src/handlers/terminal.rs:96-108`
- **Description:** Dead clients accumulate; one full queue blocks terminal and LSP traffic for all. RC-9.
- **Proposed fix:** Prune on disconnect; use `try_send` with per-client drop/lag policy.
- **Risk:** Low. **Verification:** Test with a stalled client: another client's terminal output keeps flowing.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-031 — Blocking work runs on async worker threads under std mutexes
- **Category:** Performance / Concurrency · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/git.rs:15-52` (`git::discover`, up to 10 s), `crates/axiomd/src/handlers/fs.rs:248` (recursive watch setup), `watch_git_dir`; audit-log writes in `crates/agent/src/gate/mod.rs:380-398` (RS-F18)
- **Description:** These stall Tokio workers and every request queued behind them. RC-10.
- **Proposed fix:** Move into `spawn_blocking`; never hold a std mutex across them.
- **Risk:** Low. **Verification:** Test with a slow `git` shim: `fs/list` latency stays < 50 ms meanwhile.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-032 — ACP agents and MCP servers are started and handshaken under daemon-wide locks and never evicted
- **Category:** Reliability / Concurrency · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/sessions.rs:241-244,542-571,656-671`, `crates/agent/src/acp.rs:686-690`
- **Description:** The global ACP/MCP map locks are held during process start and handshake (up to 2 × 60 s), blocking other sessions; a crashed agent stays cached so the session fails every later turn until deleted. RC-11, RS-F12.
- **Proposed fix:** Insert a "starting" placeholder and release the lock while starting; evict on process exit; stop idle agents after a timeout.
- **Risk:** Low–medium. **Verification:** Test with a fake ACP that hangs on handshake: another session's turn proceeds; killing the agent lets the next turn restart it.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-033 — Session attach is recorded before the request succeeds
- **Category:** Reliability (agent) · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/sessions.rs:116-147,345`
- **Description:** A failed or cancelled send/open leaves the connection marked attached with no forwarder; later events are lost for that client. RC-12. Related shell side: FS-F10 (AUD-088).
- **Proposed fix:** Record attachment only after success; on failure roll back.
- **Risk:** Low. **Verification:** Test: failing open, then a successful open receives events.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-034 — A cancelled or timed-out `preview/start` leaves the preview stuck in `Starting`
- **Category:** Reliability (M5 inspector) · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/preview.rs:117-125,287-317`
- **Description:** Every later start returns the stuck preview; a double-click can launch two dev servers. RC-13.
- **Proposed fix:** Guard the start with a cancellation-safe state transition (reset to `Stopped` and kill the child on drop); de-duplicate concurrent starts.
- **Risk:** Low. **Verification:** Test: cancel during start → state `Stopped`, no child left.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-035 — Agent `task.run`/`build.run` keep all output in memory; `build.run` has no timeout
- **Category:** Reliability / Memory · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/tools.rs:335-392,810-826`
- **Description:** Only a 16 KiB tail is used, but the whole output is buffered; a noisy or endless command can exhaust the daemon's memory. RC-14, RS-F7.
- **Proposed fix:** Ring buffer of the tail; default timeout for `build.run` with an explicit override.
- **Risk:** Low. **Verification:** Test with `yes | head -c 500M`: daemon RSS stays bounded.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-036 — Terminal output buffering is bounded by count, not bytes
- **Category:** Memory · **Priority:** P2 · **Confidence:** Suspected
- **Affected components:** `crates/term/src/lib.rs:377-436`
- **Description:** `yes` or a large `cat` with a slow consumer can grow memory to gigabytes. RC-15.
- **Proposed fix:** Byte budget with backpressure (pause reading the PTY) or drop-oldest with a "output skipped" marker.
- **Risk:** Low. **Verification:** Stress test with `yes` for 10 s and a stalled client: bounded RSS.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-037 — `std::env::vars()` panics on a non-UTF-8 environment
- **Category:** Reliability · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/env.rs:71`
- **Description:** A single non-UTF-8 variable breaks terminals, LSP launches and previews. RC-16.
- **Proposed fix:** Use `vars_os()` and pass `OsString`s through.
- **Risk:** Low. **Verification:** Test launching the daemon with `X=$'\xff'`.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-038 — `ui/verify` keeps the first 500 build-error lines and drops new ones, so verification can pass on a broken page
- **Category:** Functional (M5 verification) · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/handlers/ui.rs:297-310`
- **Description:** Once the buffer fills, fresh errors after a patch are not recorded and the "build and runtime clean" check sees none. RC-17. This undermines requirement R13 ("verified, not 'it compiled'").
- **Proposed fix:** Keep the newest lines (ring buffer) and a per-verification error counter reset at patch time.
- **Risk:** Low. **Verification:** Test: 600 old errors, then one new error after patch → verdict `failed`.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-039 — ACP terminal approval shows argv only; the agent sets env (`LD_PRELOAD`, `PATH`, `NODE_OPTIONS`) and cwd unseen, and `outputByteLimit` is uncapped
- **Category:** Security · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/tools.rs:1528-1576`
- **Description:** RC-18, RS-F4 (duplicate reports merged).
- **Proposed fix:** Show env overrides and cwd on the card; deny dangerous variables unless approved explicitly; cap `outputByteLimit`.
- **Dependencies:** AUD-010. **Risk:** Low.
- **Verification:** Gate test: env with `LD_PRELOAD` requires approval listing it.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-040 — ACP permission answers become grants that do not match what was approved
- **Category:** Security · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/agent/src/acp.rs:838-848,911,962-976`
- **Description:** A "read"-kind request auto-allowed by policy grants writes to the same paths; an approved execute lets the next terminal run any argv. RS-F3.
- **Proposed fix:** Derive the grant from the exact approved operation kind and target.
- **Dependencies:** AUD-009. **Risk:** Low. **Verification:** Gate tests per kind.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-041 — `env.install` is approved blind
- **Category:** Security · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/axiomd/src/tools.rs:216-224,961-977`
- **Description:** The approval card has no provider, version, steps or commands; any plan id from the daemon-wide table can be used. RS-F5. (Requirement N5: installs never silent.)
- **Proposed fix:** Render the plan (provider, version, sources, commands, elevation) on the card; bind plan ids to the requesting session.
- **Risk:** Low. **Verification:** Gate test asserting the card content; foreign plan id rejected.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-042 — Inspector runs a blocking workspace text search on Tokio workers for every hover event
- **Category:** Performance · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `crates/inspect/src/session.rs:357-363`, `crates/inspect/src/normalize.rs:107,122,189-258`
- **Description:** Up to 2 000 files scanned per hover in inspect mode. RS-F8.
- **Proposed fix:** Resolve source on click (or debounce hover), cache per element stamp, run in `spawn_blocking`.
- **Risk:** Low. **Verification:** Benchmark hovering 50 elements: daemon CPU and request latency bounded.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-043 — Inspection WebSocket: no Origin check; the "one-time" token is reusable, inlined in the served module, and a new connection replaces the live session
- **Category:** Security · **Priority:** P2 · **Confidence:** Confirmed (code); cross-site exploitation Suspected (depends on the project's Vite CORS)
- **Affected components:** `crates/inspect/src/session.rs:215-246`, `crates/inspect/src/lib.rs:251-269`, `inspectors/web/plugin.mjs:112-117`
- **Description:** RS-F9.
- **Proposed fix:** Check `Origin` against the preview origin; make the token single-use per page session; reject a second connection rather than replacing silently.
- **Risk:** Low. **Verification:** Test: connection with a foreign Origin rejected; replayed token rejected.
- **Status:** Fixed with a deviation (see Remediation status, 2026-11-23)

### AUD-044 — Version probes and system installs run with the shared temp directory as cwd
- **Category:** Security · **Priority:** P2 · **Confidence:** Suspected
- **Affected components:** `crates/env/src/detect.rs:323-337`, `crates/env/src/install.rs:358-364,678-684`
- **Description:** A planted `/tmp/rust-toolchain.toml` (`path=`) or `global.json` could make the probe run another local user's binary. RS-F10.
- **Proposed fix:** Use a private, freshly created directory (0700) under the data dir as cwd.
- **Risk:** Low. **Verification:** Test: planted `rust-toolchain.toml` in the probe cwd has no effect.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-045 — The CLI approval prompt prints untrusted text without neutralising terminal control sequences
- **Category:** Security · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `apps/cli/src/main.rs:383-398,436-466`
- **Description:** Model text and previews can rewrite the prompt visually (ANSI/OSC). RS-F11.
- **Proposed fix:** Escape C0/C1 controls (show `\x1b` etc.) before printing.
- **Risk:** Low. **Verification:** Unit test on the formatter.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-046 — Dialogs freeze the theme captured when they opened; Settings does not follow its own Theme/Motion change
- **Category:** Visual · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/ui/overlays.dart:399-419` (`showAxDialog`)
- **Description / repro:** Settings → Appearance → switch Dark/Light: the window changes, the Settings sheet does not. FUI-03.
- **Proposed fix:** Read the theme inside the dialog route from an inherited listenable instead of capturing it.
- **Risk:** Low. **Verification:** Widget test toggling theme with the dialog open.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-047 — Settings is a fixed 820×600 dialog, taller than the 900×560 minimum window
- **Category:** GUI / Layout · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/shell/settings_page.dart`
- **Description:** At the minimum window the dialog clips (Close at the bottom is unreachable). FUI-04.
- **Proposed fix:** Size to `min(820, w - 48) × min(600, h - 48)`; scroll the content.
- **Dependencies:** AUD-014 batch. **Risk:** Low. **Verification:** Screenshot at 900×560.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-048 — The command palette does not trap focus; Tab and screen readers reach the window behind it
- **Category:** Accessibility · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** palette in `app/lib/shell/shell.dart` (Stack child), `app/lib/shell/palette.dart`
- **Description:** FUI-06. Also the active row is never announced (FUI-35).
- **Proposed fix:** Wrap in `FocusScope` + `BlockSemantics`/`ModalBarrier`; announce the active row via `liveRegion` or semantics `selected`.
- **Risk:** Low. **Verification:** `a11y_test.dart`: Tab inside the palette stays in it.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-049 — The palette re-ranks every item on every rebuild, including each arrow key
- **Category:** Performance · **Priority:** P2 · **Confidence:** Confirmed (code); impact partly measured
- **Affected components:** `app/lib/shell/palette.dart`, `app/lib/ui/fuzzy.dart`
- **Description:** Go to File loads up to 20 000 paths; ranking runs in `build`. FUI-07. Measured: `frames.sh` "palette open/close" had 6 of 48 frames over 16.7 ms (raster p99 24.6 ms) at load average 4.7 (see AUD-062).
- **Proposed fix:** Memoise ranking per query; move selection into its own notifier; rank on an isolate above a size threshold.
- **Risk:** Low. **Verification:** Frame benchmark: Go to File over 20 000 files, ArrowDown ×20, no frame over budget.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-050 — Search results and Source Control changes are built eagerly, not lazily
- **Category:** Performance · **Priority:** P2 · **Confidence:** Confirmed (code); impact not reproduced (2 000 hits rendered within budget, `search_render` in perf JSON, 1 frame over)
- **Affected components:** `app/lib/search/search_page.dart`, `app/lib/scm/scm_page.dart`
- **Description:** A Column per group. FUI-08. Becomes visible with large change sets (thousands of modified files after a formatter run) or longer previews.
- **Proposed fix:** Flatten to a `ListView.builder` over (group header | row) items.
- **Risk:** Low. **Verification:** Frame benchmark with 10 000 changed files.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-051 — Tooltips are centred under their control with no edge handling; rail and right-edge tooltips are clipped
- **Category:** GUI · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/ui/overlays.dart` (tooltip)
- **Description:** FUI-09. **Proposed fix:** Shared overlay positioner (AUD-013). **Risk:** Low. **Verification:** Widget test near window edges.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-052 — The LSP hover card is clipped at the editor's right and bottom; the completion popup is clipped in narrow groups
- **Category:** GUI · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/editor/lsp/lsp_view.dart`
- **Description:** FUI-10 (hover card has no flip; completion has a 320 px minimum). **Proposed fix:** Shared positioner (AUD-013) with flip above the line; clamp width to the group. **Risk:** Low. **Verification:** e2e hover at the last visible line and rightmost column.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-053 — The find bar is a fixed-width Row that overflows in split or narrow groups
- **Category:** GUI / Layout · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** find bar in `app/lib/editor/editor_area.dart`
- **Description:** Needs ~700 px; in a split group the count, Next and Close are hidden. FUI-12.
- **Proposed fix:** Flexible query field, collapse toggles into a menu below a width threshold. **Risk:** Low. **Verification:** Widget test at 400 px.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-054 — Reduced motion is incomplete and the OS reduced-motion preference is ignored
- **Category:** Accessibility / Motion · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/ui/theme.dart`, springs in `app/lib/shell/shell.dart` (rail), spinner, dialog scale
- **Description:** At "Reduced", springs, the spinner and dialog scale still animate (contrary to `docs/design/motion-system.md` §2, §6); `MediaQuery.disableAnimations` is not read. FUI-13.
- **Proposed fix:** Default the Motion setting to "System" honouring `disableAnimations`; route all animations through `AxiomMotion`.
- **Risk:** Low. **Verification:** Widget test with `disableAnimations: true` asserting zero-duration transitions.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-055 — Semantic-token colours keep the old theme after a theme switch until the next edit
- **Category:** Visual · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/editor/lsp/semantic_tokens.dart`
- **Description:** Token styles are stored as concrete colours. FUI-14.
- **Proposed fix:** Store token roles; resolve colours at paint time from the current theme.
- **Risk:** Low. **Verification:** Unit test: switch theme → spans use new colours without an edit.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-056 — Controls that look interactive but do nothing (requirement N6)
- **Category:** GUI / Usability · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** Environments side-bar rows (`onPressed: () {}`, `app/lib/env/env_page.dart`), status-bar language-server segment (no-op unless failed)
- **Description:** FUI-15. N6: "Every visible control does something".
- **Proposed fix:** Rows select/scroll to the toolchain's card; the server segment opens a server menu (restart, show log). Or render them non-interactive.
- **Risk:** Low. **Verification:** Test that every `Pressable` in these surfaces has an effect (or is not a button in semantics).
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-057 — The floating approvals stack can grow beyond the window and is covered by toasts
- **Category:** GUI · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** approvals overlay (chat), `ToastHost`
- **Description:** Up to 3 cards of ~400 px with no height limit; toasts occupy the same corner. FUI-16.
- **Proposed fix:** Cap height with a scrolling stack and "N more"; move toasts or offset them.
- **Risk:** Low. **Verification:** Widget test with 3 approvals at 900×560.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-058 — Long output and diff lines are cut off, with no wrap and no horizontal scroll
- **Category:** GUI / Usability · **Priority:** P2 · **Confidence:** Confirmed (code); diff truncation visible in `40-size-800x600.png`
- **Affected components:** `app/lib/console/output.dart`, `app/lib/scm/scm_page.dart`
- **Description:** FUI-17. Output is where compiler errors appear; clipped lines lose the error text.
- **Proposed fix:** Soft-wrap output lines (toggle) and a horizontal scroll for the diff.
- **Risk:** Low. **Verification:** Widget test with a 300-char line: full text reachable.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-059 — `AxTabs` is a non-scrolling Row; many terminals or a long folder name overflow it
- **Category:** GUI · **Priority:** P2 · **Confidence:** Confirmed (code)
- **Affected components:** `app/lib/ui/tabs.dart`
- **Description:** FUI-18 (console terminals, Chat scope tabs with long folder names).
- **Proposed fix:** Scrollable tab row with ellipsised labels (share with AUD-017).
- **Risk:** Low. **Verification:** Widget test with 20 tabs.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-060 — Test suites report skipped scenarios as passes, and visual geometry is untested
- **Category:** Testing gap · **Priority:** P2 · **Confidence:** Confirmed (runtime)
- **Affected components:** `app/integration_test/languages_test.dart` (11 cases "pass" by printing SKIPPED without `AXIOM_TEST_LSP_INSTALL=1`), `lsp_test.dart` (2 skipped), `env_rust_test.dart` (skipped), `crates/axiomd/tests/preview.rs` (returns early unless `AXIOM_REQUIRE_PREVIEW=1`; `cargo test` hides the message on success)
- **Description:** `e2e.log` reports "All tests passed!" for `languages_test.dart` while every case was skipped. No test asserts on-screen geometry, which is how AUD-003 (gutter), AUD-016 (explorer) and AUD-017 (tabs) shipped. No test runs below the default window size or at text scales > 1.
- **Evidence:** `evidence/logs/e2e.log`; `evidence/logs/check-rust.log`.
- **Proposed fix:** Use `skip:` so runners report skips; print a summary of skipped suites from `e2e.sh`; add a layout-sanity e2e (window sizes 900×560, 1180×760, 1920×1080; text scale 1.3) that fails on any `FlutterError` (overflow) and checks a few geometry invariants (gutter marks align with lines; active tab visible).
- **Risk:** Low. **Verification:** The new suite fails on the current `main` for AUD-003/004/014/016/017 and passes after their fixes.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-061 — The performance budgets cannot be re-measured from the repository: the startup/idle/memory harness was retired with the Tauri shell
- **Category:** Testing gap / Process · **Priority:** P2 · **Confidence:** Confirmed
- **Affected components:** `docs/design/performance.md` §4.1 (references `apps/desktop-tauri/bench/run.sh`, gone), `docs/roadmap/verification/M5.5.md` (P2/P7 numbers produced with an uncommitted harness; `M5-bench/run.py.diff` only)
- **Description:** Only frame timings (`frames.sh`) are reproducible. Startup (P2), idle CPU (P7–P9) and memory (P10) budgets have no committed harness for the Flutter shell, so regressions in them will not be caught.
- **Proposed fix:** Commit a Flutter-era harness (the audit's `evidence/harness/startup.sh` and `audit_perf_test.dart` are a starting point: external timing of exec → first frame → ready → folder listed, `/proc` CPU and PSS sampling, interleaved A/B runs) and update `performance.md`.
- **Risk:** Low. **Verification:** `scripts/check.sh perf` (opt-in) produces a JSON comparable across commits.
- **Status:** Fixed (see Remediation status, 2026-11-23)

### AUD-062 — `frames.sh` no longer meets its own gate: "palette open/close" had 6 frames over 16.7 ms
- **Category:** Performance · **Priority:** P2 · **Confidence:** Confirmed (runtime, one run, load average 4.7)
- **Affected components:** palette open/close animation (`app/lib/shell/palette.dart`), see AUD-049
- **Description:** `verification/FL.md` records "no frame over 16.7 ms"; this run: palette open/close raster p99 24.6 ms, 6 of 48 frames over; splitter drag 1 frame over (build 17.5 ms). Note the run reported dpr 2.0 at a 1.25 compositor scale (GTK 3 renders at the next integer scale and the compositor downscales), so raster work is ~2.56× the 125 % pixel count.
- **Evidence:** `evidence/logs/frames-sh.log`.
- **Proposed fix:** Re-run on an idle machine to separate load; if it reproduces, profile the palette's first frame (shadow/blur layers, ranking in build — AUD-049).
- **Risk:** Low. **Verification:** `frames.sh` with `AXIOM_FRAMES_ASSERT=1` passes 3 consecutive runs.
- **Status:** Fixed (see Remediation status, 2026-11-23)

---

## P3 — Low

Compact entries. Fields not listed are: Dependencies none, Risk low, Status Not started.

### AUD-063 — The "Cannot open X: not UTF-8" banner stays on other documents
- **Category:** GUI · **Confidence:** Confirmed (runtime)
- **Affected:** `app/lib/state/documents.dart` (`banner` is store-wide)
- **Description / evidence:** After a failed open of `blob.bin`, opening `notes.txt` still shows the banner (R7 `banner: Cannot open blob.bin…`; `13-binary.png`, `14-dirty.png`, `17-conflict-dialog.png`).
- **Reproduction:** Open a binary file, then any text file.
- **Root cause:** The banner is not tied to a document or cleared on successful open.
- **Proposed fix:** Clear on the next successful open/activation, or show it as a toast.
- **Verification:** Repro R7 asserts `banner == ''`.

### AUD-064 — The status bar's hint line keeps a stale hint when the hovered control disappears
- **Category:** GUI · **Confidence:** Confirmed (runtime)
- **Affected:** `app/lib/ui/pressable.dart` / hint plumbing
- **Description / evidence:** `repro-R3-tabs-16.png` shows "Restart the Backend" in the hint line long after that button was removed. FUI-21 (Flutter does not call `onExit` on unmount).
- **Proposed fix:** Clear the hint in `Pressable.dispose` when it owns the current hint.
- **Verification:** Widget test: hover then unmount → hint null.

### AUD-065 — A failed language server logs one line per keystroke
- **Category:** Reliability / Diagnostics · **Confidence:** Confirmed (runtime)
- **Affected:** `app/lib/editor/lsp/lsp_editor.dart`, `app/lib/state/lsp.dart`
- **Description / evidence:** With rustup's `rust-analyzer` stub missing, every edit prints `lsp textDocument/didChange: The language server exited while starting…` (55 lines in `perf-run-2.log`); each keystroke still issues a request to a failed server.
- **Proposed fix:** Stop sending to a server in `failed` state; log the failure once.
- **Verification:** Test with a failing fake-lsp: one log line, no requests after failure.

### AUD-066 — Folder-gone and other errors show internal wording; the Dismiss button is misaligned
- **Category:** Usability / Visual · **Confidence:** Confirmed (runtime)
- **Affected:** `app/lib/shell/shell.dart` (workspace error block, ~1400-1425), daemon error messages
- **Description / evidence:** `48-folder-deleted.png`: "Could not list the folder: \`the workspace root\` was not found." (Markdown backticks, internal noun) with "Dismiss" left-aligned under a centred title; the explorer still lists the deleted folder's files.
- **Proposed fix:** Map `not_found` on the root to "The folder ‹name› was moved or deleted." with actions Close Folder / Locate…; centre actions; clear the tree.
- **Verification:** e2e: delete the open folder → message and actions as specified.

### AUD-067 — Search's truncation message is unclear
- **Category:** Usability · **Confidence:** Confirmed (runtime)
- **Description / evidence:** `23-search-results.png`: "2000+ results in 2000 files (the first shown)" when 3 000 files matched; no way to see more or narrow automatically.
- **Proposed fix:** "Showing the first 2 000 results. Refine the search or add an include filter." with a "Search all" action if limits allow.
- **Verification:** Widget test on the summary text.

### AUD-068 — Environments offers "Install…" for a toolchain it already reports Ready
- **Category:** Usability · **Confidence:** Confirmed (runtime)
- **Description / evidence:** `27-environments.png`: Rust "Ready (system)" in the project table and "Install…" under All toolchains.
- **Proposed fix:** Show "Installed (system) · Manage" in the catalogue for detected toolchains.

### AUD-069 — Status dots' cut-out ring does not match the surface (halo in dark mode)
- **Category:** Visual · **Confidence:** Confirmed (code) · FUI-22. **Fix:** draw the ring in the parent's surface colour or drop it.

### AUD-070 — Some focusable controls have no visible focus ring
- **Category:** Accessibility · **Confidence:** Confirmed (code) · FUI-23 (chat row "More" menu, language-server status segment). **Fix:** wrap in `FocusRing`. N7.

### AUD-071 — `Pressable` drops the focusable/focused semantics flags
- **Category:** Accessibility · **Confidence:** Suspected · FUI-24. **Fix:** forward `focusable`/`focused` in the outer `Semantics`. **Verification:** semantics test.

### AUD-072 — Rebuild scope is too wide for shell-level state
- **Category:** Performance / Architecture · **Confidence:** Confirmed (code); impact within budget in the measured scenarios
- **Description:** Opening the palette and each splitter step rebuild the whole shell row including the editor; `AxTabs` builds twice per parent rebuild; LSP popups rebuild the code view; streaming chat rebuilds the whole Chat view; Problems is rebuilt and sorted three times per console rebuild (FUI-25, FS-F15). `frames.sh` splitter drag build p50 6.1 ms / max 17.5 ms is consistent with this.
- **Fix:** Narrow `ListenableBuilder`s (selectors per field), lift the editor subtree out of the shell row's rebuild, memoise Problems.
- **Verification:** Rebuild-count test (debug `debugOnRebuildDirtyWidget`) for palette open and splitter drag.

### AUD-073 — Text fields have no context menu (Cut/Copy/Paste) and rebuild on every keystroke
- **Category:** Usability · **Confidence:** Confirmed (code) · FUI-26. **Fix:** add a widgets-layer context menu to `AxTextInput` (and the editor).

### AUD-074 — Switching console views discards Output's prompt text, scroll and selection
- **Category:** Usability · **Confidence:** Confirmed (code) · FUI-27. **Fix:** keep views alive with `IndexedStack`/`Offstage`.

### AUD-075 — The Environments page calls the store from `build`; it can loop if the daemon returns zero providers
- **Category:** Reliability · **Confidence:** Confirmed (code) · FUI-28. **Fix:** load in `initState`/on folder change; guard empty results.

### AUD-076 — The chat composer changes its tree shape when attachments appear, which can drop focus
- **Category:** Usability · **Confidence:** Suspected · FUI-29. **Fix:** stable keys / constant structure.

### AUD-077 — Hover documentation is pointer-only
- **Category:** Accessibility · **Confidence:** Confirmed (code) · FUI-30. **Fix:** "Show Hover" command (Ctrl+K Ctrl+I as VS Code).

### AUD-078 — Long branch names and roots overflow the status bar
- **Category:** GUI · **Confidence:** Confirmed (code) · FUI-31. **Fix:** flexible, ellipsised segments with priorities.

### AUD-079 — The side-by-side diff runs its line alignment (up to 4 M cells) on the UI thread; its header path overflows
- **Category:** Performance / GUI · **Confidence:** Confirmed (code); header overflow seen at 640 px (`scm_page.dart:308` error in `explore-results.json`) · FUI-32. **Fix:** compute on an isolate (as the gutter diff does); ellipsise the path.

### AUD-080 — Animation loops do not pause when the window is unfocused (performance budget P9)
- **Category:** Performance · **Confidence:** Confirmed (code) · FUI-33. **Fix:** gate `TickerMode` on window focus; step the spinner; one spinner per tree.

### AUD-081 — Toasts stack without limit and have no motion
- **Category:** GUI · **Confidence:** Confirmed (code) · FUI-34. **Fix:** cap at 3, collapse duplicates, fade/slide per motion spec.

### AUD-082 — Live regions are noisy, and the palette's active row is never announced
- **Category:** Accessibility · **Confidence:** Confirmed (code) · FUI-35. **Fix:** remove `liveRegion` from hover/inspector text; announce palette selection.

### AUD-083 — Session-long shell caches never pruned (FocusNode maps, TextPainters created in build, agent/terminal/problem caches)
- **Category:** Memory · **Confidence:** Confirmed (code); measured open/close of 20 files × 20 rounds grew shell PSS 198 → ~221 MB and then plateaued (`audit-perf-*.json`), so no unbounded leak was demonstrated in that workload · FUI-36, FS-F14. **Fix:** dispose per-path nodes on close; reuse painters; LRU for caches.

### AUD-084 — Visual inconsistencies between equivalent elements (Design opinion; values confirmed)
- **Category:** Visual · FUI-37: three heading styles for equivalent side-panel sections (compare `04-nofolder-*.png`), different side-bar gutters, radii/spacings off the token scale, alpha tints not covered by the contrast test, a hard-coded About version, the explorer's brass marker shown only for keyboard focus; Search's include/exclude placeholders in the mono face while the query uses Sans (`23-search-results.png`); terminal background differs from its panel with an 8 px seam (`34-terminal.png`); gutter line numbers sit ~2 px above the code baseline (`08-editor-main-rs.png`). **Fix:** tokenise and align; see UI_UX_REPORT §Recommendations.

### AUD-085 — `_focusedDoc` relies on `FocusNode.debugLabel`, which is null in release builds
- **Category:** Functional · **Confidence:** Confirmed (code) · FUI-20 (`app/lib/shell/shell.dart:227-231`; the right-group branch is dead in release; the e2e assertion only passes in debug). **Fix:** track focus by node identity.

### AUD-086 — After a restart the old daemon's notifications are still dispatched and ids are reused
- **Category:** Reliability · **Confidence:** Confirmed (code), impact Suspected · FS-F8 (`app/lib/daemon/client.dart:420-440,525-543`). **Fix:** tag subscriptions with the connection generation; drop stale ones. **Dependencies:** AUD-001.

### AUD-087 — Terminal keystrokes are independent requests the daemon may apply out of order
- **Category:** Reliability · **Confidence:** Suspected · FS-F9 (`app/lib/state/terminals.dart:154-158`, `crates/rpc/src/server.rs:359-360` spawns a task per request). **Fix:** sequence numbers or a per-terminal ordered queue on the daemon.

### AUD-088 — The first open of a conversation drops events that arrive before `agent/sessionOpen` answers
- **Category:** Reliability · **Confidence:** Suspected · FS-F10. **Fix:** buffer events by session id until the open resolves. **Dependencies:** AUD-033.

### AUD-089 — Overlapping flushes of `shell-local.json` can corrupt it; a corrupt file silently resets the layout
- **Category:** Data integrity · **Confidence:** Suspected · FS-F11 (`app/lib/app/local_prefs.dart:17-61`). **Fix:** serialise writes; write to temp + rename; keep a `.bak`.

### AUD-090 — "The backend exited unexpectedly" can lack the daemon's last stderr lines
- **Category:** Diagnostics · **Confidence:** Suspected · FS-F12. **Fix:** await stderr EOF (bounded) before reporting. Seen: `49-backend-died.png` shows only "(code -9)".

### AUD-091 — A result that fails to decode escapes as `TypeError`, leaving `opening`/`running` stuck
- **Category:** Reliability · **Confidence:** Confirmed (code) · FS-F13 (`app/lib/daemon/client.dart:264-282`). **Fix:** wrap decode errors in `DaemonException(kind: protocol)`.

### AUD-092 — Switching folders flips Chat to the General scope
- **Category:** Usability · **Confidence:** Confirmed (code) · FS-F16. **Dependencies:** AUD-002 (same transient state).

### AUD-093 — Results of a cancelled search can be applied after the folder changed
- **Category:** Reliability · **Confidence:** Suspected · FS-F17 (`app/lib/state/search.dart:252-264,323`). **Fix:** bump the sequence on folder change and cancel.

### AUD-094 — The socket accept loop spins on persistent `accept()` errors (EMFILE)
- **Category:** Reliability · **Confidence:** Confirmed (code) · RC-19 (`crates/axiomd/src/socket.rs:199-203`). **Fix:** back off on error.

### AUD-095 — Two daemons starting at once can leave a token file that does not match the listener
- **Category:** Reliability · **Confidence:** Confirmed (race in code) · RC-20. **Fix:** lock file; write token atomically after bind.

### AUD-096 — Invalid-Request errors answer `id: null` even when the id is readable
- **Category:** Protocol · **Confidence:** Confirmed (code) · RC-21 (`crates/rpc/src/codec.rs:64-71`). **Fix:** echo a readable id per JSON-RPC 2.0 §5.

### AUD-097 — External `fs/readFile` reads the whole file before its size check; FIFOs block a thread
- **Category:** Reliability · **Confidence:** Confirmed (code) · RC-22. **Fix:** check metadata (regular file, size) first; read with a limit.

### AUD-098 — A cancel racing a successful `task/start`/`build/run` leaves the process with no forwarder or history
- **Category:** Reliability · **Confidence:** Confirmed (code) · RC-23. **Fix:** make start atomic w.r.t. cancel (cancel after success stops the task).

### AUD-099 — Daemon-lifetime caches grow without bound or go stale (ignore cache, previews, ACP terminals, executor history)
- **Category:** Memory · **Confidence:** Confirmed (code) · RC-24. Measured daemon PSS stayed 14–49 MB across the perf workload, so not urgent. **Fix:** LRU/eviction on workspace close; call `Previews::remove`.

### AUD-100 — Outgoing lines are not checked against the 64 MiB frame limit
- **Category:** Protocol · **Confidence:** Suspected · RC-25. **Fix:** check size; return `too_large` errors instead of emitting.

### AUD-101 — Windows: Job Object assigned after spawn; terminal close kills only the shell
- **Category:** Platform · **Confidence:** Confirmed (code; Windows deferred by the owner) · RC-26. **Fix:** create suspended, assign, resume.

### AUD-102 — `crates/desktop-bridge` is unused product code that must be kept in step with the Dart client
- **Category:** Architecture / Dead code · **Confidence:** Confirmed · RC-27. **Fix:** owner decision: move its lifecycle tests to the Dart client's suite or to `axiomd` tests and remove the crate, or keep it as the documented reference client.

### AUD-103 — `build.run` re-resolves the task at execution and ignores the approved argv
- **Category:** Security · **Confidence:** Confirmed (code) · RS-F6 (`crates/axiomd/src/tools.rs:725-806`). **Fix:** execute the exact approved argv.

### AUD-104 — One ordinary write lets the agent relax workspace policy or plant workspace providers
- **Category:** Security (design) · **Confidence:** Confirmed (design) · RS-F13. **Fix:** treat `.axiom/project.toml` and `.axiom/providers/**` as protected paths requiring explicit approval.

### AUD-105 — System package names are not validated before an elevated package manager
- **Category:** Security · **Confidence:** Confirmed (code); exposure limited to user-dir manifests · RS-F14 (`crates/env/src/system.rs:94-137`). **Fix:** validate names (`^[a-z0-9][a-z0-9+.-]*$`), insert `--`.

### AUD-106 — Policy glob matching is exponential in the number of `*`
- **Category:** Security / DoS · **Confidence:** Confirmed (code) · RS-F15. **Fix:** linear-time glob (two-pointer) or `globset`.

### AUD-107 — Providers bootstrap from unpinned `releases/latest`; some checksums come from the artifact's own origin
- **Category:** Supply chain · **Confidence:** Confirmed · RS-F16 (`providers/python/provider.toml:40-80`, `providers/rust/provider.toml:44-81`). **Fix:** pin versions and embed digests.

### AUD-108 — Archive extraction and downloads have no size or entry-count limit
- **Category:** Security / Robustness · **Confidence:** Confirmed (code); path-escape checks are sound · RS-F17. **Fix:** byte and entry caps from the manifest.

### AUD-109 — Smaller daemon robustness items
- **Category:** Reliability · **Confidence:** Confirmed (code) · RS-F18 (unbounded child stdout/SSE line buffers; test credential file written 0644; self-declared MCP principal; `env.planInstall` labelled read but does network). (Blocking audit-log writes are in AUD-031; `api_key_env` inheritance is in AUD-010.)

### AUD-110 — First-run and empty states read as developer-facing (Design opinion)
- **Category:** Usability / Visual · **Confidence:** Observed
- **Description / evidence:** `02-welcome-dark.png`: the welcome copy explains the architecture ("Axiom opens folders through its backend, axiomd.") instead of offering next steps (recent folders, clone, docs); its block sits ~95 px left of centre because content is left-aligned in a centred 460 px column; the side bar repeats the same Open Folder button above a large empty panel. Chat setup (`04-nofolder-chat.png`) puts install buttons ~900 px from their labels at 1440 px and shows install commands as plain prose that cannot be copied.
- **Proposed fix:** See UI_UX_REPORT §Recommendations R-3, R-4.

### AUD-111 — Shortcut and command conventions surprise IDE users (Design opinion)
- **Category:** Usability · **Description:** Ctrl+N = New Chat (not New File); "Preferences" is a top-level menu while Edit is absent; the palette lists commands in an arbitrary order (Save, New Chat, Next Tab, Save All…, `19-palette.png`) rather than by category or recent use; Ctrl+L = AI dock (VS Code: select line). **Proposed fix:** owner decision; see UI_UX_REPORT.

### AUD-112 — Documentation and workspace hygiene drift
- **Category:** Process · **Confidence:** Confirmed
- **Description:** `docs/roadmap/current.md` still says "Next: CI green on `task/m5.5`, merge to main" (merged in `adc63c1`); `AGENTS.md` says M5.5 is "on branch `task/m5.5`"; `docs/design/performance.md` §4 documents the retired Tauri harness (AUD-061); 44 tasks are `implemented` but never reviewed (`scripts/tasks.py`: FL-*, M5-*, M5.5-*, all of the milestones marked complete), contrary to `agents/REVIEW_GUIDE.md`; `security.md` describes a control that does not exist (AUD-010). Local: `target/` is 119 GB (109 GB debug), and git-ignored Tauri/Monaco leftovers (`apps/desktop-tauri/`, `editor/`, 328 MB) remain in the checkout.
- **Proposed fix:** Update the status docs; schedule reviews of the implemented tasks (this audit's findings are a head start); `cargo clean -p` old profiles or a periodic `cargo sweep`.
