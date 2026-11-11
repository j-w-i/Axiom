# xtool — cross-platform iOS build/deploy (Xcode replacement) for Axiom
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.

**Tooling status for this session:** `web_search` worked (two queries; returned Apple/PyPI URLs but no licence text). `web_fetch` worked for GitHub, api.github.com and raw.githubusercontent.com. The docs site (xtool.sh) returned an empty page for `/documentation/xtool/` and a 404 for `/documentation/xtooldocs/getting-started`, so DocC markdown was read from the repo instead. Apple's `xcode.pdf` licence could **not** be retrieved: `web_fetch` rejects `application/pdf`, and the `http://` to `https://` redirect to `images.apple.com` was blocked. No licence text is quoted below. Nothing was installed or executed.

## Summary and recommendations

- **Adapt — xtool as an optional, version-pinned `ios` toolchain provider (Linux/WSL + macOS).** It is the only FOSS path found that builds SwiftPM iOS apps and installs them to a device from Linux, and the source is MIT. Reason: it is a CLI with a real device path, but it relies on a Darwin SDK extracted from Apple's Xcode.xip and on Apple's private APIs for password login.
- **Adapt — Darwin Swift SDK (`swift sdk`) as the underlying mechanism.** xtool generates a Swift SDK bundle; Axiom should call xtool rather than re-implement SDK generation. Reason: the SDK builder has undocumented internals (ld64 trampoline, OpenAppleMacros stubs) that change per release.
- **Adapt — macOS host path: use Xcode/`xcrun`/`xcodebuild` directly as the reference provider.** Reason: on macOS the Xcode SDK ships with Xcode and xtool itself skips SDK install there, so the native path is simpler and supports the simulator.
- **Adapt — swiftly (Apache-2.0) for the Swift 6.4 toolchain prerequisite on Linux.** Reason: official swiftlang toolchain manager; avoids hand-rolled install steps. Swift 6.4 is a hard xtool requirement.
- **Adapt — libimobiledevice / usbmuxd (LGPL-2.1) for device discovery on Linux, invoked as external binaries.** Reason: xtool already depends on usbmuxd; keep it at arm's length to avoid linking obligations.
- **Adapt — go-ios (MIT) as an optional device-automation backend (install/launch/XCUITest/instruments).** Reason: permissive licence, Linux-capable, but its feature-to-iOS-version matrix is unverified here.
- **Adapt (with caution) — pymobiledevice3 (GPL-3.0) only as an out-of-process CLI, never linked.** Reason: GPL-3.0 copyleft; the project itself says closed-source use needs a FAQ read. Useful for screenshots/syslog/accessibility-adjacent work on iOS 17+.
- **Reject — XcodeGen (MIT) and Tuist (NOASSERTION/"Other") as Linux-host components.** Reason: both generate or drive Xcode projects and need macOS/Xcode. They are only relevant to a macOS-host provider.
- **Reject — Theos as an Axiom build dependency for v1 (licence "Other"/NOASSERTION).** Reason: its positioning (iOS tweak/deb builds) is unverified against Axiom's app-build target; revisit only if Axiom needs jailbreak/tweak targets.
- **Adapt (macOS only, experimental) — WebDriverAgent (appium fork, licence NOASSERTION) for UI-hierarchy inspection.** Reason: the only widely used accessibility-tree path over WebDriver; the upstream Facebook repo is archived; the Appium fork is active.
- **Reject — treating xtool as a stable machine-readable API.** Reason: CLI output is plain text; the Swift library surface has no semver guarantee (per README).

## Findings

### xtool (xtool-org/xtool) — the core candidate

**Purpose.** "Cross-platform Xcode replacement. Build and deploy iOS apps with SwiftPM on Linux, Windows, and macOS." ([GitHub repo description/README](https://github.com/xtool-org/xtool)). Capabilities listed in the README: build a SwiftPM package into an iOS app; sign and install iOS apps; interact with Apple Developer Services programmatically ([README](https://raw.githubusercontent.com/xtool-org/xtool/main/README.md)).

**Maturity / activity.**
- Latest release: **1.24.0**, published 2026-10-26 (tag created 2026-10-25T23:54:57Z) ([release API](https://api.github.com/repos/xtool-org/xtool/releases/latest)).
- Recent cadence from the releases API: 1.19.0 (2026-09-04), 1.19.2 (2026-09-10), 1.20.0 (asset timestamps ~2026-09-20), 1.23.0 (2026-10-22), 1.24.0 (2026-10-26) ([releases API](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)). Roughly 1–3 releases/month in Sep–Oct 2026, but only 1.20.0 onwards is significant and most releases are auth/SDK fixes.
- Repo: created 2024-11-17; pushed 2026-11-09; 15,217 stars; 891 forks; 119 open issues; not archived ([repo API](https://api.github.com/repos/xtool-org/xtool)).
- Release notes highlight: 1.20.0 added Swift 6.4 and SwiftUI macros support, moved to SwiftBuild, changed SDK layout (~3 GB → ~7 GB) and requires Darwin SDK reinstall on Linux ([release 1.20.0 body](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)).
- 1.23.0: Apple-ID login must be redone after updating ([release 1.23.0 body](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)). 1.24.0: "fixes another source of HTTP 503 errors when authenticating with an Apple ID"; adds Sign in with Apple entitlement support (PR #377) ([release 1.24.0](https://api.github.com/repos/xtool-org/xtool/releases/latest)).

**Licence.** MIT (SPDX `MIT`), per the GitHub licence API field ([repo API](https://api.github.com/repos/xtool-org/xtool)). The `LICENSE` file path returned 404 on `main` (raw fetch), so the API field is the only confirmation obtained. Re-check the file in the pinned tag before relying on it.

**Platform restrictions (sourced).**
- Hosts: Linux (x86_64, aarch64 AppImages), WSL on Windows, macOS ([README](https://raw.githubusercontent.com/xtool-org/xtool/main/README.md); [Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)). Native Windows is not documented; the GitHub description says "Windows", but the docs say WSL with USB passthrough. Treat native Windows as unsupported.
- The release pipeline builds Linux AppImages (ubuntu-24.04 x86_64 and arm) and a macOS `.app` (runner `xcode-27`) ([release.yml](https://raw.githubusercontent.com/xtool-org/xtool/main/.github/workflows/release.yml)).
- Package platforms: iOS 17, macOS 14 ([Package.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift)).
- WSL: USB devices must be bound with USBIPD; a documented alternative relays through iTunes on Windows (`AFCClient.Error.muxError`) ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)).

**Installation steps and prerequisites (Linux).** ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md))
1. **Swift 6.4 toolchain** from swift.org; check with `swift --version` (docs show `swift-6.4-RELEASE`). SDKBuilder source notes 6.3 has bugs resolving librarian/linker paths, so 6.4+ is required ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)).
2. **usbmuxd** (distro package; `usbmuxd --help` check). Optional: `libimobiledevice-utils` for `ideviceinfo`.
3. **Xcode.xip** — docs say "Download **Xcode 27.1**" from developer.apple.com (requires Apple ID login and licence acceptance in a browser; `curl` not supported by the docs).
4. Install the AppImage: `curl -fL https://github.com/xtool-org/xtool/releases/latest/download/xtool-$(uname -m).AppImage -o xtool`, `chmod +x`, move to PATH.
5. `xtool setup` — choose login mode, then provide the path to Xcode.xip. xtool "will extract the Xcode XIP to generate and install an iOS Swift SDK"; verify with `swift sdk list` → `darwin`.

**Installation (macOS).** Requires Xcode installed and launched once (`xcrun -sdk iphoneos -show-sdk-path`, `swift --version` documented as Swift 6.1 in the sample output). Install via `brew install xtool-org/tap/xtool` or the `xtool.app` zip ([Installation-macOS](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-macOS.md)). On macOS, xtool's SDK install is skipped: "Skipping SDK install; the iOS SDK ships with Xcode on macOS" ([SDKCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift)).

**Darwin SDK — how it is obtained and extracted (sourced from code).**
- Input: `Xcode.xip` (checked for `xar!` magic) or an `Xcode.app` directory (must contain `Contents/Developer`) ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)).
- Extraction uses a bundled libunxip Swift port of unxip (dependency `saagarjha/unxip`) ([Package.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift)).
- Output: `darwin.xtoolsdk` / Swift SDK bundle with `swift-sdk.json`, `toolset.json`, `info.json`, and triples for `arm64-apple-ios`, `arm64-apple-ios-simulator`, `x86_64-apple-ios-simulator`, `arm64-apple-macosx`, `x86_64-apple-macosx` ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)).
- Modes: `normal` (keeps Xcode.app/Contents/Developer via hard links, updatable in place, ~7 GB) vs `--slim` (~ smaller, cannot be updated in place) ([release 1.20.0](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8); [SDKCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift)).
- Subcommands: `xtool sdk install|update|remove|build|status`. `status` prints human text (`Darwin SDK is installed`, `Path`, `Flavor`, `Version`) ([SDKCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift)).
- **Downloads at SDK-build time from third-party GitHub releases**: `xtool-org/darwin-tools-linux-llvm` v1.1.0 (`toolset-{x86_64,aarch64}.tar.gz`, includes LLVM `ld64.lld`/`llvm-lib`) and `xtool-org/OpenAppleMacros` v1.4.1 (`OpenAppleMacrosServer-*.tar.gz`). Env overrides: `XTL_TOOLSET_TARBALL`, `XTL_OAM_TARBALL` accept a tarball **or** a directory ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)). No checksum verification of these downloads was seen in the code path read.
- The SDK builder patches Apple's `Info.plist` platform manifests (`SWIFT_RESOURCE_DIR`, `ALTERNATE_LINKER=lld`, and `CODESIGN=true` stub) and symlinks XCTest/Testing frameworks into SDKs ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)).
- SDK version string: `epoch=4,darwinTools=1.1.0,oam=1.4.1`; a mismatch triggers a rebuild on the next command ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift); [SDKCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift)).

**Licence considerations for using Apple's SDK from Xcode.xip on non-Apple hardware (report only; not legal advice).**
- The Xcode.xip is downloaded by the user under Apple's terms (docs: "You'll be asked to log in with your Apple ID and accept the license agreement") ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)).
- xtool extracts Apple's SDK/toolchain files locally and does not publish them as a release artifact (the release assets are the xtool AppImage/app, and the darwin-tools/OpenAppleMacros tarballs are separate third-party releases) ([release.yml](https://raw.githubusercontent.com/xtool-org/xtool/main/.github/workflows/release.yml); [SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)).
- Apple's Xcode/iOS SDK licence language was **not retrieved** (see Unverified). Apple's own Xcode SLA is at `images.apple.com/legal/sla/docs/xcode.pdf` (listed by the search tool, not fetched).
- xtool's own docs describe password login as using "private Apple APIs" ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)). Apple's terms on private APIs are not checked here.

**Project format.** SwiftPM package (`Package.swift`) with exactly one library product for the app (the `NewCommand` template comments this) plus optional `xtool.yml` ([NewCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/NewCommand.swift)). Template: `swift-tools-version: 6.0`, `platforms: .iOS(.v17), .macOS(.v14)`, SwiftUI `@main App` ([NewCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/NewCommand.swift)). `xtool.yml` keys documented: `version: 1`, `bundleID`, `product`, `extensions` (list with `product`, `infoPath`), `infoPath`, `iconPath` (PNG, ideally 1024×1024), `entitlementsPath`, `resources` (top-level copy into the .app root) ([Control.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md); [Appex.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Appex.md)). Also `skipLSP: true` disables BSP generation ([1.20.0 notes](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)). SwiftPM resources (`.copy`) are supported ([Control.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md)).

**What it builds.**
- iOS app targets (default triple `arm64-apple-ios`; `dev build` takes `--triple`) ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)).
- SwiftUI and UIKit: the template is SwiftUI; UIKit is not separately documented (unverified beyond the SDK's iOS triple).
- App extensions: a Widget Extension example via a second library product + `extensions` in `xtool.yml`. **ExtensionKit is not supported yet**; Foundation extensions are (per docs) ([Appex.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Appex.md)).
- Swift macros: 1.20.0 notes "SwiftUI macros" support via OpenAppleMacros (PR #101) ([release 1.20.0](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)); the SDK builder installs the macro server and plugin stubs ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)). Breadth of macro coverage is unverified.
- watchOS / tvOS / visionOS / macOS **app** targets: no evidence of support. The SDK definition includes macOS triples (for building SwiftPM packages), but the CLI pipeline (`PackOperation`) is iOS-only in the code read ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)). The 1.20.0 notes speak of watchOS as hypothetical future work ([release 1.20.0](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)). Declare **unsupported**.

**Signing and provisioning.**
- `xtool setup` login modes ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)):
  - **0: API Key** — requires a paid Apple Developer Program membership; uses the public App Store Connect API; needs a Team Key with the **App Manager** role.
  - **1: Password** — works with any Apple ID; uses private Apple APIs; the docs suggest a throwaway Apple ID.
- Credentials "are only sent to Apple" per docs (not independently audited here).
- Free accounts: xtool prefixes the bundle ID (e.g. `XTL-1234.com.example.Hello`) because free accounts cannot reuse bundle IDs across accounts ([Control.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md)).
- Some entitlements (Network Extension) need a paid program; some need Apple approval; some capabilities are not mapped yet ([Control.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md)).
- Credentials are saved via `KeyValueSigningInfoManager` (property-list storage) — the storage location and file permissions were not checked ([SigningInfo.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XKit/Utilities/SigningInfo.swift)).
- Anisette: macOS auth backend moved from Omnisette to XADI in 1.20.0; logins must be redone ([1.20.0 notes](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)).
- Signing: `dev build --sign` requires a saved token from `xtool auth`; `dev run` always uses the saved token ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)).
- Developer Services API: `xtool ds devices list`, `ds certificates`, `ds identifiers`, `ds profiles`, `ds teams` are implemented in source; output is `- id:` plus `key: value` lines ([DSDevicesCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DSCommands/DSDevicesCommand.swift)).

**Device install / launch / debug.**
- `xtool devices` (waits for a device by default; `--no-wait` available) prints `name [connectionType]: udid` lines ([DevicesCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevicesCommand.swift)).
- `xtool dev` / `dev run` builds, signs, and installs via `IntegratedInstaller` to the device (`lookupMode: .only(connectionType)`) ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)).
- `xtool install <ipa>`, `uninstall <bundleID>`, `launch <bundleID> [args]` are listed in the README ([README](https://raw.githubusercontent.com/xtool-org/xtool/main/README.md)).
- `launch` uses the `debugserver` protocol (`qLaunchSuccess`, then `D` detach). It launches the app, it does **not** attach a debugger or set breakpoints ([LaunchCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/LaunchCommand.swift)).
- Logs / crash reports / lldb attach: not found in the command list ([README](https://raw.githubusercontent.com/xtool-org/xtool/main/README.md)).

**Simulator.** `xtool dev -s/--simulator` exists only under `#if os(macOS)` and runs `xcrun simctl install booted <app>`. It does **not** boot a simulator, and there is no launch flag in the simulator path ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)). On Linux/WSL: **no simulator support**.

**CLI surface and machine-readable output.** Top-level groups: Configuration (`setup`, `auth`, `sdk`), Development (`new`, `dev`, `ds`), Device (`devices`, `install`, `uninstall`, `launch`) ([XTool.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/XTool.swift)). `dev` subcommands: `generate-xcode-project`, `build` (`-c/--configuration`, `-s/--sign`, `-i/--ipa`, `--triple`), `run` (default), and `bsp` ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)). The `xtool dev build` output path is `./xtool/<App>.app` ([Control.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md)).
- Output is human text ("Wrote to …", "Installing to device: …"). **No `--json` flag was seen in any command read.** Error details are printed when `XTL_DEBUG_ERRORS` is set ([XTool.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/XTool.swift)). Exit codes: `ExitCode.failure` on sign/install errors; cancellation prints `Cancelled.` ([DevCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift)).
- Library use: `XKit` and `XToolSupport` are SwiftPM products; "the Swift interface does not offer the same [semver] guarantees" ([README](https://raw.githubusercontent.com/xtool-org/xtool/main/README.md)). Pin with `.upToNextMinor(from: "1.2.0")` (README example, even though current release is 1.24.0 — example is stale).

**Known limitations vs Xcode (sourced or explicitly flagged).**
- Interface Builder / storyboards / XIBs: not mentioned in any xtool doc; **unverified** (assume unsupported; test).
- Asset catalogs (`.xcassets`): not mentioned; SwiftPM `.copy` resources are documented ([Control.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md)). **Unverified** for `.xcassets` compile.
- Swift macros: partial (see above).
- SwiftUI previews: not mentioned; **unverified**, assume unsupported.
- Instruments / profiling: not found. Assume unsupported.
- App Store submission: not in README feature list. Assume unsupported; **unverified** by search.
- ExtensionKit extensions: unsupported (documented, [Appex.md](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Appex.md)).
- Some entitlements unmapped; some need paid programs (documented).
- Build system: xtool uses SwiftBuild directly on Swift 6.4 instead of `swift build --swift-sdk`; `.sourcekit-lsp/config.json` is no longer enough for IDE support; a `.bsp/xtool.json` is generated ([1.20.0 notes](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)).
- Simulator: macOS only.

**Dependencies to note (licence check for Axiom).** From `Package.swift`: `xtool-core`, `SwiftyMobileDevice` (libimobiledevice binding), `swift-rcodesign`, `swift-certificates`, `swift-crypto`, `XcodeProj` (tuist), `Yams`, `BigInt`, `unxip`, `swift-openapi-generator`, plus Apple's `swift-system`, `swift-argument-parser`, `swift-collections`, `swift-subprocess`, and `async-http-client`/`websocket-kit` ([Package.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift)). Only libimobiledevice's licence (LGPL-2.1, per [GitHub API](https://api.github.com/repos/libimobiledevice/libimobiledevice)) was checked. Other dependency licences are **unverified**.

---

### Darwin Swift SDK (`swift sdk`, SwiftPM Swift SDKs)

**Purpose.** SwiftPM-visible cross-compilation SDK bundle. xtool produces one named `darwin` with triples above and `swift sdk list` shows `darwin` after install ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)).

**Maturity.** Used by xtool 1.20.0+. The SDK layout changes with xtool epochs (`sdkEpoch = 4`) ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)). xtool's own release notes say Swift 6.4 uses SwiftBuild rather than `--swift-sdk` ([1.20.0 notes](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)).

**Licence.** The SDK is derived from Apple's Xcode; licence governed by Apple's Xcode SLA (not retrieved). The xtool builder is MIT.

**Platform restrictions.** Built on Linux only by xtool (on macOS xtool uses Xcode directly). The SDK assumes a Linux host architecture (`x86_64` or `aarch64`) via `--arch` ([SDKCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift)).

**Integration complexity.** High on the SDK-generation side (34 KB SDKBuilder, Xcode extraction, third-party LLVM/macro tarballs). Low on the consumption side: `swift build --swift-sdk darwin` (path-by-env) once installed (unverified CLI form — check against Swift 6.4 SwiftPM docs).

**Alternatives.** Build the SDK yourself from Xcode.xip using the same tool (no alternative found). Or use Apple's own Xcode on macOS.

**Verdict: Adapt.** Let xtool own generation; Axiom only reads `swift sdk list` / `xtool sdk status`.

---

### swiftly (swiftlang/swiftly)

**Purpose.** "A Swift toolchain installer and manager, written in Swift" ([repo API](https://api.github.com/repos/swiftlang/swiftly)).

**Maturity.** Created 2022-06-20; pushed 2026-11-03; 977 stars; 142 open issues; not archived ([repo API](https://api.github.com/repos/swiftlang/swiftly)). Official swiftlang org; project site swift.org/swiftly (homepage field).

**Licence.** Apache-2.0 (SPDX `Apache-2.0`) ([repo API](https://api.github.com/repos/swiftlang/swiftly)).

**Platform restrictions.** Not verified against the README (not fetched). Linux/macOS likely; Windows unverified.

**Integration complexity.** Low–medium: Axiom can invoke `swiftly install 6.4` or accept a preinstalled toolchain. The xtool docs themselves link to swift.org/install/linux instead ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)).

**Alternatives.** swift.org tarball / distro packages (documented by xtool).

**Verdict: Adapt.** Use as the install step for the required Swift 6.4 toolchain; keep swift.org tarballs as fallback. Exact swiftly command syntax: unverified (not fetched).

---

### libimobiledevice / usbmuxd

**Purpose.** "A cross-platform protocol library to communicate with iOS devices" ([repo API](https://api.github.com/repos/libimobiledevice/libimobiledevice)). usbmuxd is the multiplexing daemon xtool requires on Linux ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)).

**Maturity.** libimobiledevice: created 2013-02-13; pushed 2026-06-10; 8,338 stars; 851 open issues; not archived ([repo API](https://api.github.com/repos/libimobiledevice/libimobiledevice)). usbmuxd repository not fetched (unverified state).

**Licence.** LGPL-2.1 (SPDX `LGPL-2.1`) ([repo API](https://api.github.com/repos/libimobiledevice/libimobiledevice)).

**Platform restrictions.** Linux-first; Windows via iTunes relay or WSL USBIPD per xtool docs ([Installation-Linux](https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md)). macOS support unverified here.

**Integration complexity.** Low if called as external binaries (`usbmuxd`, `ideviceinfo`). xtool itself links `SwiftyMobileDevice`, which is a Swift wrapper ([Package.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift)).

**Verdict: Adapt.** Treat as an external daemon/CLI. Do not link into Axiom's binary without an LGPL-2.1 review.

---

### pymobiledevice3 (doronz88/pymobiledevice3)

**Purpose.** Pure-Python implementation to interact with iOS devices: installing/launching apps, syslog, syscall tracing, screenshots, location simulation, screen mirroring, DDI mounting, backups, Safari/WebView CDP bridge. Works on Windows, Linux, macOS ([README](https://raw.githubusercontent.com/doronz88/pymobiledevice3/master/README.md)).

**Maturity.** Created 2021-04-14; pushed 2026-11-11; 3,018 stars; 30 open issues; not archived; PyPI latest 2.39.2 per the search result ([repo API](https://api.github.com/repos/doronz88/pymobiledevice3); [search result](https://pypi.org/pypi/pymobiledevice3/2.39.2/json)).

**Licence.** GPL-3.0 (SPDX `GPL-3.0`) ([repo API](https://api.github.com/repos/doronz88/pymobiledevice3)). README: "Planning to ship it in a closed-source app? Read this FAQ first." ([README](https://raw.githubusercontent.com/doronz88/pymobiledevice3/master/README.md)). The FAQ was not fetched.

**Platform restrictions.** Windows/Linux/macOS. On iOS 17+ "over a no-root tunnel each command builds for you, on Windows and Linux too" ([README](https://raw.githubusercontent.com/doronz88/pymobiledevice3/master/README.md)). Windows/Linux first need iTunes or usbmuxd.

**Integration complexity.** Low if used as a subprocess via `uv tool`/`pipx`; the project recommends `uv tool install --managed-python pymobiledevice3`. Python dependency management in Axiom would be an added surface.

**Alternatives.** libimobiledevice tools (LGPL-2.1, narrower), go-ios (MIT).

**Verdict: Adapt (out-of-process only).** Use for device-side tasks xtool does not do (syslog, screenshots, location, DDI, CDP). Never link; the GPL-3.0 copyleft applies to combined works.

---

### go-ios (danielpaulus/go-ios)

**Purpose.** "An operating system independent implementation of iOS device features. You can run UI tests, launch or kill apps, install apps etc." ([repo API](https://api.github.com/repos/danielpaulus/go-ios)).

**Maturity.** Created 2019-05-28; pushed 2026-11-10; 2,329 stars; 114 open issues; not archived ([repo API](https://api.github.com/repos/danielpaulus/go-ios)). Topics include `xcuitest`, `instruments`, `dtx`, `appium-ios`, `linux` ([repo API](https://api.github.com/repos/danielpaulus/go-ios)).

**Licence.** MIT (SPDX `MIT`) ([repo API](https://api.github.com/repos/danielpaulus/go-ios)).

**Platform restrictions.** Linux and OS-independent per description; a Go binary. Exact iOS version coverage: **unverified** (not fetched).

**Integration complexity.** Low: single binary. The `xcuitest` topic suggests it can run UI test bundles, which is relevant to the visual-intelligence engine — **unverified** for Axiom's required element-tree output.

**Alternatives.** pymobiledevice3 (GPL-3.0, broader), WebDriverAgent (XCUITest-based, NOASSERTION licence).

**Verdict: Adapt.** Candidate device-automation backend. Verify the iOS version matrix and tree/accessibility output before committing.

---

### XcodeGen (yonaskolb/XcodeGen)

**Purpose.** "A Swift command line tool for generating your Xcode project" from a YAML spec ([repo API](https://api.github.com/repos/yonaskolb/XcodeGen)).

**Maturity.** Created 2017-07-19; pushed 2026-10-13; 8,946 stars; 408 open issues; not archived ([repo API](https://api.github.com/repos/yonaskolb/XcodeGen)).

**Licence.** MIT ([repo API](https://api.github.com/repos/yonaskolb/XcodeGen)).

**Platform restrictions.** Generates `.xcodeproj` — Xcode is needed to build, so macOS-only in practice. (The xtool release workflow itself installs `xcodegen` via brew on macOS, showing xtool's own macOS build depends on it ([release.yml](https://raw.githubusercontent.com/xtool-org/xtool/main/.github/workflows/release.yml)).)

**Verdict: Reject for Linux hosts; Adapt for macOS provider** only if Axiom needs `.xcodeproj` output (`xtool dev generate-xcode-project` covers part of this on xtool's side).

---

### Tuist (tuist/tuist)

**Purpose.** "Tuist builds infrastructure for faster, more reliable build and test workflows across developers, CI, and coding agents" — remote cache, tests, insights, alongside existing CI ([repo API](https://api.github.com/repos/tuist/tuist)).

**Maturity.** Created 2018-04-16; pushed 2026-11-11; 5,864 stars; 601 open issues ([repo API](https://api.github.com/repos/tuist/tuist)).

**Licence.** Repository field reports `NOASSERTION` / "Other" — the licence text was not fetched; **must be read before any use** ([repo API](https://api.github.com/repos/tuist/tuist)).

**Platform restrictions.** Xcode-based; macOS-only in practice (`xcodebuild` topic).

**Verdict: Reject for Axiom v1.** Heavy, service-oriented, and licence unclear from metadata.

---

### Theos (theos/theos)

**Purpose.** Repo description: "A cross-platform suite of tools for building and deploying software for iOS and other platforms" ([repo API](https://api.github.com/repos/theos/theos)). Its docs/positioning were not fetched; historically it targets jailbroken-device tweaks (**unverified** in this session).

**Maturity.** Created 2010-02-04; pushed 2026-11-07; 4,944 stars; 70 open issues; homepage theos.dev ([repo API](https://api.github.com/repos/theos/theos)).

**Licence.** Reported as "Other" / NOASSERTION; the LICENSE text was not read ([repo API](https://api.github.com/repos/theos/theos)).

**Platform restrictions.** Topics include linux, macos, windows ([repo API](https://api.github.com/repos/theos/theos)). Relevance to non-jailbroken app builds: **unverified / likely low**.

**Verdict: Reject for v1.** Unclear licence and likely jailbreak-oriented scope.

---

### WebDriverAgent (appium/WebDriverAgent; facebookarchive/WebDriverAgent)

**Purpose.** "A WebDriver server for iOS, tvOS and watchOS" (appium fork description) ([repo API](https://api.github.com/repos/appium/WebDriverAgent)). Upstream (facebookarchive) describes it as running inside the Simulator ([repo API](https://api.github.com/repos/appium/WebDriverAgent)). Accessibility/element-tree access for UI automation.

**Maturity.** Appium fork: created 2017-03-14; pushed 2026-10-29; 1,967 stars; 42 open issues; not archived. Upstream facebookarchive/WebDriverAgent: **archived**, last push 2019-12-26 ([repo API](https://api.github.com/repos/appium/WebDriverAgent)).

**Licence.** `NOASSERTION` / "Other" on both repos; the licence text was not read ([repo API](https://api.github.com/repos/appium/WebDriverAgent)). **Must be read before use.**

**Platform restrictions.** Built with Xcode (Objective-C); typically run via `xcodebuild` on a macOS host with a signed runner — **unverified**; the Appium docs were not fetched. Simulator/device use both documented in upstream description (simulator only in the upstream text).

**Integration complexity.** High; needs Xcode, signing, and a running XCUITest runner on-device.

**Verdict: Adapt (macOS-host only, experimental).** Only as an accessibility-tree source for the visual-intelligence engine, after the licence and macOS-build requirement are verified.

---

### Other neighbours checked briefly

- **swift-openapi-generator / swift-argument-parser / swift-crypto** (Apple packages used by xtool): listed as dependencies in [Package.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift); not evaluated further.
- **swift-docc-plugin**: used by xtool docs ([Package.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift)); not relevant to Axiom.

---

## Architectural implications for Axiom

Axiom's agent runtime calls structured tools; the iOS provider should be one pluggable toolchain provider with an explicit capability matrix. The adapter below maps to real xtool commands.

### Provider adapter: `xtool` (`ios.xtool`)

**Interface**

```text
detect(host) -> { available: bool, version: string|null, hostOS: "linux"|"wsl"|"macos"|"unsupported", reasons: [] }
prerequisites(host) -> [ check items ]
install_plan(host) -> [ steps ]
build(project, opts) -> { appPath|ipaPath, log }
run(project, device, opts) -> { exitCode, log }
deploy(app, device) -> { exitCode }
list_devices() -> [ { name, connection, udid } ]    // parsed from plain text
capabilities() -> { simulator: bool, ... }           // static per host
```

**`detect()` (no installs; read-only probes)**
- Host OS: `uname -s` → Linux or Darwin; WSL via `/proc/version` containing `microsoft`; native Windows → `unsupported` (docs say WSL only).
- `xtool --version` (or `xtool --help` exit 0). Note: `XTool.version` is a compiled-in string ("xtool \(XTool.version)") ([XTool.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/XTool.swift)). The `--version` flag is implied by `ArgumentParser`'s `version:` config — **unverified** by running.
- `swift --version` must print Swift 6.4 or newer (xtool docs/SDK code require 6.4+).
- Linux: `command -v usbmuxd` and `pgrep -x usbmuxd`; WSL: check USBIPD binding (manual step, cannot automate).
- SDK: `swift sdk list` contains `darwin`; `xtool sdk status` prints "Darwin SDK is installed" or "Not installed" ([SDKCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift)). Verify version line matches expected epoch.
- Auth: `xtool` has no `status` subcommand in the code read; check `~/` config via `xtool auth` behaviour — **unverified**. Treat "logged in" as unknown until a `ds devices list` call succeeds.

**Prerequisites (declared, per host)**
- Linux/WSL: Swift 6.4+, usbmuxd, Xcode.xip (user-downloaded, 27.1 per docs), ~7 GB free disk for normal SDK (~3 GB legacy figure in 1.20.0 notes is the old layout; slim is smaller) ([release 1.20.0](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)).
- macOS: Xcode with iOS SDK (`xcrun -sdk iphoneos -show-sdk-path`), Swift 6.1+ per docs sample (**verify**: Swift 6.4 requirement is documented for Linux only; the macOS docs show Swift 6.1 output as an example).
- Apple account: paid Developer Program (API key) or throwaway Apple ID (password). Axiom must never store Apple credentials itself; it should defer to `xtool setup`/`xtool auth`.

**`install_plan()` (idempotent steps; each step shows the exact action and needs user confirmation)**
1. Install Swift 6.4 via swiftly or swift.org tarball (adapter cannot invent a swiftly syntax without verification).
2. Install usbmuxd via the distro package manager (`apt-get install usbmuxd` as in the docs). Require sudo; prompt user.
3. Download the pinned xtool AppImage from GitHub Releases for the matching arch; verify `digest` (sha256) against the API asset `digest` field before installing ([release API](https://api.github.com/repos/xtool-org/xtool/releases/latest) shows `sha256:` digests per asset). Place in `~/.local/bin` or Axiom's private tools dir. Pin to a known version (e.g. 1.24.0).
4. **User-manual step:** download Xcode.xip (requires Apple ID; cannot be automated — docs explicitly say browser only).
5. `xtool setup`: interactive (login mode, credentials, Xcode.xip path). Axiom must relay prompts through its UI; never pipe credentials.
6. Verify: `swift sdk list` shows `darwin`; `xtool sdk status` is installed and up to date.
7. Optional: set `XTL_TOOLSET_TARBALL` / `XTL_OAM_TARBALL` to pinned local copies for reproducible SDK builds ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)).

**Operations mapped to xtool CLI**

| Axiom operation | xtool command (from source/README) | Notes |
|---|---|---|
| detect / version | `xtool --help`, `xtool sdk status` | Parse text; no JSON |
| setup (interactive) | `xtool setup` | Requires TTY prompts |
| login | `xtool auth` | Password/API key modes |
| SDK install (from Xcode.xip) | `xtool sdk install <Xcode.xip>` (`--slim` optional) | Long-running; progress on stdout |
| SDK update | `xtool sdk update` | Not for slim installs |
| new project | `xtool new <name>` | Runs setup unless `--skip-setup`; name `[A-Za-z_][A-Za-z0-9_-]*` ([NewCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/NewCommand.swift)) |
| build | `xtool dev build` (`-c debug\|release`, `--triple`) | Output `./xtool/<App>.app` |
| build IPA | `xtool dev build --ipa` | Writes `.ipa` |
| build + sign | `xtool dev build --sign` | Requires saved token |
| build + deploy (device) | `xtool dev` / `xtool dev run` | Installs to first matched device |
| simulator (macOS only) | `xtool dev run -s` | Installs to `booted` sim; does not launch |
| list devices | `xtool devices` | Text: `name [conn]: udid`; waits unless `--no-wait` |
| install ipa | `xtool install <ipa>` | README-listed |
| uninstall | `xtool uninstall <bundleID>` | README-listed |
| launch | `xtool launch <bundleID> [args]` | Launch only; no debugger |
| Developer Services devices | `xtool ds devices list` | Requires paid/API auth |
| Xcode project (macOS) | `xtool dev generate-xcode-project` | Writes an Xcode project; validation unverified |
| Language server | `xtool dev bsp` / `.bsp/xtool.json` | Auto-generated on Swift 6.4 |

**Parsing guidance:** xtool emits human text. For Axiom's structured tools, the adapter should (a) parse `devices` lines with a regex, (b) treat exit code ≠ 0 as failure, (c) capture stdout/stderr, (d) set `XTL_DEBUG_ERRORS=1` on failure to get the error detail ([XTool.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/XTool.swift)). Avoid depending on the Swift library surface (no semver).

**Must be declared unsupported (`capabilities()` returns `false`) on the xtool provider:**
- `simulator` on Linux/WSL.
- Native Windows host (WSL only).
- Targets: watchOS, tvOS, visionOS, macOS app bundles, ExtensionKit extensions.
- Storyboards/XIBs, asset catalog compilation, SwiftUI previews, Instruments/profiling, App Store submission/TestFlight — unless a later test proves otherwise (see Unverified).
- Debugger attach / breakpoints (launch only).
- Universal Xcode-project features beyond `generate-xcode-project`.
- Automatic Apple account provisioning without user-supplied credentials (requires a human in the loop for login).
- Any CI use without a pre-populated SDK and `XTL_*` pinned tarballs (SDK rebuild is >7 GB and requires Xcode.xip download — a Linux CI cannot fetch it automatically; docs say Xcode.xip must be downloaded in a browser).

### Architectural decisions this implies for Axiom

1. **Provider isolation.** Run xtool as an external process behind the provider interface; never link its Swift library. Its version is the cache key for the SDK, which follows the xtool epoch mechanism.
2. **Capability matrix in the UI.** Show "simulator: not available on Linux/WSL" rather than failing late.
3. **Credentials.** Axiom must not store Apple ID or API keys; delegate to xtool's keychain/key-value storage and surface prompts in the GUI. Mark the Apple-licence aspect as a human/legal review item (see Risks).
4. **Pinning.** Pin the xtool AppImage by version + sha256 digest; pin the darwin-tools (1.1.0) and OpenAppleMacros (1.4.1) tarballs through `XTL_*` overrides; record SDK version string.
5. **Visual intelligence bridge.** Device UI hierarchy is not available from xtool. A second provider (go-ios/pymobiledevice3 or WebDriverAgent on macOS) would be needed for element→source mapping; xtool's LSP/BSP output could feed symbol info but that is unverified.
6. **Build artefacts.** Store `./xtool/<App>.app` and the `darwin` SDK path in the project's `.axiom/` cache; never commit them (xtool's `new` template already ignores `/xtool`, [NewCommand.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/NewCommand.swift)).
7. **Native macOS path first.** On macOS, prefer Xcode/`xcodebuild`/`xcrun simctl` directly; use xtool only where Axiom needs Linux parity.

## Risks

- **Apple licence/terms (legal review required).** Using Apple's Xcode.xip SDK on non-Apple hardware, and redistributing or caching the extracted SDK, may conflict with Apple's Xcode/SDK licence. The licence text was not retrieved in this session. Do not ship or cache the extracted SDK in Axiom releases without legal review. Not legal advice.
- **Private Apple APIs.** xtool's password login "relies on private Apple APIs" (docs). Apple may break or restrict this at any time; xtool has shipped multiple auth-only fixes (1.19.2, 1.23.0, 1.24.0) ([release notes](https://api.github.com/repos/xtool-org/xtool/releases?per_page=8)). Account-lock risk for a user's Apple ID is plausible but unverified.
- **Supply chain.** SDK build fetches LLVM toolset and OpenAppleMacros from GitHub release tarballs with no checksum check in the code read ([SDKBuilder.swift](https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift)). Pin and verify these, or vendor them with `XTL_*` overrides.
- **Maintenance concentration.** Core contributors appear limited (org `xtool-org`, most recent PRs by a small set of accounts) — **unverified** contributor count. 119 open issues.
- **Version churn.** SDK layout changed in 1.20.0 (reinstall required); auth backend changed (re-login required). Expect breakage on upgrades.
- **Platform gap.** No Windows-native host; no simulator off macOS; no visionOS/watchOS/tvOS.
- **Swift 6.4 dependency.** xtool requires Swift 6.4; Axiom's own toolchain must match or be separated.
- **GPL-3.0 / LGPL-2.1 dependencies.** pymobiledevice3 (GPL-3.0) and libimobiledevice (LGPL-2.1) must not be statically linked into Axiom without a review.
- **Unknown licences.** Tuist, Theos, WebDriverAgent report NOASSERTION/"Other" — do not adopt until the text is read.
- **Xcode.xip download is a manual, authenticated step.** Blocks unattended CI and any "one-click" onboarding.
- **Machine-readable output absent.** Text parsing is brittle across releases.
- **Stale README example.** README pins `.upToNextMinor(from: "1.2.0")` while the current release is 1.24.0 — docs may lag the code.

## Unverified / needs experiment

- **Apple Xcode/SDK licence text.** Not retrieved (`web_fetch` rejects PDF; redirect blocked). Obtain the current Xcode SLA and the Apple Developer Program License Agreement and review the clauses on: installing on non-Apple hardware, extracting SDK components, redistribution, and use of private APIs. Do not quote from memory.
- **xtool LICENSE file.** The `main` branch raw `LICENSE` returned 404; only the GitHub API field `MIT` was read. Confirm the file in tag 1.24.0.
- **xtool.sh documentation pages.** The landing page was empty and the getting-started path 404'd; installation text came from the repo's DocC sources. Compare with the live site.
- **`xtool --version`** flag and exact output format: not run.
- **`xtool install` / `uninstall` / `launch` arguments** beyond the README listing: `InstallCommand.swift`, `UninstallCommand.swift` not read.
- **Auth state detection:** whether any command reports login state without a network call; location of stored credentials and their file permissions (`KeyValueSigningInfoManager`/`keyValueStorage` backing not read).
- **Free-account limits:** number of App IDs/devices/certificate lifetimes for free Apple IDs under xtool's flow; whether xtool hits them.
- **Device compatibility:** iOS version matrix for xtool install/launch; behaviour on iOS 26/27; Windows/WSL reliability with USBIPD.
- **Simulator:** whether `xtool dev run -s` works on macOS end-to-end; no test performed.
- **SwiftUI macros coverage** (which OpenAppleMacros plugins ship with 1.4.1 — `info.json` `plugins` list not read).
- **Asset catalogs (`.xcassets`), storyboards/XIBs, SwiftUI previews, Instruments** — not documented; test with a minimal package.
- **App Store / TestFlight submission:** not in README; search found no support statement.
- **Dependency licences** for swift-rcodesign, SwiftyMobileDevice, xtool-core, XcodeProj, unxip, Yams, BigInt (only libimobiledevice checked).
- **swiftly:** README and exact `swiftly install` syntax not fetched; platform support unverified.
- **Theos:** scope (jailbreak vs app builds) and LICENSE text unverified.
- **Tuist and WebDriverAgent licence texts** (NOASSERTION/"Other") not read.
- **WebDriverAgent build requirements** on macOS (signing, `xcodebuild` invocation, Appium server) — not verified.
- **go-ios iOS version matrix and element-tree / accessibility output** — not verified.
- **pymobiledevice3 FAQ** on closed-source use (linked from README) — not read.
- **usbmuxd status/licence** — repo not fetched.
- **Contributor/maintainer count** for xtool and for pymobiledevice3 — not checked.
- **Swift 6.4 on macOS**: docs show Swift 6.1 sample on macOS; whether xtool requires 6.4 there is unclear.
- **swift-sdk CLI form** for consuming the `darwin` bundle with SwiftPM 6.4 — not verified against swift.org docs.

## Sources

- xtool repo (GitHub, page fetched but navigation-only): https://github.com/xtool-org/xtool
- xtool repo metadata (licence, stars, pushed): https://api.github.com/repos/xtool-org/xtool
- xtool README (raw): https://raw.githubusercontent.com/xtool-org/xtool/main/README.md
- xtool latest release (1.24.0): https://api.github.com/repos/xtool-org/xtool/releases/latest
- xtool release list (1.19.0–1.24.0 notes): https://api.github.com/repos/xtool-org/xtool/releases?per_page=8
- xtool release page (1.24.0, HTML): https://github.com/xtool-org/xtool/releases/tag/1.24.0
- xtool releases page (HTML, truncated): https://github.com/xtool-org/xtool/releases
- xtool DocC — Linux install: https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-Linux.md
- xtool DocC — macOS install: https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Installation-macOS.md
- xtool DocC — Control (xtool.yml, resources, icons, entitlements): https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Control.md
- xtool DocC — App extensions: https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/Appex.md
- xtool DocC — landing: https://raw.githubusercontent.com/xtool-org/xtool/main/Documentation/xtool.docc/xtool.md
- xtool docs site (empty / 404 on fetch): https://xtool.sh/documentation/xtool/ ; https://xtool.sh/documentation/xtool/installation-linux/ ; https://xtool.sh/documentation/xtool/installation-macos/ ; https://xtool.sh/documentation/xtooldocs/installation-linux ; https://xtool.sh/documentation/xtool/getting-started
- xtool Package.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Package.swift
- xtool SDKBuilder.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKBuilder.swift
- xtool SDKCommand.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/SDKCommand.swift
- xtool DevCommand.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevCommand.swift
- xtool DevicesCommand.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DevicesCommand.swift
- xtool LaunchCommand.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/LaunchCommand.swift
- xtool DSDevicesCommand.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/DSCommands/DSDevicesCommand.swift
- xtool NewCommand.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/NewCommand.swift
- xtool XTool.swift (command tree, XTL_DEBUG_ERRORS): https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XToolSupport/XTool.swift
- xtool SigningInfo.swift: https://raw.githubusercontent.com/xtool-org/xtool/main/Sources/XKit/Utilities/SigningInfo.swift
- xtool release workflow: https://raw.githubusercontent.com/xtool-org/xtool/main/.github/workflows/release.yml
- Apple Xcode SLA (listed by search; not fetched — PDF rejected): http://images.apple.com/legal/sla/docs/xcode.pdf ; https://images.apple.com/legal/sla/docs/xcode.pdf
- web search (two queries; results: Apple SLA URL, PyPI JSON, unrelated pages): web_search "Apple Xcode and iOS SDK license agreement ..." ; "pymobiledevice3 github license README iOS 17 tunnel"
- pymobiledevice3 README: https://raw.githubusercontent.com/doronz88/pymobiledevice3/master/README.md
- pymobiledevice3 repo metadata: https://api.github.com/repos/doronz88/pymobiledevice3
- pymobiledevice3 PyPI JSON (2.39.2, from search): https://pypi.org/pypi/pymobiledevice3/2.39.2/json
- libimobiledevice repo metadata: https://api.github.com/repos/libimobiledevice/libimobiledevice
- ios-deploy (404, not used): https://api.github.com/repos/leetal/ios-deploy
- WebDriverAgent (appium fork, metadata): https://api.github.com/repos/appium/WebDriverAgent
- swiftly repo metadata: https://api.github.com/repos/swiftlang/swiftly
- Theos repo metadata: https://api.github.com/repos/theos/theos
- go-ios repo metadata: https://api.github.com/repos/danielpaulus/go-ios
- XcodeGen repo metadata: https://api.github.com/repos/yonaskolb/XcodeGen
- Tuist repo metadata: https://api.github.com/repos/tuist/tuist
