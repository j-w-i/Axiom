---
id: FL-DS-002
title: Design system: the Tauri shell's design, as the owner directed
status: implemented
status_note: 2026-11-22; Linux verified (screenshots, flutter gate); end-to-end and macOS per the handoff
depends_on: [FL-DS-001]
order: 523
scope: apps/desktop-flutter/lib/ (styling only), apps/desktop-flutter/test/ui/, docs/design/flutter/
---

# FL-DS-002: Design system: the Tauri shell's design, as the owner directed

## Objective
The owner judged FL-DS-001's look unacceptable ("Precision Instrument × Temporal Tactility":
cool near-black surfaces, ink accents, 10–14 px radii, springs with overshoot) and asked for the
shell to look the same as the Tauri shell, which they liked. A first pass that imitated Unitable
(the owner's own app) was shown and set aside on the owner's word; it is kept on the local branch
`wip/unitable-design`.

The reference is the Tauri shell as shipped (`apps/desktop-tauri/src/styles/` at the tag
`tauri-shell-final`; ADR-0013's M1.6 design) and its screenshots
(`docs/roadmap/verification/M1.6/macos/`):

- Its tokens as they are: the graphite surfaces (canvas, chrome, panel, editor, well, raised,
  overlay), warm inks, brass accent with its high and low stops, sage, ember, brick, the lines
  (line, line-mid, line-input), hover, selected, press, the syntax, glyph and ANSI colours.
- Its materials: raised keys (a gradient, a 1 px border, a lit top edge, a contact shade),
  recessed wells (an inner shadow along the top, a lit edge under), overlays (a two-layer
  shadow); radii 3, 4 and 6 px; IBM Plex Sans and Mono at 15/13/12/11 px.
- Its controls: the raised button, the brass primary, quiet and danger; icon buttons that sink
  into a well when engaged; the recessed field with its brass focus border and soft ring; the
  switch; keycaps as raised keys; the count badge.
- Its layout: the title row (brass mark and word, menus, the folder's name and path, the
  recessed command field, toggles), the 44 px rail with its brass edge marker, 30 px panel heads,
  the tree (folder tone, file-type chips, full-width selection with a marker), editor tabs (chips,
  the active tab in the editor's tone with the brass lit edge), the console's view tabs with a
  brass underline, status-bar segments, the welcome (mark, title, line, primary with its keys,
  recent rows) and the empty editor's list of ways in.
- Motion as Tauri moved: ease-out, no overshoot.

## Deviations from the Tauri values
- `ink3` (Tauri's `--text-faint`) is `#A19D94` in dark and `#5D5A54` in light, not `#75726B` and
  `#83807A`: the faint tier is under 4.5:1 on several surfaces, and `test/ui/tokens_test.dart`
  checks every text colour on every surface. The same holds for the line numbers (`#6C6962`,
  `#8A867E`; ≥ 3:1) and the comment colour (`#8A867E`, `#6B675F`), as FL-DS-001 had them.
- The tab's brass edge does not glide between tabs; each active tab carries its own.

## Acceptance criteria
1. The design-system tests hold unchanged: names and roles, keyboard operation, focus, contrast
   of every text token on its surfaces in both themes, no literal colour outside `tokens.dart`,
   no Material or Cupertino import.
2. The gallery goldens are regenerated for the new design (the expectation changes because the
   design changes; no test is removed or loosened).
3. Screenshots of the real app in both themes, reviewed by the owner.
4. `scripts/check.sh flutter` and the end-to-end suites pass.

## Tests
`scripts/check.sh flutter`; `scripts/check.sh e2e`.

## Documentation updates
`docs/design/flutter/README.md`; the handoff.
