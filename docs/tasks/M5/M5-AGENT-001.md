---
id: M5-AGENT-001
title: Agent tools preview.start, ui.snapshot, ui.inspect, ui.selection, ui.verify; assertion-first
status: planned
status_note:
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
