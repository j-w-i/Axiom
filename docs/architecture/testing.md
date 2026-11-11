# Test strategy

One gate: `scripts/check.sh`. It runs everything below that exists, locally
and in CI. A task is not Tested until it passes.

## Layers

| Layer | What it proves | Where | Runs without |
|-------|----------------|-------|--------------|
| Contract (shape) | Wire JSON shapes, error codes, schema freshness | `crates/protocol/tests/contract.rs`, `drafts.rs` | everything |
| Unit | One crate's logic | `crates/<name>/tests/`, `src/**` `#[cfg(test)]` | other crates' internals |
| RPC | Framing, dispatch, cancellation, backpressure | `crates/rpc/tests/` (in-process duplex streams) | a daemon |
| Daemon integration (behaviour contract) | Every behaviour in `docs/protocols/ipc.md`, against the real `axiomd` binary over stdio | `crates/axiomd/tests/` | Qt |
| Editor unit | Bridge logic (dirty tracking, throttling) against a fake host | `editor/` (Vitest) | Qt, a browser |
| Bridge client | Process supervision, framing, correlation, cancel, timeout, crash/restart, bounded shutdown against the real `axiomd` | `crates/desktop-bridge/tests/bridge.rs` | a webview, a display |
| UI state | The shell's stores (workspace, documents, tasks, settings) against the real `axiomd`, no webview | `apps/desktop-tauri/tests/*.test.ts` (Vitest + a Node stdio transport) | a webview, a display |
| Web design guard | No literal colours, both themes define every token, WCAG AA contrast | `apps/desktop-tauri/tests/tokens.test.ts` | — |
| Shell component (Qt) | C++ models and `DaemonClient` against the real daemon | `apps/desktop/tests/tst_*.cpp` (Qt Test, offscreen) | a display |
| GUI smoke (Qt) | The real window, real editor bundle, real daemon: open → type → save → run | `apps/desktop/tests/tst_smoke_*.cpp` (offscreen) | a display |
| End-to-end (Tauri) | The real Tauri app, real webview, real daemon, real key events, driven over WebDriver | `apps/desktop-tauri/e2e/smoke.mjs` (`tauri-driver`) | Linux/Windows: WebDriver; macOS: embedded driver |
| Provider contract (M2) | Every manifest under `providers/` | `crates/env/tests/provider_contract.rs` | network (except opt-in install tests) |
| Agent (M3) | Loop, tools, permissions, checkpoints with a scripted model | `crates/agent/tests/` | network, a real model |
| Inspection (M5) | Select → source → patch → verify on fixture apps with known element→source maps | `tests/fixtures/inspect-*`, `crates/inspect/tests/` | a real model |

## Rules

1. **Real processes over mocks at boundaries.** Integration tests spawn the
   real daemon; shell tests talk to the real daemon; the smoke and E2E tests
   drive the real app with real input. Mocks are for third parties we cannot
   run in CI (model APIs → `ScriptedProvider`).
   **Never a window on a developer's desktop:** every GUI/E2E/benchmark run on
   Linux must go through `apps/desktop-tauri/scripts/headless.sh` (private
   Xvfb, Wayland removed), which refuses to run otherwise.
2. **Fail closed.** A timeout is a failure, never a pass (lesson from E1).
   Waits are bounded and assert on the awaited condition.
3. **No sleeps as synchronisation.** Wait for an event or poll a condition
   with a deadline.
4. **Isolation.** Every test uses its own temp directories and
   `AXIOM_DATA_DIR`; nothing touches the user's profile; tests are
   parallel-safe.
5. **Fixtures are tiny and committed** (`tests/fixtures/`, each described in
   its README). Inspection fixtures carry an `expected-map.json` of
   element → exact source location.
6. **Platform coverage is stated, not assumed.** CI runs Linux, macOS and
   Windows. A report says which OS a test actually ran on; platform-gated
   tests (`#[cfg(unix)]`, `QSKIP`) are listed in the handoff.
7. **Tests are part of the contract.** Changing an existing expectation in
   `contract.rs` or the daemon integration suite is a protocol change and
   follows `governance.md`.
8. **Flakiness is a defect.** New async/IO tests are looped (10–20 runs)
   before the task is reported Tested.

## What CI runs

| Job | OS | Added by |
|-----|----|----------|
| `rust`: fmt, clippy, tests, schema check (+ deps matrix, cargo-deny, task board) | Linux, macOS, Windows | exists; guards by M1-CI-001 |
| `editor`: typecheck, build, unit tests | Linux | M1-EDITOR-001 |
| `desktop`: build + Qt tests + GUI smoke, offscreen | Linux, macOS, Windows | M1-CI-002 |
| `tauri`: bridge tests + UI-state tests + token guard + E2E, headless | Linux, macOS, Windows | M1.5 T-09 |

## Manual verification

Some things are not yet automatable (IME, drag and drop, screen readers,
real GPU rendering). They are listed as checklist items in
`agents/REVIEW_GUIDE.md` and recorded per milestone in
`docs/roadmap/verification/`. "Verified manually on <OS>" is acceptable
evidence when it says who looked at what.
