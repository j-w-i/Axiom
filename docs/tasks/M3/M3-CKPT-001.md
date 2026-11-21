---
id: M3-CKPT-001
title: Checkpoints: shadow git repository, create/list/restore with conflict detection
status: planned
status_note:
depends_on: [M3-ARCH-001]
order: 406
scope: crates/agent/
---

# M3-CKPT-001: Checkpoints: shadow git repository, create/list/restore with conflict detection

## Objective
`agent::checkpoint::Checkpoints` implements ADR-0015 §5 with the `git` CLI: `snapshot`,
`create(session, label)`, `list(session)`, `restore(session, n)` returning restored files and
per-file conflicts, never touching the user's repository.

## Context
Epic 3.4. ADR-0015 §5; research §Checkpoints.

## Implementation requirements
1. Isolated git invocation (env cleared of `GIT_*`, `GIT_CONFIG_NOSYSTEM=1`, `HOME` = shadow
   dir, `-c core.hooksPath=/dev/null`, `-c core.autocrlf=false`, `-c core.fsmonitor=false`).
2. Snapshot honours the workspace's `.gitignore` files; skips files > 16 MiB (listed).
3. Restore compares current content to the agent's last post-state per file; conflicts are
   reported with reason (`modifiedSinceAgent`, `deletedSinceAgent`); restore is preceded by a
   checkpoint.
4. `git` missing → `prerequisite_missing {missing: ["git"]}`.

## Acceptance criteria
1. In a workspace that is itself a git repo with staged and unstaged changes and a stash: after
   create/restore cycles the user's `git status --porcelain`, index, HEAD, refs and stash list
   are byte-identical.
2. Non-git workspace: create, modify, add, delete, restore → original tree.
3. A file the user changed after the agent → conflict, file untouched.
4. Ignored `target/` not captured.

## Tests
`cargo test -p axiom-agent checkpoint_`.

## Failure conditions
Any change to the user's `.git`.

## Documentation updates
`agents.md` §Checkpoints (as built).

## Handoff
Snapshot timing on a medium repository (this repository).
