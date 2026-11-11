# Qt → Tauri: performance assessment

_Status: Phase 3. Targets, method and **one machine's** measurements. The
brief says "do not assume Tauri will outperform Qt"; this document reports
what was measured, including where Tauri loses, and what was not measured._

## 1. Method

### Principles
1. **Same hardware, same workload, same daemon binary**, both shells as
   black boxes (no app-side instrumentation in either).
2. **Interleaved runs**: each repetition runs Qt then Tauri (order
   alternating), so background load hits both; the 1-minute load average is
   recorded per sample.
3. **One clock for both shells**: `bench/daemon_tap.py` sits between each
   shell and `axiomd`, forwards bytes unchanged, and timestamps protocol
   events (`initialize` answered, first `fs/list`/`fs/readFile` answered).
4. **"Painted" means pixels on the X server changed**: the harness samples
   the relevant screen region with `XGetImage` at ~1.5–2 kHz (period recorded
   per run, 0.45–0.65 ms) after the triggering event. X DAMAGE was tried and
   rejected: Xvfb does not report sub-window drawing on the root.
5. **Real input**: XTEST key and button events, delivered by the X server
   exactly like a keyboard/mouse.
6. **Never on the developer's desktop**: everything runs on a private Xvfb
   (`scripts/headless.sh` refuses to run otherwise).

### Harness
`apps/desktop-tauri/bench/run.sh [--runs N] [--heavy-runs M] [--scenario …]`
→ JSON in `bench/results/`. Scenarios:

| Scenario | What is measured |
|----------|------------------|
| startup (cold, warm) | launch → window mapped → daemon connected → first `fs/list` answered → explorer pixels changed. The workspace is reopened automatically (reopen-last), so no input is involved. "Cold" = first launch with a fresh data dir after 3 s idle (dropping the OS page cache needs root and was not done; true cold-disk start is **not** measured). |
| memory | PSS (proportional set size, so shared libraries are not double-counted) of the shell process tree — shell + web engine helper processes — with the daemon reported separately: (a) workspace open, idle; (b) a document open in Monaco; (c) after 200 keystrokes |
| CPU | CPU-seconds of the shell tree over 10 s idle (a) and (b); CPU while typing |
| typing | 200 real keystrokes into Monaco at ~25/s; keystroke → editor pixels change; p50/p95/p99/max |
| explorer | restore an expanded directory of 5 000 entries (one 305 kB `fs/list` answer) → first paint and settled; then 60 mouse-wheel ticks over the tree → paint latency per tick and CPU |
| output | `seq 1 50000` from the command field → time until the output region stops changing for 1 s (minus the 1 s quiet window) and CPU used |

### Fairness notes (read before quoting numbers)
- **Qt binary**: `build/desktop/axiom` is RelWithDebInfo (`-O2 -g`). Its
  code is a small part of the process (Qt libraries and Chromium dominate),
  so this mostly affects binary size, which is reported stripped.
- **Rendering**: Xvfb, no GPU. Qt WebEngine and WebKitGTK both fall back to
  software paths; neither shell's GPU behaviour is measured. Real-GPU
  results are likely to differ, especially for scrolling and animation.
- **Window sizes differ** (Qt default 1100×720, Tauri 1180×760); regions are
  window-relative.
- **Typing latency includes compositing and readback** (~1–2 ms of sampling
  overhead) for both.
- **Web engines differ by OS**: this is a Linux result for WebKitGTK 2.54.2
  vs Chromium 140 (Qt 6.12 WebEngine). It says nothing about WebView2 or
  WKWebView.

## 2. Targets (budgets for the Tauri shell)

Budgets are set relative to Qt so the gate is "no regression", with absolute
caps where Qt itself is poor. They apply per OS on CI runners and on one
real-GPU machine per OS (roadmap T-12).

| Metric | Budget |
|--------|--------|
| Cold start → explorer painted | ≤ Qt, and ≤ 1.0 s on the reference machine |
| Warm start → explorer painted | ≤ Qt |
| Idle PSS, workspace open (shell tree) | ≤ Qt and ≤ 350 MiB |
| PSS with one Monaco document | ≤ Qt and ≤ 400 MiB |
| Idle CPU (10 s) | ≤ 1 % of one core |
| Typing latency | p95 ≤ 25 ms, p99 ≤ 50 ms, and p95 ≤ Qt + 2 ms |
| Explorer 5 000 entries: answer → settled | ≤ 300 ms |
| Explorer wheel scroll: per-tick paint | p95 ≤ 20 ms (≈ one 60 Hz frame + slack) |
| 50 000-line output settles | ≤ 1 s after the task exits, UI responsive throughout |
| Package (installed, excluding daemon) | ≤ 50 MiB Linux/Windows, ≤ 60 MiB macOS universal |

## 3. Results — Fedora 44, x86_64, 12 threads, 31 GiB, Xvfb (software rendering)

Run `JWI-Fedora-20261112-035614.json` (all scenarios, 3 startup reps, 1 heavy
rep) and `…-040109.json` (startup, 5 reps, after the Monaco-deferral fix
below). Load average 1.4–2.3 during both (desktop session idle, browser
open). An earlier run during a heavy parallel compile (load ≈ 10) is
**discarded** (kept as `DISCARDED-under-load-…-033542.json`; it also predates harness fixes): it inflated Qt's
typing p50 to 46 ms and is not comparable.

### 3.1 Startup (median of 5, ms from launch)

| Milestone | Qt cold | Tauri cold | Qt warm | Tauri warm |
|-----------|--------:|-----------:|--------:|-----------:|
| Window mapped | 766 | **105** | 690 | **102** |
| Daemon connected (`initialize` answered) | 847 | **150** | 748 | **149** |
| First `fs/list` answered | 853 | **417** | 754 | **413** |
| Explorer painted | 866 | **421** | 768 | **418** |

Tauri shows its window ~7× sooner and paints the restored explorer in about
half Qt's time. The gap between "connected" (150 ms) and "first list"
(417 ms) is the webview loading and parsing the 370 kB UI bundle under
software rendering; it is the largest remaining startup cost in Tauri.

**Regression found and fixed during measurement:** the first run measured
Tauri's explorer paint at 753–826 ms (no better than Qt) because opening a
workspace immediately loaded Monaco (~4 MB of JS) on the critical path. Monaco
now loads only when a document is opened, after the explorer frame. That
halved time-to-explorer; time-to-editor is unchanged.

### 3.2 Memory (PSS, MiB, shell process tree; daemon separate)

| State | Qt | Tauri | Δ |
|-------|---:|------:|---|
| Workspace open, idle | 445.5 | **281.7** | −37 % |
| + one document in Monaco | 494.2 | **307.0** | −38 % |
| after 200 keystrokes | 626.2 | **457.5** | −27 % |
| after 5 000-entry explorer + 60 wheel ticks | 450.2 | **316.0** | −30 % |
| after 50 000-line task | 450.6 | **291.2** | −35 % |
| `axiomd` (same binary) | 11.9 | 11.8 | — |
| Processes in the shell tree | 4 (shell + 3 QtWebEngine helper processes) | 3 (shell + 2 WebKit helper processes) | |

Both shells grow by 130–150 MiB after typing (Monaco/V8/JSC heaps; neither
had been garbage-collected at the sample point). The relative gap holds.

### 3.3 CPU

| Measure | Qt | Tauri |
|---------|---:|------:|
| Idle, workspace open (10 s, % of one core) | 0.2 | **0.0** |
| Idle with Monaco document (10 s) | 2.7 | **0.2** |
| While typing ~25 keys/s | 66.5 % | 64.2 % |
| 60 wheel ticks over 5 000-row explorer (CPU-s) | 2.74 | **1.42** |
| 50 000-line output (CPU-s, shell tree) | **0.08** | 0.16 |

The cause of Qt's 2.7 % idle CPU with Monaco open was not profiled (likely
the blinking cursor repainting through Chromium's software compositor); it
is reported as measured.

### 3.4 Editor input latency (keystroke → pixels, 200 samples, ms)

| | p50 | p95 | p99 | max |
|---|---:|---:|---:|---:|
| Qt (WebEngine/Chromium 140) | 19.1 | 22.8 | 27.6 | 36.8 |
| Tauri (WebKitGTK 2.54.2) | **17.2** | **18.8** | **22.4** | **33.6** |

Within ~2–4 ms of each other; both well inside one 60 Hz frame at p95 with
software rendering. This is *not* evidence that WebKit is faster in general;
it shows no regression on this platform.

### 3.5 Large workspace / explorer (5 000 entries in one directory)

| | Qt | Tauri |
|---|---:|---:|
| `fs/list` answer → first paint | 186 ms | **164 ms** |
| → last change (settled) | **192 ms** | 203 ms |
| Wheel tick → paint, p50 / p95 | **13.7 / 15.7 ms** | 16.3 / 17.8 ms |
| CPU for 60 wheel ticks | 2.74 s | **1.42 s** |

Equivalent. Qt scrolls ~2.5 ms faster per tick; Tauri uses half the CPU.
Both stay within one frame. The explorer is virtualised in both
(`TreeView` / fixed-row virtual list).

### 3.6 Streaming output (`seq 1 50000`)

| | Qt | Tauri |
|---|---:|---:|
| Enter → output region stable | **44.6 ms** | 98.9 ms |
| CPU (shell tree) | **0.08 s** | 0.16 s |

Qt wins: its C++ model applies chunks off the JS heap. Tauri passes each
`task/output` notification through the Rust channel to JS and re-renders a
virtual list per animation frame. Both are far inside the 1 s budget and the
UI stayed responsive (typing test not affected). The daemon chunks output at
8 KiB, so the event rate is the same for both. If this matters later, batch
notifications in the host (one channel message per frame).

### 3.7 Packaging size (Linux x86_64)

| Component | Qt shell | Tauri shell |
|-----------|---------:|------------:|
| Shell executable (stripped) | 2.0 MB | 8.0 MB *(includes the whole frontend + Monaco, compressed)* |
| Toolkit libraries the binary loads (Qt 6.12 + ICU, incl. WebEngineCore 200+ MB) | 292.8 MB | system WebKitGTK/GTK (not shipped in .deb/.rpm; ~100 MB in an AppImage, not measured) |
| WebEngine resources (`resources/`, one locale) | 26.2 MB | — |
| QML modules + platform plugins | 1.9 MB | — |
| Editor assets (`editor/dist`) | 14.3 MB | in the executable |
| **Total to ship (excluding `axiomd`, 8.3 MB in both)** | **≈ 337 MB** (≈ 321 MiB) | **8.0 MB** (+ system webview) |

Comparison caveats: the Qt figure is the set of libraries the binary loads
from the aqtinstall SDK (what `linuxdeployqt`/AppImage would bundle), not a
built installer. On Windows, Tauri relies on the Evergreen WebView2 runtime
(preinstalled on Windows 11; ~2 MB bootstrapper otherwise); on macOS on the
system WKWebView. Installer sizes on Windows/macOS are **not measured**.

### 3.8 UI animation smoothness

Not measured meaningfully: under Xvfb with software rendering, frame timing
reflects the CPU rasteriser, not either toolkit. The prototype animates only
opacity/transform (160 ms) and disables motion under reduced-motion. A
real-GPU frame-time capture per OS is part of roadmap T-10.

## 4. Summary against budgets (Linux, this machine)

| Metric | Budget | Tauri | Pass |
|--------|--------|-------|------|
| Cold start → explorer | ≤ Qt (866 ms), ≤ 1.0 s | 421 ms | ✅ |
| Warm start → explorer | ≤ Qt (768 ms) | 418 ms | ✅ |
| Idle PSS | ≤ Qt, ≤ 350 MiB | 282 MiB | ✅ |
| PSS with Monaco | ≤ Qt, ≤ 400 MiB | 307 MiB | ✅ |
| Idle CPU | ≤ 1 % | 0.0–0.2 % | ✅ |
| Typing p95 / p99 | ≤ 25 / ≤ 50, ≤ Qt+2 | 18.8 / 22.4 | ✅ |
| Explorer settle | ≤ 300 ms | 203 ms | ✅ |
| Wheel p95 | ≤ 20 ms | 17.8 ms | ✅ |
| 50k output | ≤ 1 s | 99 ms | ✅ (Qt faster) |
| Package | ≤ 50 MiB | 8 MB + system webview | ✅ (Linux .deb/.rpm) |
| macOS, Windows, real GPU | — | **not measured** | ❌ unknown |

**Sample size caveat:** heavy scenarios (memory, typing, explorer, output)
have one repetition per shell on a quiet machine (plus the discarded
loaded run, whose *relative* ordering for memory and startup was the same).
Startup has 5 + 3 repetitions. The harness supports `--heavy-runs N`;
the T-12 gate requires ≥ 5.

## 5. Reproducing

```sh
apps/desktop-tauri/scripts/sysroot.sh           # Fedora, no sudo: headers + Xvfb
cmake --build build/desktop                     # the Qt shell under test
apps/desktop-tauri/bench/run.sh --runs 5 --heavy-runs 5
```
Close CPU-heavy work first: the harness records load average but cannot
correct for it.
