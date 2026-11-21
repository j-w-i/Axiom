---
id: FL-EDIT-001
title: The editor: re_editor, documents (open, save, conflicts, dirty, recovery), tabs, split, highlighting, find and replace
status: planned
status_note:
depends_on: [FL-FILES-001]
order: 525
scope: apps/desktop-flutter/lib/editor/, apps/desktop-flutter/integration_test/editor_test.dart
---

# FL-EDIT-001: The editor: re_editor, documents (open, save, conflicts, dirty, recovery), tabs, split, highlighting, find and replace

## Objective
A documents store with the Tauri store's rules (state/documents.ts): open, save with
`expectedVersion`, the conflict dialog, dirty marks, close prompts, tab order and drag, restore
after restart (`editor.openPaths`), the split (Mod+\\, Mod+1/2), external changes. The editor
widget on re_editor with re_highlight colouring for the languages the providers cover,
find/replace, go to line, cursor position in the status bar, font settings, indentation, undo
across saves.

## Acceptance criteria
1. smoke 5, 6a, 6b, 8 (restore) and split.mjs's checks pass in Flutter.
2. Typing and scrolling a 5 000-line file stay within 16.7 ms per frame at 125 % scale.

## Tests
`scripts/flutter-e2e.sh editor_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
