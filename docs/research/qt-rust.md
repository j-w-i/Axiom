# Qt 6 + Rust desktop architecture
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.

**Research access:** Web search and most fetches worked. Fetching the GNU LGPL text failed, and an unversioned WebEngine licensing URL redirected across origins and could not be followed; the versioned Qt 6.8 WebEngine licensing page was fetched successfully. Legal conclusions below are practical interpretations, not legal advice; verify against the actual licence text and counsel.

## Summary and recommendations

- **Adopt — Qt 6.12 LTS:** current documented release 6.12.0, with commercial standard support through 2031-09-30; pin a tested public binary and separately plan security updates. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Adopt — LGPL Qt modules and dynamic linking:** preserves an MIT or Apache-2.0 Axiom code licence if the complete distributed combination satisfies the applicable LGPL obligations. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Adapt — Qt WebEngine:** required by the proposed editor architecture; budget for Chromium notices, helper processes, patching, sandboxing and platform packaging. [WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **Adopt — C++ Qt shell + Rust daemon over local RPC:** strongest default for independent CLI/agent clients and process isolation; operational details require prototype validation. [QLocalSocket](https://doc.qt.io/qt-6/qlocalsocket.html)
- **Adapt — CXX-Qt 0.10.0:** credible small in-process bridge for selected UI-facing Rust models; its upstream calls the API early and frequently changing. [CXX-Qt](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md), [release](https://github.com/KDAB/cxx-qt/releases/tag/v0.10.0)
- **Reject — qmetaobject-rs as primary bridge:** QML capable, but upstream explicitly describes it as passively maintained. [Repository](https://github.com/woboq/qmetaobject-rs)
- **Reject — rust-qt/ritual as primary bridge:** generated, unsafe bindings and uncertain maintenance are a poor default compared with a small C++ shell. [CXX-Qt comparison](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Reject — Slint for this architecture:** different UI language and different licensing choices; reconsider only if QML/WebEngine are no longer requirements. [Slint](https://github.com/slint-ui/slint)
- **Adopt — JSON-RPC 2.0 with LSP-style byte-length framing:** observable and interoperable framing conventions; do not claim wire compatibility with LSP or DAP themselves. [LSP](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/), [JSON-RPC](https://www.jsonrpc.org/specification)
- **Adapt — jsonrpsee / tower-lsp:** useful implementation references, but neither README demonstrates a ready-made generic server over both stdio and Qt-compatible local sockets. [jsonrpsee](https://github.com/paritytech/jsonrpsee), [tower-lsp](https://github.com/ebkalderon/tower-lsp)
- **Defer — tonic/gRPC and Cap'n Proto:** valuable typed alternatives if measured throughput or schema requirements justify extra integration. [tonic](https://github.com/hyperium/tonic), [Cap'n Proto](https://capnproto.org/)
- **Adopt — platform-native deployment and CI matrix:** exercise actual WebEngine artifacts, signed macOS distribution and MSVC Windows builds, not just Rust unit tests. [Qt deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **Adopt — Qt Quick Test plus real-display WebEngine smoke tests:** offscreen QML tests alone do not establish WebEngine or GPU reliability. [Qt Quick Test](https://doc.qt.io/qt-6/qtquicktest-index.html)

## Findings

### Qt 6 releases and support

- **Purpose:** Qt provides the desktop shell, QML/Quick UI, and Qt WebEngine for the editor.
- **Maturity:** Qt 6.12.0 is the newest release in Qt's supported-version table at research time; 6.12 is LTS. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Support:** Qt 6.12.0 standard commercial support ends **2031-09-30**; Qt 6.11.3 ends **2027-03-17**. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Support:** Qt 6.10.3 standard support ended **2026-10-07**; 6.8.8 LTS commercial support ends **2029-10-08**. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Support:** Qt 6.5 LTS standard support has ended; extended security maintenance is listed separately. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Cadence:** minor releases are normally twice yearly; since 6.8, every fourth minor is LTS. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Access distinction:** initial LTS patches are public, but immediate access to additional LTS releases is limited to Qt commercial customers; WebEngine source remains under its default open-source licences. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Licence:** Qt is available under commercial, LGPL and GPL terms depending on the component; inspect the exact version and module. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Platform restrictions:** Qt compatibility depends on platform, toolchain and module; WebEngine has stricter restrictions than Qt generally. [WebEngine platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Integration complexity:** Qt's dynamic binary compatibility promise requires the same toolchain, system environment and build configuration, and excludes previews. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Alternatives:** use a distro-maintained Qt with its own security support, or a commercial LTS subscription; verify the exact channel.
- **Verdict:** adopt 6.12 as the target, subject to exact WebEngine and packaging availability tests.

### Qt licensing and Axiom's own licence

- **Purpose:** select a distribution model that permits users to run and modify Axiom and its LGPL components.
- **Maturity:** Qt publishes module-specific open-source and commercial choices, plus SPDX 2.3 third-party SBOMs starting with 6.8. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Licence:** Qt's overview describes `LGPL-3.0` or `GPL-3.0` for many components and commercial licensing as an alternative; consult the source files for exact `-only` / `-or-later` expressions. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **GPL-only modules:** Qt 6.12 lists Canvas Painter, CoAP, Graphs, GRPC, HTTP Server, Lottie Animation, MQTT, Network Authorization, Qml Compiler, Quick 3D, Quick 3D Physics, Quick Timeline, Virtual Keyboard and Wayland Compositor as GPLv3, rather than LGPL. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Qualification:** GPL-only means GPL on the *open-source Qt licence path*, not unavailable commercially. Check exact module version, file headers and exceptions rather than extrapolating this list to older releases. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Commercial:** a commercial Qt agreement can provide a non-GPL route for those modules; it does not eliminate third-party licence obligations. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **MIT Axiom:** generally viable with LGPL Qt as an independently licensed application, subject to combined-work compliance and the exact dependencies; MIT alone does not relicense Qt. [Qt open-source obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- **Apache-2.0 Axiom:** generally viable with LGPLv3 Qt under the same qualification; account for Apache notices and patent terms too. [Apache 2.0 text](https://www.apache.org/licenses/LICENSE-2.0), [Qt obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- **GPL-3.0-only or GPL-3.0-or-later Axiom:** an appropriate GPLv3-compatible route for GPLv3 Qt components, subject to checking exact dependencies and copyleft scope. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **GPL-only Qt with permissive Axiom source:** publishing Axiom's original code under MIT/Apache does not by itself make a distributed GPL-linked combined work permissively distributable; satisfy GPL for the combination or select commercial Qt. [Qt GPL](https://doc.qt.io/qt-6/gpl.html)
- **LGPL binary checklist:** retain licence/copyright notices, provide licence texts and prominent notice that Qt is used, and provide the corresponding source for the LGPL library including distributed modifications as required. [Qt obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- **LGPL binary checklist:** allow modification of the library and reverse engineering for debugging such modifications; do not add contradictory EULA restrictions. [Qt obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- **LGPL binary checklist:** use an appropriate shared-library mechanism that permits a user-modified compatible Qt library, or satisfy the alternative relinkable-object route; shipping DLLs alone is not a complete compliance test. [Qt obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- **LGPL binary checklist:** assess installation information obligations for covered user products and ensure signing, packaging and integrity controls do not negate required replacement rights. [Qt obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- **LGPL binary checklist:** archive exact source, patches, build configuration, attribution and the offer/access mechanism for each shipped binary; test replacement on each supported OS.
- **Platform restrictions:** signing, operating-system library loading and sandbox rules can complicate a practical replacement workflow; this is not resolved by a generic claim of dynamic linking.
- **Integration complexity:** high for release engineering and legal review; CXX-Qt's permissive licence does not change Qt's licence.
- **Alternatives:** GPL-compatible distribution, commercial Qt, or replace a GPL-only Qt module with a separately assessed implementation.
- **Verdict:** adopt LGPL modules with explicit per-release compliance evidence; do not import a GPL-only module accidentally.

### Qt WebEngine and Chromium licensing

- **Purpose:** embed Chromium-based web content into Qt Quick via Qt WebEngine. [Qt WebEngine](https://doc.qt.io/qt-6.8/qtwebengine-index.html)
- **Maturity:** Chromium is incorporated into Qt WebEngine Core at build time; it is not merely a system browser. [Qt licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **Licence:** Qt-specific WebEngine portions are offered under commercial, LGPLv3, GPLv3 or GPLv2; exact file terms still matter. [Qt WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **Chromium obligations:** Chromium contains many separately licensed components; the page lists licences including LGPL-2.1, BSD, MIT, Apache-2.0 and custom terms. [Qt WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **Concrete examples:** Qt lists FFmpeg and Speech Dispatcher under LGPL 2.1, and WebKit under BSD/LGPL terms; inspect the version-specific shipped build and source. [Qt WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **GPL clarification:** Qt says GPL entries in that listing access Linux system resources and that the distributed WebEngine libraries/plugins do not link to or distribute GPL binary code. [Qt WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **Commercial caveat:** even commercial users must satisfy the applicable third-party obligations; Qt allows distributing/building modified WebEngine source in connection with LGPL 2.1 compliance. [Qt WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **Platform restrictions:** Linux, Windows and macOS desktop support is documented; Windows MinGW and static WebEngine builds are unsupported in the cited 6.10 notes. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Integration complexity:** ship matching notices and source for the exact selected build, plus Chromium resources, process helper, Qt libraries and any relevant codecs. [Deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **Alternatives:** system web views change the engine and compatibility profile; do not assume they satisfy the editor requirement.
- **Verdict:** adapt WebEngine but treat security patching and third-party licence inventory as release blockers.

### CXX-Qt

- **Purpose:** bidirectional Rust/C++ bindings and Rust-defined QObject subclasses usable by C++, QML and JavaScript. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Maturity:** latest GitHub release is **0.10.0**, published **2026-08-24**; upstream describes the project as early development with frequently changing APIs. [Release API](https://api.github.com/repos/KDAB/cxx-qt/releases/latest), [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Recent capabilities:** 0.10.0 added bindings for QJsonArray, QJsonObject and QJsonValue, among other types. [Release](https://github.com/KDAB/cxx-qt/releases/tag/v0.10.0)
- **Licence:** `MIT OR Apache-2.0`; some helper CMake files have `BSD-3-Clause`. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Platform restrictions:** upstream tests Linux, Windows, macOS and WebAssembly in CI; this does not establish WebEngine support on all of them. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Qt requirement:** upstream currently aims to support Qt 5.15 LTS and all Qt 6 versions; below Qt 5.15 is explicitly unsupported. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **QObject features:** generators extend CXX for properties and signals/slots; C++ and QML can access Rust-backed objects. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **QML modules:** upstream examples demonstrate QQmlExtensionPlugin; build integration supports QML registration, to be checked against the pinned release. [Examples](https://github.com/KDAB/cxx-qt/tree/main/examples), [book](https://kdab.github.io/cxx-qt/book/)
- **Threading:** examples demonstrate threading; Qt object thread affinity remains important, so dispatch work back to the owning thread rather than mutating QObjects from arbitrary workers. [CXX-Qt examples](https://github.com/KDAB/cxx-qt/tree/main/examples), [Qt threading](https://doc.qt.io/qt-6/threads-qobject.html)
- **Cargo build:** `cxx-qt-build` runs from `build.rs`, generating and compiling C++; Rust macro handles corresponding Rust-side generation. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **CMake build:** supported for integration into C++ applications; upstream's own example/test build uses CMake calling Cargo. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Limitations:** not comprehensive bindings for Qt APIs; upstream explicitly says QWidgets APIs are not exposed as Rust bindings. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Limitations:** mixed Rust/C++ code generation and Qt toolchain alignment remain necessary; upstream notes one Cargo-only example needs a non-default linker on Linux. [README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Alternatives:** ordinary C++ QObjects talking to a Rust daemon, or qmetaobject-rs.
- **Verdict:** adapt for narrowly scoped bridge code only; pin version and isolate generated APIs.

### qmetaobject-rs

- **Purpose:** Rust derives generate QObject metadata, QML registration, properties, signals, slots, and extension plugins. [README](https://raw.githubusercontent.com/woboq/qmetaobject-rs/master/README.md)
- **Maturity:** repository says it is **passively maintained** because attention shifted to Slint; applications including Gyroflow are listed. [README](https://raw.githubusercontent.com/woboq/qmetaobject-rs/master/README.md)
- **Licence:** inspect exact crate and transitive licences at adoption; no exact SPDX assertion verified here.
- **Platform restrictions:** upstream states Qt >= 5.8 and offers a disabled-by-default WebEngine feature, but that does not prove the current Qt 6.12 three-OS matrix. [README](https://raw.githubusercontent.com/woboq/qmetaobject-rs/master/README.md)
- **Integration complexity:** Cargo-centered, but C++ compiler and inline `cpp!` wrappers are needed for missing APIs; those wrappers can require unsafe Rust. [README](https://raw.githubusercontent.com/woboq/qmetaobject-rs/master/README.md)
- **Alternatives:** CXX-Qt, or C++ shell and IPC.
- **Verdict:** reject as the main new dependency unless maintenance and 6.12 tests change the assessment.

### Slint

- **Purpose:** declarative UI with `.slint` files and Rust/C++/other language bindings, not Qt/QML. [README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)
- **Maturity:** upstream describes a stable 1.x API and desktop support for Linux, macOS and Windows. [README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)
- **Licence:** upstream lists `GPL-3.0-only`, a `LicenseRef-Slint-Royalty-free-2.0` proprietary-use option and a commercial option; assess exact version and conditions. [README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)
- **Platform restrictions:** supports desktop, but its own UI stack does not implement Axiom's specified QML surface. [README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)
- **Integration complexity:** changing to Slint entails replacing QML and reassessing the embedded editor; Qt-style widgets do not make it a QML runtime. [README](https://raw.githubusercontent.com/slint-ui/slint/master/README.md)
- **Alternatives:** Qt Quick plus Rust service.
- **Verdict:** reject for the stated design; revisit if the architectural requirement changes.

### rust-qt and ritual

- **Purpose:** generate Rust bindings to Qt's C++ APIs using ritual. [rust-qt](https://rust-qt.github.io/)
- **Maturity:** CXX-Qt's comparison labels rust-qt unmaintained, but its maintenance comparison is explicitly dated August 2022 and is not proof of present status. [Comparison](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Licence:** exact current SPDX for every generated crate is unverified; inspect the particular repository/crate before selection.
- **Platform restrictions:** exact current Qt 6.12 and Windows/macOS/Linux support is unverified.
- **Integration complexity:** generated C++ bindings expose unsafe Rust in the upstream comparison, increasing ownership and ABI audit burden. [Comparison](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Alternatives:** CXX-Qt for targeted object exposure, or avoid broad bindings with local RPC.
- **Verdict:** reject as default; current-maintenance status requires direct investigation before a definitive assessment.

### C++ Qt shell and Rust daemon: FFI versus RPC

- **Purpose:** the C++/QML process owns the Qt event loop and embedded editor; a separate Rust process owns backend services.
- **Maturity:** Qt documents event-driven local socket and process APIs; the proposed division is an architectural recommendation, not a demonstrated Axiom implementation. [QLocalSocket](https://doc.qt.io/qt-6/qlocalsocket.html), [QProcess](https://doc.qt.io/qt-6/qprocess.html)
- **Licence:** Axiom's shell and daemon can use its selected project licence, subject to each dependency's terms and review of the whole distribution.
- **Platform restrictions:** QLocalSocket uses Unix-domain sockets on Unix and named pipes on Windows. [QLocalSocket](https://doc.qt.io/qt-6/qlocalsocket.html)
- **FFI comparison — failure:** an in-process native crash can take down both layers; a process boundary normally isolates address-space faults, not filesystem corruption or shared external state.
- **FFI comparison — cost:** FFI avoids serialization/IPC but requires cross-language ABI, lifetime, ownership and thread discipline; RPC trades that for protocol and process lifecycle work.
- **FFI comparison — testability:** daemon handlers can be exercised without Qt, and a protocol fake can drive GUI tests; this is a proposed test design.
- **FFI comparison — multi-client:** a socket server can accept GUI, CLI and agent clients; one GUI-owned stdio stream is naturally a single connection unless an explicit multiplexer or separate server is added.
- **FFI comparison — consistency:** IPC introduces disconnection, stale views, duplicate operations and concurrent clients; design request semantics rather than assuming call-like delivery.
- **Integration complexity:** C++/QML remains necessary, but the Rust service builds and runs separately; native pipe naming, authentication, supervision and packaging add complexity.
- **Alternatives:** in-process CXX-Qt for small latency-sensitive view models, or a hybrid with a daemon for durable services.
- **Verdict:** adopt process boundary for core; use in-process FFI only when measured requirements justify it.

### JSON-RPC 2.0, framing and Rust/Qt libraries

- **Purpose:** language-neutral requests, responses, notifications, IDs and standard error objects over a separate transport. [JSON-RPC 2.0](https://www.jsonrpc.org/specification)
- **Maturity:** JSON-RPC 2.0 is a published specification; LSP uses JSON-RPC with headers and DAP uses its own protocol messages and similar content-length framing. [JSON-RPC](https://www.jsonrpc.org/specification), [LSP](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/), [DAP](https://microsoft.github.io/debug-adapter-protocol/specification)
- **Important distinction:** DAP is **not** JSON-RPC 2.0: it defines `seq`, `type`, request/response/event messages and DAP-specific semantics. [DAP specification](https://microsoft.github.io/debug-adapter-protocol/specification)
- **Framing:** LSP uses ASCII headers, `Content-Length` in bytes, `\r\n\r\n` delimiter and UTF-8 JSON content; calculate the encoded byte length, not character count. [LSP specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)
- **Alternative framing:** newline-delimited compact JSON is simple to inspect, but literal message delimiters and escaping rules must be specified; it is not LSP framing. [NDJSON specification](https://github.com/ndjson/ndjson-spec)
- **Recommendation:** use LSP-style framing consistently across Axiom stdio and socket transports; the application methods remain Axiom-specific.
- **Protocol discipline:** partial headers, partial UTF-8 bodies, multiple frames per read, maximum frame sizes and invalid JSON need explicit handling.
- **Protocol discipline:** reserve stdout exclusively for frames in stdio mode; write diagnostics to stderr.
- **Protocol discipline:** define version negotiation, request IDs per connection, errors, cancellation, progress, reconnect and idempotency before exposing side-effecting tools.
- **Rust — serde_json:** provides JSON serialization/deserialization; it is not an RPC server or stream-framing protocol. [serde_json](https://github.com/serde-rs/json)
- **Rust — jsonrpsee:** MIT-licensed async JSON-RPC library; upstream advertises HTTP/HTTP2 and WebSocket server/client plus custom client transport, not a ready-made stdio/local-socket server. [README](https://raw.githubusercontent.com/paritytech/jsonrpsee/master/README.md)
- **Rust — tower-lsp:** MIT OR Apache-2.0 LSP implementation, with `LanguageServer`, `LspService` and a server processing stdio or TCP; reuse internals only after checking public API and licence. [README](https://raw.githubusercontent.com/ebkalderon/tower-lsp/master/README.md)
- **Qt — JSON:** QJsonDocument provides JSON parsing and serialization; combine with QJsonObject/QJsonArray and explicit schema validation. [QJsonDocument](https://doc.qt.io/qt-6/qjsondocument.html)
- **Qt — streams:** QLocalSocket supplies event-driven local transport; QProcess can start the Rust child and expose its streams. [QLocalSocket](https://doc.qt.io/qt-6/qlocalsocket.html), [QProcess](https://doc.qt.io/qt-6/qprocess.html)
- **Licence:** library-specific licences are separate from Qt's; audit actual crate versions and transitive dependencies.
- **Platform restrictions:** local socket endpoint names and permissions differ by OS; do not equate a Qt local socket with a TCP port. [QLocalSocket](https://doc.qt.io/qt-6/qlocalsocket.html)
- **Integration complexity:** moderate: typed application messages plus a small robust codec and service dispatcher; authentication remains Axiom's responsibility.
- **Alternatives:** NDJSON, gRPC, Cap'n Proto.
- **Verdict:** adopt JSON-RPC and byte-length framing; prototype transport and library fit before choosing an RPC framework.

### gRPC/tonic and Cap'n Proto

- **Purpose:** typed schema-based communication; tonic implements gRPC in Rust, while Cap'n Proto provides schema-driven serialization and RPC. [tonic](https://github.com/hyperium/tonic), [Cap'n Proto](https://capnproto.org/)
- **Maturity:** both are established open-source projects; no comparative benchmark for Axiom has been run. [tonic](https://github.com/hyperium/tonic), [Cap'n Proto](https://capnproto.org/)
- **Licence:** inspect exact implementations and generated-code dependencies before adoption; no blanket Qt gRPC licence assumption is safe.
- **Platform restrictions:** transport implementation, Qt integration and named-pipe interoperability need an actual three-platform test.
- **Integration complexity:** protobuf/schema generation and C++/Rust runtimes add tooling; Qt's own Qt GRPC module is GPL-only on its open-source licensing path. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Alternatives:** standard C++ gRPC is a separate technology and has its own licence and build consequences; do not conflate it with the Qt GRPC module. [gRPC](https://github.com/grpc/grpc)
- **Verdict:** defer until a measured need for binary schemas, streaming semantics or throughput exceeds the cost of a JSON protocol.

### Windows, macOS and Linux packaging

- **Purpose:** distribute the GUI, Rust service, WebEngine runtime and resources as a functioning, patchable application.
- **Maturity:** Qt documents `windeployqt` and `macdeployqt` as deployment automation, not as substitutes for testing the resulting artifact. [WebEngine deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **Licence:** each artifact needs a Qt/Chromium third-party inventory and a tested LGPL-compliant replacement path. [Qt licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **All platforms:** deploy libraries, QML imports, `QtWebEngineProcess`, resource `.pak` files, ICU/V8 data as applicable, locales and relevant codecs. [WebEngine deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **Windows:** run `windeployqt` with the QML source path or equivalent deployment configuration; include matching MSVC runtime and smoke-test on a clean machine. [Windows deployment](https://doc.qt.io/qt-6/windows-deployment.html)
- **Windows restrictions:** Qt 6.10 WebEngine does not compile with MinGW; use an MSVC-compatible Qt and Rust MSVC target/toolchain. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Windows signing:** Authenticode signs Windows binaries; signing is recommended for publisher identity and distribution trust, not a universal requirement to execute all desktop apps. [Microsoft signing](https://learn.microsoft.com/en-us/windows/win32/seccrypto/cryptography-tools)
- **macOS:** `macdeployqt` creates a deployable app bundle; sign nested frameworks and helper apps in the correct order. [macOS deployment](https://doc.qt.io/qt-6/macos-deployment.html)
- **macOS WebEngine:** Qt requires the helper to be signed with at least the entitlements shipped for QtWebEngineProcess; camera/microphone need additional usage descriptions and relevant entitlements. [WebEngine deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **macOS notarization:** for smooth default Gatekeeper distribution outside the Mac App Store, use Developer ID signing and Apple notarization; it is not a claim that all local development builds must be notarized. [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution)
- **Mac App Store:** Qt explicitly says WebEngine apps are incompatible because Chromium uses private APIs and the App Sandbox interferes with Chromium sandbox initialization. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Linux AppImage:** linuxdeploy plus its Qt plugin is a community option for an AppDir/AppImage, not an official Qt deployment guarantee. [linuxdeploy Qt plugin](https://github.com/linuxdeploy/linuxdeploy-plugin-qt), [AppImage packaging](https://docs.appimage.org/packaging-guide/from-source/native-binaries.html)
- **Linux Flatpak:** KDE runtime plus `io.qt.qtwebengine.BaseApp` supplies WebEngine missing from the runtime; upstream lists 6.10 and 6.11 branches, so a 6.12 branch is not confirmed by the retrieved README. [BaseApp README](https://raw.githubusercontent.com/flathub/io.qt.qtwebengine.BaseApp/master/README.md)
- **Flatpak sandbox:** that BaseApp describes sandboxed Chromium render processes; its build-time `--no-sandbox` instructions are not justification for disabling protection in distributed apps. [BaseApp README](https://raw.githubusercontent.com/flathub/io.qt.qtwebengine.BaseApp/master/README.md)
- **Size:** no trustworthy Axiom-specific compressed or installed byte count was found; WebEngine's additional helper, libraries and resources make measurement necessary. [Deployment list](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **Platform restrictions:** Linux kernel namespaces/seccomp and container shared memory affect Chromium; Windows fullscreen popup and macOS sandbox limitations are documented. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Integration complexity:** high: native toolchains, helper paths, security updates, licences, signing and per-architecture artifacts.
- **Alternatives:** distro-native packages or system Qt where security/support tradeoffs are acceptable.
- **Verdict:** adapt platform-native workflows and measure actual signed artifacts before committing to a download-size target.

### Qt acquisition and CI

- **Purpose:** reproducible builds and GUI smoke tests on Linux, macOS and Windows.
- **Maturity:** `jurplel/install-qt-action@v4` wraps aqtinstall, supports version/architecture selection, and accepts `qtwebengine` in the modules list. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md)
- **Licence:** the action states MIT; downloading Qt still requires compliance with the Qt licences selected. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md)
- **Direct option:** `aqtinstall` can obtain Qt archives outside GitHub Actions; availability is a function of host, architecture, release and repository. [aqtinstall](https://github.com/miurahr/aqtinstall)
- **Linux:** action documentation names `linux_gcc_64` for Qt >=6.7 and `linux_gcc_arm64` as an ARM64 option; it says Linux x86 prebuilts are not provided by Qt. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md)
- **macOS:** action documents `clang_64`; validate actual native arm64/x86_64 artifacts and WebEngine module availability. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md)
- **Windows:** action defaults to `win64_msvc2022_64` for Qt >=6.8; use Rust's compatible MSVC target, not MinGW, for WebEngine. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md), [platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Version caveat:** `qtwebengine` is an addon; its presence for a specific version/host/architecture must be checked rather than inferred from the generic module input. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md)
- **Distro packages:** a useful alternative on Linux, but Qt and WebEngine versions and security backports follow the distro; verify package repository and supported life cycle per distro.
- **Commercial LTS:** the action documents an official-installer mode with credentials for licensed customers; do not assume commercial patches are public archives. [Action README](https://raw.githubusercontent.com/jurplel/install-qt-action/master/README.md)
- **Integration complexity:** pin action commit, Qt patch, Rust toolchain, OS image and architecture; cache only with keys that include those values.
- **Alternatives:** official installer, distro packages or a fully managed toolchain image.
- **Verdict:** adopt a three-OS MSVC/Clang/GCC matrix with an explicit WebEngine availability check.

### QML testing, Squish and offscreen WebEngine

- **Purpose:** exercise declarative components, signals, interaction and full editor integration.
- **Maturity:** Qt Quick Test provides QML `TestCase` support for Quick applications. [Qt Quick Test](https://doc.qt.io/qt-6/qtquicktest-index.html)
- **Licence:** Qt tools and utilities have their own licensing, including GPLv3 plus Qt GPL exception where applicable; inspect the exact test component and distribution. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Squish:** Qt's commercial GUI automation product is an option for cross-platform application-level tests. [Squish](https://www.qt.io/quality-assurance/squish)
- **Alternatives:** Qt Quick Test for QML component coverage, Qt Test for C++ tests and OS-level browser/GUI automation to investigate for end-to-end scenarios. [Qt Test](https://doc.qt.io/qt-6/qttest-index.html)
- **Headless:** Qt selects a QPA plugin through `QT_QPA_PLATFORM`; `offscreen` is a candidate for noninteractive QML tests, subject to build/plugin availability. [Qt QPA](https://doc.qt.io/qt-6/qpa.html)
- **WebEngine caveat:** a successful offscreen Quick test does not prove Chromium starts, renders pixels, supports GPU acceleration, or preserves sandboxing; test WebEngine separately.
- **Platform restrictions:** Linux namespaces, seccomp and container memory can affect WebEngine; do not normalize `--no-sandbox` as a production fix. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Integration complexity:** layer unit, QML, protocol and packaged real-display integration tests; compare screenshots only under controlled font, DPI and rendering configuration.
- **Alternatives:** a virtual display on Linux and native desktop runners on macOS/Windows; exact GPU and headless support remains experimental.
- **Verdict:** adopt Qt Quick Test plus native/virtual-display WebEngine smoke tests; keep Squish optional.

## Architectural implications for Axiom

- Recommended topology, **proposal rather than demonstrated system**:
- `QML / Qt Quick + WebEngine` → `C++ Qt adapter` → `JSON-RPC client`.
- `CLI / agent clients` → same local endpoint → `Rust service`.
- A GUI-owned child stdio mode can be a simpler fallback, but does not automatically allow multiple clients.
- Put editor-to-host integration behind a narrow, versioned and capability-checked bridge.
- Make service startup and shutdown explicit: lock ownership, stale endpoint recovery, restart and request failure.
- Separate one-shot command requests from long-running tool jobs and their progress notifications.
- Define per-connection identity, authorization and endpoint permissions before allowing shell or filesystem tools.
- Restrict Unix socket parent directory and validate permissions; establish a Windows named-pipe ACL and identity policy.
- Treat all incoming editor and agent payloads as untrusted; validate paths, method names, size and schemas.
- Maintain a UI model snapshot plus revisioned events so a reconnect can recover without silent drift.
- Route GUI changes onto the Qt object-owning thread; never block the GUI event loop on socket waits. [Qt threading](https://doc.qt.io/qt-6/threads-qobject.html), [QLocalSocket](https://doc.qt.io/qt-6/qlocalsocket.html)
- Keep visual-intelligence capture/mapping APIs separate from the transport; the IPC selection does not prove source-map accuracy.
- Pin the Qt SDK, C++ compiler, Rust target, service protocol and editor build in every release manifest.
- Generate an SBOM and retain corresponding sources and notices for each platform artifact. [Qt SBOM](https://doc.qt.io/qt-6/licensing.html)
- Gate releases on a real modified-Qt replacement experiment, WebEngine startup, sandbox and editor smoke test.

## Risks

- **Security update gap:** commercial LTS dates are not an assurance of immediate public binary patches. [Qt releases](https://doc.qt.io/qt-6/qt-releases.html)
- **Licensing contamination:** one GPL-only Qt module can change obligations for the linked combination; review module import and generated deployment manifests. [Qt licensing](https://doc.qt.io/qt-6/licensing.html)
- **Chromium drift:** WebEngine notices and LGPL source obligations depend on the exact distributed build. [WebEngine licensing](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- **User modification:** signing/hardened runtime and loader constraints may defeat a theoretically replaceable shared library; test and obtain legal review.
- **Mac distribution:** Mac App Store is not a supported WebEngine route. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Windows:** mismatched MSVC and Rust targets or accidental MinGW selection will break the WebEngine build. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **Sandbox:** turning off Chromium sandboxing to make CI green can hide a production security regression. [Platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- **RPC:** local access is not automatically trusted; endpoint spoofing and same-user compromise are separate threats.
- **Availability:** independent service failure, upgrade skew and partial writes demand recovery design.
- **API churn:** CXX-Qt upstream warns of frequent API changes. [CXX-Qt](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- **Packaging:** helper/resources and Qt plugins can work on a developer machine but be absent on a clean host. [WebEngine deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- **Headless:** offscreen component tests may conceal GPU, compositor, font, focus and sandbox defects.

## Unverified / needs experiment

- Exact public 6.12 patch availability and supported WebEngine binary architectures for every CI runner; the support table is not an archive inventory.
- Whether KDE Flatpak runtime/BaseApp 6.12 has become available beyond the fetched BaseApp branch list.
- Exact Qt 6.12 WebEngine platform notes, module licence expressions, codec configuration and Chromium dependency versions for the shipped SDK; cited 6.8/6.10 documents are version-specific.
- Exact current qmetaobject-rs and ritual crate licences and 2026 maintenance/Qt 6.12 support.
- QML module registration, custom object types and safe threaded callbacks on CXX-Qt 0.10.0 with the chosen 6.12 SDK on all three OSes.
- Windows Qt QLocalSocket ↔ Rust named-pipe naming, byte-stream behavior, peer verification and ACL interoperability.
- macOS signed app compatibility with replacing LGPL Qt/WebEngine libraries and required installation information; obtain legal advice.
- Qt WebEngine real rendering and renderer sandbox under `QT_QPA_PLATFORM=offscreen` with each OS/GPU/CI environment; no blanket support claim is made.
- Compressed and installed Axiom/WebEngine size on each architecture, with and without shared Flatpak runtime; no estimate is asserted.
- End-to-end VS Code-technology editor functionality, accessibility, source mapping and devtools compatibility within the exact Qt WebEngine version.
- Whether `tower-lsp` internals expose a stable, reusable generic codec and whether jsonrpsee can serve the chosen native transports without significant custom code.
- Legal assessment of exact MIT, Apache-2.0 or GPL distribution combinations, additional components, and commercial contractual terms; GNU licence text fetch failed during research.

## Sources

- [Qt 6 releases and support table](https://doc.qt.io/qt-6/qt-releases.html)
- [Qt 6 licensing and GPL module list](https://doc.qt.io/qt-6/licensing.html)
- [Qt open-source obligations](https://www.qt.io/licensing/open-source-lgpl-obligations)
- [Qt WebEngine 6.8 licensing and third-party list](https://doc.qt.io/qt-6.8/qtwebengine-licensing.html)
- [Qt WebEngine 6.10 platform notes](https://doc.qt.io/qt-6.10/qtwebengine-platform-notes.html)
- [Qt WebEngine deployment](https://doc.qt.io/qt-6.8/qtwebengine-deploying.html)
- [CXX-Qt repository README](https://raw.githubusercontent.com/KDAB/cxx-qt/main/README.md)
- [CXX-Qt latest release API](https://api.github.com/repos/KDAB/cxx-qt/releases/latest)
- [qmetaobject-rs repository](https://github.com/woboq/qmetaobject-rs)
- [Slint repository](https://github.com/slint-ui/slint)
- [Qt local sockets](https://doc.qt.io/qt-6/qlocalsocket.html)
- [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification)
- [LSP 3.18 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)
- [DAP specification](https://microsoft.github.io/debug-adapter-protocol/specification)
- [jsonrpsee](https://github.com/paritytech/jsonrpsee)
- [tower-lsp](https://github.com/ebkalderon/tower-lsp)
- [Qt WebEngine Flatpak BaseApp](https://github.com/flathub/io.qt.qtwebengine.BaseApp)
- [install-qt-action](https://github.com/jurplel/install-qt-action)
- [Qt Quick Test](https://doc.qt.io/qt-6/qtquicktest-index.html)
