---
id: FL-SHELL-001
title: The shell: title row, rail (Files, Chat, Search, Source Control, Environments), side bar, editor area, console, status bar and hint line, palette, settings, layout persistence
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-CORE-001, FL-DS-001]
order: 523
scope: apps/desktop-flutter/lib/shell/, apps/desktop-flutter/lib/state/, apps/desktop-flutter/integration_test/shell_test.dart
---

# FL-SHELL-001: The shell: title row, rail (Files, Chat, Search, Source Control, Environments), side bar, editor area, console, status bar and hint line, palette, settings, layout persistence

## Objective
The frame every page lives in, with the Tauri shell's behaviour (smoke.mjs and geometry.mjs are
the spec): menus, the rail in ADR-0020's order with badges and the underline, a resizable side bar
and console (splitters remembered across restarts), the status bar with connection state, branch
and the hint line, the command palette (commands, Go to File, MRU), settings (Appearance, Editor,
Models, About), keyboard shortcuts (Mod+B, Mod+J, Mod+K, Mod+P, Mod+,), the window title, quit
with unsaved changes, the answer-first line per page. Settings are the daemon's (`settings/*`).

## Acceptance criteria
1. integration_test against a real daemon in a private compositor: every rail page reachable by
   keyboard; layout restored after restart; palette filters and runs.
2. Frame budget (ADR-0020 §5) for rail switching, splitters and the palette.

## Tests
`scripts/flutter-e2e.sh shell_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
