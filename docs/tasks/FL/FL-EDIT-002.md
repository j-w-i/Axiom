---
id: FL-EDIT-002
title: Language servers in the editor: diagnostics, completion, hover, signature, definition, rename, formatting, code actions; Problems
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-EDIT-001]
order: 526
scope: apps/desktop-flutter/lib/editor/lsp/, apps/desktop-flutter/lib/problems/, apps/desktop-flutter/integration_test/lsp_test.dart
---

# FL-EDIT-002: Language servers in the editor: diagnostics, completion, hover, signature, definition, rename, formatting, code actions; Problems

## Objective
The LSP client of M4-LSP-002 in Dart over `lsp/*`: document sync (full and incremental),
diagnostics as squiggles and Problems rows with their source, completion popup, hover card,
signature help, F12 across files, F2 rename across files (undoable, unsaved), Format Document,
code actions, the status-bar server segment; Problems panel with click/Enter to source.

## Acceptance criteria
1. lsp.mjs's checks pass against fake-lsp; with `AXIOM_REQUIRE_LSP=1`, rust-analyzer and
   typescript-language-server pass (CI Ubuntu).
2. Completion opens within one frame of the server's answer.

## Tests
`scripts/flutter-e2e.sh lsp_test.dart`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
