---
id: M2-PROV-001
title: Node provider (official archives with SHASUMS256)
status: planned
status_note:
depends_on: [M2-ENV-002]
order: 320
scope: providers/node/ (new)
---

# M2-PROV-001: Node provider (official archives with SHASUMS256)

## Objective
A `node` provider detects Node projects (`package.json`), detects `node`/`npm`, installs official Node archives per platform verified against nodejs.org's `SHASUMS256.txt`, and offers `npm` tasks.

## Context
Epic 2.8. `docs/research/toolchains.md` §Node, Python and Go artifact manifests.

## Required interfaces
- Provider manifest schema (frozen by M2-ENV-001); contract suite.

## Implementation requirements
1. Versions from `https://nodejs.org/dist/index.json` (`Json` source).
2. Artifacts per OS/arch (tar.xz Linux, tar.gz macOS, zip Windows) with `sha256_url` to `SHASUMS256.txt`.
3. Tasks: `npm run build`, `npm test`, `npm start` (argv), diagnostics `regex` for tsc output where applicable.

## Constraints
- ADR-0014 (accepted) is the decision record; do not reopen its decisions in this task.
- Frozen contracts (IPC 0.1 meanings, editor bridge 1) unchanged; additive protocol changes only where this spec says so.
- `scripts/check.sh` green; no test deleted, weakened or skipped.

## Acceptance criteria
1. Contract suite passes for `node` on all three OSes.
2. CI install round trip of one LTS version (`ci_install = true`).

## Tests
- `cargo test -p axiom-env` (contract suite, `node` round trip in CI).

## Failure conditions
- Any artifact without a checksum source.

## Documentation updates
`providers/node/` README comment, `docs/roadmap/current.md`.

## Handoff
The version and sizes installed in CI.
