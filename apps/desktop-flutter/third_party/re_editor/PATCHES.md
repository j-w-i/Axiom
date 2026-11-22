# re_editor, patched

- **Upstream:** [re_editor](https://pub.dev/packages/re_editor) **0.10.0** from pub.dev
  (sha256 `66671c4774a6b4c5254c9a53ab35a083e7e7da9ae371c519bcf491c70a2a4e56`), MIT (`LICENSE`,
  unchanged). Repository: https://github.com/reqable/re-editor.
- **What is here:** upstream's `lib/`, `pubspec.yaml`, `LICENSE`, `README.md` and `CHANGELOG.md`.
  The upstream `example/`, `test/` and `arts/` are left out. The shell uses this copy through
  `dependency_overrides` in `apps/desktop-flutter/pubspec.yaml`. The first commit (FL-PERF-001)
  is the unmodified copy, so `git diff <that commit> -- .` shows the whole patch.
- **Why (FL-PERF-001):** in upstream, a focused editor's caret blinks every 500 ms for as long as
  the editor has focus, and the blink cannot be configured. The caret already has its own repaint
  boundary. Even so, each blink is a frame, and the Linux embedder re-rasterises the whole window
  for every frame. On a software renderer, that put an idle editor at about 27 % of a core.

## The patch
`lib/src/code_editor.dart` and `lib/src/_code_editable.dart` change for the caret (FL-PERF-001);
`lib/src/_code_field.dart` for a repaint after lines were removed (M5.5-EDIT-001, below).

1. `CodeEditor` gets two new optional parameters, passed through to `_CodeEditable`:
   - `bool? cursorBlink`: false draws the caret solid. The default is true.
   - `Duration? cursorBlinkTimeout`: how long the caret blinks after the last edit, selection
     change or focus change. After that it stays solid and schedules no frames until the next
     change, which restarts the blink. The default is null: blink forever, as upstream.
2. `_CodeCursorBlinkController` honours both settings:
   - `startBlink` starts the periodic timer only when the caret blinks;
   - the tick stops the timer and shows the caret once `timeout` has passed;
   - `stopBlink` always hides the caret. Upstream returned early when no timer ran, and that would
     leave a solid caret showing after focus left.
3. `_CodeEditableState.didUpdateWidget` applies changed settings, for example when motion is
   switched in Settings.

With both parameters left at their defaults, the behaviour is upstream's.

## Dropping the patch
When upstream has an equivalent API:
1. remove `dependency_overrides` from the app's `pubspec.yaml` and this directory;
2. map the two parameters in `lib/editor/code_view.dart` to upstream's names;
3. keep `test/editor/caret_blink_test.dart` passing.

## A repaint after lines were removed (M5.5-EDIT-001)
`_CodeFieldRender._buildDisplayRenderParagraphs` built every line of the render's own copy of
the lines, which is updated on the next rebuild; the highlighter reads the controller's. A repaint
forced in between (the shell's diagnostics arriving right after an edit that removed lines) asked
the highlighter for a line that no longer existed. It now builds only the lines both have; the
render's own update follows in the same frame.

