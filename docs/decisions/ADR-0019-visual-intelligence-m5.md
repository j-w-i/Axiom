# ADR-0019: Visual intelligence (M5) on Tauri: protocol 0.5, injected web adapter, no CDP

Status: **Accepted** (2026-11-22). Written by the M5 planning pass and accepted under the owner's
standing instruction to work through M5 autonomously ("Work all the way through to M5
autonomously", 2026-11-21). Refines ADR-0008 (inspection adapters, build-time source stamping,
objective verification) for the Tauri shell (ADR-0012). The owner may reopen any point; a change
is a new ADR.

## Context
M5 closes the product's loop for React + Vite apps:
- click an element in a running preview and the exact source opens;
- the agent patches it, hot reload applies the patch, and the daemon verifies the result
  objectively (`docs/roadmap/milestones.md` §M5).

The draft contract is `docs/protocols/inspection.md` with `crates/protocol/src/inspection.rs`.
E3 proved the source stamping and a verification loop against Chrome over CDP. The draft
assumed a Qt WebEngine preview driven over CDP.

That is no longer the shell. The Tauri webviews (WebKitGTK, WKWebView, WebView2) expose no CDP,
and two of the three are not Chromium. Windows is deferred by the owner.

## Decisions

### 1. The adapter is a script in the previewed page, not CDP
The draft already lets an adapter dial the daemon: "the adapter connects to the daemon on a
loopback WebSocket … `AXIOM_INSPECT_ENDPOINT`, `AXIOM_INSPECT_TOKEN`". M5 uses only that path.

- **The runtime adapter** (`inspectors/web/adapter.js`) runs inside the previewed page. It
  answers the frozen `inspect/*` methods with DOM APIs:
  - `snapshot` walks the DOM (bounds from `getBoundingClientRect`, `data-axiom-src`, text and
    roles);
  - `hitTest` uses `elementsFromPoint`;
  - `nodeDetails` reads `getComputedStyle` (namespaced `css.*` properties);
  - `highlight` draws an overlay element;
  - `didMutate` comes from a `MutationObserver`, plus Vite's HMR events (`hotReload`).
- **Selection inside the page.** With inspect mode on, the adapter takes the pointer itself:
  hover highlights the element, and a click selects it and is not passed to the app. It sends
  notifications to the daemon (`inspect/hovered`, `inspect/selected`: additive adapter
  notifications).
- **No pixel capture in M5.** A portable capture of a cross-origin page is not available from
  the page or from all three webviews, so `Capabilities.screenshot` is `false`, and the two
  pixel checks report `skipped`.
  - The contract's verdict rule already makes `verified` depend on a stated geometry or
    property check, never on pixels, so the exit criterion is met without them.
  - Native webview snapshots (`webkit_web_view_get_snapshot`, `WKWebView.takeSnapshot`) are a
    later task.

### 2. The preview is shown inside the app; injection does not touch the user's config
- **Starting it.** `preview/start` runs the workspace's Vite dev server as a task:
  - the provider's `dev` task, or `npm run dev`, through the M2 environment;
  - with `--config <data dir>/previews/<id>/vite.config.mjs`, a generated file that imports the
    user's own config (if any) and appends the Axiom plugin;
  - the daemon finds the URL in the dev server's output.
- **The plugin** (`inspectors/web/plugin.mjs`, from E3):
  - stamps host JSX elements with `data-axiom-src` (dev server only);
  - injects the adapter script with the endpoint and the one-time token
    (`transformIndexHtml`).
- **Showing it.** The shell displays the URL in an `<iframe>` in the Inspector page. The CSP
  allows `frame-src` for `http://127.0.0.1:*` and `http://localhost:*`. The page and the shell
  never script each other; everything goes through the daemon.

### 3. Protocol 0.5: `preview/*` and `ui/*` for clients; `inspect/*` stays the adapter's
IPC 0.1–0.4 are frozen; the daemon reports **0.5**. Exact types come in M5-ARCH-001.

| Method / notification | Purpose |
|-----------------------|---------|
| `preview/start` | start the dev server with the plugin; the result has the preview id and URL when it is up |
| `preview/stop` / `preview/list` | |
| `preview/state` (notification) | `starting`, `ready` (with the adapter's level), `failed` (with output), `stopped` |
| `ui/inspectMode` | inspect mode on or off in the page (hover highlight, click to select) |
| `ui/snapshot` | the normalised tree (sources resolved to workspace paths) |
| `ui/hitTest` | the node at a point, with its ancestors and source candidates |
| `ui/nodeDetails` | computed properties and source candidates of one node |
| `ui/highlight` | show or hide the overlay for a node |
| `ui/verify` | evaluate the stated checks after a change, giving a `VerificationReport` |
| `ui/selected` / `ui/hovered` (notification) | the page's selection and hover in inspect mode |
| `ui/didMutate` (notification) | the page changed (reload, HMR) |

The draft's `inspect/*` adapter methods and types (`UiNode`, `SourceCandidate`, `Check`,
`VerificationReport`) are frozen with the additions above. Error kinds `preview_failed` and
`adapter_unavailable` are added.

### 4. `crates/inspect`
A new crate owning:
- the adapter WebSocket server: loopback, one port per daemon, and a token per preview, checked
  by `inspect/hello`;
- sessions and the latest snapshot per preview;
- normalisation;
- source resolution:
  - `data-axiom-src` gives an `instrumented` candidate at confidence 1.0, mapped to a workspace
    path;
  - an element without a stamp gets its nearest stamped ancestor in a different file as the
    `owner`, at confidence ≤ 0.6;
  - without any stamp, text search for the element's text gives `heuristic` at ≤ 0.4;
  - candidates under 0.6 are presented as uncertain, never as exact;
- the verification engine (quiescence wait, re-identification, checks, verdict).

The WebSocket library is `tokio-tungstenite` (MIT). The edges are `inspect → protocol` and
`axiomd → inspect`.

### 5. Agent integration
- **Tools:** `preview.start`, `ui.snapshot`, `ui.inspect` (hit test or node, with candidates),
  `ui.selection` (what the person selected) and `ui.verify`.
- **Assertion-first:** before patching a UI, the agent states its checks; after the reload,
  `ui.verify` evaluates them. The tool descriptions say so.
- **Ask AI about this element:** the Inspector sends the selection's context (sources, computed
  properties, path) to the dock as an attachment.

### 6. Fixture and tests
- `tests/fixtures/inspect-react/`: a minimal React + Vite app with `expected-map.json`, plus
  one element rendered by a component from a local "library" folder that is not instrumented,
  to test uncertainty.
- The end-to-end loop runs with the scripted model, as in E3. It must:
  - click the button and open the exact source line and column;
  - apply a spacing patch, take the HMR, and get `verified` with a geometry check;
  - get `failed` for a deliberately wrong patch;
  - present the library element as uncertain.

## Consequences
- Inspection works the same in every webview: the adapter is plain DOM JavaScript.
- With no pixel evidence in M5, a change whose intent cannot be stated as geometry or a property
  is `inconclusive`. The UI says so plainly.
- The adapter runs inside the user's app in development only, and it is gone in production
  builds: the plugin is `apply: "serve"`.

## Alternatives rejected
- **CDP against a Chromium child process** (headless or a separate window): it is not the
  preview the person sees, needs a Chromium installed, and doubles the rendering.
- **Scripting the iframe from the shell:** cross-origin pages cannot be scripted, and making
  them same-origin through a proxy changes the app under test.
