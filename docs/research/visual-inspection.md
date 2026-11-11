# Mapping rendered UI elements to source code
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.
> Web search and fetch worked overall, but individual requests failed: one search had an invalid query count, Qt's unversioned page redirected across origins, and two raw GitHub paths returned 404. The Qt 6.11 page and a pinned GammaRay page were fetched successfully instead; unresolved points are listed below.

## Summary and recommendations
- **Adopt — compiler-stamped web nodes:** exact host JSX source locations demonstrated locally on React 19/Vite 8 (Axiom experiment E3); implement per-bundler adapters, not a dependency on Fiber internals.
- **Adopt — CDP for embedded Chromium:** rich DOM, styles, hit testing and screenshots; negotiate capabilities against Qt's actual Chromium version ([CDP](https://chromedevtools.github.io/devtools-protocol/)).
- **Adapt — Playwright:** optional regression driver, not a prerequisite or guaranteed Qt WebEngine client ([API](https://playwright.dev/docs/api/class-browsertype#browser-type-connect-over-cdp)).
- **Adapt — React owner stacks:** useful component ancestry, not guaranteed exact host-node locations ([React API](https://react.dev/reference/react/captureOwnerStack)).
- **Adapt — QML in-process probe:** exact Qt 6.11 private metadata locally demonstrated (Axiom experiment E2); build/version isolate, with public file-only fallback.
- **Adapt — GammaRay:** powerful external diagnostic reference; `GPL-2.0-or-later` or commercial terms affect reuse ([README](https://raw.githubusercontent.com/KDAB/GammaRay/master/README.md)).
- **Adapt — Flutter/Android:** native inspector integrations can reveal richer trees than accessibility; defer until supported targets warrant dedicated adapters.
- **Adopt — OS accessibility:** cross-framework semantic and geometry fallback, not a source locator.
- **Adapt — image grounding:** useful for otherwise opaque targets; candidate-only evidence, never an exact source claim.
- **Adopt — explicit provenance, confidence and re-identification:** prevent plausible agent guesses from masquerading as compiler facts.
- **Reject — universal public source mapping:** iOS/SwiftUI, production builds and arbitrary processes do not offer a verified universal source-tree bridge.

## Findings

### Chrome DevTools Protocol (CDP)
- **Purpose/maturity:** Chromium's JSON protocol documents domains, commands, events, experimental annotations, stable 1.3 and tip-of-tree schemas; tip-of-tree is not the exact contract of every installed Chromium ([protocol](https://chromedevtools.github.io/devtools-protocol/)).
- **Licence/platform:** protocol project licence and Qt-distributed Chromium component licences require separate review; Chromium is cross-platform but CDP availability depends on the target implementation ([CDP repository](https://github.com/ChromeDevTools/devtools-protocol)).
- **DOM:** `DOM.getNodeForLocation` hit-tests viewport coordinates; `describeNode`, box models and DOM traversal supply structural context ([DOM](https://chromedevtools.github.io/devtools-protocol/tot/DOM/)). E3 demonstrated coordinate selection locally.
- **CSS:** `CSS.getMatchedStylesForNode` returns matched rules, inheritance and pseudo-element matches; `CSS.getComputedStyleForNode` gives resolved properties ([CSS](https://chromedevtools.github.io/devtools-protocol/tot/CSS/)).
- **Overlay:** `Overlay.setInspectMode` enables a browser-managed picker; inspect-node events connect a user selection back to the protocol ([Overlay](https://chromedevtools.github.io/devtools-protocol/tot/Overlay/)).
- **DOMSnapshot:** `captureSnapshot` returns flattened document/layout/style data; useful for bulk comparison but snapshots and node references are not durable IDs ([DOMSnapshot](https://chromedevtools.github.io/devtools-protocol/tot/DOMSnapshot/)).
- **Page:** `captureScreenshot` captures an image, with format/clip controls; record viewport and scale as well ([Page](https://chromedevtools.github.io/devtools-protocol/tot/Page/)).
- **Accessibility:** `getPartialAXTree`/`getFullAXTree` report accessible roles, names and states; accessibility nodes need not correspond one-to-one with DOM nodes ([Accessibility](https://chromedevtools.github.io/devtools-protocol/tot/Accessibility/)).
- **Runtime:** `evaluate` and object inspection can read app-side metadata, but execute code in a privileged debugging session ([Runtime](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/)).
- **Integration:** attach to target, enable required domains, hit-test, collect source stamp + layout + styles, then snapshot and verify after HMR. Handle frames, shadow roots, target changes and stale handles explicitly.
- **Verdict:** adopt direct CDP as the primary web adapter; CDP is browser-specific, not a cross-browser standard.

### Attaching to Qt WebEngine, port versus pipe
- Qt 6.11.3 documents `--webEngineArgs --remote-debugging-port=<port>` and `QTWEBENGINE_REMOTE_DEBUGGING` as a port or host:port; discover the local target then connect to its debugging endpoint ([Qt](https://doc.qt.io/qt-6.11/qtwebengine-debugging.html)).
- Qt's `WebEngineView.devToolsView` and `inspectedView` connect an embedded developer-tools view and an inspected view; this is an interactive UI, not proof of a public Rust CDP connection API ([QML WebEngineView](https://doc.qt.io/qt-6.11/qml-qtwebengine-webengineview.html)).
- Qt states that when remote debugging is enabled without an explicit allowlist it adds `--remote-allow-origins=*`; specify narrowly scoped origins, bind loopback and avoid untrusted local pages ([Qt](https://doc.qt.io/qt-6.11/qtwebengine-debugging.html)). Origin restrictions are not authentication.
- Chromium also has a pipe transport used by browser automation; a child-process pipe avoids a listening TCP port, but **Qt WebEngine pipe support and descriptor ownership are unverified** ([Chromium DevTools server](https://chromium.googlesource.com/chromium/src/+/main/content/browser/devtools/)).
- Google Chrome 136 changed remote-debugging switch handling for the default data directory; use a non-default profile in standalone Chrome. Do not automatically transfer Chrome-specific policy to Qt WebEngine ([Chrome security note](https://developer.chrome.com/blog/remote-debugging-port)).
- **Complexity/verdict:** port is documented for Qt; prefer it for a controlled local dev target. Pipe is an experiment, not an advertised Qt feature.

### WebDriver BiDi and Playwright
- **BiDi:** the [W3C WebDriver BiDi document](https://w3c.github.io/webdriver-bidi/) defines a bidirectional browser-automation protocol; implementations and commands vary. Its existence is not evidence of Qt WebEngine support or CDP feature parity.
- **Playwright:** `chromium.connectOverCDP` accepts HTTP/WS endpoints but is Chromium-only and explicitly lower fidelity than the native Playwright protocol; launching a browser outside Playwright can break functionality ([API](https://playwright.dev/docs/api/class-browsertype#browser-type-connect-over-cdp)).
- Playwright repository uses `Apache-2.0` ([licence](https://github.com/microsoft/playwright/blob/main/LICENSE)). It offers locators, actions, assertions and screenshots; it does not itself guarantee source mapping.
- **Verdict:** use Playwright as an optional test driver; verify Qt's target discovery and CDP compatibility before selecting it. Use raw CDP for precise inspection if the higher-level attachment fails.

### React 18/19, owner stacks and DevTools
- Historically JSX transforms supplied development `__source`/Fiber `_debugSource`; React 19 users report `_debugSource` removed and inspectors breaking ([React issue #32574](https://github.com/facebook/react/issues/32574), [issue #31981](https://github.com/facebook/react/issues/31981)). E3 independently observed undefined on the local React 19/Vite 8 setup.
- React's newer development stack machinery uses `_debugStack` and owner/component stacks; React's DevTools change describes using Error objects to leverage browser source mapping ([React PR #30289](https://github.com/facebook/react/pull/30289)). These are not a stable public Fiber source-location API.
- Public `captureOwnerStack()` returns an owner stack in supported development execution contexts; React documents it as development-only and potentially `null`, not as arbitrary DOM-node-to-JSX lookup ([React API](https://react.dev/reference/react/captureOwnerStack)).
- **DevTools current approach:** component stack/error stack handling leverages source mapping; an exact full current open-in-editor path and support matrix were not confirmed from implementation. Do not equate mapped owner frame with the selected host JSX element ([PR](https://github.com/facebook/react/pull/30289)).
- **Robust design:** instrument pre-lowering JSX host elements with project-relative source coordinates, or opaque IDs referencing a private build manifest; transform before React/SWC compilation and preserve composed maps. React version independence is an architectural hypothesis beyond E3.
- **Complexity:** Vite pre-transform is demonstrated in E3. Webpack loader, Next/Turbopack loader/rules and SWC pipeline each require an independently verified ordering and framework compatibility test.
- **Verdict:** adopt stamps as primary, owner-stack information as secondary; preserve both host declaration and component owner as distinct candidates.

### Existing web element inspectors
- **click-to-component:** Option-click UI selects a React component and opens its source in an editor; treat its runtime technique and React 19 coverage as version-specific pending source audit ([repository](https://github.com/ericclemmons/click-to-component)).
- **react-dev-inspector:** documented Babel transform injects path/line/column into JSX data attributes; runtime inspector locates selection and optional dev-server middleware launches the editor. README also describes older Fiber-based development metadata; do not assume that fallback survives React 19. Its README states `MIT` ([README](https://raw.githubusercontent.com/zthxxx/react-dev-inspector/master/README.md)).
- **LocatorJS:** provides click-to-source tooling; exact current transform/runtime strategy and React 19 support are unverified ([repository](https://github.com/infi-pc/locatorjs)).
- **code-inspector-plugin:** README reports `MIT`, source-click support for React, Vue and Svelte and adapters for Vite, webpack and Turbopack; it gives separate Next.js configuration for 15.0–15.2 and >=15.3. These are project claims, not an Axiom compatibility test ([README](https://raw.githubusercontent.com/zh-lx/code-inspector/main/packages/code-inspector-plugin/README.md)).
- **vite-plugin-vue-inspector:** Vite integration implements click-to-editor; `data-v-inspector` is used for element locations, but component-tag coverage has been requested separately ([repository](https://github.com/webfansplz/vite-plugin-vue-inspector), [issue #84](https://github.com/webfansplz/vite-plugin-vue-inspector/issues/84)).
- **Svelte inspector:** Vite plugin implements an inspector and has evolving component-stack work; do not assume older `__svelte_meta` is present in current Svelte output ([plugin](https://github.com/sveltejs/vite-plugin-svelte), [component-stack PR](https://github.com/sveltejs/vite-plugin-svelte/pull/1370)).
- **Verdict:** adapt established transform patterns; never promise all combinations merely from a package support list.

### Vue, Svelte and Angular runtime metadata
- **Vue:** `__file` is a component-level development convention, not necessarily a selected template element's exact line. Template instrumentation such as `data-v-inspector` can supply element-level locations ([Vue inspector](https://github.com/webfansplz/vite-plugin-vue-inspector)). Availability and build flags need validation.
- **Svelte:** historical `__svelte_meta` is not a stable cross-version contract; prefer the version-matched inspector transform and verify Svelte 4/5 separately ([Svelte Vite plugin](https://github.com/sveltejs/vite-plugin-svelte)).
- **Angular:** development global `ng.getComponent(element)` returns a component instance for a host element; it is not itself a documented file/line source locator ([Angular API](https://v19.angular.dev/api/core/globals/getComponent)).
- **Verdict:** framework plugins provide enrichment; expose missing source mapping explicitly instead of inferring from names.

### Source maps and CSS origins
- **Standard:** [ECMA-426 first edition, December 2024](https://ecma-international.org/wp-content/uploads/ECMA-426_1st_edition_december_2024.pdf) specifies the source-map format and decoding; generated-to-original mappings describe positions, not inherently DOM identity.
- Rust [`sourcemap`](https://github.com/getsentry/rust-sourcemap) can parse/query maps; JS alternatives include [`@jridgewell/trace-mapping`](https://github.com/jridgewell/trace-mapping) and [`source-map`](https://github.com/mozilla/source-map). Verify exact selected releases and their licences before packaging.
- Combine a CDP stylesheet rule's stylesheet/range and source-map reference with generated-to-original lookup. Inline, constructed, injected, minified and unmapped CSS may not resolve; CSS rule provenance is distinct from the JSX element's declaration ([CDP CSS](https://chromedevtools.github.io/devtools-protocol/tot/CSS/)).
- Source-map column bases, path roots, virtual URLs, source content and composed-transform maps must be normalized and validated before editor navigation; avoid claiming an exact match for nearest preceding mapping without checking its span.
- **Verdict:** adopt ECMA-426 lookup as an enrichment service, not as a replacement for runtime-to-generated-position association.

### Qt/QML probes and GammaRay
- **Demonstrated:** Axiom experiment E2 walked in-process `QQuickItem` trees on Qt 6.11. Private `QQmlData` line/column and `outerContext` URL gave locations; `qmlContext(item)->baseUrl()` gave file only. This is not a public ABI contract.
- **GammaRay capabilities:** live QObject tree/properties/signals, Qt Quick item and scene-graph trees, widget/Quick layout overlays, model inspection and remote targets ([KDAB README](https://raw.githubusercontent.com/KDAB/GammaRay/master/README.md)).
- **Licence:** GammaRay is `GPL-2.0-or-later`, with commercial licensing available; evaluate separate components and redistribution before reuse ([KDAB README](https://raw.githubusercontent.com/KDAB/GammaRay/master/README.md)).
- **Injection:** a historical GammaRay man page lists `preload` on Linux/macOS, `gdb` on Linux and `windll` on Windows; it supports launch and PID attach. It dates from the Qt 4/5 era and is evidence of historical methods, not a current OS compatibility guarantee ([pinned man page](https://raw.githubusercontent.com/KDAB/GammaRay/464d5e61c16b307d15a3a241ce16b32f8b7d7c0f/docs/gammaray.pod)).
- Probe ABI must match the target Qt build; modern platform hardening, signatures and debugger permissions are deployment constraints to verify. Remote probe/client operation is documented; a stable public protocol contract for an independent Rust client was not confirmed ([README](https://raw.githubusercontent.com/KDAB/GammaRay/master/README.md)).
- **Verdict:** use GammaRay as an optional external tool and design reference; prefer an opt-in, version-locked Axiom probe over depending on undocumented wire protocol.

### Qt QML debug services and accessibility
- [`QQmlDebuggingEnabler`](https://doc.qt.io/qt-6.11/qqmldebuggingenabler.html) enables debugging services and can start a TCP debugging server; Qt warns debugging exposes powerful access and should not be enabled in production.
- Qt documents the `-qmljsdebugger` argument and debugging/profiling services; the exact `QmlInspector` service messages and current stable public protocol suitability need a source-code audit ([QML debugging](https://doc.qt.io/qt-6.11/qtquick-debugging.html)).
- Qt exposes accessibility through [`QAccessible`](https://doc.qt.io/qt-6.11/qaccessible.html); platform bridges surface accessible objects but do not guarantee QML file/line metadata.
- **Complexity/verdict:** public file-only context, optional private in-process source probe, and accessibility fallback form separate capability tiers. Do not imply debug service equals E2's private source data.

### Flutter inspector and Dart VM service
- Flutter DevTools' inspector examines the widget tree and layout; source-location facilities depend on widget-creation tracking and development tooling ([Flutter inspector](https://docs.flutter.dev/tools/devtools/inspector)).
- Flutter's runtime implements inspector service extensions under `ext.flutter.inspector.*`; these are accessed through the Dart VM service rather than CDP ([inspector source](https://github.com/flutter/flutter/blob/master/packages/flutter/lib/src/widgets/widget_inspector.dart)).
- `--track-widget-creation` associates widget construction with source locations; the tooling implementation describes a kernel transformer ([Flutter commit](https://github.com/flutter/flutter/commit/8a483dada68b81fbe1f21f99980c7fd83ff91e15)). Exact defaults/build modes need verification against the selected Flutter release.
- Creation site is a widget constructor location, not necessarily the exact paint operation or state definition; release builds and remote VM exposure require separate checks.
- Flutter is `BSD-3-Clause` ([licence](https://github.com/flutter/flutter/blob/master/LICENSE)). **Verdict:** adapt a debug-only VM-service provider when Flutter support is in scope.

### Android Views and Compose
- Android Studio [Layout Inspector](https://developer.android.com/studio/debug/layout-inspector) inspects running layouts; Compose inspection adds composition information and can connect inspected nodes to source where supported ([Compose tooling](https://developer.android.com/develop/ui/compose/tooling/debug)).
- UI Automator's [`UiDevice.dumpWindowHierarchy`](https://developer.android.com/reference/androidx/test/uiautomator/UiDevice#dumpWindowHierarchy(java.io.File)) exports the accessibility window hierarchy; it is a semantic fallback, not a universal source map.
- Compose exposes merged/unmerged semantics trees for accessibility and testing; semantic nodes, layout nodes and composable source calls are different structures ([Compose semantics](https://developer.android.com/develop/ui/compose/accessibility/semantics)).
- **Restrictions:** source information and inspection capabilities depend on app build configuration, OS, tooling and process access; do not assume Compose inspection for release builds.
- **Verdict:** adapt Android-specific inspector if Android becomes a target; otherwise reuse accessibility-like snapshots only.

### Apple UIKit, SwiftUI and third-party inspectors
- Xcode's view debugger can diagnose a running app's view hierarchy and appearance ([Apple](https://developer.apple.com/documentation/xcode/diagnosing-issues-in-the-appearance-of-your-running-app)). This is not a documented general-purpose API for arbitrary source resolution.
- XCUITest accesses accessibility/UI test elements and their snapshots; WebDriverAgent exposes automation around XCTest rather than universal SwiftUI source metadata ([WebDriverAgent](https://github.com/appium/WebDriverAgent)).
- SwiftUI has no confirmed public general-purpose API to enumerate the original declarative source view tree with file/line per rendered element; accessibility and hosted UIKit views are incomplete proxies, not proof that source mapping is impossible under all private tooling.
- [Reveal](https://revealapp.com/) and [Lookin](https://github.com/QMUI/Lookin) are prior-art hierarchy tools; inspect current distribution licences, runtime injection and supported OS versions before use.
- **Verdict:** realistic cross-app ceiling is accessible semantics/geometry, supplemented by explicitly integrated app-side metadata; treat source mapping as unavailable unless independently demonstrated.

### Desktop accessibility bridges
- Linux AT-SPI exposes an accessibility bus and accessible object interfaces through [`libatspi`](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/); availability depends on application/toolkit and session configuration.
- Windows [UI Automation](https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomationoverview) provides a client/provider tree and control patterns; privilege boundaries and provider quality apply.
- macOS [`AXUIElement`](https://developer.apple.com/documentation/applicationservices/axuielement) accesses accessibility elements subject to OS authorization.
- **Common denominator:** role, name, state, bounds and actions when exposed; not complete paint order, CSS, component hierarchy or source code. Permission and headless-session behavior must be tested on each OS.
- **Verdict:** adopt as Level 2 generic inspection; source should be `null` rather than guessed.

### Screenshot grounding and visual comparison
- [Set-of-Mark](https://github.com/microsoft/SoM) overlays numbered region labels to improve model grounding; image labels are proposals, not source identity.
- [OmniParser](https://raw.githubusercontent.com/microsoft/OmniParser/master/README.md) parses UI screenshots into candidate regions. Its README says code `MIT`, caption models MIT, older Ultralytics-based detectors AGPL, and a July 2026 YOLOv9-E detector option; check the exact weights and dependency licences, not only the repository badge.
- [ScreenSpot-Pro evaluation](https://github.com/microsoft/OmniParser/blob/master/docs/Evaluation.md) is a grounding benchmark; scores depend on dataset, model, prompt and setup and do not establish source-code mapping accuracy.
- [`pixelmatch`](https://github.com/mapbox/pixelmatch) is pixel-level comparison; [`odiff`](https://github.com/dmtrKovalenko/odiff) is a dedicated image comparison tool; [`dssim`](https://github.com/kornelski/dssim) is perceptual; Rust [`image-compare`](https://github.com/AlxHnr/image-compare) offers image comparison. Audit selected versions/licences separately.
- E3 demonstrated pixelmatch-based before/after verification locally. Freeze viewport, DPR, fonts, animation and network state; compare target and surrounding regions and report raw and perceptual results separately.
- **Verdict:** visual grounding is a Level 3 fallback and verification signal, never source evidence by itself.

### Select-to-edit prior art
- **Onlook:** open-source visual editing of a real React app is documented; the exact current DOM-to-source implementation was not established here ([repository](https://github.com/onlook-dev/onlook)).
- **Stagewise:** browser-based coding agent repository exists; exact element-to-source resolution and current open-source component boundaries require code audit ([repository](https://github.com/stagewise-io/stagewise)).
- **Cursor:** [Design Mode](https://cursor.com/docs/agent/design-mode) documents visual editing; underlying source resolution is not confirmed by that product documentation.
- **Vercel toolbar/v0:** [Edit Mode](https://vercel.com/docs/edit-mode) provides visual editing of supported content; do not infer universal local source mapping, and do not conflate CMS editing with JSX mapping.
- **Lovable:** preview toolbar supports selecting/editing elements ([docs](https://docs.lovable.dev/features/preview-toolbar)); internal resolver not confirmed.
- **Bolt:** visual edits allow direct selection and edits ([docs](https://support.bolt.new/building/visual-edits)); internal resolver not confirmed.
- **Replit:** element editing is documented ([docs](https://docs.replit.com/features/design/elements)); internal resolver not confirmed.
- **Tidewave:** documents detecting React/Vue components and sending source location to its agent, and a Vite route proxy for split frontend/backend development; exact source-resolution internals require audit ([frontend docs](https://raw.githubusercontent.com/tidewave-ai/tidewave_phoenix/main/pages/references/frontend.md)).
- **Zed:** a comparable current built-in select-element-to-source-edit workflow was not verified; no parity claim is justified ([Zed documentation](https://zed.dev/docs/)).
- **Verdict:** adopt the interaction pattern; borrow implementations only after code/licence review, not by assuming product behavior reveals the mapping technique.

## Architectural implications for Axiom

### Normalized inspection record — proposed, not an existing standard
- Required `schema_version`, `session_id`, `capture_id`, `timestamp`, provider name/version, framework and provider capability flags.
- Required `target`: process/target and window/frame scope, session-scoped runtime node ID if available, and an explicit `identity_scope`/expiry.
- Required `selection`: method (`coordinate`, `tree`, `semantic`, `image`), original coordinate space, viewport dimensions, device scale, and bounds or explicit unknown.
- Required `source_candidates`: possibly empty array; each candidate has workspace-relative URI, optional source range, relationship (`host-declaration`, `owner`, `style-rule`, `widget-creation`), provenance, confidence class and evidence.
- Required `confidence`: `exact-instrumented`, `exact-runtime-private`, `mapped-generated`, `owner-only`, `heuristic`, `visual-only`, or `unknown`; these are ordinal categories, not calibrated probabilities.
- Optional `semantic`: role, accessible name, text, states, actions and a redaction marker.
- Optional `structure`: parent/owner chains, shadow/frame paths, component name, stable author key and repeated-instance discriminator.
- Optional `style`: computed values, matched rule order, original/generated stylesheet range, specificity and map provenance.
- Optional `visual`: screenshot reference, crop, geometry, occlusion/visibility and pixel/perceptual diff metadata.
- Optional `build`: revision/content hash, source-map ID, compiler version, adapter version, HMR generation and instrumenter configuration.
- Capability flags must be independently true/false/unknown: `hit_test`, `tree`, `bounds`, `semantics`, `source_file`, `source_range`, `owner_chain`, `styles`, `screenshot`, `actions`, `stable_author_key`, `hot_reload`.
- Example capabilities: E3 React/Vite has demonstrated hit-test, exact stamped host source and HMR verification; E2 Qt 6.11 has demonstrated item-tree and private location. Everything else varies by provider/build and should be probed, not hard-coded.

### Source confidence and stable identity
- **Exact-instrumented:** trusted source stamp from a known transform and matching build revision; exact for the stamped declaration, not necessarily the component owner or the intended edit.
- **Exact-runtime-private:** runtime-provided exact location with version-pinned adapter, as in E2; mark private API separately and validate source revision.
- **Mapped-generated:** source map translates a known generated location; report precision and mapping span, and downgrade on ambiguous/missing map or stale content.
- **Owner-only:** stack/component metadata identifies an enclosing component, not the selected child; show the distinction to the user.
- **Heuristic/visual-only:** text, proximity, accessibility role, VLM or inferred CSS selector produces candidates only; require human confirmation before applying a source edit.
- Runtime node IDs die on navigation, replacement or HMR. Re-identify by explicit stable author key/test ID plus frame and ancestor context; these keys can be absent or duplicated.
- Secondary fingerprint: source declaration + component ancestry + role/name + sibling occurrence and geometry, with revision and ambiguity checks. A single declaration may render many list instances; include app-provided item key if available.
- On ambiguous matches, capture all candidates, request selection and refuse to silently verify a different element. Never treat pixel position or DOM path alone as durable.
- Proposed loop: inspect → present evidence → edit workspace-scoped source → wait for HMR/render settle → re-identify → capture geometry/screenshot → compare expected and unintended changes → report ambiguity and rollback option.

## Risks
- **Security:** CDP Runtime, QML debugging and probe injection are powerful; bind loopback, disable by default, strictly gate target selection and do not rely on origin checks as authentication ([Qt warning](https://doc.qt.io/qt-6.11/qtwebengine-debugging.html)).
- **Data exposure:** absolute file paths, screenshots, text, style values, source maps and app state may contain secrets; redact before agent/model transmission.
- **Private ABI:** E2 accesses internal Qt state and needs exact-build tests and a clean failure path.
- **Instrumentation:** stamps can leak repository structure, collide with app props, affect hydration or be removed by other transforms; prefer opaque development-only IDs and test framework boundaries.
- **Mapping ambiguity:** repeated renders, portals, generated code, templates and source map gaps can invalidate naive one-to-one assumptions.
- **Visual nondeterminism:** animation, async data, font rendering, platform scaling and antialiasing make screenshot comparison noisy; a passing diff is not proof of correct semantics.
- **Licence:** GammaRay GPL and OmniParser detector variants require particular attention; check the exact release, model weights and transitive dependencies.

## Unverified / needs experiment
- Whether Qt WebEngine's particular build accepts remote-debugging pipe; whether Playwright can attach to its actual target and perform the required operations.
- React 18/19, Vite 8, webpack/SWC, Next/Turbopack coverage of Axiom's transform, including server components, hydration, portals, fragments, HMR and dependency code.
- Exact current React DevTools open-in-editor implementation from selected host node through mapped stack to editor; exact LocatorJS and click-to-component React 19 behavior.
- Vue `__file`, `data-v-inspector`, Svelte 4/5 `__svelte_meta` and Angular dev metadata under all supported build modes.
- Current GammaRay injectors and probe compatibility on each supported OS, hardened macOS and Windows; suitability and stability of the probe wire protocol.
- QmlInspector service current protocol, security and support for useful element source locations; Qt debug builds and private ABI across 6.x versions.
- Flutter flag defaults, exact service methods and availability in profile/release modes for the chosen SDK.
- Android Compose source information requirements and Apple's permitted external introspection under specific build/signing/device configurations.
- Exact source-resolution methods for Onlook, Stagewise, Cursor, Vercel/v0, Lovable, Bolt, Replit and Zed; published UI behavior alone is insufficient evidence.
- Licence and release audit for each selected sourcemap, inspector, visual-diff and third-party product dependency.
- Quantitative accuracy and calibration of visual grounding and of re-identification against Axiom's own multi-framework fixtures.

## Sources
- [CDP domains and versioning](https://chromedevtools.github.io/devtools-protocol/); [Qt WebEngine 6.11.3 debugging](https://doc.qt.io/qt-6.11/qtwebengine-debugging.html).
- [React 19 source metadata issue](https://github.com/facebook/react/issues/32574); [React owner stacks](https://react.dev/reference/react/captureOwnerStack); [DevTools Error-stack change](https://github.com/facebook/react/pull/30289).
- [React Dev Inspector README](https://raw.githubusercontent.com/zthxxx/react-dev-inspector/master/README.md); [code-inspector-plugin README](https://raw.githubusercontent.com/zh-lx/code-inspector/main/packages/code-inspector-plugin/README.md).
- [ECMA-426](https://ecma-international.org/wp-content/uploads/ECMA-426_1st_edition_december_2024.pdf); [Rust sourcemap](https://github.com/getsentry/rust-sourcemap).
- [GammaRay README and licence](https://raw.githubusercontent.com/KDAB/GammaRay/master/README.md); [historical injector documentation](https://raw.githubusercontent.com/KDAB/GammaRay/464d5e61c16b307d15a3a241ce16b32f8b7d7c0f/docs/gammaray.pod).
- [Flutter inspector](https://docs.flutter.dev/tools/devtools/inspector); [Android Layout Inspector](https://developer.android.com/studio/debug/layout-inspector); [Apple view debugging](https://developer.apple.com/documentation/xcode/diagnosing-issues-in-the-appearance-of-your-running-app).
- [OmniParser README and model licences](https://raw.githubusercontent.com/microsoft/OmniParser/master/README.md); [Tidewave frontend](https://raw.githubusercontent.com/tidewave-ai/tidewave_phoenix/main/pages/references/frontend.md).
- Axiom experiments E2/E3: user-supplied local results; no external URL supplied. No new local experiments were run in this research.
