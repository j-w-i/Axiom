---
id: M5-VERIFY-001
title: Verification: quiescence, re-identification, checks, verdict; ui/verify
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [M5-INSPECT-001]
order: 605
scope: crates/inspect/, crates/axiomd/
---

# M5-VERIFY-001: Verification

## Objective
`ui/verify` evaluates the stated checks after a change (inspection.md §Verification, ADR-0019 §1):
waits for quiescence (no `didMutate` for 150 ms, at most 2 s), re-identifies the node (stamp, then
path and text), evaluates `geometry` (relations within 0.5 px) and `property` checks, adds
`buildClean` / `runtimeClean`, reports pixel checks `skipped`, and gives the verdict (`failed` if any
check fails; `verified` if none fails and a geometry or property check passed; else `inconclusive`).

## Acceptance criteria
Unit tests per check kind and verdict rule; a fake-adapter test where a true geometry check is
`verified`, a false one `failed`, and a node that cannot be re-identified `inconclusive`.
