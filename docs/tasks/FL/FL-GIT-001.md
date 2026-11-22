---
id: FL-GIT-001
title: Source Control, gutter marks, the diff view; the Search page
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-EDIT-001]
order: 528
scope: apps/desktop-flutter/lib/scm/, apps/desktop-flutter/lib/search/, apps/desktop-flutter/integration_test/{git,search}_test.dart
---

# FL-GIT-001: Source Control, gutter marks, the diff view; the Search page

## Objective
`git/*` and `workspace/search` in the UI: branch in the status bar, explorer and tab marks, gutter
markers, the Source Control page, a side-by-side diff view (Esc closes), Open in diff from Chat;
the Search page (text, case, regex, grouped results, Enter opens at the match).

## Acceptance criteria
1. git.mjs and search.mjs's checks pass in Flutter.

## Tests
`scripts/flutter-e2e.sh git_test.dart search_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
