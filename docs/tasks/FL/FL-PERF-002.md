---
id: FL-PERF-002
title: Selecting text does no whole-document work: dirty mark, gutter and LSP sync on text changes only; the caret text has its own notifier
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-PERF-001]
order: 610
scope: apps/desktop-flutter/lib/editor/editor_area.dart, apps/desktop-flutter/lib/state/documents.dart, apps/desktop-flutter/lib/shell/shell.dart, apps/desktop-flutter/integration_test/frames_test.dart
---

# FL-PERF-002: Selecting text does no whole-document work

## Objective
The owner reported that selecting text felt laggy (2026-11-22). A caret or selection move must
cost only the editor's own repaint and the status bar's caret text.

## Context
Each selection change notified the controller's listeners, and the shell treated it as an edit:
- the dirty check compared the whole document;
- the git gutter and the LSP sync were rescheduled;
- the selected count joined the selected text;
- the documents store notified, which rebuilt the editor area and the status bar.

## Acceptance criteria
- The dirty mark, the git gutter and the LSP sync run only when the text changed (the code lines
  are no longer identical).
- The selected count comes from line lengths, without building the text.
- The caret text is a `ValueNotifier` on the store, notified after the frame. The status bar
  listens to it, and the store's listeners are not notified for caret moves.
- `frames_test.dart` measures drag selection and Shift+Down on the 10,000-line file, within
  budget.

## Result
Implemented in `fa551b6`.
