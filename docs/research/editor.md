# Editor infrastructure: embedding VS Code editing technology
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.

**Research access:** web search and fetch worked, but individual fetches failed: the attempted Microsoft Marketplace terms URL returned 404, the cpptools Marketplace license page returned 503, and a Qt documentation URL redirected across origins. The versioned Qt documentation was fetched successfully. The Pylance and C# Dev Kit Marketplace license pages returned no substantive license text. Consequently, extension-specific restrictions and the current authoritative Marketplace terms need further verification, as noted below.

## Summary and recommendations
- **Adopt Monaco Editor** for milestone 1: MIT, offline-bundled editor with no claim of VS Code extension compatibility. [Monaco](https://github.com/microsoft/monaco-editor/blob/main/README.md)
- **Adapt LSP** behind a Rust-owned process supervisor: language intelligence without an extension host. [LSP](https://microsoft.github.io/language-server-protocol/)
- **Adapt DAP** after the editing milestone: debugger interoperability requires adapters and a real debug UI. [DAP](https://microsoft.github.io/debug-adapter-protocol/)
- **Adapt CodinGame monaco-vscode-api** only at a measured compatibility milestone: real services and web extension host, with version and integration costs. [Project](https://github.com/CodinGame/monaco-vscode-api)
- **Adapt TypeFox monaco-languageclient** if its aligned service stack and WebSocket examples fit; do not mistake it for a Node extension host. [TypeFox](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md)
- **Adapt Code-OSS web workbench/server** as a high-compatibility spike or optional mode, not as a small embeddable widget. [Code-OSS](https://github.com/microsoft/vscode)
- **Adapt code-server** as a reference or optional full workbench; it carries a server and substantial distribution footprint. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md)
- **Defer OpenVSCode Server** pending release-health and platform verification: latest GitHub stable release observed is February 2026. [Release](https://github.com/gitpod-io/openvscode-server/releases/tag/openvscode-server-v1.109.5)
- **Defer Eclipse Theia** as an alternative IDE architecture, not a drop-in host library; compatibility is tracked, not universal. [Theia](https://github.com/eclipse-theia/theia/blob/master/README.md)
- **Adopt Open VSX** as the candidate extension registry, with per-extension licensing and security review. [Open VSX](https://github.com/eclipse-openvsx/openvsx)
- **Reject assuming Microsoft Marketplace or proprietary extensions are freely reusable** by an independent OSS product. [code-server FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-cant-code-server-use-microsofts-extension-marketplace)
- **Adapt Qt WebEngine** as the desktop host; validate input, accessibility, origin security and renderer cost on all three OSes. [Qt](https://doc.qt.io/qt-6.11/qtwebengine-overview.html)

## Findings

### Monaco Editor
- **Purpose:** browser code editor generated from VS Code sources with standalone service shims; model, editor, URI and language-provider APIs. [README](https://github.com/microsoft/monaco-editor/blob/main/README.md)
- **Maturity:** latest GitHub stable release observed: **0.57.0**, published 2026-09-24. [Release](https://github.com/microsoft/monaco-editor/releases/tag/v0.57.0)
- Monaco's version does not directly correspond to a VS Code version. [README](https://github.com/microsoft/monaco-editor/blob/main/README.md)
- **Licence:** MIT; preserve notices and audit bundled dependencies separately. [License](https://github.com/microsoft/monaco-editor/blob/main/LICENSE.txt)
- **What is missing:** a VS Code extension does not run on standalone Monaco; it is not the VS Code workbench and does not ship its extension host. [FAQ](https://github.com/microsoft/monaco-editor/blob/main/README.md#faq)
- TextMate grammars are not supported out of the box; upstream points to integrations using `vscode-textmate` and `vscode-oniguruma`. [FAQ](https://github.com/microsoft/monaco-editor/blob/main/README.md#faq)
- Standalone Monaco has built-in syntax/tokenization and language features; lack of TextMate is not lack of all highlighting. [Monarch](https://microsoft.github.io/monaco-editor/monarch.html)
- Models carry text, language and undo history; editors display models; URIs identify models and influence language services. [Concepts](https://github.com/microsoft/monaco-editor/blob/main/README.md#concepts)
- **Packaging:** `/esm` is supported; AMD remains for backwards compatibility but is deprecated and slated for removal. [README](https://github.com/microsoft/monaco-editor/blob/main/README.md#installing)
- Only `monaco.d.ts` is the versioned public API; private internals may change at any release. [README](https://github.com/microsoft/monaco-editor/blob/main/README.md#installing)
- **Offline:** bundle ESM, CSS, fonts and all configured worker entrypoints into the application; do not depend on a CDN at runtime. [ESM integration](https://github.com/microsoft/monaco-editor/blob/main/docs/integrate-esm.md)
- Upstream warns that `file://` pages cannot create the needed web workers; use a suitable origin and test Qt custom schemes. [FAQ](https://github.com/microsoft/monaco-editor/blob/main/README.md#faq)
- **Platforms:** intended for desktop browsers, not supported mobile browsers; Axiom's three Qt platforms require their own tests. [FAQ](https://github.com/microsoft/monaco-editor/blob/main/README.md#faq)
- **Complexity:** modest for editing; substantially larger for workspace, persistence, services and native shell integration.
- **Alternatives:** CodeMirror or the full VS Code-derived web workbench; evaluate separately for different compatibility ceilings.
- **Verdict:** Adopt; treat the editor as replaceable presentation, not the owner of the workspace.

### CodinGame monaco-vscode-api
- **Purpose:** imports VS Code service implementations into Monaco using service overrides; examples include configuration, themes, files, extensions, search and quick access. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md)
- The project provides a Monaco-compatible editor API alias and a `vscode` extension API alias; this is not the unmodified standalone Monaco package. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#installation)
- Configure and initialize services before creating the first editor; `initialize` can be called only once. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#monaco-service-override)
- The documented extension override can create a **web-worker extension host running in an iframe**; it can execute compatible web extensions, not arbitrary Node modules. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#list-of-service-overrides), [extension host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- VSIX loading is supported via a Rollup/Vite plugin; importing an archive does not establish that its code, required services, licence or UI contributions work. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#loading-vsix-file)
- Default VS Code language, grammar and theme extensions are separately published; select the relevant packages and overrides. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#default-vscode-extensions)
- File overlays and model references provide a bridge to VS Code services; plain `createModel` models are not discoverable by those services. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#model-creation)
- Its demo lists TextMate, debugging and a remote agent, but these demonstrations are not blanket compatibility guarantees. [Demo](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#demo)
- **Remote Node extensions:** the project documents a VS Code server integration; Node extensions require a compatible server-side Node extension host, not merely an LSP server. [Server guide](https://github.com/CodinGame/monaco-vscode-api/wiki/How-to-install-and-use-VSCode-server-with-monaco%E2%80%90vscode%E2%80%90api), [host architecture](https://code.visualstudio.com/api/advanced-topics/extension-host)
- A server running on the same machine can still be the remote host from the web client's architectural perspective; exact supported binaries and versions need a spike.
- **Maturity/cadence:** latest GitHub release observed **39.0.4**, 2026-11-09; 39.0.3 immediately precedes it in its release comparison, indicating active patching, not a fixed support promise. [Release](https://github.com/CodinGame/monaco-vscode-api/releases/tag/v39.0.4)
- **Licence:** MIT for project code; retain separate VS Code and imported extension notices. [License](https://github.com/CodinGame/monaco-vscode-api/blob/main/LICENSE)
- **Platforms:** building this repository itself is documented only on Linux/macOS; Windows contributors are directed to WSL. This is not a claim that the resulting web app cannot run on Windows. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#demo)
- **Limitations:** shadow DOM and iframe sandbox modes are explicitly beta; services cannot simply be unloaded/reinitialized. [README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#sandbox-mode-%EF%B8%8F-beta-%EF%B8%8F)
- **Complexity:** high: bundler aliases, workers, iframe, model lifecycle, services, CSS and VS Code version alignment.
- **Alternatives:** plain Monaco plus LSP; full Code-OSS web workbench; Theia.
- **Verdict:** Adapt later, after an extension compatibility contract and tests exist.

### TypeFox monaco-languageclient
- **Purpose:** connects Monaco to language servers; sibling `vscode-ws-jsonrpc` transports JSON-RPC over WebSocket. [README](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md)
- Examples include browser-worker language servers and external server processes bridged by WebSocket; a language server is not a general VS Code extension host. [Examples](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#examples-overview)
- The project builds on `vscode-languageclient` and CodinGame's VS Code service/API integration, with adjustments and integration examples. [History](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md#history)
- **Maturity/cadence:** repository's stated alignment is client **11.0.3**, 2026-10-15; `vscode-ws-jsonrpc` **4.0.3**; CodinGame **37.6.0**, VS Code **1.138.0**, Monaco **0.57.0**. [Compatibility table](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#changelogs-current-versions-and-compatibility-table)
- This documented CodinGame version differs from latest **39.0.4**: do not silently combine latest versions. [TypeFox](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md), [CodinGame release](https://github.com/CodinGame/monaco-vscode-api/releases/tag/v39.0.4)
- GitHub's latest-release API returned 404 for this repository; package versions above are from the maintained README, not a GitHub release. [API](https://api.github.com/repos/TypeFox/monaco-languageclient/releases/latest)
- **Licence:** MIT for both client and WebSocket JSON-RPC packages. [Licences](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#licenses)
- **Platforms:** browser plus whatever platforms the chosen language-server binaries support; examples show external Node, Java and worker servers. [Examples](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#examples-overview)
- **Complexity:** medium/high because it imports the VS Code compatibility stack; its migration guide and compatibility table should govern upgrades. [Docs](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#official-documentation)
- **Alternatives:** a smaller custom Monaco provider-to-LSP client; full extension-host-backed language extensions.
- **Verdict:** Adapt where the pinned stack earns its dependency cost; benchmark against a narrow first-milestone bridge.

### Code-OSS, VS Code web and remote extension host
- **Purpose/maturity:** `microsoft/vscode` is the open-source Code-OSS repository; Microsoft reports monthly VS Code product updates. [README](https://github.com/microsoft/vscode/blob/main/README.md)
- **Licence:** repository source is MIT; Microsoft's distributed Visual Studio Code has Microsoft-specific customizations and a separate Microsoft product licence. [README](https://github.com/microsoft/vscode/blob/main/README.md#visual-studio-code), [product licence](https://code.visualstudio.com/license)
- MIT on the repository does not grant the rights to Microsoft's trademarks, marketplace, proprietary product binaries, or independently licensed extensions. [README](https://github.com/microsoft/vscode/blob/main/README.md), [Marketplace FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-cant-code-server-use-microsofts-extension-marketplace)
- **Configuration:** `product.json` and gallery endpoints are product configuration, not a licence grant; code-server documents `extensionsGallery` and its own override. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#how-do-i-use-my-own-extensions-marketplace)
- The web workbench and server are materially more than the editor widget: browser UI connects to a host that can provide filesystem and Node extension services. [OpenVSCode](https://github.com/gitpod-io/openvscode-server/blob/main/README.md), [extension host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- Microsoft documents local Node, browser-worker and remote Node extension hosts as separate configurations. [Host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- An extension's `main` entry targets Node; `browser` targets web worker; `extensionKind` indicates preferred UI/workspace placement, not the runtime by itself. [Host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- An extension restricted to `ui` and Node cannot necessarily run in web plus remote, where there is no local Node host. [Host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- **`code serve-web`:** code-server's FAQ identifies it as a way to run VS Code web; Microsoft's installed product and a self-built Code-OSS server must not be treated as the same licensed artifact. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#whats-the-difference-between-code-server-and-vs-code-web), [product licence](https://code.visualstudio.com/license)
- **Protocol:** remote-agent/extension-host communication is VS Code-specific, distinct from LSP and DAP; do not assume it is a stable public interoperable standard. See [remote source](https://github.com/microsoft/vscode/tree/main/src/vs/platform/remote) and [extension host](https://code.visualstudio.com/api/advanced-topics/extension-host).
- **Embedding:** the full workbench is served as a browser application; loading a served instance into Qt WebEngine is a plausible integration, not evidence that arbitrary iframe nesting is supported. [OpenVSCode](https://github.com/gitpod-io/openvscode-server/blob/main/README.md)
- Iframe embedding requires explicit tests for frame policy, origins, workers, webviews, authentication, navigation and integration APIs.
- **Platform restrictions:** actual server builds, native modules and extensions vary by target OS/architecture; verify each artifact.
- **Complexity:** high: build/rebase workbench, product configuration, compatible server, security and duplicated UI.
- **Alternatives:** code-server, OpenVSCode, Theia, or selective CodinGame services.
- **Verdict:** Adapt as a separately evaluated compatibility mode, not the initial editor component.

### OpenVSCode Server
- **Purpose:** minimal VS Code fork for a remote server plus browser workbench. [README](https://github.com/gitpod-io/openvscode-server/blob/main/README.md)
- **Licence:** repository declares MIT. [License](https://github.com/gitpod-io/openvscode-server/blob/main/LICENSE.txt)
- **Maintenance evidence:** GitHub API reports repository not archived and last push 2026-09-24; latest stable GitHub release returned is **1.109.5**, 2026-02-20. These are observations, not proof of abandonment. [Repository API](https://api.github.com/repos/gitpod-io/openvscode-server), [release](https://github.com/gitpod-io/openvscode-server/releases/tag/openvscode-server-v1.109.5)
- **Platforms:** observed stable release assets are Linux x64, arm64 and armhf; do not assume native Windows/macOS support from them. [Release](https://api.github.com/repos/gitpod-io/openvscode-server/releases/latest)
- Its README describes localhost binding and connection-token options; it also warns the Docker image defaults to no connection token. [Security](https://github.com/gitpod-io/openvscode-server/blob/main/README.md#securing-access-to-your-ide)
- **Embedding:** web endpoint can be evaluated in a WebEngine view; supported iframe composition and Qt compatibility are unverified.
- **Footprint:** latest stable Linux x64 compressed archive is 76,686,959 bytes, not a runtime RAM measurement. [Release assets](https://api.github.com/repos/gitpod-io/openvscode-server/releases/latest)
- **Complexity:** server lifecycle, tokens, extension-host transport and full-workbench UX.
- **Alternative:** actively released code-server or a self-built Code-OSS server.
- **Verdict:** Defer pending current release and cross-platform evidence.

### Coder code-server
- **Purpose:** patched Code-OSS browser workbench plus server wrapper, password auth, sub-path hosting and other self-hosting changes. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#whats-the-difference-between-code-server-and-github-codespaces)
- **Licence:** MIT for code-server; audit upstream and extension components separately. [License](https://github.com/coder/code-server/blob/main/LICENSE)
- **Maturity:** stable release observed **4.145.2**, 2026-11-07, based on Code **1.145.2**. [Release](https://github.com/coder/code-server/releases/tag/v4.145.2)
- **Platforms:** that release lists Linux, macOS and Windows x64 archives, plus Linux/macOS arm64 archives; this is artifact availability, not a support guarantee. [Release assets](https://api.github.com/repos/coder/code-server/releases/latest)
- **Footprint:** the 4.145.2 Linux x64 archive is 236,495,502 bytes compressed; do not equate download size with memory. [Release assets](https://api.github.com/repos/coder/code-server/releases/latest)
- **Embedding:** browser-accessible workbench is documented, but iframe-specific support and Qt compatibility are unverified. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md)
- **Known issue:** browser interception of keyboard shortcuts is documented; application-specific Qt shortcut routing still needs measurement. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#how-do-i-make-my-keyboard-shortcuts-work)
- Webviews use service workers and require a secure context; its docs name localhost/127.0.0.1 as an option. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-do-web-views-not-work)
- **Complexity:** separate process, server authentication, updates, Open VSX, webview origins and a full competing IDE shell.
- **Alternative:** direct Code-OSS or OpenVSCode server.
- **Verdict:** Adapt as an optional compatibility experiment, not a mandatory first-milestone dependency.

### Eclipse Theia
- **Purpose:** modular, extensible cloud/desktop IDE framework with a VS Code extension-protocol implementation, not a VS Code fork. [Theia](https://github.com/eclipse-theia/theia/blob/master/README.md), [code-server comparison](https://github.com/coder/code-server/blob/main/docs/FAQ.md#whats-the-difference-between-code-server-and-theia)
- **Maturity:** project publishes releases and changelogs; its [API compatibility report](https://eclipse-theia.github.io/vscode-theia-comparator/status.html) tracks coverage against VS Code API rather than asserting 100% extension compatibility. [README](https://github.com/eclipse-theia/theia/blob/master/README.md)
- An API-surface report is not an end-to-end passing rate for all extensions: runtime behavior, contribution points and native dependencies still matter.
- **Licence:** EPL-2.0, with GPL-2.0-only plus Classpath exception identified as secondary licence; review applicability to the actual component and distribution. [Licence section](https://github.com/eclipse-theia/theia/blob/master/README.md#license)
- **Architecture:** browser or Electron desktop frontend plus backend services and plugin mechanisms; it is designed to compose IDE applications. [Composition](https://theia-ide.org/docs/composing_applications/), [plugin API](https://github.com/eclipse-theia/theia/blob/master/doc/Plugin-API.md)
- **Standalone plugin host:** no documented supported standalone reuse contract was verified; its plugin architecture should not be assumed independently reusable without Theia services.
- **Footprint:** a full IDE framework plus backend is qualitatively more than a single Monaco widget; no comparable current RAM or bundle figure was verified.
- **Platforms:** browser/desktop configurations are supported as project goals; verify native dependencies and OS targets of a chosen product build. [README](https://github.com/eclipse-theia/theia/blob/master/README.md#scope)
- **Complexity:** high if Axiom retains its own Qt workbench and duplicates Theia UI/backend responsibilities.
- **Alternatives:** CodinGame services or full Code-OSS web workbench.
- **Verdict:** Defer as an architectural alternative, not a small compatibility library.

### Microsoft Marketplace, Open VSX and extension licences
- **Marketplace:** code-server quotes Microsoft's terms as limiting Marketplace offerings to Visual Studio Products and Services and concludes non-Microsoft VS Code forks cannot use the Marketplace. [FAQ and quoted terms](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-cant-code-server-use-microsofts-extension-marketplace)
- The FAQ links a Microsoft-hosted [terms PDF](https://cdn.vsassets.io/v/M146_20190123.39/_content/Microsoft-Visual-Studio-Marketplace-Terms-of-Use.pdf); the current authoritative terms and applicability to Axiom require legal confirmation.
- Switching a gallery URL technically does not make using that gallery contractually permissible. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#how-do-i-use-my-own-extensions-marketplace)
- **Open VSX:** Eclipse's vendor-neutral open-source registry implementation provides server, web UI and publishing CLI; public instance is open-vsx.org. [Project](https://github.com/eclipse-openvsx/openvsx/blob/main/README.md)
- Registry implementation is **EPL-2.0**; this does not relicense extension packages. [Project licence](https://github.com/eclipse-openvsx/openvsx/blob/main/README.md#license)
- It supports a registry API and a VS Code-compatible gallery API; use the official [API documentation](https://github.com/eclipse-openvsx/openvsx/wiki/Registry-API) and validate exact endpoints/compatibility when integrating.
- Open VSX is not a complete mirror of Microsoft's Marketplace; code-server specifically documents missing proprietary extensions and recommends Open VSX. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-cant-code-server-use-microsofts-extension-marketplace)
- No verified current coverage percentage or live extension count is asserted.
- Public registry service terms and publisher obligations must be reviewed independently from the registry source licence; see [public-instance project](https://github.com/EclipseFdn/open-vsx.org).
- **Microsoft extensions:** Pylance, C/C++ cpptools, C# Dev Kit and Remote-* must each be assessed as distinct distributions; Code-OSS MIT is not permission to redistribute them.
- The code-server FAQ explicitly identifies Microsoft Remote SSH/Containers/WSL extensions as unavailable closed-source offerings. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-cant-code-server-use-microsofts-extension-marketplace)
- Exact current Pylance, cpptools and C# Dev Kit licence text could not be verified through the fetched pages; do not assert their exact clauses or permission to bundle.
- **C/C++ alternative:** [clangd](https://github.com/clangd/clangd) implements a language server; it is not a claim of feature equivalence to cpptools.
- **Python alternatives:** [Pyright](https://github.com/microsoft/pyright) and [basedpyright](https://github.com/DetachHead/basedpyright) are separately licensed projects, not Pylance binaries.
- **Rust alternative:** [rust-analyzer](https://github.com/rust-lang/rust-analyzer), with separately distributed extension and language server.
- **C# alternatives:** [OmniSharp](https://github.com/OmniSharp/omnisharp-roslyn) and [Roslyn](https://github.com/dotnet/roslyn); check server launch, runtime and current distribution licence separately.
- **Complexity:** publisher trust, dependency and VSIX scanning, update provenance, malware response, terms and platform-native binaries.
- **Verdict:** Adopt Open VSX as candidate; curate and verify each extension and avoid unlicensed Microsoft assets.

### Language Server Protocol and Rust integration
- **Purpose:** LSP standardizes editor/language-server communication over JSON-RPC for completion, navigation and related language features. [LSP](https://microsoft.github.io/language-server-protocol/)
- **Version:** official landing page calls **3.18** the latest specification. [LSP](https://microsoft.github.io/language-server-protocol/)
- Version is not a compatibility guarantee: use capability negotiation and test each server's actual implementation. [Specification](https://microsoft.github.io/language-server-protocol/specifications/specification-current/)
- Rust [lsp-types](https://crates.io/crates/lsp-types) supplies protocol types; check the version of specification covered by the selected release.
- Rust [async-lsp](https://crates.io/crates/async-lsp) and [tower-lsp](https://crates.io/crates/tower-lsp) are implementation options; do not assume identical client/server suitability or maintenance cadence without inspecting selected versions.
- TypeFox's examples demonstrate WebSocket JSON-RPC from browser to backend-hosted servers or a language server in a browser worker. [Examples](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#examples-overview)
- Axiom can supervise native language-server processes in Rust and bridge their stdio to an authenticated page transport; this is an architectural recommendation, not an upstream turnkey integration.
- **Platforms:** server binaries and runtime dependencies must exist for each OS/architecture; worker-based implementations have browser runtime restrictions. [Examples](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md#examples-overview)
- **Complexity:** document synchronization, URIs, cancellation, server lifecycle, client capabilities, encoding and workspace edits.
- **Alternatives:** Monaco providers only, language-specific integrations, or extension-host-managed clients.
- **Verdict:** Adopt LSP as the language-service boundary; select libraries after a narrow prototype.

### Debug Adapter Protocol and adapters
- **Purpose:** DAP separates debugger UI from the debug adapter that talks to the debugger/runtime. [DAP](https://microsoft.github.io/debug-adapter-protocol/)
- **Version:** official landing page lists specification **1.71.0**. [DAP](https://microsoft.github.io/debug-adapter-protocol/)
- Candidate Rust ecosystem crate: [dap](https://crates.io/crates/dap); crate maturity, latest version, licence and supported protocol revision require a fresh audit.
- **Adapters:** [CodeLLDB](https://github.com/vadimcn/codelldb) for LLDB; [debugpy](https://github.com/microsoft/debugpy) for Python; [js-debug](https://github.com/microsoft/vscode-js-debug) for JavaScript.
- An adapter's project licence, packaged binaries and runtime restrictions must be checked individually; a VS Code debugger extension is not automatically a standalone adapter distribution.
- **Platforms:** adapter binaries, debugger runtimes and architectures constrain support independently of DAP. [Implementations](https://microsoft.github.io/debug-adapter-protocol/implementors/adapters/)
- **Complexity:** launch/attach configuration, breakpoints, stack, variables, console, path mapping, lifecycle and capabilities; DAP does not supply the UI. [Overview](https://microsoft.github.io/debug-adapter-protocol/overview)
- **Alternatives:** debugger-specific integrations or VS Code extension-host-based debugger contributions.
- **Verdict:** Adapt after the editor/LSP milestone; prototype one adapter end to end.

### Qt WebEngine host, assets and communications
- **Purpose:** embed Chromium-based web content inside Qt; QML host is `WebEngineView`. [Qt overview](https://doc.qt.io/qt-6.11/qtwebengine-overview.html)
- **Current observed Chromium bases:** Qt WebEngine **6.11.3** uses Chromium **140.0.7339.264**, security version **155.0.8059.85**; **6.10.3** uses **134.0.6998.208**, security version **146.0.7680.164**. [Qt version table](https://wiki.qt.io/QtWebEngine/ChromiumVersions)
- The Qt table marks **6.12.0 unreleased** and describes a new WebEngine version scheme; its listed mapping is tentative, not a shipped promise. [Qt version table](https://wiki.qt.io/QtWebEngine/ChromiumVersions)
- Qt explains that base Chromium and backported security versions are distinct; check the actual deployed runtime via `qWebEngineChromiumVersion()` or `chrome://qt`. [Qt version table](https://wiki.qt.io/QtWebEngine/ChromiumVersions)
- **Licensing:** audit Qt WebEngine's actual distribution options and third-party notices for the chosen Qt build; do not assume Qt's entire stack is MIT. [Qt licensing](https://doc.qt.io/qt-6.11/qtwebengine-licensing.html)
- **Known browser-side concern:** code-server documents intercepted shortcuts and service-worker secure-origin requirements; these do not establish an identical Qt-specific defect. [FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#how-do-i-make-my-keyboard-shortcuts-work), [webviews](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-do-web-views-not-work)
- **Qt permissions:** clipboard access has settings/permission controls; design explicit user-mediated access rather than globally enabling it. [QWebEngineSettings](https://doc.qt.io/qt-6.11/qwebenginesettings.html)
- **Focus, keyboard, IME, clipboard, drag/drop, accessibility:** these are release-gate test areas, not verified universal Monaco/Qt failures.
- **Memory:** Qt WebEngine uses multiple processes; renderer count and incremental RSS depend on site isolation, workers, OS and workload. No defensible per-renderer number was verified. [Qt overview](https://doc.qt.io/qt-6.11/qtwebengine-overview.html)
- **QWebChannel:** exposes Qt objects to JavaScript via a transport and `qwebchannel.js`; a narrow allowlisted native bridge fits shell commands and notifications. [Qt WebChannel](https://doc.qt.io/qt-6.11/qtwebchannel-javascript.html)
- **WebSocket:** fits streaming bidirectional LSP/JSON-RPC to a Rust-owned service and can decouple it from Qt; a loopback listener still requires origin checks, authentication and lifecycle control. [TypeFox transport](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md)
- **Custom scheme handler:** `QWebEngineUrlSchemeHandler` serves requests for a registered scheme; it is an asset/request mechanism, not by itself a bidirectional RPC channel. [Handler](https://doc.qt.io/qt-6.11/qwebengineurlschemehandler.html)
- Register the scheme before Qt WebEngine objects, generally before application creation; use host-based syntax for same-origin policy. [Qt scheme](https://doc.qt.io/qt-6.11/qwebengineurlscheme.html)
- `SecureScheme` marks an origin potentially trustworthy; use only if asset authenticity, integrity and confidentiality justify it. [Qt flags](https://doc.qt.io/qt-6.11/qwebengineurlscheme.html#Flag-enum)
- `CorsEnabled` allows CORS participation; it is not a substitute for access-control policy. `FetchApiAllowed` and `ServiceWorkersAllowed` are distinct flags. [Qt flags](https://doc.qt.io/qt-6.11/qwebengineurlscheme.html#Flag-enum)
- Do not enable `ContentSecurityPolicyIgnored` or broad local-file access merely to make the editor work. [Qt flags](https://doc.qt.io/qt-6.11/qwebengineurlscheme.html#Flag-enum)
- `http://localhost` is a documented secure-context option for code-server webviews, but it creates a network listener and requires authorization against other local processes and malicious pages. [code-server FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-do-web-views-not-work)
- Custom secure scheme support for all needed worker, iframe, service-worker and VS Code webview combinations must be experimentally verified; secure classification alone proves none of those combinations.
- **Alternatives:** custom secure asset origin with QWebChannel; authenticated loopback HTTP/WebSocket; full server-hosted workbench.
- **Verdict:** Adapt Qt WebEngine; start with bundled assets and a minimal bridge, and select origin only after worker/security experiments.

## Architectural implications for Axiom
- **Stage 0 — contract:** keep canonical documents, workspace identity, access policy and process supervision outside editor internals.
- Define versioned commands/events for open, edit, dirty state, save, selection and diagnostics; support asynchronous failure and cancellation.
- Decide which side owns unsaved buffers and resolve external changes and concurrent agent edits explicitly.
- Use stable document URIs and translate them consistently among Monaco, Rust, LSP and remote workspaces. [Monaco models](https://github.com/microsoft/monaco-editor/blob/main/README.md#concepts)
- **Stage 1 — plain Monaco:** pin 0.57.0, ship ESM/CSS/fonts/workers offline, test in Qt WebEngine on Linux/macOS/Windows. [Release](https://github.com/microsoft/monaco-editor/releases/tag/v0.57.0)
- Provide basic syntax, model lifecycle, undo, search, multi-file tabs and a narrow native bridge.
- Explicit ceiling: no VS Code extension host, workbench or built-in TextMate grammar support. [Monaco FAQ](https://github.com/microsoft/monaco-editor/blob/main/README.md#faq)
- **Stage 2 — language intelligence:** supervise clangd, rust-analyzer and one Python server; bridge LSP and negotiate capabilities.
- Start with a narrow Monaco-LSP implementation or evaluate TypeFox's documented aligned dependency set. [TypeFox](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md)
- Add DAP separately and implement debugger UI; a language server does not provide debugging. [DAP](https://microsoft.github.io/debug-adapter-protocol/)
- **Stage 3 — selective compatibility:** spike CodinGame service overrides, VS Code model references, TextMate and a browser-compatible VSIX. [CodinGame](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md)
- Explicit ceiling: browser-worker-compatible extensions and implemented service/contribution surfaces only; Node `main` extensions require a separate Node host. [Host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- **Stage 4 — Node extension compatibility:** experiment with a version-matched Code-OSS-compatible remote agent and Node extension host, local or remote; ship an authenticated lifecycle-managed process.
- Test `extensionKind`, APIs, webviews, native dependencies, file access, platform and legal permissions per extension. [Host](https://code.visualstudio.com/api/advanced-topics/extension-host)
- Explicit ceiling: not all desktop UI-only, proprietary, proposed-API, binary-specific or Microsoft-service-bound extensions will work or be licensable.
- **Alternative branch:** expose full code-server/Code-OSS web workbench as a separate mode if broad compatibility outweighs duplicate UI and server size; do not imply it is the same migration as adding a Monaco service override.
- Maintain a tested matrix of Qt version × OS × editor/service versions × extension × host kind × licence.
- Keep extension execution outside the trusted native bridge; enforce narrowly scoped operations and protect local listener endpoints.
- Preserve an escape hatch: stable Axiom document/tool interfaces should allow swapping the presentation and extension strategy.

## Risks
- **Security:** webview content, VSIXes and Node hosts can execute code; a bridge that exposes general filesystem or process access defeats the separation.
- **Origin confusion:** permissive CORS, opaque origins, token leakage, navigations and localhost cross-site requests can expose privileged APIs. [Qt scheme security](https://doc.qt.io/qt-6.11/qwebengineurlscheme.html)
- **Version drift:** CodinGame's latest 39.0.4 is not the TypeFox 11.0.3 documented 37.6.0 alignment. [CodinGame](https://github.com/CodinGame/monaco-vscode-api/releases/tag/v39.0.4), [TypeFox](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md)
- **Licensing:** repository, branded binary, service terms, individual extension and native adapter are separate legal layers. [VS Code](https://github.com/microsoft/vscode/blob/main/README.md), [Marketplace FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md#why-cant-code-server-use-microsofts-extension-marketplace)
- **UX:** shortcut routing, focus, IME, accessibility and clipboard problems can make a technically functional editor unusable; test instead of assuming Chromium parity.
- **Footprint:** full server distributions can be hundreds of MB compressed; renderer, Node and language-server memory must be measured separately. [code-server assets](https://api.github.com/repos/coder/code-server/releases/latest)
- **Compatibility:** a VS Code API surface or successful extension installation does not prove activation, feature completeness or licensing.
- **Supply chain:** independently verify registry publisher identity, updates, archive contents, dependency licences and platform binaries.
- **Privacy:** prevent workspace contents or credentials leaking into optional registries, telemetry, servers or untrusted extension webviews.

## Unverified / needs experiment
- Current authoritative Microsoft Marketplace terms, applicability to Axiom, and exact separate Pylance, cpptools, C# Dev Kit and Remote-* licence clauses; Marketplace fetches were incomplete or failed.
- Open VSX public-instance current terms, rate limits, and numerical coverage for an agreed representative extension corpus.
- Latest crates.io versions, licence expressions, release activity and LSP 3.18 coverage of `lsp-types`, `async-lsp`, `tower-lsp`, and DAP crate candidates.
- CodinGame remote server's supported binary provenance, exact installation commands, version pairing, protocol authentication and Windows/macOS/Linux behavior; wiki content was truncated by fetch.
- The remote-agent protocol's exact wire framing, stability guarantees and negotiation behavior for each pinned client/server pair.
- Supported full workbench iframe embedding for Code-OSS, OpenVSCode and code-server, including CSP/frame headers and extension webviews.
- Whether a standalone Theia plugin host is supported without Theia backend/frontend services; measure dependencies before planning reuse.
- Qt WebEngine custom secure scheme behavior for module workers, cross-origin iframes, WASM, service workers, CSP, clipboard and VS Code extension webviews on each target OS.
- Whether localhost versus custom-scheme packaging is safer and more reliable for Axiom's exact deployment; threat-model local processes and browser-origin attacks.
- Concrete Qt-specific Monaco issues for keyboard/focus, IME composition, clipboard, drag/drop and screen readers; reproduce and record Qt version and OS rather than asserting universal defects.
- Renderer/process incremental RSS, peak memory, startup time and bundle size for plain Monaco, CodinGame stack and full workbench, using identical workloads.
- Compatibility corpus: at least a theme, TextMate grammar, web language extension, Node workspace extension, debugger, webview and UI-only extension.
- Licensing and actual feature parity of proposed open language-server and debugger alternatives for each redistribution target.
- OpenVSCode release/maintenance plans after the February stable release; non-archived status and September push do not establish a maintenance commitment.

## Sources
- [Monaco README and integration guidance](https://github.com/microsoft/monaco-editor/blob/main/README.md); [0.57.0 release](https://github.com/microsoft/monaco-editor/releases/tag/v0.57.0).
- [CodinGame README](https://github.com/CodinGame/monaco-vscode-api/blob/main/README.md); [39.0.4 release](https://github.com/CodinGame/monaco-vscode-api/releases/tag/v39.0.4); [remote-server guide](https://github.com/CodinGame/monaco-vscode-api/wiki/How-to-install-and-use-VSCode-server-with-monaco%E2%80%90vscode%E2%80%90api).
- [TypeFox README, package versions and compatibility](https://github.com/TypeFox/monaco-languageclient/blob/main/README.md).
- [Code-OSS README](https://github.com/microsoft/vscode/blob/main/README.md); [Microsoft extension host documentation](https://code.visualstudio.com/api/advanced-topics/extension-host).
- [OpenVSCode Server README](https://github.com/gitpod-io/openvscode-server/blob/main/README.md); [latest stable API](https://api.github.com/repos/gitpod-io/openvscode-server/releases/latest).
- [code-server FAQ](https://github.com/coder/code-server/blob/main/docs/FAQ.md); [latest stable API](https://api.github.com/repos/coder/code-server/releases/latest).
- [Theia README](https://github.com/eclipse-theia/theia/blob/master/README.md); [VS Code API comparison](https://eclipse-theia.github.io/vscode-theia-comparator/status.html).
- [Open VSX implementation](https://github.com/eclipse-openvsx/openvsx/blob/main/README.md); [public service](https://github.com/EclipseFdn/open-vsx.org).
- [LSP official specification](https://microsoft.github.io/language-server-protocol/); [DAP official specification](https://microsoft.github.io/debug-adapter-protocol/).
- [Qt WebEngine Chromium version table](https://wiki.qt.io/QtWebEngine/ChromiumVersions); [custom scheme security](https://doc.qt.io/qt-6.11/qwebengineurlscheme.html); [WebChannel](https://doc.qt.io/qt-6.11/qtwebchannel-javascript.html).
