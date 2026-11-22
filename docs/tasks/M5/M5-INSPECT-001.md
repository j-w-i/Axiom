---
id: M5-INSPECT-001
title: crates/inspect: adapter WebSocket server, sessions, normalisation, source resolution
status: implemented
status_note: 2026-11-22; Linux (cargo test -p axiom-inspect 6/6, -p axiomd --test inspect)
depends_on: [M5-ARCH-001]
order: 602
scope: crates/inspect/, crates/axiomd/, Cargo.toml, deny.toml (if needed)
---

# M5-INSPECT-001: The inspection service

## Objective
A loopback WebSocket server (one port per daemon, a one-time token per preview) that adapters dial;
`inspect/hello` checks the token and records capabilities; sessions keep the latest snapshot;
results are normalised (workspace paths) and given source candidates (ADR-0019 §4): `instrumented`
from `data-axiom-src` (1.0), `owner` from the nearest stamped ancestor in another file (≤ 0.6),
`heuristic` from text search (≤ 0.4). `axiomd` serves `ui/snapshot|hitTest|nodeDetails|highlight|
inspectMode` and relays `ui/selected|hovered|didMutate`.

## Acceptance criteria
1. Tests with a fake adapter (a WebSocket client in the test): a wrong token is refused; snapshot,
   hit test and details round-trip; a stamped element resolves to the exact workspace file, line and
   column; an unstamped one gets an owner candidate below 0.6; timeouts mark the session degraded.
2. Paths outside the workspace are never returned as candidates.

## Tests
`cargo test -p axiom-inspect`; `cargo test -p axiomd --test inspect`.
