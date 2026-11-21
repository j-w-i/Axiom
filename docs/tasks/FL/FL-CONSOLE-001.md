---
id: FL-CONSOLE-001
title: Console: Output and task runs, Problems, Terminal (xterm.dart)
status: planned
status_note:
depends_on: [FL-SHELL-001]
order: 527
scope: apps/desktop-flutter/lib/console/, apps/desktop-flutter/integration_test/console_test.dart
---

# FL-CONSOLE-001: Console: Output and task runs, Problems, Terminal (xterm.dart)

## Objective
The console region: the run prompt and task output (stdout/stderr distinct, exit status, Stop,
Rerun, the transport), provider tasks in the palette, Problems from builds, and the Terminal on
`terminal/*` with the xterm package (tabs, resize, themes, Mod+Shift+`, the shell exits with the
app).

## Acceptance criteria
1. smoke 7a–7c, 14, 25a/b and terminal.mjs's checks pass in Flutter.

## Tests
`scripts/flutter-e2e.sh console_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
