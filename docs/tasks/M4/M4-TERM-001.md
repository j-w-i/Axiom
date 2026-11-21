---
id: M4-TERM-001
title: crates/term: PTY service (portable-pty), terminal/* served
status: planned
status_note:
depends_on: [M4-ARCH-001]
order: 504
scope: crates/term/, crates/axiomd/, Cargo.toml, deny.toml (if needed)
---

# M4-TERM-001: Terminal service

## Objective
`axiom-term` runs the person's shell in a PTY (`portable-pty` 0.9 behind a `Pty` trait), in the
workspace root with the project's environment, streams UTF-8 output (never splitting a character)
coalesced to ≤ 1 notification / 16 ms, keeps a 1 MiB backlog, resizes, and ends the process group
on close. `axiomd` serves `terminal/create|write|resize|close|list` and emits `terminal/output` /
`terminal/exited` to the creating workspace's clients.

## Context
ADR-0018 §4; `crates/env` resolution (`env/resolve`) for the environment; `crates/exec` (process
groups).

## Implementation requirements
1. Shell: `$SHELL` else `/bin/sh`; `-l` on macOS; `TERM=xterm-256color`, `COLORTERM=truecolor`;
   `argv` overrides for tests.
2. Reader on a blocking thread; streaming UTF-8 decoder; coalescing; sequence numbers; backlog
   replayed on `terminal/list` re-attach (the shell reloads without losing the screen).
3. Close: SIGHUP to the group, then kill after 2 s; all terminals end on workspace close and
   daemon shutdown.
4. Not gated (ADR-0018 §4), and never exposed to agents.

## Acceptance criteria
1. Integration test: an interactive `sh` echoes input, reports `$COLUMNS` after a resize, sees the
   project environment's `PATH` entry, exits with the code given; a 4-byte character written in
   two halves arrives whole.
2. Closing a terminal running `sleep 1000` leaves no process.
3. 10 MB of output does not grow memory beyond the backlog bound.

## Tests
`cargo test -p axiom-term`; `cargo test -p axiomd --test terminal`.

## Failure conditions
A shell survives the daemon; output arrives split inside a character.

## Documentation updates
`modules.md` §term, `ipc.md`, `security.md`.

## Handoff
Measured output throughput and latency.
