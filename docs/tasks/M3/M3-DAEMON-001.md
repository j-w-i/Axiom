---
id: M3-DAEMON-001
title: Multi-client daemon (Unix socket + token) and the axiom CLI
status: planned
status_note:
depends_on: [M3-LOOP-001]
order: 408
scope: crates/axiomd/, crates/rpc/, apps/cli/, Cargo.toml (members)
---

# M3-DAEMON-001: Multi-client daemon (Unix socket + token) and the axiom CLI

## Objective
`axiomd` listens on `<data dir>/axiomd.sock` with a token (ADR-0015 §6); connections share
services; per-connection cleanup; the `axiom` CLI connects (or spawns a daemon) and offers
`task run`, `build`, `agent send|sessions`, and `mcp` (M3-MCP-001 fills it).

## Context
Epic 3.6. ADR-0015 §6; `processes.md`.

## Implementation requirements
1. Socket mode 0600 in a 0700 directory; stale socket (no live listener) replaced; a live one
   left alone; token regenerated at start, file 0600; wrong/missing token → connection closed
   without serving.
2. Daemon-wide shutdown only when the stdio client ends; a socket client's disconnect stops
   only its watchers and cancels its in-flight requests.
3. CLI: exits non-zero with the task's exit code for `task run`/`build`; `agent send` prints
   streamed text and tool rows, answers approvals interactively on a TTY (deny otherwise).
4. Unix only; on Windows the listener is not compiled and the CLI always spawns its own daemon.

## Acceptance criteria
1. Integration: GUI-like stdio client and a socket client share a workspace; a task started by
   one is listed by the other; the socket client disconnecting leaves the stdio client working.
2. Bad token rejected. Socket permissions verified.
3. `axiom task run -- sh -c 'exit 3'` exits 3.

## Tests
`cargo test -p axiomd socket_`; `cargo test -p axiom-cli`; `scripts/check.sh`.

## Failure conditions
A socket client can act without the token.

## Documentation updates
`processes.md`, `security.md` §Local socket, `modules.md` §cli, `dev-setup.md` (CLI usage).

## Handoff
