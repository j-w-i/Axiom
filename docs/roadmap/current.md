# Current state

_Last updated: 2026-11-17._

## Default shell switched (T-14), 2026-11-17

At the owner's instruction the **Tauri shell is now the default**: `scripts/check.sh`
runs the Tauri gates, `scripts/run.sh` launches it, and the Qt shell is reached with
`scripts/check.sh desktop-qt` (alias `desktop`, `everything` = default + Qt) and
`scripts/run.sh --shell qt`. The Qt shell is not removed and stays in CI. The
owner **waived T-14's precondition that T-12 be recorded as passed**: T-12 has not
passed (open parity rows S-8/S-12/T-4, manual verification and macOS/Windows
budgets and installs unrun). The risk accepted is the one roadmap §4 names: a
revert after M2 UI work lands on Tauri. T-15 (retire Qt) remains gated on all six
§5 conditions; the 30-day CI window started 2026-11-16 (`verification/M1.5.md` §7).
There is no separate release workflow: `ci.yml` already builds and uploads the
Tauri packages, so nothing points at Qt as a release artifact.

## Where we are

**M1 (platform foundation) is complete on the Qt shell; M1.5 (the Qt → Tauri
migration) is in progress.** ADR-0012 is accepted: the desktop shell becomes
Tauri 2 + React. The migration is gated — the Qt shell (`apps/desktop`) is
still the release artifact and stays buildable and tested until task T-14,
which may only run after the parity gate T-12.

| Area | State |
|------|-------|
| Architecture, ADRs 0001–0012 | Accepted (0012 supersedes the Qt shell decision) |
| IPC protocol 0.1 | Frozen; `crates/protocol`, schemas generated and checked |
| Editor bridge 1 | Frozen; implemented in `editor/` (43 Vitest tests) and shared by both shells |
| Backend (`crates/{rpc,store,workspace,exec,axiomd}`) | M1, 227 tests, unmodified by M1.5 |
| `crates/desktop-bridge` | Tested: 13 behaviour tests against the real daemon (incl. stderr forwarding) + 12 unit tests (framing, daemon lookup order) |
| `apps/desktop-tauri` | M1.5 shell: root-workspace Rust host + React UI; 5 host-surface + 3 release-guard tests, 51 Vitest tests, 51 end-to-end checks in 4 suites (smoke 22, quit 8, geometry 5, parity 13) through two WebDriver servers |
| CI | Actions budget for the month is nearly exhausted (owner, 2026-11-12): verification is local on Linux plus one batched run, not a per-change loop. Last run 43146126579: Linux+macOS green, Windows at typing-fixed/commands-pending |
| Qt shell | M1, 9 Qt test executables / 107 test functions, green; kept until T-15 |
| M1.5 gate (T-12) | **Not passed**: on Linux everything is verified (parity 52/56, all ten performance budgets, 48 end-to-end checks, package launches/finds its sidecar/connects) and macOS went green end to end in run 43141643434; Windows still fails at typing-free stages and KD-WIN-1 (Qt) is unchanged; macOS/Windows budgets, the manual protocol and the E-17 waiver are open — `docs/roadmap/verification/M1.5.md` |

## Latest handoff

`agents/handoffs/2026-11-12-M1.5.md`

## Next task

Finish the M1.5 gate (T-12), in this order:

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

Only after that may T-14 (make Tauri the default) and, a milestone later, T-15
(delete the Qt shell) run.

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
hosts), and **T-14 and T-15 are held**: the Qt shell remains the release
artifact until the gate passes.

## Open questions (do not block M1)

See `docs/architecture/open-questions.md`.

## Documents to read for M1

`AGENTS.md` → `agents/IMPLEMENTATION_GUIDE.md` → the task file → only the
documents that task lists under *Context*.
