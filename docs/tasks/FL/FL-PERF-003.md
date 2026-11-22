---
id: FL-PERF-003
title: The shell draws at the display's rate (60 Hz, not 30) - the Dart UI on its own thread; editors share their worker isolates and copy nothing on the UI thread
status: implemented
status_note: 2026-11-22; Linux (Fedora 44, private virtual KWin on the NVIDIA GPU at 125 %): frame interval 33.3 → 16.7 ms and vsync-to-raster 29–38 → 4–7 ms in every interaction; the worst tab-switch frame 59 → 15–18 ms. Single frames over 16.7 ms move between interactions from run to run under load 5–6 (a clean run on an idle machine owed). Above 60 Hz needs the Flutter engine changed (see Result)
depends_on: [FL-PERF-002]
order: 611
scope: apps/desktop-flutter/linux/runner/my_application.cc, apps/desktop-flutter/third_party/re_editor/, apps/desktop-flutter/integration_test/frame_report.dart, apps/desktop-flutter/integration_test/frames_test.dart, docs/roadmap/verification/FL.md
---

# FL-PERF-003: The shell draws at the display's rate

## Objective
The owner reported that the app felt like it ran at a low frame rate, unlike VS Code
(2026-11-22). The shell must draw every frame the display shows.

## Context
The frame suite measured build and raster time per frame (all within 16.7 ms). It did not
measure how often frames were drawn. With the interval between frames added to the report:
- **Every interaction drew at 30 frames a second:** a 33.3 ms interval, with frames waiting
  29–38 ms from vsync to raster.
- **The cause:** by default, the Linux embedder runs the Dart UI on GTK's thread. After each
  frame it asks GTK to redraw the window. GTK 3 does that in software, at twice the logical size
  under fractional scaling (4080×2240 at 125 %), on that same thread. The next frame missed the
  following tick of the engine's 60 Hz timer.
- **A stall on tab switches:** each switch made a new editor with three new isolates. Each one
  was sent the whole document, and the highlighter its grammar, copied on the UI thread: up to
  54 ms per send.

## Acceptance criteria
- The runner sets `FL_UI_THREAD_POLICY_RUN_ON_SEPARATE_THREAD`.
- `frame_report.dart` reports the interval between frames and the time from vsync to raster
  finished.
- re_editor's workers are shared, and the UI thread copies no document or grammar
  (`third_party/re_editor/PATCHES.md`).
- After quick tab switching, the visible tab is highlighted (`frames_test.dart`).

## A crash on quit, found afterwards
With the UI on its own thread, Flutter's quit returned from `main()` while that thread still ran.
`exit()` then ran the static destructors under it, and laying out text hit Skia's
`check(fUnicode)` (SIGILL): every quit in a private KWin that closed the window crashed. The
runner now ends the process with `_exit()` once the app has finished its own shutdown
(`linux/runner/main.cc`). Destroying the window first was tried, and crashed in the GL driver.

## Result
- **The frame interval:** 16.7 ms in every interaction (`verification/FL.md` §Frame rate).
- **Above 60 Hz:** not reachable with the stock engine. On Linux the embedder registers no vsync
  callback, so the engine falls back to a fixed 1/60 s timer
  (`engine/src/flutter/shell/common/vsync_waiter_fallback.cc`). Drawing at a 120 or 144 Hz
  display's rate needs the Linux embedder to drive vsync from the compositor's frame callbacks,
  which means a patched Flutter engine. The owner chose to keep 60 Hz for now (2026-11-22). On macOS the engine
  follows the display (ProMotion included).
