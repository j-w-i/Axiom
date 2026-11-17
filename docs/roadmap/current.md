# Current state

_Last updated: 2026-11-17._

## M1.6 — Axiom Visual Experience: approved, Phase B ready (2026-11-17)

The owner approved the recommended direction, **"Precision Instrument"**, and
accepted decisions D2–D10 as recommended. **ADR-0013** records them, together with the
material grammar, the motion policy and the scope rule, and amends ADR-0011 to allow
OFL-1.1 font assets (IBM's unmodified files only). M1.6-DESIGN-001 is tested. Work
lives on branch `task/m1.6`.

| # | Decision |
|---|----------|
| D1 | Synthesis "Precision Instrument" (A + B's signals + C's restraint) |
| D2 | Bundle IBM Plex Sans/Mono (IBM's unmodified Latin1 subsets) |
| D3 | macOS overlay title bar; Windows/Linux native decorations themed with the setting |
| D4 | No activity rail until M2 |
| D5 | `appearance.motion` Standard/Reduced/Off |
| D6 | Stderr not painted red |
| D7 | Drag-to-reorder tabs in M1.6 |
| D8 | Radix Tooltip |
| D9 | Restyle in place, task by task; no feature flag |
| D10 | Light theme with equal care |

- Design package: `docs/design/` (normative for M1.6). Prototypes:
  `experiments/e5-visual-concepts/`. Reference screenshots: `docs/design/concepts/r-*.png`.
- Milestone: `docs/roadmap/milestones.md` §M1.6; 27 tasks in `docs/tasks/M1.6/`;
  graph and waves: `dependency-graph.md` §M1.6.
- **M1.6-QA-001 is tested (2026-11-17):** screenshot harness (`scripts/e2e.sh shots.mjs`,
  `e2e/shots-compare.mjs`) and the Linux visual baseline in
  `docs/roadmap/verification/M1.6/baseline/linux/`.
- **M1.6-QA-002 is tested (2026-11-17):** performance baseline `bench/results/JWI-Fedora-20261117-061550.json`; six new bench scenarios; `docs/design/performance.md` §1 is now measured. Both baselines exist.
- **M1.6-SYSTEM-001 is tested (2026-11-17):** generated theme tokens (`scripts/gen-themes.mjs` + `themes.json`, freshness step in `check.sh`), legacy aliases until QA-006, extended guard tests. Owner accepted CR-M1.6-SYSTEM-001-line-contrast (option A: `--line-input` for field borders).
- **M1.6-SHELL-001 is tested (2026-11-17):** `App.tsx` 433 → 117 lines, shell modules in `src/shell/`, 0 px change. **Wave B is complete.**
- **M1.6-SYSTEM-002 is tested (2026-11-17):** IBM Plex bundled (143 kB, unmodified IBM files). `bench/run.sh --ab <baseline>` now compares two builds interleaved, so before/after checks no longer need an idle machine.
- **M1.6-SYSTEM-003 is tested (2026-11-17):** control primitives in the material grammar, component gallery (`pnpm run gallery`, S17 in the shots harness), `ui/` import guard.
- **M1.6-SYSTEM-004 is tested (2026-11-17):** overlays (Menu, ContextMenu, Tooltip via Radix Tooltip 1.4.1, Dialog, Sheet, Toast, Banner) in `src/ui/overlays.tsx`.
- **M1.6-INTERACTION-001 is tested (2026-11-17):** motion level setting, presence, loop gate; idle CPU with a task running 8.0% → ~2.8% focused, 7.0% → ~0.3% unfocused.
- **Next:** see `scripts/tasks.py | grep '^M1.6-' | grep READY` (`scripts/tasks.py | grep '^M1.6-' | grep READY`).
- _(Wave B history:)_ M1.6-QA-002 (performance baseline and new scenarios),
  M1.6-SYSTEM-001 (generated tokens), M1.6-SHELL-001 (split `App.tsx`, no visual
  change). The QA baselines must be captured before the first restyle merges.
- **Inherited blocker:** M1.6-QA-005 (macOS/Windows review) needs real hosts or a
  CI run, as the M1.5 gate did.

## Qt shell retired (T-15), 2026-11-17

At the owner's instruction the **Qt shell is removed**. `apps/desktop/`, its CI
job, `bootstrap-qt.sh`, `check_qml.py`, `smoke.sh` and `dev-env.sh` are deleted;
`scripts/check.sh` has no Qt branch and `scripts/run.sh` launches the Tauri shell.
Recovery: tag `qt-shell-final` (`c9f3d82`), e.g.
`git checkout qt-shell-final -- apps/desktop scripts`. Earlier the same day T-14
made Tauri the default.

**Owner waivers** (recorded in `verification/M1.5.md` §6b): T-14's precondition
that T-12 be passed; §5.1 (a milestone as default), §5.2 (30 days of green CI;
the window started 2026-11-16 and keeps running as evidence) and §5.5 (the manual
accessibility run). Risk accepted: no real macOS or Windows launch of the
installers and no screen-reader pass preceded the removal; those remain the first
things to do (`agents/MANUAL_VERIFICATION.md`). The `emit` test helper and the Qt-vs-Tauri
comparison in `apps/desktop-tauri/bench/` are left in place and are now dead weight.

## Where we are

**M1 (platform foundation) is complete; M1.5 (the Qt → Tauri migration) is
closed by owner decision with the T-12 parity gate not formally passed.**
ADR-0012 is accepted: the desktop shell is Tauri 2 + React and the Qt shell is
gone (tag `qt-shell-final`).

| Area | State |
|------|-------|
| Architecture, ADRs 0001–0012 | Accepted (0012 supersedes the Qt shell decision) |
| IPC protocol 0.1 | Frozen; `crates/protocol`, schemas generated and checked |
| Editor bridge 1 | Frozen; implemented in `editor/` (43 Vitest tests) and shared by both shells |
| Backend (`crates/{rpc,store,workspace,exec,axiomd}`) | M1, 227 tests, unmodified by M1.5 |
| `crates/desktop-bridge` | Tested: 13 behaviour tests against the real daemon (incl. stderr forwarding) + 12 unit tests (framing, daemon lookup order) |
| `apps/desktop-tauri` | M1.5 shell: root-workspace Rust host + React UI; 5 host-surface + 3 release-guard tests, 51 Vitest tests, 51 end-to-end checks in 4 suites (smoke 22, quit 8, geometry 5, parity 13) through two WebDriver servers |
| CI | Actions budget for the month is nearly exhausted (owner, 2026-11-12): verification is local on Linux plus one batched run, not a per-change loop. Last run 43146126579: Linux+macOS green, Windows at typing-fixed/commands-pending |
| Qt shell | Retired by T-15 (2026-11-17); recover from tag `qt-shell-final` |
| M1.5 gate (T-12) | **Not passed** (superseded by the owner's T-14/T-15 decisions above): on Linux everything is verified (parity 52/56, all ten performance budgets, 48 end-to-end checks, package launches/finds its sidecar/connects) and macOS went green end to end in run 43141643434; Windows still fails at typing-free stages and KD-WIN-1 (Qt) is unchanged; macOS/Windows budgets, the manual protocol and the E-17 waiver are open — `docs/roadmap/verification/M1.5.md` |

## Latest handoff

`agents/handoffs/2026-11-17-M1.6-INTERACTION-001.md`. Earlier: `agents/handoffs/2026-11-17-M1.6-SYSTEM-004.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-003.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-002.md`, `agents/handoffs/2026-11-17-M1.6-SHELL-001.md`, `agents/handoffs/2026-11-17-M1.6-SYSTEM-001.md`, `agents/handoffs/2026-11-17-M1.6-QA-002.md`, `agents/handoffs/2026-11-17-M1.6-QA-001.md`, `agents/handoffs/2026-11-17-M1.6-DESIGN-001.md` (M1.6 approval), `agents/handoffs/2026-11-12-M1.5.md`.

## Next task

**M1.6 wave B**, on branch `task/m1.6` (see the M1.6 section at the top). Take the
first `READY` task whose id starts with `M1.6-`, in `order`:
`scripts/tasks.py | grep '^M1.6-' | grep READY`. Do **not** use `scripts/tasks.py
next` for this: it prints T-11 (an M1.5 spike with a lower `order` that is still
READY and is not part of M1.6). Wave B (M1.6-QA-001, M1.6-QA-002, M1.6-SYSTEM-001, M1.6-SHELL-001) is done; wave C is next
and M1.6-SHELL-001. Both QA baselines must be captured before any restyle merges.

## M1.5 follow-ups (open by owner waiver; not blocking M1.6)

These need macOS/Windows hardware or a CI run. T-14 and T-15 already ran by
waiver; the items stay open as evidence for the M1.5 record:

1. **Spend one CI run on the branch as it stands** (`task/m1.5`, 7 commits
   ahead of origin). It should turn the Windows cell green — typing now works
   there (`editContext: false`) and the command checks no longer use a POSIX
   shell — re-confirm macOS (green in run 43141643434), and produce the NSIS
   package. Expect exactly one known red cell after it: the Qt `desktop` job on
   Windows (KD-WIN-1), which disappears with T-15.
2. **Measure the budgets on macOS and Windows** (`bench/run.sh --runs 3
   --heavy-runs 3`, quoting the `medians` block and the load) — exit criterion
   3 needs them there, and Linux is done (all ten rows met).
3. **Run `agents/MANUAL_VERIFICATION.md`** on one real machine per OS and
   record the results in `docs/roadmap/verification/M1.5.md`. This is also
   where trusted keyboard input, IME, clipboard, drag and drop, HiDPI, screen
   readers, the 720x480 drag minimum and installing a package are verified on
   macOS and Windows.
4. **Close the three open parity rows**: S-8's screen reader and S-12's native
   folder dialog are manual-protocol items; T-4 is the bench's `output`
   scenario. (E-17 was waived by the owner on 2026-11-12.)

(T-14 and T-15 ran on 2026-11-17 by owner waiver; see the top of this file.)

## Blockers

Two, both outside the code:

- **GitHub Actions budget** for the month is spent (owner, 2026-11-12). CI is
  not available until it resets, so every local-only item is done and the rest
  waits for one batched run.
- **No macOS or Windows machine** in this environment: their budgets, package
  installs and the manual protocol need real hosts.

## Things the next agent should know

- This machine (Fedora 44) had no Rust and no Qt development files. Phase 0
  installed Rust via rustup (`~/.cargo`) and Qt 6.11.2 + 6.12.0 via aqtinstall
  into `.toolchains/` (git-ignored, ≈ 4 GB). `source scripts/dev-env.sh` picks
  them up. On another machine run `scripts/bootstrap-qt.sh`.
- `sudo` is not available non-interactively here; OpenGL headers were
  extracted locally (`scripts/bootstrap-qt.sh --gl-headers`).
- `cargo` is not on the default `PATH`: scripts add `~/.cargo/bin`
  themselves; in an interactive shell run `source scripts/dev-env.sh`.
- There *is* a remote: `origin` = github.com/j-w-i/Axiom. M1.5 work lives on
  `task/m1.5` (draft PR #1); `main` is untouched by the migration.

## M1.5 — Qt → Tauri migration (in progress, 2026-11-12)

ADR-0012 is **accepted**: the desktop shell becomes Tauri 2 + React. Tasks:
`docs/tasks/M1.5/`. Evidence and design: `docs/migration/qt-to-tauri/`.
Verification record: `docs/roadmap/verification/M1.5.md`.

Done and verified on Linux: T-01 (decisions), T-02 (bridge in
`crates/desktop-bridge`), T-03 (host in the root workspace, `--version`), T-04
(generated wire types), T-05 (test parity, 107 Qt functions mapped, 52 of 56
matrix rows ✅), T-06 (UI gaps), T-07 (in-window menus + macOS menu, the latter
compiled by cross-check), T-08 (crash recovery; the macOS hook is built but
never triggered), T-09 (CI wiring plus the end-to-end suites through two
WebDriver servers), T-13 (deb + rpm + macOS `.app` built in CI; a `.deb` also
verified as an installed copy: it finds its sidecar and completes the
handshake).

Done as recorded experiments: M5-PRE-001 (E4: preview over CDP, option (a)
adopted), T-11 (analysis; recommends *not* adopting editor isolation until one
of three stated triggers occurs).

Open: T-10 and T-12 (manual protocol and the gate — macOS and Windows need real
hosts). T-14 and T-15 are done by owner waiver (see the top of this file).

## Open questions (do not block M1)

See `docs/architecture/open-questions.md`.

## Documents to read for M1

`AGENTS.md` → `agents/IMPLEMENTATION_GUIDE.md` → the task file → only the
documents that task lists under *Context*.
