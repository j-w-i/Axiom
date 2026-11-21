---
id: M3-GATE-001
title: Permission gate: policy, evaluation, approval round trip, audit log
status: planned
status_note:
depends_on: [M3-TOOLS-001]
order: 405
scope: crates/agent/, crates/axiomd/, crates/rpc/ (connection capabilities)
---

# M3-GATE-001: Permission gate: policy, evaluation, approval round trip, audit log

## Objective
Every tool call passes `PermissionGate::check(principal, tool, effect, targets)` → `Allow | Deny |
Ask`. `Ask` sends `agent/approvalRequested` to connections that declared `approvals` and waits for
`agent/approve`; no such connection → `approval_required` (deny with hint). Policy from settings
`agent.policy` plus stricter-only workspace rules; session grants in memory; audit log appended
for every decision and outcome; `agent/policy`, `agent/setPolicy`, `agent/approve`,
`agent/toolCall` served.

## Context
Epic 3.3. ADR-0015 §8; `agents.md` §Permission policy, §Execution pipeline.

## Implementation requirements
1. Rule matching: effect, tool name, path glob (workspace-relative), argv prefix; specificity
   order; deny > ask > allow among equals; default `ask`; built-in default policy allows `read`.
2. Workspace `[agent.policy]` rules may only add deny/ask or turn allow into ask/deny.
3. "Allow for session" never applies to `credentials` or `deploy`.
4. Approval waits are cancellable (turn cancel, connection gone); a request answered twice keeps
   the first answer.
5. Audit record: ts, principal (session id or `mcp:<client>` / `cli`), tool, targets, decision,
   rule, outcome, duration.

## Acceptance criteria
1. Table-driven policy tests (≥ 20 cases incl. stricter-only workspace override).
2. axiomd integration: a tool call needing approval with an approving test client → approved
   once → executed; denied → not executed (file unchanged), audit shows both.
3. No approving connection → `approval_required`, tool not executed.

## Tests
`cargo test -p axiom-agent gate_`; `cargo test -p axiomd approval`; `scripts/check.sh`.

## Failure conditions
Any path executes a tool after `Deny` or without a decision.

## Documentation updates
`agents.md` §Permission policy (as built), `security.md`.

## Handoff
The default policy as shipped.
