---
id: FL-DS-001
title: Design system: Precision Instrument × Temporal Tactility tokens, controls, motion, themes
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-ARCH-001]
order: 522
scope: apps/desktop-flutter/lib/ui/, apps/desktop-flutter/assets/fonts/, apps/desktop-flutter/test/ui/, docs/design/flutter/
---

# FL-DS-001: Design system: Precision Instrument × Temporal Tactility tokens, controls, motion, themes

## Objective
ADR-0020 §4 as code: tokens for both themes (tone steps, lit edge, contact shade, hairline, ink
levels, error, accent), IBM Plex (the OFL files ADR-0013 allows), tabular figures; Axiom's own
controls on the widgets layer with semantics and focus rings: button (quiet, primary ink circle),
icon button with tooltip, text field, the composer's bezel field, menu, context menu, dialog and
sheet, tabs with the underline, list rows with grooves, splitter (keyboard ±16, Home), toggle,
checkbox, badge, status dot in a cut-out ring, toast, spinner (smooth, stops when hidden); springs
with 1.5–4 % overshoot; Standard/Reduced/Off motion; a gallery page (debug builds) showing every
control in both themes.

## Acceptance criteria
1. Widget tests: every control has a name and role, keyboard operation, visible focus; contrast of
   every text token on its surfaces ≥ 4.5:1 in both themes (computed in a test).
2. No Material or Cupertino import anywhere in `lib/` (a test greps).
3. Golden images of the gallery in both themes.

## Tests
`flutter test test/ui/`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
