---
id: FL-QA-001
title: Parity, performance, accessibility and packaging; the Flutter shell becomes the default and the Tauri shell is retired
status: implemented
status_note: 2026-11-22; Linux verified (every suite, frames, packages, startup); macOS and the M2 real install in CI; Windows owed
depends_on: [FL-EDIT-002, FL-CONSOLE-001, FL-GIT-001, FL-ENV-001, FL-CHAT-001]
order: 531
scope: apps/desktop-flutter/, scripts/run.sh, scripts/bundle.sh, .github/workflows/, docs/roadmap/verification/FL.md, apps/desktop-tauri/ and editor/ (removal), docs/
---

# FL-QA-001: Parity, performance, accessibility and packaging; the Flutter shell becomes the default and the Tauri shell is retired

## Objective
The gate of ADR-0020 §7: the M1–M4 exit scenarios pass in the Flutter shell on Linux and macOS
(CI); the frame budget holds for every listed interaction at 125 % scale; a11y (names, roles,
focus order, contrast) in both themes; packages (deb, rpm with the desktop entry and icon named
after the app id, a macOS app bundle) verified; startup no worse than the Tauri shell's P2. Then
run.sh launches Flutter, the Tauri shell is tagged `tauri-shell-final` and removed with `editor/`,
and the docs follow.

## Acceptance criteria
1. verification/FL.md records every scenario, the frame timings and what is owed (Windows).
2. `scripts/check.sh` passes without the Tauri shell.

## Tests
Every Flutter e2e suite; the frame benchmark; package verification.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
