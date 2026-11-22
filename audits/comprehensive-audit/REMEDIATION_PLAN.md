# Remediation plan

For an implementing agent. Read `ISSUES.md` for each id (and the referenced
`evidence/static/` section for code excerpts). Follow `agents/IMPLEMENTATION_GUIDE.md`: one task
per branch, commits `<TASK-ID>: <summary>`, `scripts/check.sh` green, no weakened tests, frozen
contracts changed only through a change request (`docs/architecture/governance.md`).

**Implementation is not authorised yet.** Before each batch: re-read the issues, confirm each
still reproduces on the current `main` (the repro harness is in `evidence/harness/`), check
`git status`/recent commits for overlapping work, and update `AUDIT_STATE.md` §Implementation
log as issues move.

Batches are ordered by dependency and shared root cause, not strictly by priority. Batches 1–3
can proceed in parallel on separate branches; 4 depends on 1's layout-test harness.

---

## Batch 0 — Safety net (do first; small)

- **Issues:** AUD-060 (skips reported as passes; no geometry/layout tests).
- **Files:** `app/integration_test/languages_test.dart`, `lsp_test.dart`, `env_rust_test.dart`,
  `app/scripts/e2e.sh`, `crates/axiomd/tests/preview.rs`, new `app/integration_test/layout_test.dart`.
- **Approach:** use `skip:` so skips are visible; make `e2e.sh` print a skip summary; add
  `layout_test.dart` that, at 900×560, 1180×760 and 1920×1080 and text scale 1.0/1.3, opens a
  folder with a deep tree, a modified Git file, 20 tabs, splits the editor with the dock open,
  and fails on any `FlutterError` and on three invariants: gutter mark rects align with their
  lines, the active tab is inside the strip, no explorer row overflows.
- **Prerequisites:** none. **Risk:** low.
- **Acceptance criteria:** the new suite fails on today's `main` (AUD-003, 004, 014, 016, 017)
  and the skip summary lists the 14 skipped cases.
- **Expected outcome:** later batches have a red→green check.

## Batch 1 — Critical correctness and data safety

- **Issues:** AUD-001 (P0), AUD-002, AUD-025, AUD-086, AUD-092, AUD-091, AUD-018, AUD-019,
  AUD-024, AUD-023.
- **Files:** `app/lib/state/{workspace,documents,agent,lsp}.dart`, `app/lib/app/app_state.dart`,
  `app/lib/daemon/client.dart`, `app/lib/shell/shell.dart` (dialogs), `app/lib/ui/overlays.dart`
  (`AxQuestion` default action).
- **Approach:**
  1. Introduce one close path `_closeCurrent(reason)` in `WorkspaceStore` with reasons
     `user`, `switch`, `daemonLost`; `openFolder` calls it for the switch (AUD-002).
  2. On `daemonLost`, documents are **detached, not cleared** (AUD-001): keep `Doc`s and
     buffers, make editors read-only with a banner; after reconnect (`restartBackend` or
     auto-restart) reopen the same root, re-attach docs, re-sync LSP, and run the conflict check
     for dirty docs. Generation-tag client subscriptions (AUD-086). End running chat turns
     locally on loss (AUD-025). Persist dirty buffers to `<data>/recovery/` on every change
     (debounced) so a shell crash is recoverable too (owed item in `FL.md`).
  3. Serialise saves per document (AUD-024); separate "deleted on disk" from "changed"
     (AUD-019); `AxQuestion.defaultAction`, Cancel default for conflicts (AUD-018).
  4. Preserve per-line terminators (AUD-023); wrap decode errors (AUD-091).
- **Prerequisites:** Batch 0 harness helps but is not required.
- **Risks:** document identity across daemon generations; LSP re-sync ordering (respect the
  ordered-sync work in `618b89f`).
- **Required tests:** e2e: edit → SIGKILL daemon → restart → buffer intact and savable;
  palette Restart the Backend with dirty docs; folder switch A→B leaves no A docs;
  delete-while-dirty dialog; double Ctrl+S; unit tests for line terminators and decode errors.
- **Acceptance criteria:** all above pass; repro R2 shows `textPreserved: true`; repro of FS-F2
  cannot write A's text into B.
- **Expected outcome:** requirement N3 met; no silent data loss paths known to the audit.

## Batch 2 — High-impact functional defects (daemon and editor)

- **Issues:** AUD-003, AUD-004, AUD-007, AUD-011, AUD-012, AUD-027, AUD-034, AUD-038, AUD-037,
  AUD-098.
- **Files:** `app/lib/editor/code_view.dart` (gutter), `app/lib/editor/editor_area.dart`,
  `app/lib/ui/splitter.dart`, `crates/exec/src/executor.rs`, `crates/axiomd/src/handlers/{sessions,git,preview,ui,env}.rs`,
  `crates/axiomd/src/main.rs`, `crates/term/src/lib.rs`, `crates/rpc/src/server.rs`.
- **Approach:** AUD-003 one-line height fix; AUD-004 guarded clamp; AUD-007 bounded drain after
  child exit + group kill on cancel; AUD-027 `PR_SET_PDEATHSIG`; AUD-011 live-turn drop guard;
  AUD-012 polling fallback with backoff; AUD-034 cancellation-safe preview state; AUD-038 ring
  buffer of newest errors; AUD-037 `vars_os`; AUD-098 atomic start vs cancel.
- **Prerequisites:** none.
- **Risks:** process-group semantics differ on macOS (setsid notes in `c72cf44`).
- **Required tests:** the per-issue tests in `ISSUES.md`; `crates/exec` grandchild test
  (`sh -c 'sleep 30 & echo x'` < 3 s).
- **Acceptance criteria:** Batch 0 layout suite green for gutter and split; Rust tests green on
  Linux and macOS CI.

## Batch 3 — Security hardening (agent gate, untrusted workspaces)

- **Issues:** AUD-008, AUD-009, AUD-010, AUD-039, AUD-040, AUD-041, AUD-043, AUD-044, AUD-045,
  AUD-103, AUD-104, AUD-105, AUD-106, AUD-107, AUD-108, AUD-109.
- **Files:** `crates/agent/src/gate/*`, `crates/agent/src/acp.rs`, `crates/axiomd/src/tools.rs`,
  `crates/axiomd/src/handlers/lsp.rs`, `crates/exec/src/backend.rs`, `crates/inspect/src/*`,
  `inspectors/web/plugin.mjs`, `crates/env/src/{detect,install,system,archive,fetch}.rs`,
  `providers/*/provider.toml`, `apps/cli/src/main.rs`, `docs/architecture/security.md`.
- **Approach:** implement the documented secret filter for agent-initiated processes
  (AUD-010) first — it is the documented control; then exact-match, scoped, expiring grants
  (AUD-009, 040, 103); full approval cards (env/cwd, install plans: AUD-039, 041); trust-gated
  LSP start (AUD-008 — **owner decision on default behaviour**); protected `.axiom/` paths
  (AUD-104); Origin check + single-use token for the inspector (AUD-043); private probe cwd
  (AUD-044); terminal-control escaping (AUD-045); input validation and limits (AUD-105–108).
- **Prerequisites:** owner confirms the UX for AUD-008 and the grant model in AUD-009.
- **Risks:** more approval prompts; any change to frozen agent-tool contracts needs a change
  request (governance). Keep `security.md` in step.
- **Required tests:** gate unit tests for every widening case; daemon test that `env` output of
  an agent task lacks `*_TOKEN`; untrusted workspace starts rust-analyzer with build scripts and
  proc macros disabled (assert init options via fake-lsp); inspector rejects foreign Origin.
- **Acceptance criteria:** each test passes; `security.md` matches the code.

## Batch 4 — Major performance bottlenecks

- **Issues:** AUD-005, AUD-015, AUD-006, AUD-026, AUD-031, AUD-042, AUD-035, AUD-036,
  AUD-049, AUD-062, AUD-061.
- **Files:** `app/lib/state/documents.dart`, `app/lib/editor/lsp/lsp_editor.dart`,
  `app/third_party/re_editor` (record any patch in `PATCHES.md`), `app/lib/state/tasks.dart`,
  `app/lib/shell/palette.dart`, `crates/workspace/src/search.rs`,
  `crates/axiomd/src/handlers/{git,fs}.rs`, `crates/inspect/src/*`, `crates/axiomd/src/tools.rs`,
  `crates/term/src/lib.rs`; new committed perf harness (AUD-061).
- **Approach:** commit the harness first (AUD-061, from `evidence/harness/startup.sh` and
  `audit_perf_test.dart`) and record baselines; profile AUD-005/015 with DevTools before
  changing code; then the optimisations listed in `PERFORMANCE_REPORT.md`.
- **Prerequisites:** idle machine or interleaved A/B (owner's preference: `--ab` pairs, no
  waiting for idle load).
- **Risks:** re_editor undo changes affect editing correctness — run `test/editor/*` and the
  editor e2e.
- **Required tests / benchmarks:** typing at line 1 of 100k lines: build max ≤ 16.7 ms and PSS
  growth ≤ 50 MiB over 50 keys; open 100k lines: no frame > 100 ms; `search_probe.py` 2 MiB line
  < 1 s; 30 MB newline-free output bounded; `frames.sh` 0 over budget in 3 runs.
- **Acceptance criteria:** budgets met with before/after numbers recorded in a verification
  note.

## Batch 5 — GUI and interaction defects

- **Issues:** AUD-013, AUD-051, AUD-052 (one overlay positioner), AUD-014, AUD-047, AUD-053
  (space-aware layout), AUD-016, AUD-017, AUD-059, AUD-058, AUD-078, AUD-079, AUD-020, AUD-021,
  AUD-048, AUD-070, AUD-071, AUD-077, AUD-054, AUD-080, AUD-046, AUD-055, AUD-056, AUD-057,
  AUD-063, AUD-064, AUD-065, AUD-066, AUD-074, AUD-075, AUD-076, AUD-081, AUD-082, AUD-085,
  AUD-073.
- **Files:** `app/lib/ui/{overlays,splitter,tabs,pressable,text_input,theme,tokens}.dart`,
  `app/lib/shell/{shell,palette,settings_page}.dart`, `app/lib/files/explorer.dart`,
  `app/lib/editor/{editor_area,code_view}.dart`, `app/lib/editor/lsp/{lsp_view,semantic_tokens}.dart`,
  `app/lib/console/*`, `app/lib/scm/scm_page.dart`, `app/lib/env/env_page.dart`.
- **Approach:** build the shared overlay positioner and the shell layout resolver first; then
  per-surface fixes. Roving focus for lists (one Tab stop each). Text-scale-aware row heights.
  Reduced motion from the OS. Theme-reactive dialogs and token colours.
- **Prerequisites:** Batch 0 layout suite.
- **Risks:** wide touch surface; keep goldens (`test/ui/goldens`) updated deliberately and review
  diffs visually.
- **Required tests:** Batch 0 suite green at all sizes and text scales; `a11y_test.dart` extended
  (Tab leaves the explorer in one press; palette traps focus); widget tests listed per issue.
- **Acceptance criteria:** no `FlutterError` in the layout suite; screenshots of each fixed
  surface reviewed in both themes.

## Batch 6 — Architecture and robustness (daemon multi-client, caches)

- **Issues:** AUD-028, AUD-029, AUD-030, AUD-032, AUD-033, AUD-088, AUD-087, AUD-093, AUD-089,
  AUD-090, AUD-094, AUD-095, AUD-096, AUD-097, AUD-099, AUD-100, AUD-083, AUD-072, AUD-050,
  AUD-102, AUD-101.
- **Approach:** per-connection subscriptions and reference counts (AUD-028–030); lock-free
  starts for ACP/MCP (AUD-032); attach-after-success (AUD-033, 088); ordered terminal input
  (AUD-087); atomic prefs writes (AUD-089); cache eviction (AUD-083, 099); narrow rebuild
  scopes (AUD-072) and lazy lists (AUD-050); owner decision on `desktop-bridge` (AUD-102);
  Windows items when Windows is scheduled (AUD-101).
- **Risks:** multi-client changes touch protocol semantics described in `processes.md`; update
  the doc with the code.
- **Required tests:** two-connection daemon tests; rebuild-count test; prefs crash-safety test.

## Batch 7 — Visual polish, product gaps, docs, final regression

- **Issues:** AUD-022 and AUD-111 (owner decisions; AUD-022 needs a protocol change request),
  AUD-110, AUD-084, AUD-067, AUD-068, AUD-069, AUD-112.
- **Approach:** implement only the recommendations the owner approves (UI_UX_REPORT R-3, R-4,
  R-6, R-8, R-9); update `current.md`, `AGENTS.md`, `performance.md`; run the review procedure
  on the 44 `implemented` tasks or record why not.
- **Final regression:** `scripts/check.sh`, `scripts/check.sh e2e`, `frames.sh` ×3, the
  committed perf harness (A/B against the pre-remediation build), the audit repro suite
  (`audit_repro_test.dart` — every R-case should now show the fixed behaviour), packaging
  (`check.sh package`), and CI on Linux and macOS.
- **Acceptance criteria:** every issue in `ISSUES.md` is Fixed (with its verification noted) or
  explicitly deferred by the owner.
