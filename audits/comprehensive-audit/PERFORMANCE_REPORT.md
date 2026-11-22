# Performance report

Audit of `main` at `751ffc2`, 2026-11-22. All numbers below were measured during this audit;
nothing is estimated unless marked. Raw data: `evidence/perf/`, `evidence/logs/frames-sh.log`.

## Environment and limitations

- Fedora 44, Linux 7.3.3, 12 cores, 31 GiB RAM. **The machine was busy**: load average 4–11
  during the runs, swap full, ~8 GiB available. Absolute numbers carry that load; where a result
  is close to a budget it is flagged as needing an idle re-run.
- Display: a private `kwin_wayland --virtual` (2560×1440). Frame and perf runs use scale 1.25 as
  the owner's display; GTK 3 renders at the next integer scale, so the engine reports
  **dpr 2.0** and the window is 4080×2240 physical (the compositor downscales). Raster work is
  therefore ~2.6× what 125 % pixels would need. `frames.sh` has the same property.
- Builds: Flutter **profile** for frame and memory probes (`flutter drive --profile`), Flutter
  **release** for startup and idle CPU; `axiomd` **release** built from `751ffc2`
  (the checked-in `target/release/axiomd` was stale and was rebuilt).
- No CPU profiles or heap snapshots were captured (DevTools was not attached). Root causes for
  AUD-005 and AUD-015 are therefore hypotheses with named suspects.
- Integration-test frame policy `fullyLive` pumps frames continuously, so idle CPU measured
  inside the profile probe (14.8 %) is an artefact and **is not reported**; idle CPU was
  measured on the release app instead.

## Methodology

| Probe | What | How | Samples |
|-------|------|-----|---------|
| `evidence/harness/startup.sh` | cold/warm startup, idle CPU, PSS | release shell (audit copy with an `AUDIT_STARTUP` hook printing timestamps), release `axiomd`; exec time taken by the shell script, then `main` entered, first frame rasterised, daemon ready, folder (200 files) listed; idle: `/proc/<pid>/stat` ticks over 10 s after 15 s settle | 6 cold (fresh `XDG_CACHE_HOME` and data dir) + 6 warm, interleaved |
| `evidence/harness/audit_perf_test.dart` | large files, memory across repeated work, search and console rendering, typing under stream | profile build; engine `FrameTiming`; `/proc/self/smaps_rollup` PSS of the shell and daemon | 2 full runs (runs 2 and 3; run 1 aborted on a harness bug) |
| `apps/desktop-flutter/scripts/frames.sh` | the repository's own frame gate | unchanged, `AXIOM_FRAMES_ASSERT=0` | 1 run |
| `evidence/harness/search_probe.py` | search scaling | JSON-RPC to release `axiomd`; one file of N × `a`, query `a` | 1 per size |
| `axiom task run` | task completion with a background grandchild | CLI wall time | 1 |

## Baseline measurements

### Startup (release, 125 %, window 2040×1120) — budget P2: never > 1.0 s

| Run | exec → `main` | exec → first frame | exec → daemon ready | exec → folder listed |
|-----|--:|--:|--:|--:|
| cold #1 (first ever: page cache cold) | 818 | **1042** | 850 | 853 |
| cold #2–6 median | 251 | 483 | 282 | 286 |
| warm #1–6 median | 205 | 297 | 249 | 255 |

- Steady state is well inside budget. The very first launch (cold disk and shader cache) is
  just over 1 s, matching the known "first-ever launch compiles shaders" note in
  `verification/FL.md`.
- ~200 ms of every launch passes before Dart's `main` runs (engine and GTK start-up); first
  frame lands ~50–200 ms after the daemon is ready.
- Shutdown (`app.shutdown()`): 3–9 ms.

### Idle (release) — budgets P7–P9

Folder (200 files) + one document open, 10 s window after 15 s settle: **shell 0 % CPU, daemon
0 % CPU**; shell PSS 138 MiB, daemon PSS 22 MiB, shell threads 17. FL-PERF-001's caret fix
holds. (P8/P9 — idle with a running task, focused/unfocused — not measured; see AUD-080 for
the code-level concern.)

### Memory (profile build; release figures are lower: 138 MiB release vs 170 MiB profile with a folder)

| State | Shell PSS (MiB) run 2 / run 3 | Daemon PSS (MiB) |
|-------|--:|--:|
| connected, no folder | 170 / — | 14 |
| folder with 3 020 files | 170 / — | 20 |
| after 20 files × open/close rounds | 205 → 228 (10 rounds) / 198 → 221, flat after round 11 (20 rounds) | 22 |
| 100k-line file open | 240 / 257 | 35 |
| **after typing 34 characters in it** | **939 / 947** | 43 |
| after search (2 000 hits) and console flood (200 000 lines) | ~920–950 | 45–47 |

The open/close growth plateaus (~+23 MiB), so it is not reported as a leak; the large-file
edit growth (+~700 MiB, not released) is AUD-005.

### Large files (profile)

| Scenario | Result |
|----------|--------|
| open 100 000-line (6 MB) Rust file | 1152 / 1238 ms; **one frame with a 1005 / 1091 ms build** (AUD-015) |
| fling through it (10 flings) | 370 frames, build p99 5.8 ms, raster p99 4.8 ms, 0 over budget |
| typing 21 chars at its end | build max 12–15 ms, but frame **span p99 1.9–2.2 s** (2 over) |
| typing 13 chars at line 1 | **build max 4045 / 4471 ms**, 3–4 frames over (AUD-005) |
| open 50 000-line file (debug, exploratory) | 523 ms |

### Search, console, typing under load (profile)

| Scenario | Result |
|----------|--------|
| search `needle` across 3 000 files (6 000 hits, capped at 2 000) | first results 78 ms; 188 frames, build max 27 ms, 1 over |
| fling search results | build p99 4.7 ms, 0 over |
| console flood `seq 1 200000` | 383 frames, build max 4.2 ms, 0 over (10 000-line cap works) |
| typing while a task streams ~1 000 lines/s | build p99 8.1 ms, span p99 13.6 ms, 0 over (P12 met) |

### Search scaling (release daemon) — AUD-006

| Single-line file | `workspace/search` for `a` |
|--:|--:|
| 50 KB | 1.8 s |
| 100 KB | 7.1 s |
| 200 KB | 33.0 s |
| 400 KB | 134.2 s |

Quadratic: each doubling ×4–4.7.

### Task completion — AUD-007

`sh -c 'sleep 20 & echo started'`: output at once, task completes after **20 043 ms**.

### `frames.sh` (the repository's gate; 1 run, load 4.7)

| Interaction | frames | build p99 / max | raster p99 / max | over 16.7 ms |
|-------------|--:|--:|--:|--:|
| explorer fling | 567 | 2.0 / 2.2 | 3.6 / 4.1 | 0 |
| explorer wheel | 98 | 2.6 / 2.8 | 2.9 / 3.7 | 0 |
| side bar toggle | 35 | 10.5 / 10.5 | 3.7 / 3.7 | 0 |
| **palette open/close** | 48 | 11.6 / 11.6 | **24.6 / 24.6** | **6** |
| splitter drag | 93 | 16.3 / **17.5** | 4.9 / 7.5 | 1 |
| theme switch | 112 | 9.6 / 10.0 | 3.2 / 3.5 | 0 |
| tab switching | 186 | 8.8 / 16.6 | 7.6 / 12.4 | 0 |
| typing (5 000-line file) | 157 | 6.1 / 7.1 | 8.3 / 9.0 | 0 |
| drag selection | 161 | 4.1 / 5.8 | 7.1 / 7.2 | 0 |
| shift+down selection | 173 | 2.6 / 4.4 | 4.7 / 5.5 | 0 |
| editor fling | 301 | 4.0 / 4.2 | 5.8 / 6.3 | 0 |

The recorded gate ("no frame over 16.7 ms", `verification/FL.md`) is not met by this run for the
palette (AUD-062); it needs an idle re-run to separate load from regression.

## Identified bottlenecks (ranked by user impact)

1. **Edits in very large files** (AUD-005): multi-second UI stalls and ~700 MiB retained.
   Suspects: whole-document passes per keystroke on the UI isolate (FS-F7: `Doc.text` join for
   dirty/length checks and full-text LSP sync), line re-indexing in re_editor after an insert
   at the top, undo snapshots of the whole document.
2. **Search over long lines** (AUD-006): per-match rescans from line start, all matches kept
   before limiting, cancellation per file only.
3. **Opening very large files** (AUD-015): ~1 s synchronous work on the UI isolate.
4. **Tasks that never finish** (AUD-007): not CPU, but the console and folder switch wait on
   pipe EOF.
5. **Daemon blocking calls on async workers** (AUD-031, AUD-042): code-confirmed; not
   measured. `git::discover` (up to 10 s) and the inspector's per-hover search can stall
   unrelated requests.
6. **Palette first frames** (AUD-049, AUD-062): ranking in `build`, raster p99 24.6 ms.
7. **Rebuild scope** (AUD-072): shell-wide rebuilds on palette and splitter; currently within
   budget except splitter max 17.5 ms.

Not bottlenecks today (measured): startup, idle CPU, explorer with 3 000 entries, console
floods, typing in a 5 000-line file, typing under streaming output, search result rendering at
2 000 hits.

## Proposed optimisations, trade-offs and validation

| Issue | Optimisation | Trade-off | Validation benchmark |
|-------|--------------|-----------|----------------------|
| AUD-005 | Profile first; then incremental dirty tracking (length + rolling hash), incremental `didChange`, full-text only for full-sync servers built off-isolate, bounded/diffed undo | Vendored re_editor patch surface grows | `audit_perf_test` "huge typing at start": build max ≤ 16.7 ms, PSS growth ≤ 50 MiB over 50 keys; interleaved A/B, ≥ 3 pairs |
| AUD-006 | Single forward scan per line, per-file match cap at the remaining limit, cancellation every N matches | None material | `search_probe.py`: 2 MiB line < 1 s; linear scaling across 50 KB–2 MB |
| AUD-015 | Split lines on an isolate; progressive highlight | First paint shows a loading state | Open 100k lines: max frame build ≤ 50 ms |
| AUD-007 | Bounded drain after direct child exit; kill group on cancel | Late output of daemonised grandchildren dropped | `crates/exec` test: completes < 3 s |
| AUD-049/062 | Memoise ranking; selection in its own notifier; check palette shadow/blur cost in raster | Small code complexity | `frames.sh` palette: 0 over in 3 idle runs |
| AUD-031/042 | `spawn_blocking` for git discovery, watch setup, inspector search; debounce hover resolution | None | Daemon test: `fs/list` p99 < 50 ms while a slow git shim runs |
| AUD-072 | Narrow listenables; isolate the editor subtree from shell-row rebuilds | Refactor of `shell.dart` | Rebuild counts in a debug test; `frames.sh` splitter max < 16.7 ms |
| AUD-061 | Commit the Flutter-era startup/idle/memory harness (`evidence/harness/startup.sh` as a base) | Maintenance | Opt-in `scripts/check.sh perf` |

No improvement figures are promised beyond the budgets above; each must be shown by a
before/after interleaved run on the same machine.
