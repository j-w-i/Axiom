---
id: AUD-REM-001
title: Remediation of the comprehensive audit of 751ffc2 (all 112 issues, in batches)
status: implemented
status_note: 2026-11-23; all 112 issues addressed (ISSUES.md §Remediation status); gate, 17 e2e suites, frames and perf pass on Linux; awaits review by another session
depends_on: []
order: 710
scope: everything the issues name (audits/comprehensive-audit/ISSUES.md); no new frameworks
---

# AUD-REM-001: Remediation of the comprehensive audit

## Objective
Fix every issue in `audits/comprehensive-audit/ISSUES.md` following
`audits/comprehensive-audit/REMEDIATION_PLAN.md`, batch by batch, each batch committed with its
tests. Issue statuses and the implementation log live in `ISSUES.md` and
`audits/comprehensive-audit/AUDIT_STATE.md`.

## Acceptance criteria
1. Every issue is Fixed (with the verification named in its entry) or explicitly deferred by
   the owner, with the reason.
2. `scripts/check.sh`, `scripts/check.sh e2e` (including the new `layout_test.dart` and
   `data_safety_test.dart`) pass; `frames.sh` and `scripts/perf.sh` are re-measured.
3. Protocol changes are additive and documented (`docs/protocols/ipc.md`, a change request in
   `docs/decisions/proposed/`).

## Tests
`scripts/check.sh`, `scripts/check.sh e2e`, `scripts/perf.sh`, `apps/desktop-flutter/scripts/frames.sh`.

## Documentation updates
`audits/comprehensive-audit/*`, `docs/roadmap/current.md`, the docs each issue names.
