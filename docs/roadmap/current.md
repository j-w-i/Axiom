# Current state

_Last updated: 2026-11-12 (M1.5 implementation session)._

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
| `crates/desktop-bridge` | Tested (12 behaviour + 9 framing tests against the real daemon) |
| `apps/desktop-tauri` | M1.5 shell: root-workspace Rust host + React UI; 5 host-surface tests, 31 Vitest tests, 21 end-to-end checks |
| CI | Actions budget for the month is nearly exhausted (owner, 2026-11-12): verification is local on Linux plus one batched run, not a per-change loop. Last run 43146126579: Linux+macOS green, Windows at typing-fixed/commands-pending |
| Qt shell | M1, 9 Qt test executables / 107 test functions, green; kept until T-15 |
| M1.5 gate (T-12) | **Closer, not passed**: Linux and macOS verified in CI; Windows has two open defects (KD-WIN-1 Qt-only, KD-WIN-2 end-to-end) — `docs/roadmap/verification/M1.5.md` §5c |

## Latest handoff

`agents/handoffs/2026-11-12-M1.5.md`

## Next task

Finish the M1.5 gate (T-12), in this order:

1. **Fix KD-WIN-2** (Windows end-to-end): the app must expose a WebView2
   DevTools endpoint for msedgedriver. Needs a Windows machine to iterate;
   suspects are listed in `verification/M1.5.md` §5c.
2. **Run `agents/MANUAL_VERIFICATION.md`** on one real machine per OS and
   record the results in `docs/roadmap/verification/M1.5.md`.
3. **Build the Windows (MSI/NSIS) and macOS (.dmg) packages** in CI; Linux
   `.deb` already builds (8.5 MiB, daemon inside).
4. Then T-12 again with the owner: waive or close the 5 open parity rows.
   Only after that may T-14 (make Tauri the default) and, a milestone later,
   T-15 (delete the Qt shell) run.

## Blockers

None.

## Things the next agent should know

- This machine (Fedora 44) had no Rust and no Qt development files. Phase 0
  installed Rust via rustup (`~/.cargo`) and Qt 6.11.2 + 6.12.0 via aqtinstall
  into `.toolchains/` (git-ignored, ≈ 4 GB). `source scripts/dev-env.sh` picks
  them up. On another machine run `scripts/bootstrap-qt.sh`.
- `sudo` is not available non-interactively here; OpenGL headers were
  extracted locally (`scripts/bootstrap-qt.sh --gl-headers`).
- `cargo` is not on the default `PATH`: scripts add `~/.cargo/bin`
  themselves; in an interactive shell run `source scripts/dev-env.sh`.
- There is no git remote. Commits are local on `main`.

## M1.5 — Qt → Tauri migration (in progress, 2026-11-12)

ADR-0012 is **accepted**: the desktop shell becomes Tauri 2 + React. Tasks:
`docs/tasks/M1.5/`. Evidence and design: `docs/migration/qt-to-tauri/`.
Verification record: `docs/roadmap/verification/M1.5.md`.

Done and tested on Linux: T-01 (decisions), T-02 (bridge in
`crates/desktop-bridge`), T-03 (host in the root workspace, `--version`),
T-04 (generated wire types), T-05 (test parity, 107 Qt functions mapped),
T-06 (UI gaps), T-08 (crash recovery).

Written but **not verified here**: T-07 (macOS native menu — compiled only by
CI), T-09 (CI job — nothing pushed yet), T-10 (manual protocol, not executed),
T-12 (gate report — **the gate did not pass**), T-13 (.deb builds; Windows and
macOS packages do not).

Not started: T-11 (editor-isolation spike; analysis in
`editor-isolation.md`), M5-PRE-001 (preview-engine spike), **T-14 and T-15 are
held** — the Qt shell remains the release artifact until the gate passes.

Next: push the branch, read the `tauri` CI job on all three OSes, run
`agents/MANUAL_VERIFICATION.md` on one machine per OS, build the Windows and
macOS packages, then T-12 again with the owner.

## Open questions (do not block M1)

See `docs/architecture/open-questions.md`.

## Documents to read for M1

`AGENTS.md` → `agents/IMPLEMENTATION_GUIDE.md` → the task file → only the
documents that task lists under *Context*.
