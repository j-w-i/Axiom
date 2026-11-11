# Dependency graph and parallel plan

`scripts/tasks.py` derives readiness from each task's `depends_on`; this
file explains the shape and the merge order.

## M1 task graph

```
            ┌─ M1-RPC-001 ────┐
            ├─ M1-STORE-001 ──┤
 (start) ───┼─ M1-WS-001 ─────┼──▶ M1-DAEMON-001 ──┬──▶ M1-CI-001 ───────────────────────────┐
            │      └─▶ M1-WS-002 ─────────────┐    │                                         │
            ├─ M1-EXEC-001 ───┘               ▼    ├──▶ M1-SHELL-001 ─┐                       │
            │                         M1-DAEMON-002 ──────────────────┴▶ M1-SHELL-002 ─┬▶ M1-SHELL-004 ─┤
            │                                                                          │                ├▶ M1-CI-002
            └─ M1-EDITOR-001 ──────────────────────────────────────────────────────────┴▶ M1-SHELL-003 ─┤
                                                                                              └▶ M1-SHELL-005 ─┘
```

| Task | Depends on |
|------|-----------|
| M1-RPC-001, M1-STORE-001, M1-WS-001, M1-EXEC-001, M1-EDITOR-001 | – |
| M1-WS-002 | M1-WS-001 |
| M1-DAEMON-001 | M1-RPC-001, M1-STORE-001, M1-WS-001, M1-EXEC-001 |
| M1-DAEMON-002 | M1-DAEMON-001, M1-WS-002 |
| M1-CI-001 | M1-DAEMON-001 |
| M1-SHELL-001 | M1-DAEMON-001 |
| M1-SHELL-002 | M1-SHELL-001, M1-DAEMON-002 |
| M1-SHELL-003 | M1-SHELL-002, M1-EDITOR-001 |
| M1-SHELL-004 | M1-SHELL-002 |
| M1-SHELL-005 | M1-SHELL-003 |
| M1-CI-002 | M1-SHELL-003, M1-SHELL-004, M1-SHELL-005, M1-CI-001 |

## Parallel groups

Tasks in the same wave have disjoint scopes and may run concurrently in
separate branches/worktrees. A single agent working alone simply follows
`scripts/tasks.py next` (the `order` field encodes this sequence).

| Wave | Tasks (parallel) | Integration checkpoint after the wave |
|------|------------------|---------------------------------------|
| A | M1-RPC-001 · M1-STORE-001 · M1-WS-001 · M1-EXEC-001 · M1-EDITOR-001 | All merged; `scripts/check.sh` green on `main`. The only shared file is the root `Cargo.toml` (`members` and `[workspace.dependencies]`): merge additively. |
| B | M1-WS-002 · M1-DAEMON-001 | Daemon answers the manual session. |
| C | M1-DAEMON-002 · M1-CI-001 · M1-SHELL-001 | **Backend gate:** the integration suite is green; from here the protocol behaviour is pinned by tests. |
| D | M1-SHELL-002 | Explorer works against the real daemon. |
| E | M1-SHELL-003 · M1-SHELL-004 | Both touch `apps/desktop`: 003 owns `src/editor/`, `OpenDocumentsModel`, `EditorArea.qml`; 004 owns `Task*` models and `OutputPanel.qml`. Shared files (`Main.qml`, `CMakeLists.txt`, `main.cpp`) — merge 003 first, then rebase 004. If only one agent is available, do 003 then 004. |
| F | M1-SHELL-005 | |
| G | M1-CI-002 | **M1 exit review.** |

Sequential spine (cannot be parallelised): RPC/services → DAEMON-001 →
SHELL-001 → SHELL-002 → SHELL-003 → SHELL-005 → CI-002.

## Roles in a multi-agent run

1. **Implementers** take Ready tasks, one each, on `task/<id>` branches.
2. An **integrator** merges finished branches in the order above, resolves
   additive conflicts, runs `scripts/check.sh` on `main` after each merge.
3. A **reviewer** (a different session from the implementer) verifies each
   task per `agents/REVIEW_GUIDE.md` and moves it to `verified`.
With one agent, it plays all three roles sequentially but must still not
mark its own work `verified`; leave tasks at `tested` and note that
independent verification is pending.

## Milestone-level dependencies

```
M1 ──▶ M2 ──▶ M3 ──▶ M5 ──▶ M6.1 (QML adapter)
 │      │      └───────────▶ M6.7 (multi-agent)
 │      └──▶ M6.2, M6.3 (providers)        M3 + M5 ──▶ M6.8
 └──▶ M4 (needs M1 only; LSP server definitions use M2 manifests when available)
M4 ──▶ M6.4 (debugging)      M2 ──▶ M6.5 (isolation backends) ──▶ M6.6 (remote)
M4 ──▶ M7.1 (extensions)     everything ──▶ M7.2–7.7
```

- **M4 can start as soon as M1 is done**, in parallel with M2 and M3: it
  touches `editor/`, a new `crates/lsp`, and editor-related shell views.
- **M5 needs M3** only for the agent-driven half (epics 5.7 and the loop
  test); epics 5.1–5.6 and 5.8 need M1 (+ M2 for launching dev servers
  inside an environment) and can begin while M3 is in progress.
- Contract freeze points: IPC 0.1 and editor bridge 1 are frozen now;
  provider/diagnostics at M2 epic 2.0; agents at 3.0; inspection at 5.0.
