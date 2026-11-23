---
id: FL-PERF-001
title: An idle editor draws no frames: the caret's blink stops after 10 s (re_editor patched); solid under Reduced and Off motion
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-QA-001]
order: 609
scope: apps/desktop-flutter/third_party/re_editor/, apps/desktop-flutter/lib/editor/code_view.dart, apps/desktop-flutter/lib/ui/tokens.dart, apps/desktop-flutter/pubspec.yaml, apps/desktop-flutter/analysis_options.yaml, apps/desktop-flutter/test/editor/, docs/roadmap/verification/FL.md, docs/roadmap/verification/FL-bench/, docs/design/workspace-spec.md, docs/design/motion-system.md, docs/architecture/modules.md
---

# FL-PERF-001: An idle editor draws no frames

## Objective
With a document open and nothing happening, the Flutter shell idles at about the same CPU as with
only a folder open. The caret blinks for 10 s after the last edit, caret move or focus change and
then stays solid. With motion Reduced or Off, it never blinks.

## Context
M5-QA-001's P7 (`docs/roadmap/verification/M5.md` §Performance) found the problem. On Xvfb's
software renderer, the shell idled at about 27 % of a core with a document open and at 0.0 % with
only a workspace, in both M4's and M5's Flutter builds. The Tauri shell idled at 1.2 %.

Measured cause (`verification/FL.md` §Idle CPU with a document open):
- **The caret:** re_editor's blink is a 500 ms timer, and every toggle is a frame (21 frames in
  10 s).
- **The cost per frame:** the caret already has its own repaint boundary. Even so, the Linux
  embedder re-rasterises the whole window for each frame, about 130 ms of CPU on llvmpipe.
- **No API:** re_editor 0.10.0, the latest release, cannot configure the blink.

The owner's session approved vendoring re_editor 0.10.0 with a small patch. It is the same
package, version and licence, so governance rule 1 does not apply.

## Required interfaces
- re_editor's public `CodeEditor` API. The patch only adds parameters, and their defaults keep
  upstream's behaviour.
- `AxiomMotion` (`lib/ui/tokens.dart`): the motion setting, ADR-0013.

## Implementation requirements
1. `third_party/re_editor`: upstream 0.10.0 with its `LICENSE`, used through
   `dependency_overrides`. `CodeEditor` gets `cursorBlink` and `cursorBlinkTimeout`.
   `PATCHES.md` gives the upstream version, the change and how to drop it.
2. `AxiomMotion.caretBlinks` is true only at Standard. `caretBlinkTimeout` is 10 s.
3. `CodeView` passes both to `CodeEditor`.
4. `scripts/check_dart_licences.py` still covers re_editor (now a path package).

## Constraints
Widgets layer only. No literal colours outside `tokens.dart`. The vendored copy stays as close to
upstream as possible. No upstream issue or PR, which is the owner's call.

## Acceptance criteria
1. Idle CPU with a document open is within about 2 points of the workspace-only figure, measured
   with `bench --ab` interleaved against the previous build.
2. `scripts/check.sh flutter` passes.
3. `apps/desktop-flutter/scripts/frames.sh` shows no frame over 16.7 ms.
4. The result is in `docs/roadmap/verification/FL.md`.

## Tests
`test/editor/caret_blink_test.dart`:
- **Standard, the blink stops:** the caret blinks, stays solid after the timeout, and requests no
  frames; it is hidden after focus leaves.
- **Standard, the blink restarts:** a caret move and an edit each restart it for a full timeout.
- **Reduced and Off:** the caret is solid and requests no frames, including after a caret move.

## Failure conditions
Idle CPU with a document open more than about 2 points above the workspace-only figure; any frame
over 16.7 ms; a test weakened to pass.

## Documentation updates
`docs/roadmap/verification/FL.md`, `docs/design/workspace-spec.md` (§2.3, the caret),
`docs/design/motion-system.md` (§5, item 8), `docs/architecture/modules.md` (the vendored copy),
`docs/roadmap/current.md`, `docs/roadmap/verification/M5.md` (the item it owed).

## Handoff
**Departure from the design spec:** the caret no longer blinks indefinitely at Standard. It stops
after 10 s without input, as GTK's `gtk-cursor-blink-timeout` does. Recorded in both design
documents above.
