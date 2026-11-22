---
id: FL-FILES-001
title: Workspace and explorer: open, recent, lazy tree, ignored entries, file operations, Go to File
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-SHELL-001]
order: 524
scope: apps/desktop-flutter/lib/files/, apps/desktop-flutter/integration_test/files_test.dart
---

# FL-FILES-001: Workspace and explorer: open, recent, lazy tree, ignored entries, file operations, Go to File

## Objective
`workspace/*` and `fs/*` through the explorer: open folder (native dialog), recent list, lazy
listing with ignored entries dimmed, expanded folders restored, create/rename/delete with
confirmation, external changes, empty and error states, Git status marks; Go to File over
`workspace/search` files mode with the fuzzy scorer.

## Acceptance criteria
1. The smoke checks for the explorer (1–4, 6c, 16) pass in Flutter.
2. A 10 000-entry folder scrolls within the frame budget.

## Tests
`scripts/flutter-e2e.sh files_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
