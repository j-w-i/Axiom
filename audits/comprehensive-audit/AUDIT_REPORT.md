# Axiom — comprehensive audit report

Audit of `main` at `751ffc2` (2026-11-22, clean tree). Audit-first: **no fixes were made**; the
only files written are under `audits/comprehensive-audit/`.

Companion files: `ISSUES.md` (112 issues), `PERFORMANCE_REPORT.md`, `UI_UX_REPORT.md`,
`TEST_MATRIX.md`, `REMEDIATION_PLAN.md`, `AUDIT_STATE.md`; evidence in `evidence/`.

## Executive summary

Axiom is in better shape than most projects at this stage. Its quality gate is real and green
(532 Rust tests, 89 Flutter tests, 15 end-to-end suites driving the real app against the real
daemon), startup and idle behaviour are good, and the visual identity is coherent. The audit
nevertheless found one critical and twelve high-priority problems, most of which the existing
suites cannot see because they check state rather than what is on screen, run at one window
size, or skip without saying so.

The most serious:
1. **Unsaved work is silently lost when the daemon crashes or is restarted** (AUD-001, P0,
   reproduced). This breaks the project's own requirement N3.
2. **Switching folders keeps the previous folder's documents**, and a save can then write one
   folder's text into another's file (AUD-002, reproduced).
3. **The Git gutter draws every change marker half an editor-height below its line**
   (AUD-003, reproduced; root-caused to a zero-height `CustomPaint`).
4. **Splitting the editor crashes the editor area** in windows under ~1175 px with the default
   panels, which includes half-screen tiling and the minimum window (AUD-004, reproduced).
5. **Typing near the top of a 100 000-line file stalls the UI for 4–4.5 s per burst**, and
   shell memory grows by ~700 MiB (AUD-005, measured twice).
6. **Search is quadratic in line length**: a 400 KB minified line takes 134 s (AUD-006,
   measured).
7. **Tasks that leave a background child never finish** (AUD-007, reproduced).
8. **Security gaps in the agent and workspace model** (code-confirmed): language servers run
   untrusted workspaces' build scripts without a trust prompt (AUD-008); session grants are
   wider than what was approved (AUD-009); agent-started processes inherit every secret in
   the daemon's environment, although `security.md` documents a filter that does not exist
   (AUD-010).

## Project overview

- **Product:** an LLM-native, cross-platform IDE. Flutter desktop shell (`apps/desktop-flutter`,
  ~29 k lines of Dart, widgets layer only) over a Rust daemon (`axiomd` + 12 crates, ~49 k
  lines) speaking JSON-RPC 2.0 over stdio; a CLI (`apps/cli`), a web inspector
  (`inspectors/web`), declarative toolchain/LSP providers (`providers/`).
- **Surfaces:** title row and menus, rail (Files, Chat, Search, Source Control, Environments,
  Inspector), explorer, editor (re_editor, LSP, split, find, Git marks), console (Output,
  Problems, Terminal), palette and Go to File, Settings, Chat and AI dock with approvals,
  Visual Inspector, failure pages.
- **History relevant to the audit:** three shell rewrites (Qt → Tauri → Flutter) in ~11 days;
  FL, M5 and M5.5 landed on 2026-11-22. All 44 tasks of those milestones are `implemented`
  but none has been through the repository's own review procedure (`scripts/tasks.py`), so
  this audit is effectively their first independent review.

## Scope and method

| Phase | What was done |
|-------|---------------|
| Discovery | docs, ADRs, roadmap, task board, git history, CI workflow, both runners |
| Static | four parallel reviewers (Rust core; Rust services and security; Flutter state/client and Linux runner; Flutter UI), each finding verified by reading the code path; P0/P1 claims spot-checked by me (two reproduced at runtime, two confirmed by `grep`) |
| Build and tests | `scripts/check.sh rust`, `flutter`, the inspector tests, all e2e suites, `frames.sh` |
| GUI | a scripted exploratory session of 46 workflows against the real daemon with 62 screenshots and framework-error capture, plus 9 focused reproductions, in a private compositor (never on the developer's desktop) |
| Performance | release startup ×12 and idle CPU; profile-mode large-file, memory, search, console and typing probes ×2; search scaling; task completion |
| Visual | review of every screenshot in both themes, at five window sizes, dpr 2 and text scale 1.5 |

## Testing environment

Fedora 44 (Linux 7.3.3), 12 cores, 31 GiB RAM, busy (load 2.4–11, swap full); Flutter 3.50.1;
Rust stable; private `kwin_wayland --virtual` at scale 1 (GUI) and 1.25 (performance; GTK renders
at dpr 2). Not available: macOS, Windows, a screen reader, configured LLM credentials, network
installs of language servers (deliberately not run).

## Overall application health

| Dimension | Rating | Basis |
|-----------|--------|-------|
| Build / tests | Good | everything green; but 14 e2e cases pass by skipping and nothing tests geometry |
| Core editing | Fair | correct in normal use; data-loss paths on daemon loss and folder switch; large files slow |
| Daemon robustness | Fair | sound path containment, atomic writes, SQLite recovery; process-lifetime and multi-client gaps |
| Security | Fair–weak | strong foundations (tokens, path checks, checksums) but the agent gate and untrusted-workspace exposure need work |
| Performance | Good, with two severe outliers | startup ≤ 0.5 s, idle 0 %, smooth scrolling and typing in normal files; large-file edits and long-line search are severe |
| GUI robustness | Fair | breaks at reachable window sizes, deep trees, many tabs, larger text |
| Visual design | Good | coherent, restrained, consistent tokens; a handful of inconsistencies and developer-facing empty states |
| Accessibility | Fair | excellent naming; keyboard traversal, text scaling and reduced motion incomplete |

## Severity breakdown

| Priority | Count | Confirmed at runtime (incl. measured) | Confirmed in code | Suspected / unverified | Observed (design opinion) |
|----------|--:|--:|--:|--:|--:|
| P0 | 1 | 1 | 0 | 0 | 0 |
| P1 | 12 | 6 | 5 | 1 | 0 |
| P2 | 49 | 12 | 35 | 2 | 0 |
| P3 | 50 | 6 | 33 | 8 | 3 |
| **Total** | **112** | **25** | **73** | **11** | **3** |

By primary category (first word of each issue's category): Reliability 22 · GUI 18 ·
Security 14 (plus supply chain 1) · Performance 13 · Usability 11 · Functional 9 ·
Accessibility 8 · Visual 4 · Memory 3 · Protocol 2 · Testing 2 · Data integrity,
Diagnostics, Platform, Architecture, Process 1 each.

## Functional testing results

- Passed: connect, open folder (3 000 files), explorer keyboard, open/edit/save/undo, CRLF,
  long lines, binary refusal, close prompts, conflict detection, palette, Go to File, Go to
  Line, search, Source Control diff, Environments detection, console commands and exit status,
  terminal, Settings, restart restoring folder/tabs/theme, shutdown.
- Failed: daemon-loss data safety (AUD-001), folder switch (AUD-002), Git gutter (AUD-003),
  split at small widths (AUD-004), deleted-file save (AUD-019), tasks with background children
  (AUD-007), deep trees (AUD-016), many tabs (AUD-017).
- Details: `TEST_MATRIX.md`.

## GUI testing results

75 screenshots (`evidence/screenshots/`: 62 exploratory, 13 repro). 12 framework errors in the exploratory run, all
layout: explorer row overflow (AUD-016), split clamp (AUD-004), title row and diff header
overflow below the minimum window, and an inactive-element `findRenderObject` during split
(recorded in `explore-results.json`, step "split editor right"; root cause not isolated —
included under AUD-004's batch for investigation). 47 more framework errors in the repros for
panel sizes (AUD-014). No crash of the shell process occurred.

## Visual design assessment

Coherent dark/light themes, a disciplined single accent, good typographic hierarchy in the
editor and diff. Weak points: empty and setup states read as engineering notes (AUD-110),
equivalent headings and gutters differ across side panels (AUD-084), the chat setup layout
does not adapt to the dock (AUD-110), and long content is clipped rather than wrapped or
scrollable in Output and the diff (AUD-058). Full assessment: `UI_UX_REPORT.md`.

## Performance analysis

Startup: warm first frame ~300 ms, cold ~480 ms, first-ever 1.04 s. Idle: 0 % CPU. Frames:
smooth for explorer, editor, selection, tab switching, theme switch; palette open/close over
budget in one run (AUD-062). Severe: large-file edits (AUD-005) and opening (AUD-015); long-line
search (AUD-006). Details and validation benchmarks: `PERFORMANCE_REPORT.md`.

## Architecture assessment

The two-process design, generated wire types and service-crate layering are sound and mostly
followed; the reviewers found no wire-input panics except one (AUD-037) and confirmed path
containment and atomic writes. Weaknesses cluster around **lifecycles**: the shell ties
document lifetime to daemon workspace ids (AUD-001/002), the daemon ties process lifetime to
pipe EOF (AUD-007) and does not reap children on its own death (AUD-027), and multi-client
semantics described in `processes.md` are not implemented (AUD-028–030). Rebuild scopes in the
shell are wider than needed (AUD-072) but within budget today. `crates/desktop-bridge` is
unused product code (AUD-102).

## Security and accessibility findings

Security (15 issues whose primary category is security or supply chain, plus security-relevant reliability items): untrusted-workspace code execution via language servers (AUD-008);
grants and approvals that cover more than was shown (AUD-009, 039–041, 103); secrets inherited
by agent processes contrary to the documented control (AUD-010); inspector WebSocket origin
and token handling (AUD-043); `/tmp` cwd for probes (AUD-044); terminal-control injection in
the CLI prompt (AUD-045); supply-chain and extraction limits (AUD-105–108). No destructive
testing was done; none of these were exploited at runtime.

Accessibility (8 issues): Tab stops per row (AUD-021), palette focus trap (AUD-048), text
scaling (AUD-020), reduced motion (AUD-054), focus rings and semantics flags (AUD-070, 071),
hover docs pointer-only (AUD-077), live-region noise (AUD-082).

## Root-cause clusters

| Cluster | Issues | Shared cause |
|---------|--------|--------------|
| A. Lifecycle coupling in the shell | 001, 002, 025, 086, 092, 093 | document/chat/search state tied to the daemon's workspace id and connection; no "detached" state |
| B. Space-unaware layout | 004, 013, 014, 016, 017, 020, 047, 051–053, 058, 059, 078 | constant bounds and fixed sizes; no resolver for the space actually available |
| C. Process lifetime in the daemon | 007, 027, 032, 034, 098, 036 | EOF-driven completion, no death signal, locks across process start |
| D. Whole-document work on the UI isolate | 005, 015, 026, 079, 049 | synchronous O(document) work per edit/open/rank |
| E. Approval ≠ execution in the agent gate | 009, 010, 039, 040, 041, 103, 104 | grants and cards derived from a coarser key than what runs |
| F. Tests check state, not screens | 060 (and 003, 016, 017 shipped) | no geometry assertions, single window size, silent skips |
| G. Multi-client semantics unimplemented | 028, 029, 030, 033, 088 | daemon assumes one shell |

## Recommended priorities

1. Batch 0 (safety-net tests), then Batch 1 (data safety: AUD-001, 002 and friends).
2. In parallel: Batch 2 quick wins (AUD-003 is a one-line fix; AUD-004, 007, 011) and
   Batch 3's documented-control gap (AUD-010) plus the owner's decisions on AUD-008/009.
3. Batch 4 performance with a committed harness (AUD-061) and profiles before changes.
4. Batches 5–7 as scheduled; product gaps (AUD-022, 111) and design opinions only with owner
   approval.

## Testing limitations

- macOS and Windows not tested (no hosts; Windows build deferred by the owner).
- No real language-server sessions (rust-analyzer is a missing rustup component on this
  machine; network installs not run); LSP behaviour rests on the e2e suite's fake server.
- No LLM credentials: chat, agent loop and approvals were tested only through existing
  e2e fakes; agent-gate findings are code-level.
- No screen reader; accessibility judged from the semantics tree and keyboard traversal.
- Performance on a loaded machine; no CPU profiles or heap snapshots; one `frames.sh` run.
- GUI driving used the integration-test binding (real engine, real daemon, synthetic input),
  not OS-level input; IME, clipboard and drag-and-drop from other apps were not tested.
- Packaging (`check.sh package`) and the X11 path were not run.
- Static review did not cover everything (each `evidence/static/*.md` ends with its
  not-reviewed list: notably the anthropic/openai stream decoders, `verify.rs`, parts of
  `lsp_view.dart` and `chat/models.dart`).

## Final assessment

The foundations — architecture, contracts, gate discipline and visual identity — are strong.
The defects are concentrated in a few clusters with clear root causes, and most fixes are
local. The P0 and the two folder/daemon lifecycle issues should be fixed before anyone uses
Axiom on work they care about; the Git gutter fix is trivial and highly visible; the security
cluster should be resolved before the agent features are used on untrusted repositories.
Recommended first batch: **Batch 0 + Batch 1** (`REMEDIATION_PLAN.md`).
