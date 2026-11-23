---
id: FL-CORE-001
title: Dart daemon client: spawn and supervise axiomd, NDJSON JSON-RPC, generated wire types
status: implemented
status_note: 2026-11-23; audited (audits/comprehensive-audit), findings fixed in AUD-REM-001; awaits review by another session (REVIEW_GUIDE)
depends_on: [FL-ARCH-001]
order: 521
scope: apps/desktop-flutter/lib/daemon/, apps/desktop-flutter/tool/gen_types.dart, apps/desktop-flutter/test/
---

# FL-CORE-001: Dart daemon client: spawn and supervise axiomd, NDJSON JSON-RPC, generated wire types

## Objective
A Dart client with the desktop bridge's behaviour (crates/desktop-bridge): locate the daemon
(`--daemon`, the sibling, PATH), spawn it with `--data-dir`, NDJSON framing, requests with ids,
notifications, `$/cancel`, the initialize handshake (0.1–0.4, client capabilities), stderr to the
log, crash detection and a restart policy, a clean shutdown. Dart types generated from `schemas/`
by `tool/gen_types.dart` and checked fresh in check.sh.

## Acceptance criteria
1. Tests against the real `axiomd`: handshake, a request, a notification, cancel, a daemon crash
   and restart, shutdown leaves no process.
2. Generated types round-trip every schema's examples.

## Tests
`flutter test test/daemon/`.

## Documentation updates
`docs/roadmap/current.md`; the handoff.

## Handoff
What was verified, on which OS; the frame timings where the task has a budget; deviations.
