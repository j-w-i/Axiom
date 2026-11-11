# Review guide

How to verify a task that someone else implemented. Only a reviewer moves a
task from `tested` to `verified`. The reviewer must be a different session
from the implementer and must re-run things rather than trust the report.

## Procedure

1. Read the task file and the implementer's handoff.
2. Check out the task's branch (or the commit range named in the handoff).
3. **Scope:** `git diff --stat <base>..` — every changed path must be inside
   the task's `scope` or on the always-allowed list (`governance.md` rule 10).
4. **Gate:** run `scripts/check.sh` from a clean build
   (`cargo clean -p <crate>` / remove `build/desktop` for the affected part).
5. **Tests are real:** read the new tests. They must assert behaviour (not
   just "does not panic"), must not sleep in place of synchronisation, and
   must fail if the feature is broken — pick two (all of them, if the task added fewer than
   two) and break the implementation locally to see them fail, then restore.
6. **Acceptance criteria:** verify each one yourself. For GUI criteria, run
   the app (or the offscreen test with a screenshot) and look.
7. **Contracts:** `git diff <base>.. -- crates/protocol schemas docs/protocols`
   must be empty unless the task explicitly allows contract changes;
   `tests/contract.rs` expectations must be unmodified.
8. **Architecture:** check against `modules.md` (dependency edges,
   responsibilities), and the hard rules in `AGENTS.md`. Look specifically
   for: logic in handlers or QML, file access in the shell, paths built
   without `Workspace::resolve`, new dependencies and their licences,
   `unwrap` on fallible paths, blocking calls on the GUI thread or in async
   code, stdout writes in the daemon.
9. **Honesty:** claims in the handoff match what you observed; unverified
   items are labelled.
10. **Docs:** the task's documentation updates were made and are accurate.

## UI checklist (shell tasks)

- Both themes render; no literal colours/sizes in `Axiom.App`.
- Every control does something; nothing is a placeholder.
- Keyboard: Tab order sensible, focus visible, lists navigable by arrows.
- `Accessible.name` on interactive items.
- Errors are shown to the user in words, not swallowed or only logged.
- Window resizes down to the minimum size without overlap or clipping.

## Outcome

Append a **Review** section to the task's handoff file:

```
## Review — <date> — <reviewer session>
Result: verified | changes requested
Re-ran: <commands and summary lines>
Criteria: <each criterion: pass/fail + evidence>
Mutation check: <which two tests were shown to fail when the code was broken>
Findings: <numbered; severity blocker/major/minor; file:line>
```

- All pass → `scripts/tasks.py set <ID> verified "<date> reviewed"`, merge.
- Otherwise → `scripts/tasks.py set <ID> in-progress "changes requested, see handoff"`;
  the findings are the work list. Do not fix blockers yourself in the same
  session and then mark it verified.
