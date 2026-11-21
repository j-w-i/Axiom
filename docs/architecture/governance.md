# Architectural governance

These rules exist so that many independent implementation sessions converge
on one system. They bind every contributor, human or agent.

## Frozen and draft contracts

| Contract | File(s) | State |
|----------|---------|-------|
| IPC protocol 0.1 | `crates/protocol/src/{envelope,method,lifecycle,workspace,fs,task,settings}.rs`, `docs/protocols/ipc.md` | **Frozen for M1** |
| Editor bridge 1 | `docs/protocols/editor-bridge.md` | **Frozen for M1** |
| IPC protocol 0.2 additions | `crates/protocol/src/{env,build}.rs`, `docs/protocols/ipc.md` §Protocol 0.2 | **Frozen for M2** (M2-ARCH-001) |
| Provider manifest | `crates/protocol/src/provider.rs`, `docs/protocols/providers.md` | **Frozen for M2** (M2-ARCH-001) |
| Diagnostics | `crates/protocol/src/diagnostics.rs` | **Frozen for M2** (M2-ARCH-001) |
| IPC protocol 0.3 additions | `crates/protocol/src/agent.rs`, `docs/protocols/ipc.md` §Protocol 0.3 | **Frozen for M3** (M3-ARCH-001) |
| Agent tools & permissions | `docs/protocols/agents.md` | **Frozen for M3** (M3-ARCH-001) |
| IPC protocol 0.4 additions | `crates/protocol/src/{lsp,terminal,git}.rs`, `workspace.rs` (`workspace/search`), `provider.rs` (`lsp` table), `docs/protocols/ipc.md` §Protocol 0.4 | **Frozen for M4** (M4-ARCH-001) |
| Inspection | `crates/protocol/src/inspection.rs`, `docs/protocols/inspection.md` | Draft until M5 starts |

**Frozen** means: an implementation task may not change its meaning. If the
contract is wrong or insufficient, stop and file a change request (below).
Rewording a frozen document to remove an ambiguity or record a newly
verified fact, without changing what an implementation must do, is allowed
from any task (it counts as within scope) and must be listed in the
completion report so the reviewer can confirm it is editorial.
**Draft** means: the first task of that milestone reviews it against reality,
amends it, and freezes it.

Additive changes to a frozen contract (new optional field, new method, new
notification) are allowed *only* through a task whose spec says so, must keep
`tests/contract.rs` green without editing existing expectations, and must
regenerate `schemas/`.

## What an implementation task may not do

1. Add, replace or upgrade-across-major a framework or core dependency not
   named in its task spec or in `modules.md`. (Small leaf crates for a narrow
   job are fine if MIT/Apache-2.0/BSD/ISC/Zlib/Unicode licensed; say so in the
   completion report.)
2. Add a dependency edge between modules that `modules.md` does not allow.
3. Move a responsibility from one module to another.
4. Put business logic in the shell or in `axiomd` handlers.
5. Read or write workspace files from the shell or the editor surface.
6. Bypass `Workspace::resolve` to build a host path.
7. Delete, weaken, `#[ignore]` or skip a test to get a green run.
8. Edit generated files (`schemas/**`) by hand.
9. Create directories, modules, UI controls or settings that nothing uses yet.
10. Touch files outside the task's declared scope, except: `Cargo.lock`,
    lockfiles, the task's own status, `docs/roadmap/current.md`, its
    handoff, the files its *Documentation updates* section names, and
    editorial rewording of frozen documents as defined above.
11. Use a Qt module that is not LGPL (see ADR-0011 for the list).

## Change requests

When a task finds that a decision or contract is wrong:

1. Stop work on the affected part. Finish anything unaffected.
2. Write `docs/decisions/proposed/CR-<task-id>-<slug>.md` containing:
   the original assumption; the evidence against it (commands, output,
   links); the proposed change; affected modules and contracts; migration
   steps; what was tried as a workaround.
3. Set the task to **Blocked** with a pointer to the CR and record it in the
   handoff. Move on to another Ready task that does not depend on it.

A CR is resolved by an architect session (a human, or an agent explicitly
asked to act as architect) which either rejects it with reasons or accepts it
by writing an ADR, updating contracts and affected task specs.

Small factual corrections to documentation (a wrong command, a renamed flag)
are not architecture changes: fix them in place and mention them in the
completion report.

## ADRs

`docs/decisions/ADR-NNNN-title.md`. Immutable once accepted, except for the
status line. To change a decision, write a new ADR that supersedes the old
one and update the old one's status to `Superseded by ADR-MMMM`.

## Task states and evidence

`Planned → Ready → In Progress → Implemented → Tested → Verified`, or
`Blocked`. A dependency at Tested unblocks its dependants so a single agent
is never stalled waiting for a reviewer; if review later finds a defect, the
fix is made on the dependency and dependants re-run `scripts/check.sh`.

| State | Meaning | Evidence required |
|-------|---------|-------------------|
| Planned | spec exists; dependencies not met | – |
| Ready | derived, not stored: status is `planned` and every dependency is Tested or Verified (`scripts/tasks.py`) | – |
| In Progress | one agent owns it | status line names the session/date |
| Implemented | code written, compiles | build output |
| Tested | task's own tests and `scripts/check.sh` pass | pasted summary lines |
| Verified | a **different** session re-ran the tests and checked every acceptance criterion | review report (`agents/REVIEW_GUIDE.md`) |
| Blocked | cannot proceed | CR or blocker description |

An implementing agent may move its own task up to **Tested**, never to
Verified. If something could not be tested in the current environment (e.g.
macOS behaviour on a Linux machine) the report says "unverified on <platform>"
and CI is the verifier.

## Parallel work

- One task = one branch `task/<task-id>` (or a git worktree). Never commit to
  `main` directly while other tasks are in flight.
- A task owns its scope directories for its duration. Two In-Progress tasks
  must not have overlapping scope (the root `Cargo.toml` member/dependency
  lists excepted: additive edits, merged by keeping both); the roadmap's
  parallel waves respect this for M1, with one flagged exception (wave G)
  that requires an integrator.
- Merge order follows `docs/roadmap/dependency-graph.md`. After each merge,
  `scripts/check.sh` must pass on `main`.
- Shared files (`Cargo.toml` workspace members, `scripts/check.sh`, CI) are
  edited minimally and additively; on conflict, keep both additions.
