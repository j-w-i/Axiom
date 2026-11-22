# Visual inspection protocol

**State: frozen for M5** (M5-ARCH-001, ADR-0019), validated by experiments E2 (QML) and E3
(React/DOM). Types: `crates/protocol/src/inspection.rs`.
Schema: `schemas/inspection/inspection.schema.json`. Research:
`docs/research/visual-inspection.md`. Decision: ADR-0008.

## Roles

```
previewed app ── adapter ──(inspect/* JSON-RPC)── daemon `inspect` ──(IPC)── shell / agent tools
```

- An **adapter** lives with the running app (injected script, in-process
  probe, or an external process using a debug protocol) and answers
  `inspect/*`. It knows one framework.
- The **daemon** owns sessions, normalises and enriches results (workspace
  path mapping, source-map resolution, heuristic candidates), stores
  snapshots and runs verification.
- Clients (the Visual Inspector view, agent tools `ui.*`) only see the
  normalised model, through the client methods `ui/*` and `preview/*` (`ipc.md` §Protocol 0.5).

Adapter transport: the daemon is the client. The adapter connects *to* the
daemon on a loopback WebSocket (web) or local socket (native) whose address
and one-time token the daemon passes when launching the preview
(`AXIOM_INSPECT_ENDPOINT`, `AXIOM_INSPECT_TOKEN`); messages are the same
JSON-RPC envelopes as `ipc.md`, one per WebSocket text frame or NDJSON line.

## Inspection levels are capabilities, not modes

`Capabilities` (sent in the `inspect/hello` result) states what the adapter
can do. The three levels of the brief are derived:

| Level | Requires | Without it |
|-------|----------|-----------|
| 1 Screenshot | a capture source (adapter `screenshot`, or the preview host: `QQuickItem::grabToImage`, a native web-view snapshot) | no pixel checks (they are `skipped`); the web adapter in M5 has none |
| 2 Runtime tree | `tree: true` | selection is a rectangle; source candidates come only from `heuristic`/`visualInference` |
| 3 Source-aware | `source != none` | tree works; each node's `sources` is empty until the daemon adds heuristic candidates |

Consumers must handle every capability being absent. Nothing in the client
may assume a framework.

## Coordinates

All rectangles are **logical pixels of their surface**, top-left origin,
after transforms and scroll, i.e. where pixels actually are. A screenshot of
a surface has size `(width, height) × devicePixelRatio`; E3 measured this
(800×513 CSS px at DPR 2 → 1600×1026 px image). Nested surfaces (iframes,
embedded views) are separate `Surface`s with `hostNode` pointing at the
element that contains them; a client converts to parent coordinates by adding
the host node's bounds origin. Multiple windows are multiple surfaces.

Rotated or skewed nodes report their axis-aligned bounding box. If a
consumer needs the exact quad it asks `inspect/nodeDetails` and reads the
framework-specific property (`dom.quads`, `qml.transform`).

## Node identity

- `NodeId`: unique and stable for the life of the node within one adapter
  connection. Dies on reload/HMR replacement.
- `stableKey`: meant to find "the same element" after a rebuild. Adapters
  build it as `"<file>:<line>:<column>#<instanceIndex>"` from the primary
  `Instantiation` source when they have one; otherwise from the developer
  name (`name`) plus the path of type names from the root. It is absent if
  neither exists.
- Re-identification after a change (daemon, M5-INSPECT-006):
  1. exact `stableKey` match → found;
  2. the patch moved lines: translate the old location through the applied
     diff's line map, retry;
  3. else match by (`name` or `text`) + `typeName` + ancestor type path;
     exactly one match → found, otherwise **not re-identified**.
  A failed re-identification makes the verdict `inconclusive`, never
  `verified`. The daemon never silently verifies a different element.

## Source candidates

`UiNode.sources` is an ordered list; each entry states **what kind of
relation** it is and **how it was obtained**:

| `method` | Meaning | Max confidence | Example |
|----------|---------|----------------|---------|
| `buildInstrumentation` | stamped by Axiom's build plugin | 1.0 | E3: `data-axiom-src` |
| `runtimeMetadata` | recorded by the framework | 1.0 | E2: `QQmlData` line/column; Flutter creation locations |
| `sourceMap` | generated position mapped back | 0.9 | CSS rule → `.scss` line |
| `heuristic` | workspace search by name/text/structure | 0.6 | grep for `objectName`, visible text |
| `visualInference` | a model's guess from pixels | 0.4 | screenshot-only apps |

`confidence` is an ordinal claim, not a calibrated probability; the ceilings
are enforced (`MappingMethod::max_confidence`, tested). Rules:
- A candidate with `generated: true` (inside `node_modules`, build output,
  Qt's own QML modules) is never auto-selected as an edit target.
- Auto-patching without asking the user requires the top candidate to have
  `method` ∈ {`buildInstrumentation`, `runtimeMetadata`} and no second
  candidate with the same relation and confidence.
- Repeated components: all instances share a source; `instance` says which
  one was clicked. The context given to the agent states "this element is
  rendered N times from this location" so it does not special-case one item
  by accident.
- A component boundary yields two useful locations: where the element is
  written (`instantiation`) and who rendered it (`owner`, the nearest
  ancestor with a different file or component). E3's chain shows both:
  `<button>` at `BalanceCard.jsx:6:7`, card instantiated at `App.jsx`.

## Adapter methods

| Method | Notes |
|--------|-------|
| `inspect/hello` | token check; returns `Capabilities` |
| `inspect/snapshot` | flat node list, parents first; `maxNodes` default 5000; sets `truncated` instead of failing |
| `inspect/hitTest` | returns the hit node and its ancestors; `not_found` if empty space |
| `inspect/nodeDetails` | adds `properties` (framework vocabulary, namespaced keys: `css.margin-left`, `qml.anchors.leftMargin`) |
| `inspect/highlight` | overlay on/off |
| `inspect/didMutate` (notification) | snapshots are stale; reasons `tree`, `layout`, `navigation`, `hotReload` |
| `inspect/setMode` | inspect mode on or off (hover highlight, click to select) |
| `inspect/hovered`, `inspect/selected` (notifications) | in inspect mode: the element under the pointer; the clicked element with its ancestors |
| `inspect/runtimeError` (notification) | an uncaught or console error in the page, for `runtimeClean` |

Timeouts (daemon-side): `hello` 5 s, `snapshot` 10 s, others 3 s. On timeout
the session is marked degraded and the Inspector shows the last snapshot
labelled stale. Cancellation uses `$/cancel`.

## Web adapter (first implementation, M5)

Two cooperating parts; see ADR-0008 for why both.

1. **Build plugin** `@axiom/vite-plugin-source` (prototype:
   `experiments/e3-web-source-map/axiom-source-plugin.mjs`). Dev-server only.
   A `pre` transform stamps every *host* JSX element with
   `data-axiom-src="<workspace-relative file>:<line>:<column>"`. It does not
   depend on React internals: E3 confirmed React 19 no longer provides
   `fiber._debugSource`, and stamping before the JSX transform works
   regardless of React version or JSX compiler. Component (capitalised)
   elements are not stamped because they may not forward unknown props; their
   location is recovered as the `owner` relation from the nearest stamped
   ancestor in a different file, plus the fiber's owner component name. The daemon applies the
   same rule in snapshots, in hit-test chains, and in `ui/nodeDetails`, which walks up the
   node's parents (at most 32) before falling back to a text search.
2. **Runtime adapter** (`inspectors/web/adapter.js`, ADR-0019 §1): a plain DOM script the
   plugin injects into the page (`transformIndexHtml`) with the daemon's endpoint and the
   preview's one-time token. It dials the daemon's loopback WebSocket and answers the methods
   above with DOM APIs (`elementsFromPoint`, `getBoundingClientRect`, `getComputedStyle`), draws
   the highlight as an overlay element, reports `didMutate` from a `MutationObserver` and Vite's
   HMR events, and reports uncaught and console errors (`inspect/runtimeError`). No CDP: the
   page runs in whatever browser the person uses (ADR-0020 §8), so the adapter works the same in
   all of them. It reports `screenshot: false`: there is no pixel capture in M5.

   **Inspect mode** (`inspect/setMode`): with it on, the adapter takes the pointer: hover
   highlights the element under it (`inspect/hovered`), and a click selects it
   (`inspect/selected`, the node and its ancestors) and is not passed to the app.

Without the build plugin the web adapter still gives Level 2, and the daemon
adds `sourceMap` candidates for CSS rules and `heuristic` candidates for
elements.

## QML adapter (second implementation, M6)

In-process probe library loaded into the target app (`QML_IMPORT_PATH`
plugin or `LD_PRELOAD`/`DYLD_INSERT_LIBRARIES`/injected DLL for unmodified
apps; linking it explicitly is the supported path). Walks `QQuickItem`
trees, hit-tests in scene coordinates, reads source locations.

E2 result: `QQmlData` (private API) gives exact file, line and column of each
item's instantiation on Qt 6.11.2 and 6.12.0; the public
`qmlContext(item)->baseUrl()` gives the file only. Therefore the probe
reports `source: lineColumn` when built against the same Qt minor version as
the target and `source: file` otherwise, and must be compiled per Qt minor
version. `qrc:/` URLs are mapped to workspace files by the daemon using the
project's resource prefix → directory table.

## Verification

The agent must state, *before* patching, the checks that would show success
(`Check` list: geometry relations, property values, the region expected to
change). After reload the daemon evaluates them and adds its own:

| Check | Passes when |
|-------|-------------|
| `buildClean` | no new error diagnostics since the patch |
| `runtimeClean` | no new console errors / QML warnings since reload |
| `nodeReidentified` | selected node found again (see Node identity) |
| `geometry` | stated relation holds within 0.5 logical px |
| `property` | stated property equals the expected value |
| `pixelsChangedInRegion` | changed pixels exist inside the expected region |
| `pixelsStableElsewhere` | changed fraction outside the expected region (grown by 8 px) ≤ 0.1 % |

The agent states its checks as `ui/verify` assertions: `geometry` (`node.edge op other.edge +
offset`, or `op value`; edges `left`, `right`, `top`, `bottom`, `width`, `height`, `centerX`,
`centerY`; `eq` within 0.5 px) and `property` (a computed property, e.g. `css.padding-left`,
equals a value). Nodes are named by stable key, which survives the reload.

**M5 has no pixel capture** (`Capabilities.screenshot: false`): the two pixel checks are reported
with `skipped: true` and neither pass nor fail.

Verdict: `failed` if any check fails; `verified` if none fails **and** at
least one `geometry` or `property` check stated by the agent passed;
otherwise `inconclusive`. Screenshot comparison alone can never produce
`verified`: it shows that something changed, not that the right thing did.
Before each capture the daemon waits for quiescence: no `didMutate` for
150 ms, at most 2 s, and animations disabled where the adapter can
(`prefers-reduced-motion` emulation on web). `ui/verify` first waits for the change itself:
unless a hot reload (`didMutate` with `reason: hotReload`) has come since the previous
verification, it waits for one, at most 3 s (within the call's `timeoutMs`), so a loaded
machine's slow reload is not mistaken for a settled page.

E3 demonstrated the loop on one case: intent "+24 px left margin" → geometry
delta exactly 24 px, pixels changed only around the two cards (the change
affected both instances of the repeated component, as the instance count
predicted).

## Failure handling in the closed loop

| Step | Failure | Behaviour |
|------|---------|-----------|
| launch | process exits / port never opens | report with captured output; no session |
| adapter connect | no `hello` within 15 s | session continues at Level 1 (screenshots only), stated in the UI |
| select | `not_found` | "nothing selectable here"; offer region selection |
| resolve source | no candidates ≥ 0.6 | ask the user to confirm a candidate or pick a file; never guess silently |
| patch | search block does not match | tool error to the agent with the current text; no partial writes |
| rebuild | new error diagnostics | `failed`; diagnostics returned to the agent; checkpoint offered |
| reload | no `didMutate` within 10 s | fall back to full reload; if still nothing → `inconclusive` |
| state restore | route/scroll cannot be restored | `inconclusive` with the reason; screenshots still shown |
| verify | node not re-identified | `inconclusive` |

State restoration (v1, web): URL and scroll position are recorded before the
patch and reapplied after a full reload; HMR needs none. Deeper application
state (form contents, navigation stacks in native apps) is out of scope
until an adapter offers it as a capability.

## Fixtures and tests (M5)

`tests/fixtures/inspect-react/` and later `inspect-qml/`: tiny apps with a
`expected-map.json` listing, for named elements, the exact source location.
Tests assert (1) selection by coordinate returns the named element, (2) its
top source candidate equals the expected location, (3) a scripted patch
followed by reload yields `verified` for a true assertion and `failed` for a
false one. E3's `driver.mjs` is the seed for this suite.
