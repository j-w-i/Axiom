# Handoff — <YYYY-MM-DD> — <TASK-ID or session topic>

Session: <model / person>   Branch: <branch>   Commits: <first..last short hashes>

## Result
Task status set to: <tested | implemented | blocked | in-progress>
One-paragraph summary of what now exists.

## Behaviour implemented
- …

## Files changed
- `path` — what and why

## Commands run and actual results
```
<command>
<the relevant output lines, verbatim — test summary lines, exit codes>
```

## Acceptance criteria
| # | Criterion | Result | Evidence |
|---|-----------|--------|----------|
| 1 | … | pass / fail / unverified | test name, output line, screenshot path |

## Not verified
What could not be checked in this environment, and why (OS, display, network…).

## Deviations and decisions
Anything done differently from the spec, choices made where the spec was
silent, documentation corrected. Change requests filed: <paths>.

## Known defects / limitations
- …

## Blockers
- …

## Next steps
The exact next task (`scripts/tasks.py next` output) and anything the next
agent must know before starting it.
