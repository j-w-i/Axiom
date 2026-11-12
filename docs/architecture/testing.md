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
| Packaged app (Tauri) | A `.deb` **or** `.rpm` unpacked to a prefix and run as an installed copy: it finds its sidecar daemon with no `--daemon`, the initialize handshake is observed on the wire through `bench/daemon_tap.py`, and its window is on the private display | `apps/desktop-tauri/scripts/verify-package.sh` (7 checks, `scripts/check.sh package`) | a display |
| End-to-end parity (Tauri) | The real menus (Edit undo/redo/find/replace/select all, Help ▸ About) and task shortcuts (F5, Shift+F5, Ctrl/Cmd+Shift+R) driven through the real window | `apps/desktop-tauri/e2e/parity.mjs` (10 checks) | a display, a model |
| End-to-end (Tauri) | The real Tauri app, real webview, real daemon, real key events, driven over WebDriver: open, edit, save, conflicts, commands, themes, palette, restore, quit with unsaved work, geometry, webview-crash recovery | `apps/desktop-tauri/e2e/{smoke,quit,geometry,parity}.mjs` through `e2e/runner.mjs`; two WebDriver servers, see below | a model; macOS crash kill (see below) |
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

### End-to-end: two WebDriver servers, and what each proves

| Server | Where | Key events | How it is selected |
|--------|-------|-----------|--------------------|
| `external`: tauri-driver → WebKitWebDriver | Linux | **trusted** (WebKit's own automation) | default on Linux |
| `embedded`: `tauri-plugin-wdio-webdriver`, compiled into the app under the `e2e` Cargo feature, 127.0.0.1 only | Linux, Windows, macOS | synthetic (`isTrusted === false`), dispatched by script | default on Windows and macOS |

Linux runs the suites through **both** (`scripts/e2e.sh --driver all`, also in
CI), so the embedded path is compared with the trusted one on every change.
Check 4b reports which kind of input it saw; on the embedded server the
trusted-input claim is reported **N/A**, never as a pass. The input fidelity
of a server is measured, not assumed: `e2e/input-probe.mjs` types every
printable ASCII character into Monaco, saves, and compares the bytes on disk.
(That probe found that `tauri-plugin-wdio-webdriver` 1.5.1 sent `keyCode` =
character code, so `p` arrived as F1 and opened Monaco's command palette
mid-typing; fixed upstream in 2.0.0-next.0, which is what the app pins.)

**One editor input path.** `src/editor/surface.ts` sets `editContext: false`,
so Monaco types through its classic textarea on all three engines. Monaco
0.53+ would otherwise use Chromium's EditContext API wherever it exists —
only WebView2 on Windows has it — which makes the Windows build take a
different path for every keystroke than Linux and macOS, leaves its typing
undrivable by scripted input, and exposes it to Chromium regressions that
wedge that path mid-session (stablyai/orca#7747). Smoke check 4a' asserts the
textarea path on every OS, so losing the option fails the build.

**Script channel.** Suites run page scripts through W3C Execute Script,
except on macOS with the embedded server: there the plugin's Execute Script
awaits one `evaluateJavaScript` completion, which WKWebView on macOS 26.4
runners intermittently reclaims (the call then hangs to its timeout; seen in
CI run 43135274089, upstream #540). macOS uses the plugin's `/wdio/eval`
instead, its out-of-band channel with bounded re-dispatch and the one
@wdio/tauri-service uses. `e2e/eval-contract.mjs` proves both channels return
the same values and errors for every shape the suites use, and runs before
the suites wherever the embedded server is used.

The crash check (11) kills the webview's content process. Linux and Windows
find it as a descendant of the app (`e2e/processes.mjs`, unit-tested for all
three OSes in `tests/processes.test.ts`). On macOS WebContent is a
launchd-owned XPC service, not a descendant, and cannot be told apart from
another app's: the check is reported **N/A** there rather than killing an
arbitrary process. The macOS host reacts to WebKit's own termination callback
(`on_web_content_process_terminate`), the same recovery path as Linux.

The embedded server must never ship: `src-tauri/tests/release_guard.rs` fails
if a default build depends on it, and `scripts/bundle.sh` refuses to package a
binary that contains it.

## What CI runs

| Job | OS | Added by |
|-----|----|----------|
| `rust`: fmt, clippy, tests, schema check (+ deps matrix, cargo-deny, task board) | Linux, macOS, Windows | exists; guards by M1-CI-001 |
| `editor`: typecheck, build, unit tests | Linux | M1-EDITOR-001 |
| `desktop`: build + Qt tests + GUI smoke, offscreen | Linux, macOS, Windows | M1-CI-002 |
| `tauri`: typecheck, bundle, UI-state tests (real daemon), token + process-lookup tests, host clippy (with and without `e2e`) and tests incl. the release guard, end-to-end (Linux: both WebDriver servers; Windows, macOS: embedded), packages (deb+rpm / nsis / app) | Linux, macOS, Windows | M1.5 T-09, T-13 |

## Manual verification

Some things are not yet automatable (IME, drag and drop, screen readers,
real GPU rendering). They are listed as checklist items in
`agents/REVIEW_GUIDE.md` and recorded per milestone in
`docs/roadmap/verification/`. "Verified manually on <OS>" is acceptable
evidence when it says who looked at what.
