---
id: M3-LOOP-001
title: Agent loop and sessions: streaming, tools, cancellation, JSONL journal, resume, compaction
status: planned
status_note:
depends_on: [M3-MODEL-002, M3-GATE-001, M3-CKPT-001]
order: 407
scope: crates/agent/, crates/axiomd/, crates/store/ (migration 4)
---

# M3-LOOP-001: Agent loop and sessions

## Objective
`agent::session` and `agent::agent_loop`: a session journal (ADR-0015 §9), context assembly
(system prompt, workspace info, the workspace's `AGENTS.md` capped at 32 KiB, compaction summary,
messages), the loop (stream → tool calls → gate → checkpoint → execute → results → repeat until
end turn, max 50 tool rounds per turn), cancellation, resume, compaction; the daemon serves
`agent/sessionCreate|sessionList|sessionOpen|send|cancel|checkpoints|restore` and streams
`agent/event` to attached connections.

## Context
Epic 3.5. ADR-0015 §5, §9; `agents.md` §Sessions.

## Implementation requirements
1. Events appended before they are sent; tool intent before execution, result after;
   `interrupted` on resume when a result is missing.
2. `agent/send` returns when the turn is accepted; the turn's events stream; `turnEnd` closes it.
   One running turn per session (`busy` otherwise).
3. Cancel stops the stream and running tools (task cancel) within 1 s; the journal records it.
4. Resume after daemon restart: `sessionOpen` replays the journal; `send` continues with the
   full context (opaque blocks to the same provider kind only).
5. Compaction as ADR-0015 §9, tested with the scripted provider.
6. Model errors become `error` events and `model_error` details; no automatic retry of a turn
   that executed tools; rate-limit retry once after `retry-after` ≤ 30 s before any tool ran.

## Acceptance criteria
1. Scripted end-to-end in axiomd tests: read → patch → build fails → patch → build passes;
   every write preceded by a checkpoint; restore returns the original.
2. Denied tool not executed; the model receives the denial as a tool error.
3. Restart the daemon mid-session; `sessionOpen` + `send` continues correctly.
4. Cancel mid-stream and mid-tool.

## Tests
`cargo test -p axiom-agent loop_ session_`; `cargo test -p axiomd agent_`; `scripts/check.sh`.

## Failure conditions
A write without a prior checkpoint; a lost or reordered journal event.

## Documentation updates
`agents.md` §Sessions, `ipc.md` (agent/* behaviour), `modules.md` §agent.

## Handoff
Journal event examples.
