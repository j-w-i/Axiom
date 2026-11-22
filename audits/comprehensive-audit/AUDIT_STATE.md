# Audit state

Durable record of the comprehensive audit so another session can continue without the original
conversation. Audit of `main` at `751ffc2` (clean tree), 2026-11-22.

## Implementation authorisation

**Not authorised.** The owner's brief is audit-first: no fixes until explicitly approved. The
only repository writes made are under `audits/comprehensive-audit/` (nothing committed). When
authorised, follow `REMEDIATION_PLAN.md` and record progress in §Implementation log below.

## Progress

| Phase | State |
|-------|-------|
| 0 Discovery | Done |
| 1 Static audit | Done — four reviewers (Rust core; Rust services/security; Flutter state, client and Linux runner; Flutter UI). Raw output in `evidence/static/`; each file ends with what it did not review |
| 2 Build and tests | Done — Rust gate, Flutter gate, inspector tests, 15 e2e suites, `frames.sh` |
| 3 GUI testing | Done — exploratory pass (46 steps) and repros R1–R9 |
| 4 Visual audit | Done — 75 screenshots reviewed |
| 5 Performance | Done — startup ×12, idle, 2 profile probes, frames, search scaling, task completion |
| 6 Accessibility | Done within limits (no screen reader) |
| 7 Cross-system | Done — root-cause clusters in `AUDIT_REPORT.md` |
| 8–10 Consolidation, reports, self-check | Done |
| 11 Stop | **Waiting for the owner's instructions** |

## Environment

- Fedora 44 (Linux 7.3.3), 12 cores, 31 GiB RAM; busy during the audit (load 2.4–11, swap
  full, ~8 GiB available).
- Flutter 3.50.1 (`~/flutter`), Rust stable, `kwin_wayland` for private compositors, Xvfb in
  `.toolchains/tauri-sysroot`, Google Chrome present (preview test), pnpm present.
- `target/release/axiomd` was stale (14:23) and was rebuilt from `751ffc2` for performance runs.
- rust-analyzer is a missing rustup component on this machine (the stub fails), so real LSP
  sessions could not be exercised; network installs were deliberately not run.

## How to re-run the GUI and performance harness

The audit's suites ran in a disposable copy of the shell outside the repository; their sources
are kept in `evidence/harness/`:

```sh
rsync -a --exclude build --exclude .dart_tool apps/desktop-flutter/ /tmp/axiom-audit-app/
cp audits/comprehensive-audit/evidence/harness/*.dart /tmp/axiom-audit-app/integration_test/
H=audits/comprehensive-audit/evidence/harness
$H/run_explore.sh /tmp/axiom-audit-app audit_explore_test.dart   # debug, KWin scale 1, 1440×900
$H/run_explore.sh /tmp/axiom-audit-app audit_repro_test.dart     # repros R1–R9, 1180×760
AXIOMD=$PWD/target/release/axiomd $H/run_drive.sh /tmp/axiom-audit-app audit_perf_test.dart  # profile, 125 %
python3 $H/search_probe.py target/release/axiomd 200000         # search scaling
```

`startup.sh` additionally needs an env-gated timing hook in the copy's `lib/main.dart`
(`AUDIT_STARTUP=1`: print `main_entered_us`, `first_frame_us` via
`waitUntilFirstFrameRasterized`, `ready_us` after `app.start()`, then optionally
`AUDIT_OPEN`/`AUDIT_FILE`/`AUDIT_HOLD`, shutdown and exit) and a release build of the copy
(`flutter build linux --release`, after `source scripts/env.sh`). The scripts write into
`evidence/` (`AUDIT_OUT`). The hook was never added to the repository.

## Commands executed (all from the repository root)

```
scripts/check.sh rust                     # pass: 532 tests, 48 s; inspector 11/11; actionlint skipped
scripts/check.sh flutter                  # pass: 89 tests
apps/desktop-flutter/scripts/e2e.sh       # pass: 15 suites (14 cases pass-by-skip), 3 m 13 s
run_explore.sh … audit_explore_test.dart  # 46 steps, 12 framework errors
run_explore.sh … audit_repro_test.dart    # R1–R9
cargo build --release -p axiomd
run_drive.sh … audit_perf_test.dart       # 3 runs (run 1 aborted: harness search step; fixed)
AXIOM_FRAMES_ASSERT=0 apps/desktop-flutter/scripts/frames.sh
startup.sh <copy> 6                       # 6 cold + 6 warm + idle
target/debug/axiom task run --workspace <tmp> -- sh -c 'sleep 20 & echo started'   # 20 043 ms
python3 search_probe.py target/release/axiomd {50000,100000,200000,400000}
```

## Evidence locations

- `evidence/screenshots/` — exploratory `NN-*.png` (62) and repro `repro-R*.png` (13).
- `evidence/logs/` — `check-rust.log`, `check-flutter.log`, `e2e.log`, `explore-run.log`,
  `repro-run.log`, `perf-run-{1,2,3}.log`, `frames-sh.log`, build logs (all `*.log`, which the
  repository's `.gitignore` ignores — local only), and `explore-results.json`,
  `repro-results.json` (tracked if committed).
- `evidence/perf/` — `audit-perf-*.json` (runs 2 and 3), `startup-idle.txt`,
  `search-scaling.txt`.
- `evidence/static/` — `rust-core.md` (RC-1…27), `rust-services.md` (RS-F1…18),
  `flutter-state.md` (FS-F1…17), `flutter-ui.md` (FUI-01…37).
- `evidence/harness/` — the audit's test sources and runner scripts.

**Privacy note:** terminal screenshots (`34-terminal.png`, `36-*`, `40-*`, `42-*`) and logs show
the local user name, host name and home paths. Redact before publishing the evidence outside
the owner's machine.

## Mapping of static findings to issues

Every static finding is either an `ISSUES.md` entry or merged into one (merges are named in the
entry: e.g. RC-18 + RS-F4 → AUD-039, RC-14 + RS-F7 → AUD-035, RC-11 + RS-F12 → AUD-032,
FUI-25 + FS-F15 → AUD-072, FUI-36 + FS-F14 → AUD-083, FS-F7 → AUD-005, RS-F18 split across
AUD-010/031/109).

## Outstanding investigations (not done; worth doing before or during remediation)

1. CPU profile and heap snapshot for AUD-005 and AUD-015 (DevTools on the profile build).
2. The inactive-element `findRenderObject` exception seen once during split (exploratory step
   "split editor right"): the element is re_editor's `_CodeField` (a `GlobalKey` inside the
   vendored editor), reached from a listener after the old group's editor was deactivated —
   likely a geometry/LSP listener (`lsp_view.dart` reads `gutterKey`/render boxes) not removed
   on deactivate. Not isolated. (The shell's own `_gutter` key is per editor instance and is not
   shared between groups — checked.)
3. Idle re-run of `frames.sh` to separate load from regression (AUD-062).
4. Idle CPU with a running task, focused and unfocused (budgets P8/P9; AUD-080).
5. Runtime confirmation of the code-only P1s: AUD-008 (in a throwaway VM), AUD-009/010 with a
   configured model or the fake ACP agent, AUD-011, AUD-012 (low inotify limit), AUD-013 at
   900×560.
6. macOS run of the harness (CI only today); X11 path of the Linux runner.
7. Real language servers (install rust-analyzer component; completion/hover placement
   AUD-052).
8. Packaging (`scripts/check.sh package`).

## Assumptions

- The owner's preference (memory) that the Flutter shell look like the retired Tauri shell is
  the design baseline; no recommendation departs from it.
- Kotlin's known breakage and the deferred Windows build were not re-reported as defects.
- "Window size not restored" and "no recovery after a shell crash" are already owed in
  `verification/FL.md`; they are referenced, not re-filed.

## Implementation log

_Empty — implementation not authorised._
