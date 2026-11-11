# Prior art: open-source IDEs, dev platforms, and Rust building blocks for Axiom
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.

**Tool status for this run:** `web_search` worked and returned results. `web_fetch` worked for raw GitHub files (LICENSE files), the crates.io JSON API, the Lapce, Zed, VS Code extension-host and tasks-appendix pages, the RustSec advisory, and the Qt Creator 18 GitHub tag URL. Many GitHub repository HTML pages, the Qt Creator 18 blog, the JetBrains Fleet blog, the Eclipse Theia 1.75 blog, the ACP transports page, and the VS Code tasks appendix body returned only navigation chrome or truncated text. Claims that rest only on those pages are marked unverified. The `doc.qt.io` Qt Creator Kits page returned 404 for the URL tried.

---

## Summary and recommendations

**Per technology (Adopt / Adapt / Reject / Copy-design):**

- **Zed (GPUI, Rust, WASM extensions)**: Adapt. Its extension manifest, dev-install loop, and per-extension licence gate are worth copying. Its GPUI UI layer does not fit a Qt shell.
- **Lapce (Rust, Apache-2.0)**: Adapt. Its UI/proxy split is a design reference only; the proxy and plugin details are unverified.
- **Helix**: Reject as a platform. Consider only its modal-editing and tree-sitter ideas. Entirely unverified in this run.
- **Qt Creator 18**: Adapt the Kits (toolchain) model and the plugin-architecture concept. Do not copy code: the licence is GPL-3.0 with exceptions (unverified here), which is incompatible with a permissive Axiom without legal review.
- **KDevelop / Kate**: Adapt, reference only. KTextEditor and the LSP client are unverified.
- **JetBrains Fleet**: Reject as a dependency or design source. A December 2025 "The Future of Fleet" post exists, but its content was not retrievable, so 2026 status is unknown.
- **IntelliJ platform SDK/toolchain model**: Copy-design, after reading the docs (unverified here).
- **Eclipse Theia / Theia AI**: Adapt framework patterns. Reject as shell: Electron/browser-based TypeScript conflicts with the Qt mandate.
- **VS Code process model**: Copy-design. The extension-host placement (`extensionKind`) and lazy activation events are documented. Reject embedding VS Code or its Node extension host in Axiom.
- **Android Studio Layout Inspector**: Copy-design for the visual-intelligence engine. Architecture is unverified here, so read the docs before designing.
- **Xcode toolchain selection**: Copy-design (unverified here).
- **Gitpod / Coder / DevPod**: Adapt the devcontainer-style workspace concept. Reject as a runtime dependency for v1. Only secondary sources were seen.
- **Tauri / Electron / Wails / Slint**: Reject as shell, since Axiom is Qt. Footprint figures were not verified.
- **Wasmtime (Apache-2.0 WITH LLVM-exception)**: Adopt only if in-process WASM plugins are chosen. Pin to a patched version (see RUSTSEC-2026-0363).
- **Extism**: Evaluate only. Licence and platform are unverified.
- **Out-of-process JSON-RPC (LSP / DAP / MCP / ACP)**: Adopt for third-party tools and language servers. ACP has v1 and v2 protocol docs; details are unverified.

**Rust crates (licence and maintenance from crates.io, as of 2026-11-11):**

- `tokio` 1.54.1 (MIT), last release 2026-10-21: Adopt.
- `rusqlite` 0.42.0 (MIT), last release 2026-11-11: Adopt.
- `gix` 0.90.0 (MIT OR Apache-2.0), last release 2026-10-22: Adopt. Still pre-1.0, so expect API churn.
- `notify` 8.2.2 (CC0-1.0), last stable 2026-11-08: Adopt. Pin 8.2.x while 9.0 is at rc.
- `schemars` 1.2.3 (MIT), last release 2026-10-12: Adopt.
- `ts-rs` 12.0.1 (MIT), last release 2026-01-31: Adapt. Maintenance looks slower.
- `specta` 2.0.0-rc.28 (MIT), latest stable line 1.0.5 (2023): Watch. Do not depend on the 2.x rc in production.
- `portable-pty` 0.9.0 (MIT), last release 2025-02-11: Adapt. Vet before adoption, or wrap it behind an interface.
- `wasmtime` 50.0.1 stable (Apache-2.0 WITH LLVM-exception), 51.0.0-rc.1 on 2026-11-05: Adopt only with patched versions.
- `ignore`, `grep-searcher`, `git2`, `sqlx`, `serde`, `typeshare`, `tracing`, `keyring`, `which`, `tree-sitter`, `tantivy`: unverified in this run. See Unverified section.

**Ranked list:**

*Reuse directly*
1. `tokio` (MIT, very active, 1.54.1).
2. `rusqlite` (MIT, active, 0.42.0; bundled SQLite via a feature flag).
3. `schemars` (MIT, active, 1.2.3) for JSON Schema of IPC types.
4. `notify` (CC0-1.0, active; pin 8.2.x until 9.0 final).
5. `gix` (MIT OR Apache-2.0, active; pin tightly because it is pre-1.0).
6. `wasmtime` (Apache-2.0 WITH LLVM-exception), only if WASM plugins are adopted, and only at a patched version.

*Copy the design*
1. Out-of-process JSON-RPC for language servers, debug adapters, MCP, and ACP.
2. VS Code's extension-host placement (`extensionKind`) and lazy activation events.
3. Zed's extension manifest, dev-install loop, and licence gate for third-party extensions.
4. Qt Creator's Kits model for toolchain detection (design only, after reading the docs).
5. Lapce's UI/proxy split (verify details first).
6. Android Layout Inspector's approach for mapping running UI to source (verify first).

*Avoid*
1. Fleet as a dependency or design source (status unknown).
2. Electron, Tauri, Wails, or Slint as the shell, given the Qt mandate.
3. Embedding VS Code or Theia wholesale in the WebEngine editor (feasibility unverified).
4. `wasmtime` versions below the RUSTSEC-2026-0363 patched lines.
5. Pre-release crates in production: `notify` 9.0.0-rc.*, `specta` 2.0.0-rc.*, `wasmtime` 51.0.0-rc.1.
6. `portable-pty` as the sole PTY layer without vetting (last release Feb 2025).
7. Copying GPL-3.0 code (Qt Creator; Zed's GPL parts) into a permissively licensed Axiom.

---

## Findings

### Zed (GPUI, Rust)
- **Purpose:** High-performance multiplayer code editor; repo description from [github.com/zed-industries/zed](https://github.com/zed-industries/zed) (page title, fetched).
- **Maturity:** Active. Extension licence rules have applied since 1 Oct 2025 ([Zed docs: License Requirements](https://zed.dev/docs/extensions/publishing/license-requirements.md)).
- **Licence:** The repo ships `LICENSE-GPL`, verified as GPLv3 text in the [raw file](https://raw.githubusercontent.com/zed-industries/zed/main/LICENSE-GPL). The AGPL and Apache portions of the licence split are unverified (no LICENSE-AGPL or LICENSE-APACHE fetched). For extensions, Zed accepts Apache-2.0, BSD-2-Clause, BSD-3-Clause, CC-BY-4.0, GPL-3.0, LGPL-3.0, MIT, Unlicense, and zlib. The rule covers extension code only, not the language servers or tools it downloads ([docs](https://zed.dev/docs/extensions/publishing/license-requirements.md)).
- **Platform restrictions:** Extension Rust code compiles to `wasm32-wasip2`. Grammars also need wasi-sdk (`WASI_SDK_PATH`). In extension WASM, `cfg` attributes and `std::env::var` do not behave as on native code. Use `zed_extension_api::current_platform` and the `Worktree` API ([docs](https://zed.dev/docs/extensions/developing-extensions)).
- **Extension model:** An extension is a git repo with `extension.toml`. Extensions can provide languages, debuggers, themes, icon themes, snippets, and MCP servers. Rust code is required only for language-server, context-server, and debugger extensions ([docs](https://zed.dev/docs/extensions/developing-extensions)). Dev extensions install from a local directory and override the published version.
- **Process architecture / IPC:** Unverified. The GitHub body was not readable. Inference only: extensions reach external tools through the WASM API, and language servers run as separate binaries.
- **Integration complexity (for Axiom):** Medium. Copying the manifest and dev-install flow is cheap. Hosting a WASM-component runtime is not.
- **Alternatives:** VS Code extension host; Lapce WASI plugins (see below).
- **Verdict:** **Adapt.** Copy the manifest, dev-install loop, and licence gate. Do not adopt GPUI. Treat WASM as suitable for pure-compute extensions, not process-level tooling.

### Lapce (Rust, Floem UI)
- **Purpose:** Rust code editor. Architecture page at [docs.lapce.dev/development/architecture](https://docs.lapce.dev/development/architecture) exists (fetched, body truncated).
- **Maturity:** Active project, but details of 2026 status unverified.
- **Licence:** Apache-2.0 (verified: [raw LICENSE](https://raw.githubusercontent.com/lapce/lapce/master/LICENSE)).
- **Platform restrictions:** Unverified in this run. Expect Linux/macOS/Windows, but this was not checked.
- **Process architecture / plugins:** The UI and proxy split is named in the docs page, but the body was truncated. The claim that plugins are WASI-based via "PSP" is unverified. The PSP-named plugin doc URL returned 404 in this run.
- **Integration complexity:** Low to read, high to reuse, since it is built on Floem rather than Qt.
- **Alternatives:** Zed (above); Helix (below).
- **Verdict:** **Adapt.** Apache-2.0 makes it permissively reusable with attribution. Study the proxy split only after verifying it in source.

### Helix
- **Purpose:** Modal terminal editor with tree-sitter integration and LSP. All facts unverified in this run; no Helix page was fetched.
- **Licence / platform / plugins:** Unverified. Helix has no general plugin system as far as known (unverified).
- **Verdict:** **Reject as a platform.** Study its modal-editing and tree-sitter integration as an optional editor-mode reference only.

### Qt Creator 18
- **Purpose:** Qt IDE with plugin architecture, Kits, QML Designer, and QML Live Preview. Release tag [v18.0.0](https://github.com/qt-creator/qt-creator/releases/tag/v18.0.0) exists (URL fetched, body not readable). A Qt blog post "Qt Creator 18 released" exists ([qt.io](https://www.qt.io/blog/qt-creator-18-released)); its body was not readable.
- **Maturity:** Released, major version 18. Specific feature list unverified.
- **Licence:** GPL-3.0 with Qt exceptions. Unverified in this run; treat as a hypothesis until the repo's COPYING is read.
- **Platform restrictions:** Linux, macOS, Windows are supported by the product. Unverified in this run.
- **Kits / toolchain detection:** The Kits concept (compiler, Qt version, debugger, device bound to a project) is widely known. The [Kits how-to](https://doc.qt.io/qtcreator/creator-how-to-create-kits.html) URL returned 404 in this run, so the detailed auto-detection model is unverified.
- **Plugin architecture / QML Live Preview / Lua or extension API / LSP client:** All unverified in this run.
- **Integration complexity:** Low if Axiom is Qt-based (same toolkit). High if code is copied, because of licence.
- **Verdict:** **Adapt.** Adopt the Kits concept as a design reference. Do not copy code unless Axiom accepts GPL-3.0 for that module.

### KDevelop / Kate
- **Purpose:** KDevelop is a KDE IDE; Kate is a KDE editor built on KTextEditor. Unverified: no page fetched in this run.
- **Licence / platform / LSP client:** Unverified. Expected GPL/LGPL with KDE Frameworks (unverified).
- **Verdict:** **Adapt, reference only.** The KTextEditor component model and LSP client are candidates to read for editor-component design. Not verified here.

### JetBrains Fleet (status 2026)
- **Status:** A JetBrains blog post "[The Future of Fleet](https://blog.jetbrains.com/fleet/2025/12/the-future-of-fleet/)" exists (URL dated 2025/12). Its body was not readable (only navigation returned). **2026 status is unverified.** Do not assert that Fleet was discontinued unless the body is read.
- **Licence / platform / architecture:** Unverified in this run. Fleet's Kotlin/JVM-based backend and thin client are widely described (unverified).
- **Verdict:** **Reject as dependency or design source.** Its uncertain status is itself the reason. Do not rely on it.

### IntelliJ platform SDK / toolchain model
- **Purpose:** SDK types per project and per module, configured through the IDE. All details unverified in this run.
- **Licence:** Apache-2.0 for the community platform (unverified in this run).
- **Verdict:** **Copy-design** after reading the SDK docs. Kits-style detection (see Qt Creator) is the closest analogue.

### Eclipse Theia & Theia AI
- **Purpose:** GitHub description: "Eclipse Theia is a cloud & desktop IDE framework implemented in TypeScript" ([eclipse-theia/theia](https://github.com/eclipse-theia/theia), page title).
- **Maturity:** Active. A 1.75 release post dated 2026-09-10 exists ([EclipseSource blog](https://eclipsesource.com/blogs/2026/09/10/eclipse-theia-1-75-release-news-and-noteworthy/)); body truncated. A 2026-02 community release post also exists ([EclipseSource](https://eclipsesource.com/blogs/2026/03/26/the-eclipse-theia-community-release-2026-02/)).
- **Licence:** Eclipse Public License (EPL-2.0 expected). **Unverified in this run.**
- **Platform restrictions:** Browser and Electron; desktop via Electron. Unverified detail.
- **Theia AI:** Exists as an AI layer in Theia. Architecture unverified.
- **Integration complexity:** High. TypeScript/Node runtime in a Qt process.
- **Verdict:** **Adapt framework patterns** (extension contribution points, AI-provider abstraction). **Reject as shell or embedded runtime** given the Qt mandate.

### VS Code process model
- **Extension host (verified):** Extension host runtimes are Node.js (local and remote) and a browser WebWorker runtime (web). Extensions declare `extensionKind` as `ui` or `workspace`, which decides where they run. Activation events load extensions lazily. The host is designed so extensions cannot block startup or modify the UI ([VS Code docs: Extension Host](https://code.visualstudio.com/api/advanced-topics/extension-host)).
- **Main / renderer / shared process / pty host:** Named in the task description but **unverified in this run** (the process-architecture page was not fetched).
- **Tasks and problem matchers:** The [tasks appendix](https://code.visualstudio.com/docs/reference/tasks-appendix) exists (fetched, body truncated). Problem-matcher behaviour is unverified in this run.
- **Licence:** The VS Code product is MIT (unverified in this run). The Microsoft marketplace terms are separate.
- **Verdict:** **Copy-design** for extension-host placement and lazy activation. **Reject** embedding VS Code or its Node extension host in Axiom. Problem-matcher design is worth reading once the appendix body is available.

### Android Studio Layout Inspector
- **Purpose:** Inspects the running app's view hierarchy in a debug session. [Android docs](https://developer.android.com/studio/debug/layout-inspector?hl=en) exist (URL from search, body not fetched).
- **Architecture / IPC / licence:** Unverified in this run. Android Studio itself is not open source; the inspector's protocol and agent are unverified.
- **Relevance to Axiom:** This is the closest prior art for the visual-intelligence engine (mapping running UI elements to source). Read the docs before designing.
- **Verdict:** **Copy-design** (concept only). Do not assume reusable code.

### Xcode toolchain selection
- **Purpose:** Per-project SDK and toolchain selection. Unverified in this run; no Apple page fetched.
- **Licence:** Proprietary.
- **Verdict:** **Copy-design** concept only (selectable toolchain per project and per build configuration). Unverified detail.

### Gitpod / Coder / DevPod
- **Purpose:** Cloud or self-hosted development workspaces. Only secondary sources were seen: a 2026 comparison of Codespaces, Coder, and Ona ([easyspawn](https://easyspawn.com/blog/cloud-development-environments-compared)) and a 2026 list of self-hosted Gitpod alternatives ([canireplaceit](https://canireplaceit.com/en/alternatives/gitpod)). Neither is primary.
- **Licence / platform / status:** Unverified in this run.
- **Verdict:** **Adapt the workspace concept** (devcontainer-defined toolchains, remote workspace). **Reject as a runtime dependency** for v1.

### Tauri / Electron / Wails / Slint (shells vs Qt)
- **Comparison source:** A 2026 deep-dive covers Tauri 2, Electron 34, Wails 3, NeutralinoJS, Flutter Desktop, Sciter, Slint, and Iced ([TT Lab](https://ttlab.hopto.org/blog/culture/2026-05-16-cross-platform-desktop-apps-2026-tauri-2-electron-wails-neutralinojs-flutter-desktop-sciter-deep-dive?lang=en)). Footprint figures were not fetched. A Zenn article measures memory use across 15 desktop frameworks ([Zenn](https://zenn.dev/mizugeeks/articles/1019cf2353d343#1)); not fetched.
- **Licence / footprint / maturity:** Unverified in this run for every framework. Do not quote numbers from these sources without reading them.
- **Verdict:** **Reject as shell** given the Qt mandate. Revisit only if the Qt decision is reopened. Slint is the Rust-native option to note; its licence is unverified here.

### WASM component model / Wasmtime
- **Wasmtime licence (verified):** Apache-2.0 WITH LLVM-exception ([crates.io metadata](https://crates.io/api/v1/crates/wasmtime); [raw LICENSE](https://raw.githubusercontent.com/bytecodealliance/wasmtime/main/LICENSE)).
- **Maturity (verified):** Stable line 50.0.1 is the crates.io `max_stable_version`. 51.0.0-rc.1 was published 2026-11-05. The crate has about 44M total downloads and 239 published versions. MSRV is 1.97 for 50.x and 51.x. Edition 2024.
- **Feature flags (verified):** `component-model`, `component-model-async`, `pooling-allocator`, `cranelift`, `winch`, `pulley` (interpreter), and `cache` are present in the published 51.0.0-rc.1 feature table. A WASI feature flag was not seen in that table and is unverified.
- **Security (verified):** [RUSTSEC-2026-0363](https://rustsec.org/advisories/RUSTSEC-2026-0363.html): component-model handle tables are capped only per instance and charged to no limit. Reported and issued 2026-10-14. CVSS 3.1 6.2 (Medium), local availability impact. Patched in `>=36.0.18 <37`, `>=48.0.6 <49`, and `>=49.0.3`. A vuldb entry lists CVE-2026-119134 for the same class, with affected versions up to 36.0.17 / 48.0.5 / 49.0.2 ([vuldb](https://vuldb.com/vuln/428620#1)); this is a search snippet, not a primary read.
- **Platform restrictions:** Cross-platform (Linux, macOS, Windows) is expected of Wasmtime. Not verified in this run. The `pulley` interpreter exists as a portable fallback (feature flag verified).
- **Capability security:** WASI preopens and capability-based imports are the mechanism. Not verified in this run.
- **Developer ergonomics:** Zed uses WIT and `wasm32-wasip2`, which shows the toolchain is usable by third parties (verified via Zed docs). Language choice for plugin authors is wide (any language targeting WASI components). Unverified detail.
- **Integration complexity:** Medium to high. A WIT interface must be designed and versioned.
- **Verdict:** **Adopt for pure-compute extensions only**, at a patched version (50.0.1 or later stable line). Do not use it for tools that need native processes, sockets, or PTYs.

### Extism
- **Purpose:** "The framework for building with WebAssembly (wasm). Easily & securely load wasm modules, move data, call functions" ([github.com/extism/extism](https://github.com/extism/extism), page title).
- **Licence / platform / maturity:** Unverified in this run. A Debian DFSG review of its Go SDK exists (search result only), which is not evidence of the Rust host licence.
- **Verdict:** **Evaluate only.** Raw Wasmtime gives more control over capability and resource limits. Choose Extism only if its plugin ABI is wanted.

### Out-of-process JSON-RPC (LSP, DAP, MCP, ACP)
- **Isolation model:** An OS process boundary, not a sandbox. Any plugin that runs as a child process has the child's privileges, so it needs its own permission model.
- **ACP:** The Agent Client Protocol has docs at agentclientprotocol.com. A v1 transports page and a v2 transports page both exist ([ACP v1 transports](https://agentclientprotocol.com/protocol/v1/transports); [ACP v2 transports](https://agentclientprotocol.com/protocol/v2/transports.md)). Bodies were not readable here, so the message format and transports are unverified. Zed docs have an external-agents page, which shows ACP is used by Zed ([docs](https://zed.dev/docs/ai/external-agents)); not fetched.
- **MCP:** The VS Code docs site links an MCP registry and an MCP dev guide (navigation links on the [extension host page](https://code.visualstudio.com/api/advanced-topics/extension-host), fetched). MCP protocol details were not fetched and are unverified in this run.
- **Verdict:** **Adopt** out-of-process JSON-RPC for language servers, debug adapters, and agent protocols (MCP, ACP). Version-pin ACP because it has v1 and v2 specs.

---

## Rust building blocks

Crates.io metadata fetched 2026-11-11. "Last release" is the newest non-yanked version's `updated_at`.

### `tokio`
- **Purpose:** Async runtime. Latest 1.54.1 (MIT), published 2026-10-21. 1.54.0 was yanked the same day ([crates.io](https://crates.io/api/v1/crates/tokio)).
- **Maturity:** Very high (over 1.1B total downloads; maintained by Alice Ryhl).
- **Licence:** MIT.
- **Platform restrictions:** Linux, macOS, Windows. Feature `io-uring` is Linux-only (feature table verified; platform restriction assumed from name, unverified).
- **Integration:** Low. Feature flags `rt-multi-thread`, `net`, `process`, `signal`, `fs`, `sync`, `time`, `macros`.
- **MSRV:** 1.85.
- **Verdict:** **Adopt.**

### `portable-pty`
- **Purpose:** Cross-platform PTY abstraction used by WezTerm ([crates.io](https://crates.io/api/v1/crates/portable-pty)). Latest 0.9.0 (MIT), published 2025-02-11. Owner Wez Furlong. Repo `wezterm/wezterm`.
- **Maturity:** Widely used (about 27.8M downloads) but **no release in about 21 months** as of 2026-11-11.
- **Licence:** MIT.
- **Platform restrictions:** Windows uses ConPTY, Unix uses openpty (unverified in this run from the crate body).
- **Integration:** Medium. Wrap behind an Axiom `Pty` trait so it can be replaced.
- **Verdict:** **Adapt.** Vet it on Windows ConPTY before adoption, and keep the abstraction.

### `notify`
- **Purpose:** Cross-platform file watching. Stable 8.2.2 (CC0-1.0), published 2026-11-08. 9.0.0-rc.8 (CC0-1.0) published 2026-11-09 ([crates.io](https://crates.io/api/v1/crates/notify)).
- **Maturity:** Very high (about 195M total downloads). Maintained by Yuki Okushi (JohnTitor).
- **Licence:** CC0-1.0 (public-domain dedication). Permissive in practice, no attribution needed.
- **Platform:** `macos_fsevent` is a default feature; `macos_kqueue` and `freebsd_inotify` are optional. The 9.x line adds `tokio` and `futures` features.
- **MSRV:** 1.88 for the 8.2.x and 9.x rc lines.
- **Verdict:** **Adopt 8.2.x.** Move to 9.x after it is stable.

### `ignore` and `grep-searcher` (ripgrep libraries)
- **Purpose:** `ignore` implements gitignore-aware directory walking; `grep-searcher` is the ripgrep search engine core. Not fetched in this run.
- **Licence / maintenance / MSRV:** Unverified in this run (both are believed to be MIT or Unlicense, from memory).
- **Verdict:** **Adopt (provisional)** for the search subsystem. Verify licence and status before committing.

### `gix` (gitoxide) vs `git2`
- **`gix`:** Latest 0.90.0 (MIT OR Apache-2.0), published 2026-10-22; 0.89.0 on 2026-10-08. Maintained by Sebastian Thiel (Byron). Edition 2024, MSRV 1.88. Repo `GitoxideLabs/gitoxide`. Still pre-1.0 (0.x), so expect breaking changes between minor releases. Feature flags for status, worktree, diff, and credentials are present ([crates.io](https://crates.io/api/v1/crates/gix)).
- **`git2`:** Rust bindings to libgit2. Licence and status unverified in this run. Expected to be MIT OR Apache-2.0 for the bindings and GPL-2.0-with-linking-exception for libgit2 (from memory).
- **Verdict:** **`gix` Adopt** (pin tightly). **`git2` Evaluate** only if `gix` lacks a needed feature. Check the libgit2 licence before choosing.

### `rusqlite` vs `sqlx`
- **`rusqlite`:** Latest 0.42.0 (MIT), published 2026-11-11; 0.41.1 on 2026-10-28. Maintained by gwenn. About 151M total downloads. MSRV 1.89 for 0.42. The `bundled` feature compiles SQLite in; `ffi-sqlite-wasm-rs` is a default feature in 0.42 ([crates.io](https://crates.io/api/v1/crates/rusqlite)).
- **`sqlx`:** Not fetched in this run. Licence and status unverified. It is async and multi-database, which Axiom does not need for a local index.
- **Verdict:** **`rusqlite` Adopt.** `sqlx` **Reject** for a local single-file index unless async DB access is required.

### `serde`
- **Purpose:** Serialization framework. Not fetched in this run. Believed MIT OR Apache-2.0 and very widely used (unverified here).
- **Verdict:** **Adopt (provisional).** Use it as the base for all IPC types.

### `schemars` vs `typeshare` vs `ts-rs` vs `specta` (Rust to TypeScript / JSON Schema)
- **`schemars`:** Latest 1.2.3 (MIT), published 2026-10-12. Maintained by Graham Esau (GREsau). About 638M total downloads. MSRV 1.74. `derive` feature enables the proc-macro. Generates JSON Schema, not TypeScript ([crates.io](https://crates.io/api/v1/crates/schemars)). **Adopt.**
- **`ts-rs`:** Latest 12.0.1 (MIT), published 2026-01-31; no release in about 9 months. Maintained by NyxCode. About 22.7M downloads. Generates TypeScript directly from Rust types ([crates.io](https://crates.io/api/v1/crates/ts-rs)). **Adapt**, with a freeze note.
- **`specta`:** Latest 2.0.0-rc.28 (MIT), published 2026-11-05. Latest stable is 1.0.5 (2023-07-17). Maintained by Oscar Beaumont (oscartbeaumont; the newest rc is trusted-publisher-only). Repo `specta-rs/specta` ([crates.io](https://crates.io/api/v1/crates/specta)). **Watch**; do not use 2.x rc in production.
- **`typeshare`:** Not fetched in this run. Unverified.
- **Verdict:** Use `schemars` for the schema contract and generate TS from it, or use `ts-rs` with a pinned version. Re-evaluate `specta` once 2.0 is stable.

### `tracing`
- **Purpose:** Structured logging and spans. Not fetched in this run. Believed MIT and very widely used (unverified).
- **Verdict:** **Adopt (provisional).** Use `tracing-subscriber` for output.

### `keyring`
- **Purpose:** OS credential storage (macOS Keychain, Windows Credential Manager, Linux Secret Service). Not fetched. Licence and platform restrictions unverified; Linux Secret Service needs a running daemon (from memory, unverified).
- **Verdict:** **Evaluate.** Test on a headless Linux session before committing.

### `which`
- **Purpose:** Finds executables on `PATH`. Not fetched. Believed MIT (unverified).
- **Verdict:** **Adopt (provisional)** for toolchain detection; pair with Kits-style probing.

### `tree-sitter`
- **Purpose:** Incremental parser library used by Zed and Helix. Not fetched. Licence believed MIT (unverified). Grammar crates vary in licence.
- **Verdict:** **Adopt (provisional)** for the visual-intelligence and syntax layers. Check each grammar's licence separately.

### `tantivy`
- **Purpose:** Full-text search library (Lucene-like). Not fetched. Licence believed MIT (unverified).
- **Verdict:** **Evaluate** for code search if `rusqlite` FTS5 is insufficient.

---

## Architectural implications for Axiom

1. **Process split.** Qt GUI process (C++/QML), a Rust backend process (services), a WebEngine editor, and out-of-process tool servers. This matches VS Code's extension-host separation and Lapce's UI/proxy split (the latter unverified). Keep the GUI process free of plugin code.
2. **Plugin tiers.** Three tiers, each with its own trust level:
   - Tier 1, pure-compute: WASM components via Wasmtime (patched line), WIT interface, no ambient I/O. Candidate for formatters and analysers.
   - Tier 2, tool servers: out-of-process JSON-RPC (LSP, DAP, MCP, ACP). These can spawn subprocesses and need the OS-level permission model.
   - Tier 3, first-party: compiled into the backend crates.
3. **Toolchain providers as Kits.** Model each toolchain (compiler, SDK, debugger, runtime) as a provider that yields a Kit-like record, following Qt Creator's concept. Probe with `which`; persist the choice in `rusqlite`.
4. **PTY layer.** Put `portable-pty` behind an Axiom `Pty` trait. Test ConPTY on Windows before committing.
5. **Filesystem and search.** `notify` for watching; `ignore` + `grep-searcher` for search (provisional). Index in `rusqlite`; keep `tantivy` as a later option.
6. **Git.** `gix` for status, diff, and worktree queries, with the pinned version recorded in the lock file.
7. **IPC contract.** Derive JSON Schema with `schemars` from every IPC type. Generate TypeScript for the WebEngine side from the schema, or use `ts-rs` with a pinned version. Do not hand-write TS types.
8. **Visual intelligence.** The Layout Inspector model suggests a debug-protocol agent inside the running app that reports element trees. Axiom's mapping to source is novel relative to these tools (speculative: no prior art was verified for source mapping).
9. **Extension placement.** Follow VS Code's `extensionKind` idea: each extension declares where it runs (GUI-side, backend, or tool process). Lazy activation avoids startup cost.
10. **Licence boundary.** Decide Axiom's licence before copying any code. Apache-2.0 (Lapce) and MIT crates are compatible with a permissive project. GPL-3.0 code (Qt Creator; Zed's GPL portion) is not, without a licence decision.

---

## Risks

1. **Wasmtime advisories.** RUSTSEC-2026-0363 (Oct 2026) affects component-model handle tables. Pin to `>=50.0.1` (or the patched 49.0.3+ / 48.0.6+ / 36.0.18+ lines). Track future advisories; the stable line moves fast (36, 48, 49, 50 all receiving patches in Oct 2026).
2. **Pre-release crates.** `notify` 9.0.0-rc.8, `specta` 2.0.0-rc.28, and `wasmtime` 51.0.0-rc.1 may change API before stable.
3. **Crate maintenance.** `portable-pty` last released Feb 2025. `ts-rs` last released Jan 2026. Both are candidates for fork or wrapper.
4. **Pre-1.0 APIs.** `gix` 0.x and `specta` 2.x rc both have breaking-change risk.
5. **Rust MSRV churn.** Recent crates require 1.85–1.97 (`tokio` 1.85, `gix` 1.88, `notify` 1.88, `rusqlite` 1.89, `wasmtime` 1.97). Pin a toolchain.
6. **Licence contamination.** GPL-3.0 (Qt Creator, Zed GPL portion) and Zed's extension rules constrain code reuse. Read LICENSE files in every repo before copying.
7. **Qt licensing.** Qt's LGPL/commercial terms affect distribution of the Qt-based shell. Not verified in this run; the Qt open-source obligations page was linked from a 404 page only.
8. **WebEngine footprint.** Chromium-based WebEngine adds binary size and update churn (general knowledge; not verified here).
9. **ACP churn.** ACP has v1 and v2 specifications. Choose one version and record it.
10. **Strategic dependence on Fleet / JetBrains.** Fleet's 2026 status is unknown; do not plan around it.
11. **Windows PTY and file-watch parity.** `portable-pty` on ConPTY, and `notify` on Windows, need explicit tests (unverified here).
12. **Plugin supply chain.** Extension licence gating (Zed model) checks licence text, not code provenance. Needs a separate review step.

---

## Unverified / needs experiment

- Zed: AGPL and Apache licence files, process architecture, IPC, tasks implementation, ACP message format.
- Lapce: proxy and plugin architecture details; PSP (WASI) plugin claim; Floem platform support; 2026 status.
- Helix: licence, platform, plugin model, all facts.
- Qt Creator 18: release feature list; Kits detection model; QML Designer and Live Preview details; Lua/extension API; LSP client; GPL-3.0-with-exceptions licence text.
- KDevelop / Kate: KTextEditor and LSP client architecture; licence.
- JetBrains Fleet: 2026 status (blog body not readable); architecture; licence.
- IntelliJ platform SDK and toolchain model: details and licence.
- Eclipse Theia / Theia AI: EPL-2.0 licence; 1.75 features; architecture; WebEngine embedding feasibility.
- VS Code: main / renderer / shared-process / pty-host model; task problem-matcher behaviour; MIT licence check.
- Android Studio Layout Inspector: architecture, protocol, licence.
- Xcode toolchain selection: details.
- Gitpod / Coder / DevPod: licence, status, and runtime requirements (secondary sources only).
- Tauri / Electron / Wails / Slint / Neutralino / Iced / Flutter / Sciter: footprint, maturity, and licence (not fetched).
- Wasmtime: cross-platform test results; WASI capability model details; whether the `pulley` interpreter is viable on all targets.
- Extism: licence, platform, and Rust host maturity.
- ACP: v1 and v2 message format, transports, and the Zed integration point.
- `portable-pty`: Windows ConPTY behaviour and an open-issue review.
- `ignore`, `grep-searcher`, `git2`, `sqlx`, `serde`, `typeshare`, `tracing`, `keyring`, `which`, `tree-sitter`, `tantivy`: licence, maintenance status, and MSRV — none fetched in this run.
- Axiom's own licence choice (not stated in the brief); it decides which GPL and Apache sources can be copied.
- Whether a Rust-native alternative to Qt (e.g., Slint) is in scope; this was out of brief.

Experiments proposed (not run; no installs performed):
- Build a 30-line Wasmtime component (pinned to 50.0.1) that exposes one WIT function, and measure cold-start time and handle-table limits against RUSTSEC-2026-0363.
- Spawn `portable-pty` on Windows 11 and Linux with a bash/PowerShell child, and measure resize and exit handling.
- Index this repo with `notify` + `rusqlite` FTS5 and measure update latency on a 10k-file tree.
- Read the Qt Creator Kits source (`src/plugins/projectexplorer/kit*.cpp` in the qt-creator repo) and the licence headers before any design work.

---

## Sources

- Zed extension licence requirements: https://zed.dev/docs/extensions/publishing/license-requirements.md
- Zed developing extensions: https://zed.dev/docs/extensions/developing-extensions
- Zed repository: https://github.com/zed-industries/zed
- Zed GPL licence (raw): https://raw.githubusercontent.com/zed-industries/zed/main/LICENSE-GPL
- Zed external agents (ACP): https://zed.dev/docs/ai/external-agents
- Lapce architecture: https://docs.lapce.dev/development/architecture
- Lapce repository: https://github.com/lapce/lapce
- Lapce licence (raw, Apache-2.0): https://raw.githubusercontent.com/lapce/lapce/master/LICENSE
- Qt Creator 18.0.0 release tag: https://github.com/qt-creator/qt-creator/releases/tag/v18.0.0
- Qt Creator 18 blog: https://www.qt.io/blog/qt-creator-18-released
- Qt Creator 18 (JA): https://www.qt.io/ja-jp/blog/qt-creator-18-released
- Qt Kits how-to (404 in this run): https://doc.qt.io/qtcreator/creator-how-to-create-kits.html
- JetBrains Fleet, "The Future of Fleet" (body not readable): https://blog.jetbrains.com/fleet/2025/12/the-future-of-fleet/
- Eclipse Theia repository: https://github.com/eclipse-theia/theia
- Eclipse Theia 1.75 release post: https://eclipsesource.com/blogs/2026/09/10/eclipse-theia-1-75-release-news-and-noteworthy/
- Eclipse Theia 2026-02 community release: https://eclipsesource.com/blogs/2026/03/26/the-eclipse-theia-community-release-2026-02/
- VS Code extension host: https://code.visualstudio.com/api/advanced-topics/extension-host
- VS Code tasks appendix: https://code.visualstudio.com/docs/reference/tasks-appendix
- Android Studio Layout Inspector: https://developer.android.com/studio/debug/layout-inspector?hl=en
- Cloud dev environments comparison (secondary): https://easyspawn.com/blog/cloud-development-environments-compared
- Gitpod alternatives (secondary): https://canireplaceit.com/en/alternatives/gitpod
- Cross-platform desktop frameworks 2026 (secondary): https://ttlab.hopto.org/blog/culture/2026-05-16-cross-platform-desktop-apps-2026-tauri-2-electron-wails-neutralinojs-flutter-desktop-sciter-deep-dive?lang=en
- Desktop UI memory benchmark (not fetched): https://zenn.dev/mizugeeks/articles/1019cf2353d343#1
- Agent Client Protocol: https://agentclientprotocol.com/protocol/v1/transports ; https://agentclientprotocol.com/protocol/v2/transports.md
- Agent Client Protocol explainer (secondary): https://safeguard.sh/resources/blog/agent-client-protocol-explained-how-editors-talk-to-coding-agents-and-how-to-secure-it
- Wasmtime repository: https://github.com/bytecodealliance/wasmtime
- Wasmtime licence (raw): https://raw.githubusercontent.com/bytecodealliance/wasmtime/main/LICENSE
- Wasmtime crate metadata: https://crates.io/api/v1/crates/wasmtime
- RUSTSEC-2026-0363: https://rustsec.org/advisories/RUSTSEC-2026-0363.html
- CVE listing (search snippet, not a primary read): https://vuldb.com/vuln/428620#1
- Extism repository: https://github.com/extism/extism
- tokio crate metadata: https://crates.io/api/v1/crates/tokio
- portable-pty crate metadata: https://crates.io/api/v1/crates/portable-pty
- notify crate metadata: https://crates.io/api/v1/crates/notify
- gix crate metadata: https://crates.io/api/v1/crates/gix
- schemars crate metadata: https://crates.io/api/v1/crates/schemars
- ts-rs crate metadata: https://crates.io/api/v1/crates/ts-rs
- specta crate metadata: https://crates.io/api/v1/crates/specta
- rusqlite crate metadata: https://crates.io/api/v1/crates/rusqlite
- VS Code repository: https://github.com/microsoft/vscode
- Lapce plugin doc (404 in this run): https://github.com/lapce/lapce/blob/master/docs/plugins.md
