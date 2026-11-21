---
id: M3-MODEL-001
title: crates/agent: message model, ModelProvider, ScriptedProvider, Anthropic adapter
status: planned
status_note:
depends_on: [M3-ARCH-001]
order: 402
scope: crates/agent/, Cargo.toml (members), deny.toml
---

# M3-MODEL-001: crates/agent: message model, ModelProvider, ScriptedProvider, Anthropic adapter

## Objective
`crates/agent` exists with the normalised message model (`Message`, `Block` incl. `Opaque`), the
`ModelProvider` trait (`agents.md` §Model provider interface), a deterministic `ScriptedProvider`,
an in-house SSE parser, and an Anthropic Messages adapter that streams text, tool calls with
argument deltas, thinking blocks (opaque, replayed verbatim), usage and stop reasons.

## Context
Epic 3.1. ADR-0015 §3, §4; `agents.md`; research §Anthropic Messages.

## Implementation requirements
1. `ModelProvider::{id, models, stream}`; dropping the stream or cancelling the token aborts the
   HTTP request.
2. `ScriptedProvider::from_file(path)`: a JSON list of turns, each a list of `TurnEvent`s, plus
   optional expectations (e.g. the last message must contain a tool result for call X); it fails
   loudly when the loop's request does not match the script.
3. Anthropic adapter: `POST {baseUrl}/v1/messages` with `stream: true`, `x-api-key`,
   `anthropic-version: 2023-06-01`; maps `message_start`, `content_block_start|delta|stop`
   (text, tool_use with `input_json_delta`, thinking/redacted_thinking → `Opaque`),
   `message_delta` (stop_reason, usage), `error` events, HTTP 401/429(+retry-after)/413/5xx →
   `ProviderError`. Tool results are sent as `tool_result` blocks.
4. Errors never contain the API key; request logging redacts headers.

## Acceptance criteria
1. Recorded SSE fixtures (text only; tool use with split JSON; thinking + tool; error mid-stream;
   429) replay through a loopback fixture server and produce the expected event sequences.
2. Request bodies built from a `TurnRequest` match golden JSON.
3. Cancel during a stream ends it within 100 ms.

## Tests
`cargo test -p axiom-agent` (`sse_*`, `anthropic_*`, `scripted_*`); `scripts/check.sh`.

## Failure conditions
Any test needs the network.

## Documentation updates
`modules.md` §agent status; `agents.md` if a draft detail proved wrong (editorial).

## Handoff
Fixture provenance (hand-written from the documented event format, or captured).
