---
id: FL-EDIT-003
title: The completion list as Xcode has it - under the line being typed, the names on the typed word, kind badges, the declaration and documentation below
status: implemented
status_note: 2026-11-22; Linux (lsp_test.dart step 2b' 400 lines down: the list below the typed line, its names within 1.5 px of the typed word; check.sh flutter passes; a screenshot checked by eye)
depends_on: [M5.5-EDIT-001]
order: 613
scope: apps/desktop-flutter/lib/editor/lsp/lsp_view.dart, apps/desktop-flutter/lib/editor/editor_area.dart, apps/desktop-flutter/third_party/re_editor/, apps/desktop-flutter/integration_test/lsp_test.dart
---

# FL-EDIT-003: The completion list as Xcode has it

## Objective
The owner found the completion list bad (2026-11-22). It covered the text being typed, and the
documentation floated beside it. They asked for Xcode's design.

## Context
- **Placement:** the list's position came from arithmetic: a line height of 1.5 × the font
  size, from the top. By line 200 or so that had drifted about a line from the editor's own
  layout, so the list opened over the line being typed.
- **Xcode's list:**
  - it opens under the line, with the names in the editor's font, lined up on the word being
    completed;
  - each row is a letter badge for the kind (M, F, V, P, C, S, E, Pr, K…) and the name with
    its parameters;
  - the selected item's declaration and summary are in a panel under the list, not in the rows
    (Xcode 12 and later; Apple's documentation describes the panel below the list).

## Acceptance criteria
- **Placement from the editor's layout:** the list and the signature card are placed from the
  editor's own layout (`codeEditorPositionRect`, a re_editor patch), laid out after the editor in
  the same frame. The list goes below the line, or above it when there is no room below.
- **Rows:** the editor's font and size, lined up on the typed word (`completionTextInset`); a
  kind badge tinted by category (types, callables, values, keywords and snippets); the matched
  characters bold; the label's details in secondary ink.
- **The panel below the list:** the declaration (the item's detail) in the editor's font, then
  the documentation. The panel keeps the semantics label "Documentation".
- **Test:** `lsp_test.dart` checks the placement 400 lines down.

## Result
Implemented. The test also found a stack overflow in re_editor from the M5.5-EDIT-001 patch,
when a long document shrank while scrolled far down. It is fixed (`third_party/re_editor/PATCHES.md`).
