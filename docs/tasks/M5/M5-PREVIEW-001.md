---
id: M5-PREVIEW-001
title: preview/*: the dev server with the plugin injected, URL detection, preview state
status: planned
status_note:
depends_on: [M5-INSPECT-001, M5-WEB-001]
order: 604
scope: crates/axiomd/, crates/inspect/, tests/fixtures/inspect-react/
---

# M5-PREVIEW-001: Previews

## Objective
`preview/start` runs the workspace's Vite dev server (provider `dev` task, else `npm run dev`) in its
environment with a generated config that imports the user's config and appends the plugin; detects the
URL in the output; reports `preview/state` (`starting` → `ready` with the adapter level after `hello`,
or `failed` with the output); `preview/stop|list`. The fixture `tests/fixtures/inspect-react/` (React +
Vite, an uninstrumented local "library" component, `expected-map.json`).

## Acceptance criteria
1. Daemon test with the fixture: start → ready (URL, level 3 after the adapter connects through a
   headless DOM? or the e2e) → stop leaves no process.
2. The user's `vite.config` is not modified.

## Tests
`cargo test -p axiomd --test preview` (needs `pnpm install` in the fixture; CI does it).
