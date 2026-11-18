# Qt → Tauri: audit and target architecture

_Status: **evaluation** (Phase 1 audit + Phase 2 design). Nothing here is
accepted architecture until an ADR supersedes ADR-0001/0002/0004 — see
[decision.md](decision.md). Written 2026-11-12 against `main` at `b3e29c1`
(M1 complete except KD-WIN-1)._

Related: [feature-parity.md](feature-parity.md) ·
[performance.md](performance.md) · [design-system.md](design-system.md) ·
[prototype-plan.md](prototype-plan.md) ·
[implementation-roadmap.md](implementation-roadmap.md)

---

## 1. Audit: what the Qt frontend is responsible for today

Sources read: `AGENTS.md`, `docs/architecture/*`, `docs/protocols/{ipc,editor-bridge}.md`,
`docs/roadmap/{milestones,current}.md`, `docs/roadmap/verification/M1.md`,
ADR-0001…0011, every file in `apps/desktop/src`, `apps/desktop/qml`,
`apps/desktop/tests`, `editor/src`, and the public surface of `crates/*`.

### 1.1 Size of the thing being replaced

| Part | Files | Lines |
|------|-------|-------|
| `apps/desktop/src` (C++: daemon client, models, editor glue) | 25 | ≈ 3 900 |
| `apps/desktop/qml` (Axiom.Ui design system + Axiom.App views) | 24 | ≈ 2 050 |
| `apps/desktop/tests` (9 Qt Test executables, 115 test functions) | 10 | ≈ 2 870 |
| `editor/` (Monaco surface, bridge-1 page side, 43 Vitest tests) | 6 src + 2 tests | ≈ 650 + tests |

The daemon (`crates/*`, 227 Rust tests) is **not** part of the migration: it
is reused unchanged.

### 1.2 Qt-specific responsibilities

| # | Responsibility | Where (Qt) | Qt mechanism | Notes for migration |
|---|----------------|-----------|--------------|---------------------|
| R1 | Spawn/supervise `axiomd`, NDJSON framing, id correlation, timeouts + `$/cancel`, restart, bounded shutdown | `src/daemon/DaemonClient.*`, `LineBuffer.*` | `QProcess`, `QTimer`, `QJsonDocument` | Moves to Rust (`bridge/`), which is *closer* to the daemon's own `axiom-rpc` client |
| R2 | Locate the daemon (`--daemon`, `$AXIOMD`, sibling, `PATH`) | `Shell.cpp::locateDaemon` | `QStandardPaths` | Same order in `bridge::locate_daemon`; packaging uses Tauri `externalBin` |
| R3 | Connection state for the UI (Starting/Ready/Failed/Stopped, stderr tail) | `Backend.*` | `Q_PROPERTY` | `DaemonState` events |
| R4 | Workspace open/close/switch, deferred open during close, recent list, reopen-last | `WorkspaceController.*` | QObject + generation counter | Port to TS store; same call sequence |
| R5 | Lazy file tree, minimal diffs on `fs/didChange`, inline error rows, expanded-state restore | `FileTreeModel.*` | `QAbstractItemModel` + QML `TreeView` | Port to TS store + virtualised ARIA tree |
| R6 | Open documents, saved versions, conflict queue, unsaved-work prompts, external-change recheck, tab persistence, crash-snapshot cache | `OpenDocumentsModel.*` (770 lines — the most complex unit) | `QAbstractListModel`, `std::function` continuations | Port to TS store; the logic is protocol-level and toolkit-free |
| R7 | Editor hosting: custom scheme `axiom://editor/`, path-escape checks, navigation lock, renderer-crash detection and reload | `AssetSchemeHandler.*`, `EditorSetup.*`, `EditorArea.qml` | Qt WebEngine (Chromium 140), `QWebEngineUrlSchemeHandler` | Tauri serves bundled assets itself; Monaco moves *into* the app's webview (see §3.4) |
| R8 | Editor bridge 1 (9 signals, 7 slots) | `EditorBridge.*` + `editor/src/bridge.ts` | `QWebChannel` | Kept as an in-process TypeScript interface (`EditorPort`/`EditorListener`) |
| R9 | Task run/stop/history, output model (seq ordering, line split per stream, ANSI strip, CR handling, 10 000-line cap, batched publishing) | `TaskController.*`, `TaskOutputModel.*` | QObject, 16 ms `QTimer` batches | Port to TS; batching per animation frame |
| R10 | Settings: daemon is source of truth, no optimistic updates, rejected values reported | `SettingsStore.*` | `Q_PROPERTY` | Port to TS store |
| R11 | Window geometry persistence (per machine) | `Shell.cpp` | `QSettings` | Tauri host writes `window.json` in the app config dir |
| R12 | Design system: tokens, 18 controls, light/dark, reduced motion, accessibility rules | `qml/Axiom/Ui/*`, `docs/architecture/design-system.md` | QML singleton + Qt Quick Controls Basic | CSS custom properties + React components + Radix (see design-system.md) |
| R13 | Views: menu bar, explorer, tabs, editor area, welcome, output panel, status bar, banners, conflict/unsaved/settings/about dialogs | `qml/Axiom/App/*` | QML | React components |
| R14 | Application shortcuts not colliding with Monaco | `Main.qml`, `EditorArea.qml` | `Shortcut` with `ApplicationShortcut` | Capture-phase key handler; Monaco keeps focus-local keys |
| R15 | Native dialogs: Open Folder | `Main.qml` | `QtQuick.Dialogs.FolderDialog` (portal on Linux) | `tauri-plugin-dialog` (portal on Linux) |
| R16 | External links (http/https only) | `OpenDocumentsModel::bridgeOpenLink` | `QDesktopServices` | `open_external` host command with the same scheme check |
| R17 | Monospace font resolution | `Shell.cpp::resolveMonoFamily` | `QFontDatabase` | CSS font stack (same list) |
| R18 | Quit with unsaved work; flush explorer/tab state on close | `Main.qml onClosing`, `Shell::~Shell` | QQuickWindow `closing` | `CloseRequested` intercepted in Rust, decided by the UI |
| R19 | Tests: component tests against the real daemon (offscreen), GUI smoke with real WebEngine | `apps/desktop/tests/*` | Qt Test, `QT_QPA_PLATFORM=offscreen` | Rust bridge tests, Vitest store tests against the real daemon, WebDriver E2E |
| R20 | Build/packaging: CMake + aqtinstall Qt, `windeployqt`, MSVC on Windows | `CMakeLists.txt`, `scripts/bootstrap-qt.sh`, CI | CMake | Cargo + pnpm + Tauri bundler |
| R21 | Licensing: LGPL-3.0 Qt, dynamic linking, module allow-list | ADR-0011 | — | Tauri MIT/Apache; WebKitGTK LGPL-2.1 (system lib); WebView2 runtime (Microsoft licence) |

### 1.3 Platform integrations

| Integration | Qt today | Verified where (M1.md) |
|-------------|----------|------------------------|
| Display | Qt QPA (xcb/wayland, cocoa, windows) | Linux offscreen only; macOS/Windows build in CI |
| Web engine | Qt WebEngine = bundled Chromium 140, identical on all OSes | Linux; macOS partially (Ctrl+S chord fallback) |
| Folder dialog | QtQuick.Dialogs (native/portal) | not driven by real events |
| Clipboard, IME, DnD, HiDPI, screen readers | Qt + Chromium | **not exercised** |
| Process control | delegated to daemon (`axiom-exec`) | Linux; Windows partly (KD-WIN-1) |

### 1.4 State management

- **Daemon-owned (persisted)**: settings, recent workspaces, per-workspace
  state keys `explorer.expanded`, `editor.openPaths`, `editor.activePath`,
  `run.history`. Shell writes debounced 500 ms and on close.
- **Shell-owned (per machine)**: window geometry (`QSettings`). Panel sizes
  are *not* persisted by the Qt shell today (data-flow.md says they are — a
  documentation gap noted in feature-parity.md).
- **Editor-owned (memory)**: unsaved text, dirty tracking ids; shell keeps
  the latest `textSnapshot` for crash recovery.
- **Mechanics**: Qt properties + signals; generation counters invalidate late
  answers after workspace switches; continuations via `std::function`.

### 1.5 What does *not* depend on Qt (reusable as-is)

- All of `crates/*` and `axiomd` (protocol 0.1, frozen).
- `editor/src/documents.ts` (bridge-1 dirty tracking/saves/throttles),
  `editor/src/monaco-setup.ts` (workers, option mapping, command ids),
  `editor/src/bridge.ts` (contract types), `version.ts`.
- The behaviour specs: data-flow.md, ipc.md, editor-bridge.md.

---

## 2. Target architecture

```
 ┌──────────────── axiom (Tauri 2 app: one OS process + system webview processes) ────────────────┐
 │                                                                                               │
 │  Webview (WebKitGTK · WebView2 · WKWebView)                                                   │
 │  ┌──────────────────────────────────────────────────────────────────────────────────────┐    │
 │  │ React views ── stores (workspace, documents, tasks, settings) ── DaemonClient (TS)   │    │
 │  │                     │ EditorPort / EditorListener (= bridge 1 message set)            │    │
 │  │                 EditorSurface: Monaco + editor/src/documents.ts (shared with Qt)       │    │
 │  └───────────────────────────────┬──────────────────────────────────────────────────────┘    │
 │        7 commands (capability-gated) │ ▲ one ordered Channel<HostEvent>                        │
 │  Rust host (src-tauri) ─────────────▼─┴──────────────────────────────────────────────────    │
 │     axiom-desktop-bridge: spawn · NDJSON · id map · timeouts · $/cancel · restart · shutdown   │
 └────────────────────────────────────────────┬──────────────────────────────────────────────────┘
                                              │ JSON-RPC 2.0, NDJSON, stdio  (protocol 0.1, unchanged)
 ┌────────────────────────────────────────────▼──────────────────────────────────────────────────┐
 │ axiomd (unchanged)                                                                             │
 └────────────────────────────────────────────────────────────────────────────────────────────────┘
```

### 2.1 Invariants carried over unchanged

1. **Two processes we own**; the daemon owns *all* filesystem, workspace,
   process and task operations. The Tauri host exposes **no** file, process
   or shell command to the webview (governance rule 5). Verified by
   inspection of the command list (§2.3) and capability file.
2. **Protocol 0.1 unchanged.** The host forwards only registered 0.1 method
   names (`ui_callable`); `initialize` and `shutdown` are owned by the host.
3. **Editor bridge 1 semantics unchanged.** The message set survives as
   TypeScript interfaces; `editor/src/documents.ts` is literally the same
   code in both shells.
4. **Persisted state keys unchanged** — a user can switch between the Qt and
   Tauri builds against the same data directory (verified: both read the
   same `axiom.db`; see feature-parity.md row S-9).
5. **No business logic in the shell.** The stores contain presentation
   state and protocol sequencing only — the same scope as the Qt models.

### 2.2 What changes structurally

| Concern | Qt | Tauri | Consequence |
|---------|----|-------|-------------|
| Languages in the shell | C++ + QML + TS (editor) | Rust (≈ 1.1 k lines host+bridge) + TS/React | One fewer language; **ADR-0002 "no Rust in the shell" must be superseded** |
| Web engine | One bundled Chromium, same everywhere | System webview: WebKitGTK (Linux), WebView2/Chromium (Windows), WKWebView (macOS) | Smaller install; **three engines to test**; ADR-0001's rejection reason ("one known Chromium with CDP" for M5 preview) must be answered (§4) |
| Editor isolation | Monaco in a separate WebEngine page; renderer crash recoverable in-process | Monaco in the same document as the UI | A Monaco/page crash takes the UI with it; recovery = page reload (state is in daemon + snapshot cache in Rust — see §3.5) |
| JSON-RPC client | C++ `DaemonClient` | Rust `axiom-desktop-bridge` (tokio) | Same language as `axiom-rpc::Client`; could later *be* that client (§3.1) |
| Model layer | `QAbstractItemModel` | TS stores + `useSyncExternalStore` | Virtualisation must be explicit (done in the prototype) |
| Shortcuts | `Shortcut { context: ApplicationShortcut }` | capture-phase `keydown` handler | Equivalent; menu-bar accelerators are not native (see parity) |
| Menus | QML `MenuBar` (in-window, not native) | In-window Radix menus | Same (neither is a native macOS menu bar today) |
| Build | CMake + Qt SDK (≈ 4 GB) | Cargo + pnpm (+ WebKitGTK dev headers on Linux) | Simpler contributor setup on macOS/Windows; Linux needs distro `-dev` packages |

### 2.3 Host command surface (complete)

Pinned in `src-tauri/build.rs` (`AppManifest::commands`) and granted only to
window `main` in `capabilities/main.json`; `src-tauri/tests/surface.rs` fails
if the three lists disagree or if a command looks like it bypasses the daemon:

| Command | Purpose | Guard |
|---------|---------|-------|
| `host_subscribe(channel)` | Ordered event stream (state, notifications, responses, logs, close-requested) | replaces previous subscriber; replays buffered events |
| `daemon_request(key, method, params, timeoutMs?)` | Forward one protocol request; answer arrives on the channel with `key` | `ui_callable(method)`: registered 0.1 method, not `initialize`/`shutdown` |
| `daemon_cancel(key)` | `$/cancel` for a forwarded request | only keys of the current process generation |
| `daemon_restart()` | Replace the daemon process | — |
| `app_info()` | versions, data dir, uptime (About, benchmarks) | read-only |
| `open_external(url)` | System browser | `http`/`https` only (same rule as Qt) |
| `app_quit()` | Confirmed quit: save geometry, bounded daemon shutdown, exit | — |
| `host_heartbeat()` | Proves the page is alive (crash/hang detection, T-08) | called every 2 s by the UI |
| `snapshot_put(path, text)` | Keeps the latest unsaved text of a document in the host (memory only) | write-only; never touches the disk |
| `snapshot_clear(path)` | Forgets it (saved, reverted, or closed) | — |
| `recovery_state()` | Snapshots to restore after a webview reload, and whether this load *is* a recovery | returns memory only |

Plus one plugin permission: `dialog:allow-open` (folder picker). CSP:
`default-src 'self'`, `script-src 'self'`, `worker-src 'self' blob:`,
`connect-src ipc: http://ipc.localhost`, no remote origins. `freezePrototype`
is **off** because Monaco 0.57 assigns to built-in prototypes at load
(observed: `TypeError: Attempted to assign to readonly property` in
`editor.api`); recorded as a security trade-off in decision.md.

### 2.4 The stdio bridge (`apps/desktop-tauri/bridge`)

Requirements from the brief → how they are met → evidence:

| Requirement | Design | Evidence (`bridge/tests/bridge.rs`, real `axiomd`) |
|-------------|--------|----------------------------------------------------|
| Concurrent requests | per-process `HashMap<id, Waiter>`; writer task owns stdin; reader task owns stdout | `concurrent_requests_each_answered_once` (200 in flight) |
| Ordered delivery | responses for the UI are *events in the same stream* as notifications, emitted by the reader in wire order | `task_start_response_precedes_its_output_in_event_order` |
| Notifications | `BridgeEvent::Notification` verbatim | same test |
| Cancellation | `daemon_cancel(key)` → `$/cancel {id}`; exactly-once answer preserved | `cancel_of_a_forwarded_request_still_answers_once` |
| Timeouts | per-request timer; on expiry send `$/cancel`, answer `timeout` | `timeout_answers_with_timeout_and_sends_cancel` (SIGSTOPped daemon) |
| Process failure | waiter task reports exit/crash with stderr tail; all pending answered `internal: Backend stopped`; later requests answered immediately | `crash_fails_pending_and_restart_recovers` |
| Restart | new generation; old process gets EOF then kill after 2 s; late events from the old generation are ignored | same test |
| Graceful shutdown | `shutdown` (3 s) → close stdin → wait 2 s → kill; no `failed` state reported | `shutdown_is_bounded_and_the_process_exits` |
| Framing limits | 64 MiB line cap (fatal), `\r\n` accepted, blank lines ignored, malformed lines logged | code review; daemon-side tests cover the same codec |
| Missing binary | `failed` with the same message as Qt | `missing_program_fails_clearly` |
| Sync callers | owns a runtime handle; Tauri commands on the main thread can call it | `request_forward_from_a_thread_without_a_runtime` (regression for a crash found by the E2E run) |

12 tests, all passing on Linux; Windows/macOS unverified (they use
`kill`/`taskkill` and must run in CI).

**Back-pressure.** The UI channel is unbounded by design (as Qt's signal
queue is); output volume is bounded by the daemon's 8 KiB chunks and the UI
coalesces per frame. A 50 000-line task is in the benchmark suite.

### 2.5 UI state and rendering

- Stores are plain TypeScript classes (ports of the Qt models) exposing
  immutable snapshots to React via `useSyncExternalStore`. No state library.
- Lists that can be large (explorer, output) are virtualised with fixed row
  heights; DOM size is O(viewport), matching Qt `TreeView`/`ListView`.
- Monaco is code-split and loaded only when a workspace opens, keeping the
  welcome screen light.
- Generation counters invalidate late answers exactly as in Qt.

### 2.6 Security model deltas

| Topic | Qt | Tauri prototype | Assessment |
|-------|----|-----------------|------------|
| Page privileges | page sees only `host` with 7 slots | page sees 7 commands; **can send any 0.1 method** (incl. `fs/writeFile`, `task/start`) | Wider: in Qt the *editor page* could not write files; in Tauri the whole UI is the page. Mitigation: the UI is our bundled code under strict CSP with no remote content; Monaco is the only large third-party script. Option for production: run Monaco in an `<iframe>` with its own (narrower) capability — see roadmap T-11 |
| Remote content | none | none (CSP-enforced) | equal |
| Engine patching | Qt release cadence (Chromium 140 at Qt 6.12) | OS/vendor (WebKitGTK distro updates, Evergreen WebView2, Safari updates) | Tauri better on Windows/macOS; Linux depends on distro |
| Prototype hardening | — | `freezePrototype` disabled for Monaco | small regression vs. Tauri default; no Qt equivalent |

---

## 3. Design decisions and alternatives considered

### 3.1 Rust bridge vs. talking to `axiomd` from JS (`tauri-plugin-shell`)
Chosen: Rust bridge. JS-side spawning would require granting the webview
`shell:allow-spawn` (process creation from the UI — exactly what the
architecture forbids) and would put framing on the UI thread. The bridge is
deliberately close to `axiom-rpc::Client`; a follow-up can merge them so the
CLI (M3) and the GUI share one client implementation (roadmap T-02).

### 3.2 Channel vs. events
Chosen: one `tauri::ipc::Channel` per subscriber. Tauri documents events as
"not designed for low latency or high throughput"; channels are ordered and
are what Tauri itself uses for process output
([Calling the frontend](https://v2.tauri.app/develop/calling-frontend/)).

### 3.3 One webview vs. a separate editor webview/iframe
Chosen for the prototype: Monaco in the main document behind the bridge-1
interfaces. A separate webview per editor would restore crash isolation and
the narrower editor privilege but costs one more engine process and an IPC
hop per message; deferred to roadmap T-11 with a measurement gate.

### 3.4 Tailwind CSS, Radix, shadcn/ui — evaluated, not adopted wholesale
See design-system.md §4. Summary: **Radix primitives adopted** for dialogs and
menus (focus management, ARIA, typeahead are expensive to get right);
**Tailwind not adopted** (the token layer is CSS custom properties shared
with Monaco theme generation; utility classes would duplicate the token
vocabulary and weaken the "no literal values in views" guard);
**shadcn/ui not adopted as a dependency** (it is copy-in source over Radix +
Tailwind; useful as a reference for anatomy, not as a layer we track).

### 3.5 Crash recovery

Implemented in T-08 with two mechanisms:

1. **Dirty documents' text lives in the host** (`snapshot_put`), memory only,
   throttled by the editor surface exactly as before (≤ 1/s per document plus
   on every change; the same window the Qt shell had). When the page dies the
   text survives.
2. **Detection**: on Linux the engine's own signal
   (`WebKitWebView::web-process-terminated`) triggers an immediate reload.
   The other platforms use a heartbeat lease (`host_heartbeat` every 2 s, 30 s
   threshold, enforced only while the window is visible so a throttled
   background page is never mistaken for a dead one) until T-09/T-10 verify a
   native hook there.

After the reload the UI restores its tabs from the daemon's workspace state
and asks `recovery_state()` for the snapshots, re-opening those documents
**dirty** - the same outcome as the Qt shell's renderer-crash path, including
the version check on the next save (the current on-disk version is read
first, so a file changed while the window was dead produces a conflict rather
than a silent overwrite). The user is told: a banner says the window was
reloaded.

Qt recovers from a *renderer* crash without losing the shell. In Tauri the
UI and Monaco share one content process: on WebKitGTK a web-process crash
blanks the window. Prototype status: **not implemented** (parity gap P-12).
Design for production: keep the latest `textSnapshot`s in the Rust host
(not the page), detect web-process termination (wry `on_page_load` /
WebKitGTK `web-process-terminated`), reload the page, and replay snapshots
through the existing `ready` handshake. Roadmap T-08.

---

## 4. Open architectural questions (must be answered before acceptance)

1. **M5 preview engine.** ADR-0001 rejected system webviews partly because
   the M5 visual-inspection preview needs "one known Chromium with CDP".
   With Tauri, previews could (a) run in a *separate* Chromium (bundled
   headless shell or the user's Chrome) driven over CDP and rendered into the
   app by screenshot/stream, or (b) use WebView2's CDP on Windows only.
   Neither is prototyped. This is the largest unresolved risk.
2. **Engine divergence for Monaco.** Monaco officially targets evergreen
   browsers; WebKitGTK and WKWebView are WebKit. The prototype verified
   typing, Ctrl+S, find, colouring and themes on WebKitGTK 2.54.2 only.
3. **Accessibility** with Orca (WebKitGTK/AT-SPI), Narrator/NVDA (WebView2),
   VoiceOver (WKWebView) — not tested in either shell.
4. **Linux GPU paths** (DMABUF/NVIDIA issues documented by Tauri,
   [Linux graphics](https://v2.tauri.app/develop/debug/linux-graphics/)) —
   the prototype ran only on Xvfb with software rendering.
