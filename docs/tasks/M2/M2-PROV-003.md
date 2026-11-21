---
id: M2-PROV-003
title: Python provider delegating to uv
status: tested
status_note: 
depends_on: [M2-ENV-003]
order: 322
scope: providers/python/ (new)
---

# M2-PROV-003: Python provider delegating to uv

## Objective
A `python` provider detects Python projects (`pyproject.toml`, `requirements.txt`), uses `uv` (installed as a managed archive if missing) to install a managed Python with redirected homes, and offers `uv run`/`pytest` tasks.

## Context
Epic 2.8. `docs/research/toolchains.md` §pyenv and uv.

## Required interfaces
- Delegate machinery (M2-ENV-003); contract suite.

## Implementation requirements
1. `uv` as a managed archive (GitHub releases of `astral-sh/uv`, `sha256_url`).
2. Delegate install `uv python install ${version}` with `UV_PYTHON_INSTALL_DIR=${root}/python`, `UV_CACHE_DIR=${root}/cache`.
3. Tasks: `uv run pytest` with a `regex` matcher for pytest failures.

## Constraints
- ADR-0014 (accepted) is the decision record; do not reopen its decisions in this task.
- Frozen contracts (IPC 0.1 meanings, editor bridge 1) unchanged; additive protocol changes only where this spec says so.
- `scripts/check.sh` green; no test deleted, weakened or skipped.
- The user's own Python and uv state untouched.

## Acceptance criteria
1. Contract suite passes; CI round trip installs and removes a managed Python.

## Tests
- contract suite; CI round trip.

## Failure conditions
- Writes outside the managed root.

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
Install time and size.
