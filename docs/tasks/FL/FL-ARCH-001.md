---
id: FL-ARCH-001
title: Flutter shell: docs, module edges, toolchain in dev-setup, CI and check.sh
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: []
order: 520
scope: docs/architecture/, docs/contributing/dev-setup.md, docs/roadmap/, .github/workflows/ci.yml, scripts/check.sh, scripts/check_dart_licences.py, apps/desktop-flutter/ (empty app, analysis options)
---

# FL-ARCH-001: Flutter shell: docs, module edges, toolchain in dev-setup, CI and check.sh

## Objective
ADR-0020 is reflected everywhere a newcomer looks: `overview.md` and `modules.md` describe
`apps/desktop-flutter` (the shell, its edges: the daemon over stdio only); `dev-setup.md` installs
Flutter stable and the Linux build packages (or the sysroot); `scripts/check.sh flutter` runs
`flutter analyze`, the tests and the licence check; CI has Flutter jobs on Ubuntu and macOS. The
app is created with `flutter create --platforms=linux,macos` and trimmed to an empty window.

## Acceptance criteria
1. `scripts/check.sh flutter` passes on an empty app; CI's Flutter jobs are green.
2. The licence check lists every Dart package (direct and transitive) and fails on one outside
   MIT/BSD/Apache-2.0/Zlib.
3. milestones.md has the Flutter milestone between M4 and M5; current.md says so.

## Tests
`scripts/check.sh flutter`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
