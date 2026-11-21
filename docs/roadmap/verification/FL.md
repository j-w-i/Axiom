# FL verification — the Flutter shell (in progress)

_Started 2026-11-22. Each task adds its evidence; FL-QA-001 completes the record._

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

## End to end
`apps/desktop-flutter/scripts/e2e.sh shell_test.dart`: 1/1, in a private virtual KWin and in a
private Xvfb. It covers connecting, opening a folder, the ignored entries, the explorer by
keyboard, the hint line, the palette, Mod+B, the splitter's keys, Settings, and a restart that
brings back the folder, the expanded folders, the side bar width and the theme.
