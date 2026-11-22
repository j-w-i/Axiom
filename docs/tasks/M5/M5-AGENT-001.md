---
id: M5-AGENT-001
title: Agent tools preview.start, ui.snapshot, ui.inspect, ui.selection, ui.verify; assertion-first
status: implemented
status_note: 2026-11-22; Linux (tests/preview.rs scripted-model loop over a real page, 3/3; tests/tools.rs 8/8)
depends_on: [M5-PREVIEW-001, M5-VERIFY-001]
order: 606
scope: crates/axiomd/ (tools), crates/agent/ (descriptions), docs/protocols/agents.md
---

# M5-AGENT-001: Agent integration

## Objective
Tools (Read effect, except `preview.start`: Execute) over the services; their descriptions tell the
model to state checks before patching and to call `ui.verify` after the reload; the selection's
context can be attached to a chat turn.

## Acceptance criteria
A scripted-model test: inspect a node → patch → `ui.verify` with a geometry check → `verified`.
