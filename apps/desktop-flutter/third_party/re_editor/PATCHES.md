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


## Shared workers, and no copies on the UI thread (FL-PERF-003)
Upstream gives every editor three isolates of its own (highlighting, folding, find). It sends
the whole document to them as `CodeLines` objects on every edit, and the highlighter also sends
its `Highlight`, with every grammar it uses. Each send deep-copies those objects on the UI
thread. On a 5000-line file, a tab switch spawned the isolates again and stalled the UI thread
for up to 54 ms in `CopyMutableObjectGraph`; each keystroke paid the copy too.

- `lib/src/_isolate.dart`: an `_IsolateLane` per kind of task, shared by every editor, with two
  worker isolates. A tasker keeps only its newest request. A request is handed to a worker only
  when one is free, and the newest tasker goes first. Closing a tasker drops its waiting request.
  Results for a closed tasker are ignored, as upstream.
- `lib/src/_code_highlight.dart`: the payload is the text as one `String` (strings are shared
  between isolates, not copied), a key for the `Highlight` (the identity of its modes and
  plugins), and the size check, now done before sending. The `Highlight` is sent only the first
  time; a worker keeps the ones it was given (at most 64), and one that is missing is asked for
  again. Also `reEditorHighlightsDelivered`, a counter for tests.
- `lib/src/code_chunk.dart`: when no line is folded, the payload is the lines' text, and the
  worker rebuilds the `CodeLines`. With folded lines it is the `CodeLines`, as upstream.

`integration_test/frames_test.dart` measures tab switching and checks that the tab left showing
is highlighted afterwards.

## Where a text position is drawn; no endless rebuild (FL-EDIT-003)
- `lib/src/_code_field.dart`: `codeEditorPositionRect(root, position, ancestor:)` returns the
  rectangle of the position's line from the render's own layout, in an ancestor's coordinates.
  The shell places the completion list and the signature card with it. Arithmetic from the font
  size had drifted about a line off by line 200, so the list covered the line being typed.
- `_buildDisplayRenderParagraphs` starts at most at the last line that both the render and the
  highlighter have. Starting past them built nothing, and `_updateDisplayRenderParagraphs`
  retried until the stack overflowed: a long document shrinking while scrolled far down. The
  M5.5-EDIT-001 bound had introduced this.

## Whole highlight scopes (2026-11-22)
`lib/src/_code_highlight.dart`: upstream kept only the first part of a scope
(`title.function` became `title`), so a theme could not tell a function's name from a class's.
- The renderer keeps the whole scope.
- `_findStyle` tries the most specific part first (`title.function.invoke`, then
  `title.function`, then `title`).
- It still tries nested names (`function-title`) whole and then without their outer parts.

`test/editor/highlight_colours_test.dart` resolves scopes the same way and checks every fixture
gets several colours. `test/editor/highlight_pipeline_test.dart` checks that results reach each
editor through the shared workers.
