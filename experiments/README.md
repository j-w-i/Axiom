# Experiments

Small, isolated proofs of concept that resolved architectural uncertainties
during Phase 0. **Nothing in the product may depend on this directory.** They
are kept because (a) their recorded results are evidence cited by ADRs and
(b) their code is a working reference for the first implementation tasks.
A passing experiment is not production readiness.

Environment for all runs: Fedora 44, x86_64, GCC 16.2, Rust 1.99.1, Node 22,
Qt from aqtinstall (`scripts/bootstrap-qt.sh`), 2026-11-11.
To rebuild: `source scripts/dev-env.sh` first (provides `axiom_cmake`).

## E1 — Qt shell + Monaco in WebEngine + Rust daemon

**Hypothesis.** A C++/QML shell can host Monaco in Qt WebEngine from a custom
secure URL scheme, bridge it with QWebChannel, and drive a Rust daemon over
newline-delimited JSON-RPC on stdio, well enough to open → edit → save a file
and run a command with streamed output.

**Setup.** `e1-qt-rust-monaco/`: `daemon/` (std-only Rust, 5 methods),
`editor/` (Monaco 0.57.0 + Vite 8, TS worker), `shell/` (C++/QML,
`--self-test` mode that scripts the whole flow and prints a JSON report).

```sh
cd experiments/e1-qt-rust-monaco
(cd daemon && cargo build --release)
(cd editor && pnpm install && pnpm exec vite build)
axiom_cmake -S shell -B shell/build && cmake --build shell/build
mkdir -p tmp && printf 'const answer: number = "forty-two";\nexport default answer;\n' > tmp/sample.ts
QT_QPA_PLATFORM=offscreen shell/build/e1_shell --self-test \
  --daemon daemon/target/release/e1-daemon --assets editor/dist --file tmp/sample.ts
```
(`qwebchannel.js` is served by the shell's scheme handler from Qt's own
resource `:/qtwebchannel/qwebchannel.js`; no copy is needed.)

**Observed** (`results.json`, `results-qt6.12.0.json`): all 12 checks pass,
exit 0, on Qt 6.11.2 and 6.12.0 (both report Chromium 140), with
`QT_QPA_PLATFORM=offscreen` and on a real Wayland session.

| Check | Result |
|-------|--------|
| Page loads from `axiom://editor/`, `isSecureContext` | true |
| Monaco web workers run under the custom scheme (TS diagnostic "Type 'string' is not assignable to type 'number'") | yes |
| Daemon `initialize`, `fs/readFile`, `fs/writeFile` round trip | yes |
| Host-initiated edit reaches Monaco and is saved to disk | yes |
| Synthetic Qt key events reach Monaco (typed text on disk) | yes |
| Ctrl+S handled by Monaco's keybinding → save through the bridge | yes |
| Command with exit code 3: stdout and stderr streamed separately | yes |
| Missing binary → structured `spawn_failed` error | yes |
| Time from editor ready to end of script | ≈ 1.0 s |
| Editor bundle size (unminified workers, all languages) | 14 MB |

**Lessons that became requirements.**
- Register the scheme (`SecureScheme | CorsEnabled | FetchApiAllowed`) before
  `QGuiApplication` is constructed and call `QtWebEngineQuick::initialize()`
  first.
- Monaco 0.57 worker entry points are `monaco-editor/editor/editor.worker`
  and `monaco-editor/language/typescript/ts.worker` (via the package
  `exports` map), imported with Vite's `?worker` suffix. The older
  `monaco-editor/esm/vs/...` paths do not resolve.
- Vite `base: "./"` so asset URLs are relative.
- A stale `dist/` from a failed build still loads and fails at runtime; the
  self-test originally reported success on timeout. Fixed: a timeout is a
  failure. Smoke tests must fail closed.
- Building against aqt's Qt on a machine without OpenGL development headers
  fails at `find_package(Qt6Gui)`; `scripts/bootstrap-qt.sh --gl-headers`
  works around it without root on Fedora.

**Not covered.** macOS, Windows; IME, clipboard, drag and drop,
accessibility; memory footprint; packaging; renderer crash recovery.

## E2 — QML runtime tree with source locations

**Hypothesis.** An in-process probe can enumerate a Qt Quick scene with
geometry and map each item to the QML file, line and column that
instantiated it.

**Setup.** `e2-qml-probe/`: a window with a `Repeater` of two `BalanceCard`
components; `main.cpp` walks `QQuickItem::childItems()`, hit-tests a point
and prints the chain.

```sh
cd experiments/e2-qml-probe
axiom_cmake -S . -B build && cmake --build build
QT_QPA_PLATFORM=offscreen build/e2_probe
```

**Observed** (`results.json`, `results-qt6.12.0.json`), identical on Qt
6.11.2 and 6.12.0. Hit test at (72, 204):

| Item | Source reported | Actual source |
|------|-----------------|---------------|
| `QQuickText` | `BalanceCard.qml:13:9` | `Text { anchors.centerIn… }` at 13:9 ✔ |
| `QQuickRectangle` `transferButton` | `BalanceCard.qml:10:5` | `Rectangle {` at 10:5 ✔ |
| `BalanceCard` instance | `Main.qml:10:13` | `BalanceCard {` at 10:13 ✔ |
| `QQuickColumn` | `Main.qml:6:5` | `Column {` at 6:5 ✔ |

Scene bounds via `mapRectToScene`, screenshot via `QQuickWindow::grabWindow`.

**Limits.** Line and column come from **private** API (`QQmlData`,
`private/qqmldata_p.h`; needs `Qt6::QmlPrivate`, `Qt6::QuickPrivate`): the
probe must be compiled per Qt minor version. The public
`qmlContext(item)->baseUrl()` yields the file only. Not tested: injection
into an unmodified app, Qt Quick Controls styles (many internal items from
Qt's own QML files → `generated: true`), `Loader`/delegates created from
C++, Qt Widgets.

## E3 — Web element → source → patch → verified change

**Hypothesis.** For a React + Vite app, a click coordinate can be resolved to
the exact source location of the element without relying on React
internals, and a source edit can be verified after hot reload by geometry
and pixel comparison.

**Setup.** `e3-web-source-map/`: React 19.3 + Vite 8.3 fixture (two
`BalanceCard`s), `axiom-source-plugin.mjs` (a `pre` transform stamping host
JSX elements with `data-axiom-src="file:line:column"`), `driver.mjs`
(headless Chrome at DPR 2 through the DevTools Protocol).

```sh
cd experiments/e3-web-source-map && pnpm install && node driver.mjs
```

**Observed** (`results.json`), exit 0:

| Step | Result |
|------|--------|
| Capture | 800×513 CSS px viewport, DPR 2 → 1600×1026 px screenshot |
| Select | `DOM.getNodeForLocation(71,268)` → `BUTTON`, 1 ms |
| Resolve | `src/components/BalanceCard.jsx:6:7`; text at that location starts with `<button` (exact) |
| React internals | `fiber._debugSource` is `undefined` on React 19; `_debugOwner` still names the owner component (`BalanceCard`, `App`) |
| Ancestor chain | button → section (`BalanceCard.jsx:3:5`, instance 2 of 2) → main (`App.jsx:10:5`) |
| Repeated component | 2 live elements share the button's source location |
| Patch + HMR | `marginLeft: 0 → 24`; new geometry visible after ≈ 100 ms |
| Geometry check | button x: 32 → 56, delta 24 = intent |
| Pixel diff | 3 220 of 1 641 600 px changed; bounding box covers both cards only |

**Limits.** Chrome, not Qt WebEngine (CDP against WebEngine is the first M5
task). Vite + plain JSX only: no TypeScript-specific syntax beyond the parser
flag, no Next/webpack/SWC pipeline, no server components, portals, or
component libraries that render host elements on the app's behalf (those
elements carry the *library's* absence of stamps, so they resolve to the
nearest stamped ancestor). Stamps are plain attributes: visible in the DOM
and present only in dev builds.

## E4 — Preview rendering + CDP inspection without Qt WebEngine

**Hypothesis.** M5's preview can render a running UI into the Tauri app while
being inspected over CDP, using an external Chromium captured frame by frame
rather than a Qt WebEngine page.

**Setup.** `e4-preview-cdp/`: one dependency-free Node 22 script. Headless
Google Chrome 155.0.8059.84 with `--remote-debugging-port=0` and a temp
profile, a fixture page served over loopback, `Page.captureScreenshot` for the
frames, `DOM.getBoxModel` + `Runtime.evaluate` for the element → source lookup.

```sh
node experiments/e4-preview-cdp/probe.mjs      # writes results.json (~5 s)
```

**Observed.** First painted frame 148–150 ms after navigation; 60 PNG captures
at p50 33.9 ms / p95 50.7–51.2 ms (≈20–29 fps); JPEG p50 34.4 ms; browser group
318 MiB PSS in 12 processes; the stamped element's `data-source` read back over
CDP in both modes (`src/App.tsx:42:7`, nested `src/Button.tsx:11:3`).

**Decision.** Adopt **option (a)** as the preview mechanism to prototype
against: it meets its pre-stated criterion (< 1 s first frame, < 100 ms p95
capture, element lookup reachable). Option (b), WebView2 CDP, is **not run — no
Windows host** and is not estimated. Open risks, all recorded in the
experiment's README: the ~318 MiB / 12-process browser group against a 250 MiB
shell; bundled-browser licensing (this used installed Chrome, which cannot be
redistributed, so the product needs a redistributable Chromium/CEF/WebView2);
Linux display stack under software rendering; and the unmeasured cost of
decoding and drawing each frame inside the Tauri window.
