# M1.6 performance: baseline, budgets, procedures

_Status: **approved by ADR-0013** (2026-11-17); **implemented (M1.6, 2026-11-18)**; budgets P1–P12 adopted unchanged. Existing budgets
(`docs/migration/qt-to-tauri/performance.md` §2) stay in force; this document adds
the budgets the redesign introduces and says how each is measured._

## 1. Baseline (current shell, before any M1.6 change)

Measured by M1.6-QA-002 on branch `task/m1.6` at `90e195e` plus the QA-001/QA-002
tooling (no UI change), with `AXIOM_BENCH_NO_BUILD=1 bench/run.sh --only tauri --runs 3
--heavy-runs 3`. Source: **`apps/desktop-tauri/bench/results/JWI-Fedora-20261117-061550.json`**
(`medians` block). Median of three runs, with min–max.

Reference machine: Fedora 44, x86_64, WebKitGTK 2.54, Xvfb 1440x900 via
`scripts/headless.sh` (software raster, no GPU). Load average at start 0.29; it rose
to ~2.3 during repetition 2 from unrelated desktop activity (a first full run,
`DISCARDED-under-load-JWI-Fedora-20261117-054812.json`, was discarded because its
repetition 1 ran at load 1.2-2.0 right after a build). Rows whose spread exceeds 15% of
the median are marked †; they are counts of rare long frames or tail percentiles, which
one loaded repetition moves. The ten pre-existing rows agree with the previous run
(`JWI-Fedora-20261116-223103.json`: PSS 185/298/448 MiB, idle CPU 0.1/1.0%).

| Metric | Median | Min–max | Budget it feeds |
|--------|-------:|--------:|-----------------|
| Cold start → explorer painted | 418 ms † | 365–437 | P2 |
| Warm start → explorer painted | 383 ms † | 360–421 | P2 |
| Typing p50 / p95 / p99 (keystroke → pixels) | 17.4 / 18.5 / 20.9 ms † | p95 18.3–19.4; p99 19.5–33.9 | P1 |
| Explorer 5,000 entries, answer → settled | 48.4 ms † | 37–56 | P11 |
| Explorer wheel tick → paint p95 | 17.4 ms | 17.3–18.0 | P11 |
| 50,000-line output settles | 79.2 ms | 76–80 | P11 |
| PSS: workspace open / one document / after typing | 183.2 / 294.2 / 445.0 MiB | 180–185 / 294.0–294.3 / 445–451 | P10 |
| Idle CPU: workspace open / with editor (10 s windows) | 0.1 / 1.0% | 0.1–0.2 / 0.9–1.2 | P7 |
| Bundle: UI JS (`index-*.js`, Monaco chunks excluded) | 377.0 kB | n/a | |
| Bundle: CSS, app / all (all includes Monaco's) | 16.6 / 202.6 kB | n/a | |
| **Tab switch** (4 documents, Ctrl+Tab x20): key → frame p50 / p95 | 24 / 36 ms | p95 34–41 | P5 (≤ 32) |
| **Palette** (Ctrl+K x20): key → first frame with palette p50 / p95 | 16 / 45 ms † | p95 43–58 | P6 |
| **Side bar toggle** (Ctrl+B x10): frame p95 / max, frames > 25 ms | 17 / 17 ms, 0 | 17 / 17 | P3 |
| **Panel toggle** (Ctrl+J x10): frame p95 / max, frames > 25 ms | 17 / 23 ms, 0 | max 20–23 | P3 |
| **Splitter drag** (40 pointer moves): frame p95, React commits | 19 ms, **68** | commits 66–69 | P4 (one commit) |
| **Idle, `sleep 30` running, window focused** (CPU of the app + WebKit processes) | **8.0%** † | 6.9–8.2 | P8 (≤ 3%) |
| **Idle, `sleep 30` running, window unfocused** | 7.0% | 6.1–7.0 | P9 |
| Idle, no task, same session | 1.0% | 0.9–1.4 | P7 |
| **Typing under streaming output** (~1,500-2,000 lines/s): p50 / p95, keystrokes made while streaming | 16.5 / 18.4 ms, 82 | p95 18.4–19.0 | P12 |

**P11 output scenario repaired (M1.6-SHELL-006).** The `output` scenario clicked the command
field at a fixed window fraction; SHELL-002 moved the console, the click landed in the editor,
`seq` never ran, and the scenario reported ~1 ms "settled" for both builds without failing. It
now focuses the field from the keyboard (Ctrl+Shift+R, parity T-7) and reports no value when the
output region never changed. The 79.2 ms baseline above predates SHELL-002 and is valid; under
the 2026-11-18 load the repaired scenario measures ~100 ms for both the SHELL-005 and the
SHELL-006 build (10 interleaved pairs, paired delta −1.5 ms).

What the baseline already says about the budgets (for the redesign, not a failure of
this task):

- **P4:** the splitter commits React once per pointer move (68 commits for 40 moves)
  against a budget of one. SHELL-002 has to change that.
- **P5:** the unchanged shell's tab switch p95 (36 ms) is already above the 32 ms budget.
- **P8 / P9:** a running task costs about 7-8% of a core, and **the cost is the same with
  the window unfocused** (7.0%) as focused: nothing pauses. (WebKitWebProcess ~4.2%,
  the app process ~3%.) Whatever animates while a task runs today is the thing the
  loop-gating work has to remove; P9's "baseline + 0.2" must be read against the
  no-task 1.0%, not this 7.0%.
- This WebKitGTK reports `requestAnimationFrame` timestamps in whole milliseconds, so the
  frame statistics are too ("p95 17" is a 16-17 ms interval).

## 2. Prototype measurements (indicative)

Headless Chrome, software raster, same machine; `concepts.md` §4 has the full
table. They establish that the motion design holds 60 Hz (0 frames > 20 ms in any
concept) and quantify the cost of looping indicators (18% of a core smooth vs 4.4%
under the stepped policy). They **do not** substitute for production numbers: the
engine differs (Blink vs WebKitGTK), and so does the raster path.

## 3. Budgets added by M1.6

All are measured on the reference Linux machine by `bench/run.sh` (and on macOS and
Windows when hardware exists: QA-005). A regression beyond a budget fails the task
unless the owner accepts it in writing in the verification record.

| # | Metric | Budget | Rationale |
|---|--------|--------|-----------|
| P1 | Typing p95 / p99 | ≤ baseline + 1 ms / ≤ baseline + 2 ms | the editor is sacred (brief §4.1) |
| P2 | Cold / warm start → explorer painted | ≤ baseline + 40 ms (fonts and CSS), never > 1.0 s | bundled fonts are about 180 kB |
| P3 | Frames during shell motion (palette, side bar, console, tab glide, menus, toast) | p95 ≤ 17.5 ms, none > 25 ms | 60 Hz with headroom |
| P4 | Splitter drag (40 pointer moves) | p95 frame ≤ 17.5 ms; one React commit per drag | no render per pointer event |
| P5 | Tab switch: key → new document painted | p95 ≤ 32 ms (two frames) | Monaco model swap plus indicator |
| P6 | Palette: key → first painted frame | ≤ 1 frame after the key's frame; filter 5,000 files ≤ 8 ms per keystroke | keyboard-first surface |
| P7 | Idle CPU, no task | ≤ baseline + 0.2 points | nothing animates while idle |
| P8 | Idle CPU, task running, window focused, user idle | ≤ 3% of a core (process total) | looping policy; prototype 4.4% in *software* raster |
| P9 | Idle CPU, task running, window unfocused | ≤ baseline + 0.2 points | loops paused when unfocused |
| P10 | PSS with one document | ≤ baseline + 15 MiB | fonts, CSS, gallery excluded |
| P11 | Explorer settle / wheel / 50k output | existing budgets unchanged | restyle must keep row virtualisation |
| P12 | Output streaming while typing (agent-like load: 2,000 lines/s into the console) | typing p95 within P1 | streaming must not starve input |

**Measured at M1.6 exit (M1.6-QA-004, 2026-11-18, interleaved against the QA-002 build):** P1, P2,
P6 (median and filter), P7, P9, P10, P11 and P12 (p95) met; P3 (palette/menu exits, one toggle frame),
P4 frame p95, P5, P6 cold first open and P8 (4.1 %, Motion Off 2.2–2.5 %) not met — P4, P5 and the
P6 cold open equally in the pre-redesign build. Numbers, causes and owners:
`docs/roadmap/verification/M1.6.md` §Performance. No budget was changed.

## 4. Procedures

### 4.1 Existing harness

`apps/desktop-tauri/bench/run.sh --runs 3 --heavy-runs 3` (quote the `medians`
block and the load average). Results go to `bench/results/` with the host name.

**Comparing two builds on a busy machine (`--ab`).** `AXIOM_BENCH_NO_BUILD=1 bench/run.sh
--only tauri --ab <baseline binary> --runs N --heavy-runs N --scenario startup|memory|…` runs
the baseline as a second kind, interleaved with the current build repetition by repetition
(alternating which goes first), and adds a `paired` block: per metric, the distribution of
(current − baseline) over the repetitions. Both builds see the same background load within a
repetition, so the paired difference is the verdict for a before/after check and there is no
need to wait for an idle machine; absolute values still carry the load and are not a baseline.
Build the baseline binary from the previous commit's frontend (`vite build` in a temporary
worktree, embedded by `cargo build --release -p axiom-desktop-tauri`, copied to
`target/release/axiom-tauri-base`).

### 4.2 New scenarios (QA-002 adds them to `bench/run.py`, driving the real app through the e2e WebDriver)

| Scenario | Method |
|----------|--------|
| `tab-switch` | 4 documents open; Ctrl+Tab ×20; for each, `keydown.timeStamp` → the first `requestAnimationFrame` after Monaco's `onDidChangeModel` |
| `palette` | Ctrl+K ×20 (open, ArrowDown ×2, Escape); key → the first frame with `.palette` painted; rAF intervals during open/close |
| `panel-toggle` | Ctrl+B and Ctrl+J ×10 each; rAF intervals for 400 ms |
| `splitter-drag` | 40 pointer moves of 3 px each, 16 ms apart, as **real X input** (XTEST) on the side-bar splitter; rAF intervals and the React commit count (read through the React DevTools hook the probe installs; a `<Profiler>`'s `onRender` does not fire in a production React build) |
| `idle-running` | run `sleep 30`; sample `/proc/<pid>/stat` of the app plus WebKit web and network processes for 10 s, focused and unfocused |
| `typing-under-stream` | run `python3 -c "import time,sys;[(print('x'*80),sys.stdout.flush(),time.sleep(0.0005)) for _ in range(10000)]"`; type 200 characters with the existing typing probe. The stream ends before the typing does (about 6 s against 8 s), so the page stamps the moment the task stops and `typing_under_stream_*` counts only keystrokes made before it |

rAF intervals are collected by an e2e-only script (`window.__axiomFrames`, with
`__axiomMarks` and `__axiomCommits`; `src/e2e-probe.ts`) compiled into the e2e-mode build
only (`pnpm run build:e2e` + `--features e2e`, kept as `target/release/axiom-tauri-e2e`);
`tests/probe-absent.test.ts` fails if any of it reaches the production bundle. All input is
real X input; the WebDriver only starts tasks and reads what the page measured.
`AXIOM_BENCH_NO_BUILD=1` skips `run.sh`'s builds (a build leaves the load average high
for minutes).

### 4.3 Reporting

Each implementation task with a performance note runs the affected scenarios
before and after, on the same machine and load, and pastes both medians into its
completion report. QA-004 runs the full suite at the end of the milestone and
records the table in `docs/roadmap/verification/M1.6.md`.

## 5. Things the design deliberately avoids for performance

- No `backdrop-filter`, `filter`, blur or animated shadows (raster cost per frame).
- No animated layout properties; panels resize in one frame (motion-system §3.2).
- No JS animation library; no per-frame React state.
- Stepped loops with at most three visible, paused when unfocused.
- Gradients only on small raised controls (cheap, static); panels flat.
- The meter and elapsed timer update at 4 Hz / 1 Hz, not per frame.
