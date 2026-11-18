# Implementation guide

How to take one task from Ready to Tested. Follow it literally; it is short
on purpose.

## 1. Orient (5 minutes)

1. Read `AGENTS.md`.
2. Read `docs/roadmap/current.md`.
3. Read the handoff named under "Latest handoff" in `current.md`.
4. `git status` and `git log --oneline -10`. If the tree is dirty with
   someone else's unfinished work, do not discard it: read the handoff; if
   it is unexplained, stash nothing, commit nothing — report it and stop.
5. `scripts/check.sh` must pass before you start. If it does not, fixing
   that (or reporting why you cannot) is your first job.

## 2. Pick a task

- If the board (`scripts/tasks.py`) shows a task `in-progress` or
  `implemented`, read its handoff and finish it first. Otherwise
  `scripts/tasks.py next` gives the task, unless `docs/roadmap/current.md`
  ("Next task") gives a selection rule for the current milestone; that rule
  wins (M1.6: the first READY `M1.6-*` task by `order`).
- Read the task file fully, then **only** the documents under its *Context*
  and *Required interfaces*. Do not read all of `docs/`.
- Claim it: `scripts/tasks.py set <ID> in-progress "<date>"` and create a
  branch `task/<ID>` (when working alone with no other agents, committing
  directly to the milestone branch named in `docs/roadmap/current.md`, e.g.
  `task/m1.6`, or to `main` if none is named, is acceptable; say so in the
  handoff).

## 3. Implement

- Work inside the task's `scope`. If you believe you must touch something
  else, re-read `docs/architecture/governance.md` first.
- Where the spec gives signatures, names or behaviours, they are binding.
  Where it is silent, choose the simplest thing consistent with
  `modules.md` and the coding standards, and note the choice in the report.
- Write tests as you go; each acceptance criterion needs evidence.
- Build and run frequently. For GUI work: run the app or its offscreen test
  and look at the result (grab a screenshot) rather than assuming.
- Keep commits small: `<ID>: <summary>`.

## 4. When the plan is wrong

A spec can be wrong; do not work around it silently and do not redesign.
- **Small factual error** (a renamed flag, a wrong path): fix the doc, note
  it in the report, continue.
- **A contract or decision does not work**: write a change request
  (`docs/decisions/proposed/CR-<ID>-<slug>.md`, format in `governance.md`),
  set the task `blocked` with a pointer, finish whatever is unaffected, and
  move to another ready task.
- **Stuck on a failure** for more than ~3 focused attempts: stop, write
  down exactly what you tried and observed (commands, output), mark
  `blocked` or leave `in-progress` with the note, and move on. A precise
  failure report is a useful outcome; a guessed fix is not.

## 5. Verify

1. Run every command in the task's *Tests* section.
2. Run `scripts/check.sh` (whole thing).
3. Walk the acceptance criteria one by one and record the evidence for each
   (test name, command output line, screenshot path).
4. Anything you could not verify (other OS, no display, no network) is
   listed as **unverified**, not omitted.

## 6. Record

1. `scripts/tasks.py set <ID> tested "<date>; see handoff"` — or
   `implemented` if some acceptance criterion is unverified, or `blocked`.
   You never set your own task to `verified`.
2. Update `docs/roadmap/current.md` (status table, next task, blockers, and
   the "Latest handoff" line → the file you write in step 4).
3. Do the *Documentation updates* the task lists. Docs must describe what
   the code actually does.
4. Write the completion report into a handoff file
   `agents/handoffs/<YYYY-MM-DD>-<ID>.md` (if that name exists, append
   `-2`, `-3`, …) using `agents/HANDOFF_TEMPLATE.md`.
5. Commit.

## 7. Continue or stop

If you have budget left, go back to step 2. Before stopping, make sure the
last handoff is complete and the tree is clean (`git status`).

## Quick reference

| Need | Where |
|------|-------|
| Wire types | `crates/protocol/src/` |
| Behaviour of a method | `docs/protocols/ipc.md` |
| Which crate owns what | `docs/architecture/modules.md` |
| What I may not do | `docs/architecture/governance.md` |
| UI tokens and layout | `docs/architecture/design-system.md` |
| Working reference code for Qt+WebEngine+daemon | `experiments/e1-qt-rust-monaco/` |
| Toolchain setup problems | `docs/contributing/dev-setup.md` |
