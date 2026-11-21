---
id: FL-ENV-001
title: Environments page: tools, install plans, installs, remove
status: planned
status_note:
depends_on: [FL-SHELL-001]
order: 529
scope: apps/desktop-flutter/lib/env/, apps/desktop-flutter/integration_test/env_test.dart
---

# FL-ENV-001: Environments page: tools, install plans, installs, remove

## Objective
The M2 Environments page in Flutter: the tool list with states, Plan install with every step,
install with progress and cancel, remove with its directory named, the rail badge.

## Acceptance criteria
1. smoke 24a–24e and env.mjs's checks pass in Flutter.

## Tests
`scripts/flutter-e2e.sh env_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
