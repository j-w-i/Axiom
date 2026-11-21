---
id: FL-CHAT-001
title: Chat, first-party and under Files: conversations, streaming Markdown, models and agent harnesses (with or without a folder), approvals, the dock, checkpoints
status: planned
status_note:
depends_on: [FL-SHELL-001, FL-EDIT-001]
order: 530
scope: apps/desktop-flutter/lib/chat/, crates/agent/ and crates/axiomd/ (harness scratch directory, harness install), crates/protocol/ (additive: agent/installHarness), apps/desktop-flutter/integration_test/chat_test.dart
---

# FL-CHAT-001: Chat, first-party and under Files: conversations, streaming Markdown, models and agent harnesses (with or without a folder), approvals, the dock, checkpoints

## Objective
Chat of M3-UI-001…003 in Flutter, plus ADR-0020 §6: one conversation list and composer for Axiom's
agent and the harnesses (Claude Code, Codex, Gemini CLI); a harness conversation without a folder
runs in a scratch directory under the data directory; a missing harness can be installed by the
daemon from Chat or Settings → Models (steps shown, cancellable); Markdown with highlighted code,
streaming at the frame budget, edit/regenerate, images, the model picker (two-tone, harnesses
marked), approvals (card with the diff, once/session/deny), the dock beside the editor (Mod+L),
checkpoints and Restore, first-run setup.

## Acceptance criteria
1. chat.mjs, dock.mjs, models.mjs and agent.mjs's checks pass in Flutter (the fake ACP agent in
   place of real harnesses).
2. A no-folder chat with the fake ACP agent works end to end; harness install runs against a local
   npm registry fixture.
3. Streaming a long answer stays within the frame budget.

## Tests
`scripts/flutter-e2e.sh chat_test.dart`; daemon tests for the scratch directory and the install.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
