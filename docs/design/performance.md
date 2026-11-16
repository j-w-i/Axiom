# M1.6 performance: baseline, budgets, procedures

_Status: proposal, 2026-11-17. Existing budgets
(`docs/migration/qt-to-tauri/performance.md` §2) stay in force; this document adds
the budgets the redesign introduces and says how each is measured._

## 1. Baseline (current shell, before any M1.6 change)

The shell's UI code is unchanged since these runs (`git log -- apps/desktop-tauri/src
editor/src` is empty after 2026-11-16), so they are the provisional baseline.
**M1.6-QA-002 re-runs them on the M1.6 branch before the first restyle merges** and
replaces this table with its results.

Reference machine: Fedora 44, x86_64, WebKitGTK, Xvfb via `scripts/headless.sh`.

| Metric | Value | Source |
|--------|------:|--------|
| Cold start → explorer painted | 408 ms | migration `performance.md` §3.6 (2026-11-12) |
| Warm start → explorer painted | 404 ms | same |
| Typing p95 / p99 (keystroke → pixels) | 17.9–18.3 / 18.6–19.7 ms | same |
| Explorer 5,000 entries, answer → settled | 35.7 ms | same |
| Explorer wheel tick → paint p95 | 17.2 ms | same |
| 50,000-line output settles | 74.8 ms | same |
| PSS, workspace open / one document / after typing | 185 / 298 / 448 MiB | `bench/results/JWI-Fedora-20261116-223103.json` |
| Idle CPU, workspace open / with editor (10 s) | 0.1% / 1.0% | same |
| Bundle (UI JS, excluding Monaco chunk) | to be recorded by QA-002 | |
| Tab switch, palette open, panel toggle, splitter drag | **not measured yet** (new scenarios, QA-002) | |

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

## 4. Procedures

### 4.1 Existing harness

`apps/desktop-tauri/bench/run.sh --runs 3 --heavy-runs 3` (quote the `medians`
block and the load average). Results go to `bench/results/` with the host name.

### 4.2 New scenarios (QA-002 adds them to `bench/run.py`, driving the real app through the e2e WebDriver)

| Scenario | Method |
|----------|--------|
| `tab-switch` | 4 documents open; Ctrl+Tab ×20; for each, `keydown.timeStamp` → the first `requestAnimationFrame` after Monaco's `onDidChangeModel` |
| `palette` | Ctrl+K ×20 (open, ArrowDown ×2, Escape); key → the first frame with `.palette` painted; rAF intervals during open/close |
| `panel-toggle` | Ctrl+B and Ctrl+J ×10 each; rAF intervals for 400 ms |
| `splitter-drag` | 40 pointer moves via WebDriver actions; rAF intervals and the React commit count (dev-only profiler hook) |
| `idle-running` | run `sleep 30`; sample `/proc/<pid>/stat` of the app plus WebKit web and network processes for 10 s, focused and unfocused |
| `typing-under-stream` | run a command printing 2,000 lines/s; type 200 characters; existing typing probe |

rAF intervals are collected by an e2e-only script (`window.__axiomFrames`) compiled
into the `e2e` Cargo-feature build only.

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
