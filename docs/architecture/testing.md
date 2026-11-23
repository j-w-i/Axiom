# Test strategy

One gate: `scripts/check.sh`. It runs everything below that exists, locally
and in CI. A task is not Tested until it passes.

## Layers

| Layer | What it proves | Where | Runs without |
|-------|----------------|-------|--------------|
| Contract (shape) | Wire JSON shapes, error codes, schema freshness | `crates/protocol/tests/contract.rs`, `drafts.rs` | everything |
| Unit | One crate's logic | `crates/<name>/tests/`, `src/**` `#[cfg(test)]` | other crates' internals |
| RPC | Framing, dispatch, cancellation, backpressure | `crates/rpc/tests/` (in-process duplex streams) | a daemon |
| Daemon integration (behaviour contract) | Every behaviour in `docs/protocols/ipc.md`, against the real `axiomd` binary over stdio | `crates/axiomd/tests/` | a shell |
| Shell daemon client | The same lifecycle in the shell's Dart client, and every generated wire type round-tripped against what the real `axiomd` sends | `apps/desktop-flutter/test/daemon/{client,roundtrip}_test.dart` (`flutter test`) | a display |
| Shell unit and widget | Stores' logic (task output assembly, Git, agent events), the LSP editor layer, Markdown, controls' keyboard and semantics, fuzzy matching | `apps/desktop-flutter/test/{state,editor,chat,ui}/` (`flutter_test`) | a daemon (mostly), a display |
| Shell design guard | No literal colours outside `lib/ui/tokens.dart`, WCAG contrast of every text token on every surface in both themes, no Material or Cupertino import, golden images of the gallery in both themes (Linux) | `test/ui/tokens_test.dart`, `test/no_material_test.dart`, `test/ui/gallery_golden_test.dart` | — |
| End-to-end (Flutter) | The real app (`AxiomApp` and its state, as `main` builds them), the real daemon with a temporary data dir, real key events through the window's shortcuts: `shell`, `editor`, `lsp` (fake-lsp; the real servers where installed), `console`, `git`, `search`, `env` (loopback fixture), `env_rust` (the M2 exit scenario, needs `AXIOM_E2E_NETWORK=1`), `chat`, `agent` (the M3 exit scenario), `a11y` (named controls on every page in both themes, Tab never stuck) | `apps/desktop-flutter/integration_test/*_test.dart` through `apps/desktop-flutter/scripts/e2e.sh` (`scripts/check.sh e2e`) | a model (scripted), the developer's display |
| Frame budget | No frame over 16.7 ms (build and raster, `FrameTiming`) in the listed interactions, in a profile build at 125 % scale | `integration_test/{frames,editor_frames,chat_frames}_test.dart` through `apps/desktop-flutter/scripts/frames.sh` (private virtual KWin) | the developer's display |
| Packaged app | A `.deb` **or** `.rpm` unpacked to a prefix: the shell and `axiomd` in one directory and `/usr/bin/axiom`; the desktop entry named after the app id with `StartupWMClass` and `Icon`; the hicolor icons; run with no `--daemon` on a private X display, `--self-test` finds the sibling `axiomd` and prints `connected …`. A macOS `Axiom.app`: the shell and `axiomd` in `Contents/MacOS`, the bundle id, the icon, and `--self-test` against the bundled daemon | `scripts/verify-package.sh` (`scripts/check.sh package`) | a display |
| Provider contract (M2) | Every manifest under `providers/` | `crates/env/tests/provider_contract.rs` | network (except opt-in install tests) |
| Agent (M3) | Loop, tools, permissions, checkpoints with a scripted model | `crates/agent/tests/` | network, a real model |
| Inspection (M5) | Select → source → patch → verify on fixture apps with known element→source maps | `tests/fixtures/inspect-*`, `crates/inspect/tests/` | a real model |

## Rules

1. **Real processes over mocks at boundaries.** Integration tests spawn the
   real daemon; shell tests talk to the real daemon; the smoke and E2E tests
   drive the real app with real input. Mocks are for third parties we cannot
   run in CI (model APIs → `ScriptedProvider`).
   **Never a window on a developer's desktop:** every GUI/E2E/benchmark run on
   Linux goes through a private compositor: `apps/desktop-flutter/scripts/e2e.sh`
   (a private `kwin_wayland --virtual`, else `scripts/headless.sh`),
   `apps/desktop-flutter/scripts/frames.sh` (a private virtual KWin), or
   `scripts/headless.sh` directly (private Xvfb, Wayland removed, private XDG
   directories), which fails closed when no private display can be started.
2. **Fail closed.** A timeout is a failure, never a pass (lesson from E1).
   Waits are bounded and assert on the awaited condition.
3. **No sleeps as synchronisation.** Wait for an event or poll a condition
   with a deadline.
4. **Isolation.** Every test uses its own temp directories and
   `AXIOM_DATA_DIR`; nothing touches the user's profile; tests are
   parallel-safe.
   The shell keeps its own layout file (`shell-local.json`) in the data dir
   when it is given one (`--data-dir` or `AXIOM_DATA_DIR`), else in the
   per-user config dir; every end-to-end suite boots with a temporary data dir
   (`integration_test/support.dart`), and `headless.sh` runs also get a
   private `XDG_DATA_HOME` / `XDG_CACHE_HOME` / `XDG_CONFIG_HOME`, removed
   afterwards.
5. **Fixtures are tiny and committed** (`tests/fixtures/`, each described in
   its README). Inspection fixtures carry an `expected-map.json` of
   element → exact source location.
6. **Platform coverage is stated, not assumed.** CI runs the Rust workspace
   on Linux, macOS and Windows and the shell on Linux and macOS (there is no
   Windows desktop build; deferred by the owner). A report says which OS a
   test actually ran on; platform-gated tests (`#[cfg(unix)]`, Dart `skip:`)
   are listed in the handoff.
7. **Tests are part of the contract.** Changing an existing expectation in
   `contract.rs` or the daemon integration suite is a protocol change and
   follows `governance.md`.
8. **Flakiness is a defect.** New async/IO tests are looped (10–20 runs)
   before the task is reported Tested.

### End-to-end: how the Flutter suites run

- **Where.** On Linux, `e2e.sh` starts a private `kwin_wayland --virtual` (Wayland, the GPU) when
  it exists, else a private Xvfb through `scripts/headless.sh`; `AXIOM_E2E_DISPLAY=xvfb` forces
  Xvfb (CI does). On macOS the suites run in the session (CI). Each suite is one
  `flutter test integration_test/<suite> -d linux|macos`; with no arguments `e2e.sh` runs every
  `*_test.dart` except the frame benchmarks.
- **The daemon and its helpers.** `e2e.sh` builds `axiomd`, `fake-lsp`, `fake-acp` and `fake-mcp`,
  puts `target/debug` on `PATH` and passes `AXIOMD_UNDER_TEST`; a suite refuses to start
  without it.
- **Input.** Commands go through their real keys (the window's shortcuts). Text is put into the
  editor through the document's controller, because simulated key presses carry no text input
  on desktop; say so when a criterion is about typing itself.
- **Opt-in parts.** `env_rust_test.dart` installs a real Rust toolchain and runs only with
  `AXIOM_E2E_NETWORK=1` (CI on Ubuntu); `lsp_test.dart`'s real-server part skips a missing server
  unless `AXIOM_REQUIRE_LSP=1` (CI on Ubuntu).
- **Fail closed.** Every wait is `until(…)` with a deadline (`integration_test/support.dart`).

The Tauri shell's end-to-end machinery (two WebDriver servers, the embedded driver's release
guard, the script channels, the webview-crash check, the editor input-path assertion) retired
with it in FL-QA-001; this file at the tag `tauri-shell-final` describes it.

## What CI runs

| Job | OS | Added by |
|-----|----|----------|
| `rust`: fmt, clippy, tests, schema check (+ deps matrix, cargo-deny, task board) | Linux, macOS, Windows | exists; guards by M1-CI-001 |
| `flutter`: `scripts/check.sh flutter` (lockfile, wire types, format, analyze, tests, licences), a release build, end-to-end (Linux: Xvfb, real language servers required, the M2 install scenario), packages and their verification (deb + rpm / app), uploaded as artifacts | Linux, macOS | FL-ARCH-001, FL-QA-001 |
| `deny`: `cargo deny check` | Linux | M1-CI-001 |
| (retired) `editor`, `desktop` (Qt), `tauri` | — | removed with their shells (T-15, FL-QA-001) |

## Manual verification

Some things are not yet automatable (IME, drag and drop, screen readers,
real GPU rendering). They are listed as checklist items in
`agents/REVIEW_GUIDE.md` and recorded per milestone in
`docs/roadmap/verification/`. "Verified manually on <OS>" is acceptable
evidence when it says who looked at what.
