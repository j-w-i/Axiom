---
id: M2-PROV-002
title: CMake and Ninja providers (archives)
status: tested
status_note: 
depends_on: [M2-ENV-002]
order: 321
scope: providers/cmake/, providers/ninja/ (new)
---

# M2-PROV-002: CMake and Ninja providers (archives)

## Objective
`cmake` and `ninja` providers detect CMake projects (`CMakeLists.txt`), install official release archives with checksums, and offer configure/build tasks with `gcc_text` diagnostics.

## Context
Epic 2.8.

## Required interfaces
- Provider schema; contract suite; `gcc_text` parser (M2-BUILD-001).

## Implementation requirements
1. CMake: GitHub releases of `Kitware/CMake`, per-platform archives, `sha256_url` (the release's SHA-256 file).
2. Ninja: GitHub releases of `ninja-build/ninja` with pinned per-version digests (`Sha256 { values }`) where no checksum file is published.
3. Tasks: `cmake -S . -B build -G Ninja`, `cmake --build build` with `gcc_text` diagnostics.

## Constraints
- ADR-0014 (accepted) is the decision record; do not reopen its decisions in this task.
- Frozen contracts (IPC 0.1 meanings, editor bridge 1) unchanged; additive protocol changes only where this spec says so.
- `scripts/check.sh` green; no test deleted, weakened or skipped.

## Acceptance criteria
1. Contract suite passes for both.
2. CI round trip for both on Linux.

## Tests
- contract suite; CI round trip.

## Failure conditions
- A download without a verifiable digest.

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
How each digest is sourced.
