# axiom-exec

Process execution for the Axiom daemon: spawn a command in a workspace, stream
its output in order, cancel it, and report how it ended.

This directory is a **snapshot of real Axiom files** used by the visual QA
harness (`apps/desktop-tauri/e2e/shots.mjs`). It is data, not a crate: it is
never built. Change it only together with a new baseline.

## Layout

- `src/executor.rs` — the task executor
- `src/tree.rs` — process-tree termination
- `src/backend.rs` — the platform backend seam
- `docs/ipc.md` — the wire contract the executor serves
