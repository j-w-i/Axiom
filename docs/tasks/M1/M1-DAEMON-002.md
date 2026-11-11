---
id: M1-DAEMON-002
title: End-to-end protocol behaviour tests against the real daemon
status: tested
status_note: 2026-11-11; see handoff
depends_on: [M1-DAEMON-001]
order: 31
scope: crates/axiomd/, tests/fixtures/; bug fixes in crates/{rpc,store,workspace,exec}/ limited to defects these tests reveal
---

# M1-DAEMON-002: Daemon integration test suite

## Objective
An executable specification: tests that spawn the `axiomd` binary and verify
every behaviour stated in `docs/protocols/ipc.md`. This suite is the
compatibility gate for all future backend changes.

## Context
`docs/protocols/ipc.md` §Contract tests says a behaviour stated there
without a test here is a defect. Use `axiom_rpc::Client` over the child's
pipes (`env!("CARGO_BIN_EXE_axiomd")`).

## Implementation requirements
1. Test harness `crates/axiomd/tests/common/mod.rs`:
   `Daemon::spawn()` (fresh temp `AXIOM_DATA_DIR`), `.client`, `.initialize()`,
   `.open_fixture()` (copies `tests/fixtures/basic/` into a temp dir and opens
   it), `.next_notification(method, timeout)`, clean kill on drop.
2. Fixture `tests/fixtures/basic/`: a handful of files — `README.md`,
   `src/main.rs`, `src/lib.rs`, `.gitignore` (ignoring `target/` and
   `*.log`), `notes/crlf.txt` (CRLF line endings), `data/invalid-utf8.bin`.
   Keep it under 20 KB. Add `tests/fixtures/README.md` describing each
   fixture.
3. Every defect fixed in a service crate gets a regression test in that
   crate as well, and is listed in the completion report.
4. Test files and required cases:
   - `lifecycle.rs`: request before initialize; major-version mismatch;
     negotiated version; second initialize; `methods` list; shutdown; EOF.
   - `errors.rs`: parse error keeps connection; unknown method; invalid
     params (wrong type, missing required field); unknown fields ignored;
     every error has `data.kind` and `code == kind.code()`.
   - `workspace.rs`: open/close/idempotence; `workspace_not_open` after
     close; recent list ordering and omission of deleted directories; state
     set/get/delete-by-null; state persists across a daemon restart with
     the same data dir; 1 MiB limit.
   - `fs.rs`: list ordering and `ignored` flags; read (text, CRLF intact,
     invalid UTF-8, directory, missing); **all six rows** of the write
     decision table; path traversal (`../x`, absolute, backslash) →
     `invalid_params`; symlink escape → `permission_denied` (unix).
   - `watch.rs`: no notifications before `fs/watch`; change after enabling →
     `fs/didChange`; own `fs/writeFile` is reported; disable stops them.
   - `task.rs`: argv and shell forms; output ordering and `task/start`
     response before first notification; exit code; `spawn_failed` details;
     cwd outside → error; env applied; cancel → `cancelled`; `task/list`;
     closing the workspace cancels its tasks; 200 KiB of output arrives
     complete.
   - `settings.rs`: defaults; set/get; type validation; reset by null;
     `settings/didChange` emitted; unknown keys round trip; persistence
     across restart.
   - `cancel.rs`: `$/cancel` for an unknown id is ignored (connection still
     works).
5. Tests must be parallel-safe (each spawns its own daemon and temp dirs) and
   finish in under 60 s total on a laptop.

## Constraints
No sleeps as synchronisation except bounded waits for notifications with a
timeout (≤ 5 s) that fails the test when exceeded.

## Acceptance criteria
- ≥ 45 integration tests, all passing, 10 consecutive runs without a flake.
- A table in the completion report mapping each `ipc.md` section to the test
  functions that cover it; any uncovered statement is listed explicitly.

## Tests
`scripts/check.sh rust`; `for i in $(seq 10); do cargo test -p axiomd -q || break; done`.

## Failure conditions
A test reveals the daemon contradicts `ipc.md`: fix the daemon/service crate
if it is an implementation bug; if the *document* is wrong or ambiguous, file
a CR instead of adjusting the test to the implementation.

## Documentation updates
`docs/roadmap/current.md`; `docs/protocols/ipc.md` only for clarifications
approved by the above rule.

## Handoff
The coverage table.
