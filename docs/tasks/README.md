# Tasks

One Markdown file per task: `docs/tasks/<milestone>/<TASK-ID>.md`. The
front-matter `status:` line is the **only** place a task's state is recorded;
everything else (board, "what is ready") is derived by `scripts/tasks.py`.

```sh
scripts/tasks.py            # board
scripts/tasks.py next       # the task to pick up now
scripts/tasks.py set M1-RPC-001 in-progress "session 2026-11-12"
```

States and the evidence each requires: `docs/architecture/governance.md`.
How to work a task: `agents/IMPLEMENTATION_GUIDE.md`. Template for new
tasks: [`TEMPLATE.md`](TEMPLATE.md) (it has no `id`, so the board ignores it).

Only M1 is specified at task level. Later milestones are specified at epic
level in `docs/roadmap/milestones.md`; the first task of each milestone is to
freeze its draft contract and write that milestone's task files.
