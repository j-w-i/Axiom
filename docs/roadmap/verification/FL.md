# FL verification — the Flutter shell

_2026-11-22. Each task added its evidence; FL-QA-001 completed the record (§The gate). Linux:
Fedora 44, KDE Wayland, NVIDIA RTX 3060 Ti; the suites in a private virtual KWin and a private
Xvfb. macOS: CI. Windows: owed (deferred by the owner)._

## The gate (ADR-0020 §7, FL-QA-001)

| Criterion | Evidence | Result |
|-----------|----------|--------|
| M1 exit scenario: folder, explorer, editor with save, conflicts and restore, tasks with streamed output | `shell_test`, `editor_test`, `console_test` | pass |
| M2 exit scenario: Rust missing, plan, managed install, build inside it, Problems, Remove | `env_test` (fixture providers) passes here; `env_rust_test` (the real Rust install) needs the internet and runs in CI on Ubuntu | fixture pass; real install: CI |
| M3 exit scenario: tools-only fix, checkpoint before writes, denial, Restore, resume after restart, MCP, ACP | `agent_test`, `chat_test` | pass |
| M4 exit scenario: language servers, the terminal, Git, Search, the split | `lsp_test` (fake-lsp here; rust-analyzer and typescript-language-server required in CI on Ubuntu), `console_test`, `git_test`, `search_test`, `editor_test` | pass |
| Frame budget (16.7 ms) at 125 % in every listed interaction | §Frame timings: shell, editor, Chat streaming | no frame over |
| Accessibility: names, roles, focus order, contrast, both themes | `a11y_test` (every page, the console views, Settings, the palette, in both themes; Tab never stuck; Ctrl+M); `tokens_test` (contrast) | pass |
| Packages: .deb and .rpm (desktop entry and icons named after the app id), macOS app bundle | `scripts/bundle.sh` and `scripts/verify-package.sh` (4/4 each for the .deb and the .rpm here); the macOS bundle in CI | Linux pass; macOS: CI |
| Startup no worse than the Tauri shell's P2 | §Startup | pass, with one caveat |
| `scripts/check.sh` passes without the Tauri shell | the full gate, after the removal | pass (actionlint not installed here; CI runs it) |

**Owed:** Windows, no build exists (deferred by the owner); the macOS results of the CI run
after this push; the M2 real-install scenario's CI run.

**Parity: the deferred editor features are now in.**
- Signature help: Ctrl+Shift+Space, `(` and `,`.
- Code actions: Ctrl+., in a picker, with resolve and executeCommand.
- References: Shift+F12. Go to Symbol: Ctrl+Shift+O.
- Document highlights.
- Dragging a tab onto another moves it there.

All are in `lsp_test` against fake-lsp, which now answers them.

**Deviations from the Tauri shell:**
- **No image paste in Chat.** Flutter's clipboard is text only; images attach through the file
  picker.
- **The diff view is drawn by the shell** from the HEAD text, with no highlighting within a line.
- **No form for a custom ACP agent's command;** it is set through `agent/configureProvider`, as
  before.
- **Tab in the editor indents.** Ctrl+M makes it move focus instead, as in Monaco.

**Owed (found by the documentation pass, not in the gate):**
- **Window size and position are not restored.** The Tauri shell kept them in `window.json`; the
  GTK runner opens at 1180×760.
- **No recovery of unsaved text after a crash of the shell.** The Tauri shell had
  `snapshot_put`.

Both are follow-ups for the Flutter shell.

### Startup
The Tauri bench harness (black box: X DAMAGE on a private Xvfb, the daemon's wire through a tap)
compared the Flutter release build (`base`) and the Tauri release build. They ran interleaved, 5
pairs, against the same release axiomd, measuring startup to the explorer painted with the last
folder reopened.

Run 1: each cold run had a fresh `XDG_CACHE_HOME`, as the harness defines "cold":

| | Flutter | Tauri |
|--|--:|--:|
| Cold | 1124.7 | ≈457 |
| Warm | 246.1 | ≈430 |

Run 2: one cache directory for all runs (`AXIOM_BENCH_CACHE_DIR`), so "cold" is a fresh data
directory on a machine that has run the app before:

| | Flutter | Tauri |
|--|--:|--:|
| Cold | 258.3 | 454.1 |
| Warm | 252.6 | 434.9 |

Times are medians in ms.

The difference is the first launch ever, with an empty GL shader cache, on Xvfb's software
renderer: Flutter compiles its shaders once (1084.9 ms that run, then 250–258 ms). That is a
one-time cost per machine and driver. Every later start, cold or warm, is about 180 ms faster than
the Tauri shell. The results files are in the tag `tauri-shell-final`
(`apps/desktop-tauri/bench/results/JWI-Fedora-20261122-0658*.json`, `…-070046.json`); the harness
was retired with the Tauri shell.

### Packages
`scripts/bundle.sh` built `axiom_0.1.0_amd64.deb` and `axiom-0.1.0-1.x86_64.rpm`.
`scripts/verify-package.sh` passed 4/4 on each:
- `/usr/lib/axiom/{axiom, axiomd}` sit side by side, with `/usr/bin/axiom` linking to the shell;
- the desktop entry `dev.axiom.Axiom.desktop` has `StartupWMClass` and `Icon` set to the app id;
- the hicolor icons come in 5 sizes;
- unpacked and run as an installed copy with no `--daemon`, the shell starts the packaged axiomd
  and completes the handshake. `--self-test` printed `connected …/usr/lib/axiom/axiomd 0.1.0 0.4`.

The rpm build found a real defect: a Flutter plugin library carried an absolute RUNPATH into the
build tree. The plugins now link with `$ORIGIN` (`linux/CMakeLists.txt`). The macOS app now has
Axiom's icon; it had Flutter's.

### Accessibility
`a11y_test` found three gaps, all fixed:
- the editor's node acted without a name: it is now "src/main.rs, editor", with the find bar
  outside the merged node so its fields keep their names;
- the palette's scrim was unnamed: it is now "Close Command palette";
- the editor's wrapper was an unnamed focus stop.

With them fixed, every node a person can act on is named on every page in both themes, and Tab
never sticks outside the editor.

## Frame timings (ADR-0020 §5)
`apps/desktop-flutter/scripts/frames.sh`:
- a profile build, in a private virtual KWin on this machine's GPU (NVIDIA RTX 3060 Ti, driver
  620.70);
- the owner's display: 2560×1440 at 125 %;
- the window at 2040×1120 logical, which is 4080×2240 physical, because GTK 3 renders at 2× for
  a 1.25 scale, as it did for the Tauri shell;
- build and raster time of every frame, from the engine's `FrameTiming`.

Two runs on 2026-11-22, after FL-SHELL-001 and FL-FILES-001:

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Explorer fling, 3000 rows | 287 / 301 | 5.5 / 8.3 | 4.8 / 8.3 | 0 / 0 |
| Explorer wheel scroll | 82 / 78 | 6.3 / 11.6 | 3.6 / 4.7 | 0 / 0 |
| Side bar toggle (Mod+B ×8) | 24 / 27 | 12.6 / 12.6 | 4.7 / 4.7 | 0 / 0 |
| Palette open/close ×6 | 37 / 44 | 6.0 / 6.0 | 4.7 / 4.7 | 0 / 0 |
| Splitter drag | 77 / 82 | 6.1 / 6.9 | 4.8 / 5.5 | 0 / 0 |
| Theme switch ×3 | 64 / 56 | 8.2 / 9.0 | 3.7 / 6.3 | 0 / 0 |

**The editor** (FL-EDIT-001), `re_editor`, measured on 2026-11-22.

Alone (`editor_frames_test.dart`), with a 5000-line Rust file filling the window:

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Fling scroll | 210 | 3.2 / 3.4 | 3.9 / 5.7 | 0 |
| Wheel scroll | 104 | 4.4 / 4.4 | 6.0 / 6.8 | 0 |
| Typing (a character every 40 ms) | 86 | 6.6 / 7.6 | 5.9 / 6.1 | 0 |
| Cursor ↓ ×60 | 82 | 6.4 / 8.9 | 4.1 / 4.1 | 0 |

In the shell (`frames_test.dart`), with three 5000-line files open and the explorer listing
3000 more:

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Tab switching (Ctrl+Tab ×12) | 108 | 8.9 / 11.3 | 7.1 / 13.1 | 0 |
| Typing | 75 | 7.0 / 7.1 | 6.5 / 6.9 | 0 |
| Editor fling | 160 | 5.6 / 7.3 | 6.0 / 7.0 | 0 |
| Side bar toggle | 27 | 7.5 / 7.5 | 4.0 / 4.0 | 0 |

The side bar is kept mounted while hidden; before that, one toggle took 20.1 ms to build.

**The first palette open.** Before `SheetWarmup` it took one 33.7 ms raster frame, the first use
of the sheet's blurred shadow. The shell now paints one nearly transparent sheet for its first
three frames; after that the first open rasters within 2.7 ms.

**The Tauri shell, for comparison.** The same private compositor, GPU, scale and window size,
measured with requestAnimationFrame:
- editor scroll 44 fps;
- typing 44 fps;
- explorer scroll 58 fps;
- tab switches dropped 39 of 163 frames.

The editor's own figures come with FL-EDIT-001.

### Chat (FL-CHAT-001)
`AXIOM_FRAMES_TARGET=chat_frames_test.dart scripts/frames.sh`, same setup, against the real
daemon. The scripted model streams a long Markdown answer: 12 sections of prose, lists, tables
and Rust blocks, 444 chunks 12 ms apart. Then the transcript is flung.

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Streaming the answer | 188 / 191 | 7.4 / 8.3, 7.2 / 7.4 | 4.9 / 7.3, 7.5 / 10.7 | 0 / 0 |
| Transcript fling | 227 / 222 | 0.7 / 0.9, 0.6 / 1.1 | 6.5 / 7.8, 5.5 / 7.4 | 0 / 0 |

**The first version missed the budget:** 5 frames over, build max 36.3 ms. Each delta re-parsed
the whole answer and rebuilt the whole transcript. Three changes fixed it:
- the Markdown is split into top-level pieces, each parsed and built once (`markdownPieces`);
- the transcript is rebuilt only when its events change;
- each item's widget is reused, and the streaming text sits where its finished item will.

After the change, `frames_test.dart` (the shell's interactions) still has no frame over budget.

## Idle CPU with a document open (FL-PERF-001)
_2026-11-22. Fedora 44 (kernel 7.3.3), KDE Wayland. The measurements ran on a private Xvfb
(`scripts/headless.sh`, software rendering with llvmpipe, 1440×900), and the frame suites in a
private virtual KWin on the RTX 3060 Ti._

M5-QA-001's P7 found this: with a document open, the shell idled at about 27 % of a core, against
0.0 % with only a workspace, in both M4's and M5's builds (`M5.md` §Performance).

**The cause.** `FL-bench/frames_probe.py` launches a profile build and counts the engine's frames
over the VM service's timeline. It also reads per-thread CPU from /proc.
- With `main.rs` restored, there were **21 frames in 10 s**: re_editor's caret, which toggles every
  500 ms. Nothing else animates.
- The CPU cost was 28.1 %, almost all of it in llvmpipe's 8 threads.
- The caret already has its own repaint boundary, so on the Dart side a blink repaints only the
  caret. But the Linux embedder re-rasterises the whole window for every frame, about **130 ms of
  CPU per frame** on llvmpipe. A smaller dirty region cannot help; only fewer frames can.

**The change.**
- The caret blinks for 10 s after the last edit, caret move or focus change, then stays solid.
  This is GTK's `gtk-cursor-blink-timeout`.
- With motion Reduced or Off, it does not blink.
- re_editor 0.10.0 had no way to do this. `apps/desktop-flutter/third_party/re_editor` is
  upstream with a small patch (`PATCHES.md`).

**P7, interleaved against the previous build** (`--ab`, 3 pairs, the same release `axiomd`;
`FL-bench/JWI-Fedora-20261122-111638.json`):
- **Harness:** the retired bench harness (`tauri-shell-final`, with `M5-bench/run.py.diff`).
- **Baseline:** `d8ccb8e`, the end of M5.
- **Load:** 4.5–6.5.

| Idle CPU (median of 3 windows of 10 s) | This build | Baseline | Paired difference |
|--|--:|--:|--:|
| Workspace open | 0.6 / 0.0 / 0.0 % | 0.7 / 0.0 / 0.0 % | 0.0 (−0.1…0.0) |
| A document open | **2.8 / 0.0 / 0.0 %** | 27.4 / 28.1 / 28.6 % | **−28.1** (−28.6…−24.6) |

- **The criterion is met:** with a document open, the median is 0.0 %, the same as with only the
  workspace.
- **The first pair** read 2.8 %; its workspace-only figure in the same pair was 0.6 %, at load 6.3.
- **Other results:** memory with a document open +0.5 MiB (+0.5…+0.9); typing p50 −3.3 ms and
  p95 +0.5 ms, within the spread of the loaded host.

**The probe, the same day** (profile builds, 10 s windows, starting 14 s after launch):

| | Frames | CPU |
|--|--:|--:|
| This build, a document open (after the 10 s) | 1 | 1.3 % |
| Baseline, a document open | 21 | 28.7 % |
| This build, workspace only | 3 | 1.8 % |
| This build, the first 5 s (still blinking) | 11 | 30.0 % |

**Tests:** `test/editor/caret_blink_test.dart`, 4/4.
- the blink stops after the timeout and no frame is requested; the caret is hidden when focus
  leaves;
- a caret move and an edit each restart the blink for a whole timeout;
- the caret is solid, with no frame requested, under Reduced and under Off.

All four fail with upstream's behaviour (blinking forever, no setting).
`scripts/check.sh flutter` passes.

**Frames** (`scripts/frames.sh`, the owner's display, 2040×1120 at 125 %):
- **`editor_frames_test.dart`:** no frame over 16.7 ms in fling, wheel, typing or cursor ↓ ×60.
  The highest was a build of 7.6 ms.
- **`frames_test.dart`:** this build and the baseline, interleaved, 6 pairs.
  - **The load was 7.5–11.5:** two runaway `xdg-open` processes each held a core, and a Flatpak
    update was running.
  - **Both builds missed scattered frames:**
    - the baseline missed 12 frames in total, and none of its 6 runs was clean;
    - this build missed 11, and 3 of its 6 runs were clean.
  - **No interaction missed consistently** on either side. Tab switching: the baseline 4 frames,
    this build 5, all 5 in its first two runs at the highest load.
  - **Verdict:** no regression. A clean run of the whole shell suite on an idle machine is still
    owed.

## Frame rate (FL-PERF-003)
_2026-11-22._ Measured with `apps/desktop-flutter/scripts/frames.sh`:
- a profile build in a private virtual KWin, 2560×1440 at 125 %, on the NVIDIA GPU;
- Impeller on OpenGL ES;
- the window 4080×2240 physical, dpr 2.0.

The report gained two columns:
- **gap:** the time from one frame's vsync to the next one's;
- **span:** the time from vsync to raster finished.

| Interaction | Before: gap p50, span p50 | After: gap p50, span p50 |
|---|---|---|
| explorer fling | 33.3 ms, 29.3 ms | 16.7 ms, 3.7–4.3 ms |
| editor fling | 33.3 ms, 32.9 ms | 16.7 ms, 6.1–6.7 ms |
| typing | 33.3 ms, 35.6 ms | 16.7 ms, 5.9–9.0 ms |
| drag selection | 33.3 ms, 33.5 ms | 16.7 ms, 5.4–6.8 ms |
| tab switching | 33.3 ms, 38.4 ms (build max 12.7) | 16.7 ms, 3.1–6.3 ms (build max 135 → 15–18 after the isolate change) |

- **Before:** the Dart UI shared GTK's thread, so the shell drew at 30 frames a second on a 60 Hz
  display (FL-PERF-003 has the cause).
- **After:** the UI has its own thread, and re_editor's workers are shared and copy nothing on
  the UI thread.
- **Load:** 5–6, two cores of it from unrelated stuck processes. One to two single frames over
  16.7 ms land in a different interaction each run, so the suite's zero-over assertion does not
  hold on this machine; a clean run on an idle machine is owed.
- **Above 60 Hz:** needs a patched engine (FL-PERF-003 §Result).


**Presenting without GTK's repaint (FL-PERF-004).** The same suite, the same build, A/B by
`AXIOM_GTK_REDRAW=1`, load 2.5. CPU seconds over the run:

| | GTK redraw | Direct commit |
|---|---|---|
| The app's GTK thread (input) | 31.4 | 0.4 |
| The app | 66.8 | 29.7 |
| KWin (private) | 15.0 | ~2 |

| Interaction | GTK redraw: span p50 / p99 | Direct commit: span p50 / p99 |
|---|---|---|
| explorer fling | 3.4 / 7.5 ms | 2.7 / 5.0 ms |
| typing | 6.3 / 23.7 ms | 5.1 / 20.0 ms |
| drag selection | 5.5 / 11.4 ms | 4.5 / 6.1 ms |
| editor fling | 5.9 / 10.5 ms | 4.6 / 6.8 ms |

With the direct commit, the run had no frame over 16.7 ms.

## End to end
`env_test.dart`: 1/1, in a private virtual KWin and in a private Xvfb, against the real daemon
and two fixture providers served from loopback.
- **24a:** Environments on the rail reads "Environments, N need attention". The page lists
  Rust's tools (cargo, rustc, rustup) and shows Demo as missing, with Plan install….
- **24b:** the plan is titled "Install Demo 1.0.0". It has 3 named steps: the download with its
  URL and SHA-256, then placing the program, then "Check that demotool runs". Install runs it,
  and Demo becomes Ready at 1.0.0, managed.
- **24c:** a trickling install shows "Installing Slow Demo" in the transport. Cancel ends it with
  "The install was cancelled.", leaving no toolchain directory and no partial download.
- **24d:** Remove Demo 1.0.0 asks first, naming `toolchains/demo/1.0.0`, deletes it, and Demo is
  missing again.
- **24e:** Files brings back the editor with its tab.

`env_rust_test.dart` (the M2 exit scenario, env.mjs) installs a real Rust toolchain, so it needs
the internet. It runs with `AXIOM_E2E_NETWORK=1`, set in CI on Ubuntu. It was not run on this
machine.

`git_test.dart`: 1/1, in a private virtual KWin and in a private Xvfb, against a temporary
repository.
- The status bar reads "Branch main, 0 changes".
- An unsaved edit marks line 2 modified in the gutter.
- Once saved, the explorer row reads "lib.rs, file, modified" and the tab is marked too.
- A new file reads "notes.txt, file, untracked".
- Source Control lists both. Its row opens the diff, "Changes to lib.rs: 1 added, 1 removed",
  with the replaced line beside its replacement; Esc closes it.
- A commit made outside the app clears the marks, the list ("No changes") and the gutter.

`search_test.dart`: 1/1, in KWin and in Xvfb.
- Mod+Shift+F focuses the query. "needle" finds 3 matches in 2 files; `target/` (ignored) is
  not searched.
- ↓ ↓ Enter opens the second match at its line and column.
- Tab reaches Match case and Space turns it on; the upper-case line drops out.
- A regular expression finds its 2 matches.
- Go to File finds `src/deep/nested/needles.rs` for "dpnstd" before the explorer listed it, and
  opens it.

The shell's frame benchmark was run again after the Git marks went in: no frame over 16.7 ms
in any interaction. The highest build was 14.2 ms, opening the palette.

`console_test.dart`: 3/3, in a private virtual KWin and in a private Xvfb, against the real
daemon.
- **Output:**
  - Mod+Shift+R focuses the prompt; a command's stdout and stderr stay apart, and the exit
    status shows ("Failed with exit code 2");
  - Shift+F5 stops a running command within seconds;
  - a missing program is a structured error;
  - the transport shows the label and an advancing time, then success; a failure shows as
    such; Rerun names the last command.
- **Provider tasks:** "Build: cargo build" from the palette builds a broken crate and fails;
  the transport offers "Rerun: Build: cargo build"; the E0308 row reads "Error: mismatched types
  (E0308) at src/main.rs:2:18, from rustc" and opens the file at Ln 2, Col 18.
- **Terminal:**
  - Mod+Shift+` opens a shell in the folder, and `pwd` prints it;
  - `stty size` matches the screen, and hiding the side bar gives more columns;
  - a typed key's echo is on screen within p95 30 / 16 / 33 ms across three runs (the
    criterion is 50 ms);
  - a second terminal is a tab, and Mod+` focuses it;
  - Mod+K from the terminal opens the palette;
  - `exit` marks the tab "(ended)";
  - after quitting, no shell process is left.

`chat_test.dart`: 3/3, in a private virtual KWin and in a private Xvfb, against the real daemon.
- **No folder, the scripted model:**
  - Chat is on the rail under Files and opens with a model chosen;
  - Enter sends; streamed Markdown arrives with a table and a code block coloured per token;
  - the conversation is titled from the first message;
  - ↑ edits the message and replaces the branch; Regenerate replaces the answer;
  - an attached image reaches the model and shows as a thumbnail;
  - F2 renames; Delete asks first, then removes the conversation from the daemon too.
- **A folder, the dock:**
  - Mod+L opens the dock beside the editor, in the folder's scope;
  - an edit asks with a focused card showing the diff, and the rail reads "Chat, 1 waiting for
    you"; Enter allows it and the file changes; Esc denies the next one and the file stays;
  - Restore puts the file back;
  - a file the person changed after the agent is a conflict: it is listed, nothing is
    restored, the person's text stays;
  - Mod+L closes the dock.
- **No folder, no model:**
  - the first-run setup shows;
  - an OpenAI-compatible server (a fake one on loopback) is added with a key. The key never
    shows in the UI and reaches the server as a bearer, and the answer streams;
  - Settings → Models shows "Key stored · 1 model"; removing the provider brings the setup
    back;
  - Claude Code is installed from the setup by a fake npm into `<data>/agents`;
  - in Settings → Models, cancelling a hung install stops it within seconds;
  - the fake ACP agent, picked in the model picker, answers in `<data>/scratch/<session>`. The
    card names it as an external agent, and its write happens only after Allow once.

`apps/desktop-flutter/scripts/e2e.sh shell_test.dart`: 1/1, in a private virtual KWin and in a
private Xvfb. It covers connecting, opening a folder, the ignored entries, the explorer by
keyboard, the hint line, the palette, Mod+B, the splitter's keys, Settings, and a restart that
brings back the folder, the expanded folders, the side bar width and the theme.

`editor_test.dart`: 1/1 against the real daemon. Covers:
- opening from the explorer (the editor, Markdown in the status bar);
- Go to File for a file the explorer never listed;
- Mod+S, with the bytes on disk matching;
- go to line, with the cursor in the status bar;
- a clean reload on an external change;
- dirty plus changed on disk, then the conflict dialog, then Overwrite;
- closing a dirty tab, then Don't Save, with nothing written;
- the split: Mod+\, the same text in both views, Mod+2, typing on the right, Mod+S there,
  Mod+1;
- a restart that brings back the tabs, the active one and the split;
- closing the right group's last tab, which ends the split.
