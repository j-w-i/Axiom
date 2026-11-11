# Development-environment and toolchain management
> Researched 2026-11-11. Status of each claim: sourced unless listed as unverified.

**Research access:** web search and web fetch worked, but one malformed search was rejected and the Codex security-page fetch failed on a cross-origin redirect. Findings below use other primary sources; gaps are explicitly listed as unverified. Repository default-branch documents describe observed current behavior, not a pinned release.

## Summary and recommendations
- **Adopt — provider interface:** invoke established managers as subprocesses; standardize discovery, resolution, install, environment, and diagnostics rather than replacing them.
- **Adapt — mise:** preferred broad CLI integration and registry inspiration; distinguish core, aqua, asdf and package-ecosystem backends, which have different trust and portability properties.
- **Adapt — aqua registry:** reusable MIT-licensed package metadata, subject to provenance, template and checksum verification; do not treat every record as a complete lockfile.
- **Adapt — asdf, proto, vfox:** import existing project declarations or invoke CLIs where available; do not promise universal plugin portability.
- **Adopt — rustup, Swiftly, uv and language vendors:** prefer specialist CLIs or verified official artifacts for their own languages.
- **Adapt — nvm, fnm, Volta, pyenv, SDKMAN:** detect existing selections and offer opt-in adapters; none is a universal cross-platform source of truth.
- **Adapt — Nix, devenv, Devbox, Flox:** optional reproducible environment backends, not the only onboarding path; Windows Nix runs under WSL2.
- **Adopt — Dev Container specification and reference CLI:** interoperability-first optional backend; explicit trust prompt for host lifecycle scripts and privileged configuration.
- **Adapt — Docker and Podman:** support user-selected runtime, test individual Features and Compose; do not assume engine equivalence or Docker Desktop is free for every organization.
- **Adopt — OS process sandboxes:** capability-probe and fail closed; combine filesystem, process, network and syscall restrictions, with platform-specific disclosure.
- **Reject — Job Objects or WSL2 alone as a complete security sandbox:** resource/process controls and a compatibility VM respectively do not alone implement a complete policy.
- **Adapt — system package managers:** explicit, previewed, privilege-aware system mode; user-scoped tool installs remain default.
- **Adopt — structured build output:** Cargo JSON, CMake File API, compiler JSON and Bazel BEP where available; text problem matchers are fallback.
- **Adapt — SSH, REAPI, distcc and sccache:** distinct optional remote-execution or cache adapters; avoid claiming they provide equivalent hermeticity.

## Findings
### mise
- Purpose and maturity: general tool/environment/task manager; documentation observed version **2026.11.6** and calls `ubi` deprecated ([backends](https://mise.jdx.dev/dev-tools/backends/)).
- Data: `mise.toml` describes tool requests and configuration; the registry maps short tool names to backends; `mise.lock` is separate from the request ([configuration](https://mise.jdx.dev/configuration.html), [registry](https://mise.jdx.dev/registry.html), [lockfile](https://mise.jdx.dev/dev-tools/mise-lock.html)).
- Backends: core implementations; aqua metadata; legacy asdf plugins; `cargo:` and `npm:` ecosystem installation; deprecated `ubi:`; current docs also list `github:` and `vfox:` ([backends](https://mise.jdx.dev/dev-tools/backends/)).
- Licence: MIT ([licence](https://github.com/jdx/mise/blob/main/LICENSE)).
- Platform: native Windows exists, but a backend or shell plugin need not support it ([installation](https://mise.jdx.dev/installing-mise.html), [asdf backend](https://mise.jdx.dev/dev-tools/backends/asdf.html)).
- Integration: CLI adapter, project trust and environment capture; do not infer a stable Rust embedding API from a Rust implementation.
- Alternative/verdict: proto for WASM plugin architecture, native specialists for language-specific semantics; **Adapt**.

### asdf
- Purpose: extensible runtime version manager; `.tool-versions` stores tool/version selections and plugins implement tool-specific install behavior ([guide](https://asdf-vm.com/guide/getting-started.html), [plugin authoring](https://asdf-vm.com/plugins/create.html)).
- Licence: MIT ([licence](https://github.com/asdf-vm/asdf/blob/master/LICENSE)).
- Platform: primarily Unix shell/plugin ecosystem; native Windows support and plugin compatibility must be tested individually.
- Integration: subprocess and shell plugins, not a guaranteed portable artifact registry; sourcing a plugin means executing third-party code.
- Alternatives/verdict: mise can consume legacy plugins; **Adapt** declarations, not the assumption every plugin works on Windows.

### proto
- Purpose: multi-language toolchain manager; `.prototools` and plugin configuration represent version selections and tool definitions ([proto](https://moonrepo.dev/docs/proto), [configuration](https://moonrepo.dev/docs/proto/config)).
- Licence: MIT ([licence](https://github.com/moonrepo/proto/blob/master/LICENSE)).
- Platform: documentation covers cross-platform installation including Windows; actual tools remain platform dependent ([installation](https://moonrepo.dev/docs/proto/install)).
- Integration: CLI first; plugin system includes WASM and TOML definitions, but embedding compatibility requires separate verification ([plugins](https://moonrepo.dev/docs/proto/plugins)).
- Alternative/verdict: compare with mise rather than forcing conversion; **Adapt**.

### rustup
- Purpose: installs/selects Rust toolchains, components and compilation targets; project toolchain file may pin a channel ([overrides](https://rust-lang.github.io/rustup/overrides.html), [components](https://rust-lang.github.io/rustup/concepts/components.html)).
- Licence: `MIT OR Apache-2.0` ([repository](https://github.com/rust-lang/rustup)).
- Platform: Linux, macOS and Windows with host-dependent targets and prerequisites ([installation](https://rust-lang.github.io/rustup/installation/index.html)).
- Integration: run `rustup` as a CLI, preserve existing toolchain overrides and `CARGO_HOME`/`RUSTUP_HOME`.
- Alternative/verdict: managed archive installs lose rustup component/target semantics; **Adopt**.

### Swiftly
- Purpose: official Swift toolchain installer and selector; use its own documented toolchain/version behavior ([Swiftly](https://www.swift.org/install/)).
- Licence: `Apache-2.0` with Swift exception; confirm exact expression from repository before redistribution ([repository](https://github.com/swiftlang/swiftly)).
- Platform: consult current Swiftly install instructions per host; do not infer Windows CLI coverage from Swift itself.
- Integration: optional CLI provider; Xcode Apple SDK selection remains a separate concern.
- Alternative/verdict: official Swift packages and Xcode; **Adapt**.

### nvm, fnm and Volta
- nvm is a per-user, per-shell Node manager; POSIX shell integration and `.nvmrc` are central; Windows users generally need WSL or another manager ([nvm](https://github.com/nvm-sh/nvm)).
- fnm supports Windows as well as macOS/Linux and uses shell integration; it recognizes Node version files ([fnm](https://github.com/Schniz/fnm)).
- Volta pins Node and JavaScript tools in project metadata and offers native Windows support ([Volta](https://docs.volta.sh/guide/), [installation](https://docs.volta.sh/guide/getting-started)).
- Licences: nvm `MIT`, fnm `GPL-3.0-only` according to its repository licence, Volta `BSD-2-Clause`; validate redistribution obligations and exact repository revision ([nvm](https://github.com/nvm-sh/nvm/blob/master/LICENSE.md), [fnm](https://github.com/Schniz/fnm/blob/master/LICENSE), [Volta](https://github.com/volta-cli/volta/blob/main/LICENSE)).
- Integration: use CLI or capture shell environment; nvm is a sourced shell function, not an ordinary standalone executable.
- Alternative/verdict: official Node artifacts and mise; **Adapt** for existing workspaces.

### pyenv and uv
- pyenv selects Python versions using shims and version files; upstream pyenv does not directly support native Windows ([pyenv](https://github.com/pyenv/pyenv)).
- uv is a Python project/package manager and can install/manage Python versions across Linux, macOS and Windows ([uv Python versions](https://docs.astral.sh/uv/concepts/python-versions/), [installation](https://docs.astral.sh/uv/getting-started/installation/)).
- Licences: pyenv `MIT`; uv `MIT OR Apache-2.0` ([pyenv licence](https://github.com/pyenv/pyenv/blob/master/LICENSE), [uv repository](https://github.com/astral-sh/uv)).
- Integration: CLI adapters; do not conflate interpreter version, environment, package lock, and dependency resolution.
- Alternative/verdict: official Python installers where appropriate; **Adopt** uv adapter, **Adapt** pyenv compatibility.

### SDKMAN
- Purpose: JVM ecosystem toolchain installer and selector; candidate/version model and per-project environment file ([usage](https://sdkman.io/usage/)).
- Licence: `Apache-2.0` ([licence](https://github.com/sdkman/sdkman-cli/blob/master/LICENSE)).
- Platform: Unix-like shell; Windows documented via compatibility environments, not a native PowerShell assumption ([installation](https://sdkman.io/install/)).
- Integration: CLI/shell only; environment persistence and initialization require shell-aware handling.
- Alternative/verdict: Adoptium API for JDKs, native Windows provider; **Adapt**.

### aqua registry
- Purpose: registry of package installation recipes consumed by aqua; registry itself is `MIT` ([registry](https://github.com/aquaproj/aqua-registry), [registry documentation](https://aquaproj.github.io/docs/products/aqua-registry)).
- Data model: registry YAML records describe package source/asset selection and may specify checksum, platform overrides and version-specific behavior ([registry configuration](https://aquaproj.github.io/docs/reference/registry-config/)).
- Platform: metadata can describe multiple OS/architectures, but each package has its own support and checksum availability.
- Integration: import pinned registry snapshots as data, with licence notices and schema-version tests; templates and external scripts are untrusted inputs.
- Alternative/verdict: official publisher metadata for canonical artifacts; **Adapt**, never silently bypass missing verification.

### vfox
- Purpose: cross-platform version manager with a plugin ecosystem; supports Windows, macOS and Linux according to project documentation ([vfox](https://vfox.dev/)).
- Licence: `Apache-2.0` ([licence](https://github.com/version-fox/vfox/blob/main/LICENSE)).
- Integration: CLI and plugin adapter; mise lists a vfox backend, but plugin compatibility must be verified per tool ([mise backends](https://mise.jdx.dev/dev-tools/backends/)).
- Alternative/verdict: mise and proto; **Adapt** where a needed plugin exists.

### Nix
- Purpose: declarative dependency graphs and isolated store paths; pinned Nixpkgs inputs improve repeatability, not a guarantee of bit-identical outputs for all builds ([pinning](https://nix.dev/tutorials/first-steps/towards-reproducibility-pinning-nixpkgs), [Nix](https://nix.dev/)).
- Licence: `LGPL-2.1-only` for Nix itself; packages retain independent licences ([licence](https://github.com/NixOS/nix/blob/master/COPYING)).
- Platform: official installation instructions cover Linux, macOS and **Windows through WSL2** ([install](https://nix.dev/install-nix.html)).
- Integration: CLI `nix develop`-style execution, lock input and host identity; macOS toolchains/SDK availability can still constrain builds.
- Alternative/verdict: Dev Containers for portable Linux userland; **Adapt** as an optional provider, not native Windows environment.

### devenv, Devbox and Flox
- devenv builds developer environments on Nix and publishes cross-platform recipes ([devenv](https://devenv.sh/), [cross-platform](https://devenv.sh/recipes/cross-platform/)).
- Devbox uses Nix packages with a project configuration/lock-oriented interface ([Devbox](https://www.jetify.com/docs/devbox/), [install](https://www.jetify.com/docs/devbox/installing-devbox/)).
- Flox describes managed reproducible environments, including macOS and Windows/WSL workflows ([Flox](https://flox.dev/docs/), [WSL example](https://flox.dev/popular-packages/get-containers-on-macos-and-windows-with-colima-and-flox/)).
- Platform: distinguish native macOS execution from Linux execution under WSL2; check current product-specific support before offering an adapter.
- Licence: per-project and component licence audit required; do not inherit Nix's licence for these products.
- Integration/verdict: CLI adapters with explicit lock/provenance and environment inspection; **Adapt**, not default onboarding.

### Dev Containers specification and reference CLI
- Purpose: `devcontainer.json` describes image/Dockerfile/Compose, workspace mount, user, environment, port forwarding, lifecycle and Features ([metadata reference](https://containers.dev/implementors/json_reference/)).
- Features are installable reusable components, not merely metadata; their install scripts execute as part of container construction ([Features specification](https://containers.dev/implementors/features/)).
- The reference CLI is open source under `MIT` ([reference implementation](https://containers.dev/implementors/reference/), [CLI licence](https://github.com/devcontainers/cli/blob/main/LICENSE)).
- Security: `initializeCommand` executes on the **host**; `privileged`, mounts, capabilities and lifecycle commands alter the security boundary ([reference](https://containers.dev/implementors/json_reference/)).
- Platform: depends on runtime/VM; Linux containers do not make native macOS or Windows binaries available.
- Integration/verdict: call CLI, surface logs, image digest, mounts and host commands before launch; **Adopt** spec, **Adapt** CLI.

### Docker and Podman
- Docker Engine is a container runtime platform; Docker Desktop licensing is separate from the Engine's open-source components ([Engine](https://docs.docker.com/engine/), [Desktop licence](https://docs.docker.com/subscription/desktop-license/)).
- Docker Desktop generally requires a paid subscription for larger commercial organizations; consult current terms, not a hardcoded entitlement decision ([licence](https://docs.docker.com/subscription/desktop-license/)).
- Podman supports rootless operation; its macOS/Windows Linux containers run in a VM, with WSL or Hyper-V providers on Windows ([rootless](https://docs.podman.io/en/latest/markdown/podman.1.html), [machine](https://docs.podman.io/en/latest/markdown/podman-machine.1.html)).
- Podman is `Apache-2.0` ([licence](https://github.com/containers/podman/blob/main/LICENSE)); Docker Engine components have separate open-source licences and Desktop terms.
- A rootless container still shares the Linux kernel; a mounted workspace or socket grants corresponding access. It is not automatically a strong hostile-code boundary.
- Integration/verdict: runtime capability matrix, socket location, Compose and UID tests; **Adapt** both.

### Linux process sandboxing
- bubblewrap constructs a sandbox using Linux namespaces and controlled filesystem mounts; exact policy is supplied by the caller ([bubblewrap](https://github.com/containers/bubblewrap)).
- User namespaces map credentials and enable unprivileged namespace operations where allowed; local distribution policy can prohibit creation ([user_namespaces(7)](https://man7.org/linux/man-pages/man7/user_namespaces.7.html)).
- Landlock restricts a process and descendants through kernel-enforced access rules, with capabilities dependent on ABI/kernel; it is not by itself a full process, IPC or network isolation system ([kernel documentation](https://docs.kernel.org/userspace-api/landlock.html)).
- seccomp filters system calls and can reduce attack surface; it is not a path-aware filesystem policy or a complete sandbox ([kernel documentation](https://docs.kernel.org/userspace-api/seccomp_filter.html)).
- Codex's current upstream Linux helper uses bubblewrap plus seccomp, not Landlock as the default; legacy Landlock cannot isolate app-server Unix sockets, and WSL1 is rejected for the bubblewrap path ([Codex helper](https://github.com/openai/codex/blob/main/codex-rs/linux-sandbox/README.md)).
- Integration/verdict: **Adopt** combined controls and runtime probing; never silently fall back to unsandboxed execution.

### macOS process sandboxing
- Apple documents App Sandbox as an entitlement-based access-control technology for apps, with explicit access to resources ([App Sandbox](https://developer.apple.com/documentation/security/app_sandbox)).
- `sandbox-exec` / Seatbelt is used by actual agent runtimes, but Apple has deprecated the `sandbox-exec` command; stability of private profiles must not be assumed ([Apple command manual](https://www.manpagez.com/man/1/sandbox-exec/), [Anthropic runtime](https://github.com/anthropics/sandbox-runtime)).
- App Sandbox and a dynamically launched arbitrary process policy are different integration problems; neither automatically restricts an allowed network or file capability more narrowly than its configured policy.
- Integration/verdict: **Adapt** Seatbelt as a tested backend while researching a supported long-term strategy; show policy and failure state.

### Windows process sandboxing
- AppContainer provides an isolation boundary based on an app container security context and capabilities; resources must be made accessible through appropriate permissions ([Microsoft](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)).
- Job Objects group processes and apply limits/lifecycle controls; alone they are not a filesystem/network security boundary ([Microsoft](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)).
- Windows Sandbox is a disposable isolated desktop environment with virtualization requirements, not a transparent in-process tool wrapper ([Microsoft](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/)).
- WSL2 runs a Linux kernel in a managed VM; integration and mounted Windows files have separate access implications, so WSL alone is not a complete agent policy ([WSL architecture](https://learn.microsoft.com/en-us/windows/wsl/compare-versions)).
- Integration/verdict: **Adapt** AppContainer/restricted-token approaches only after compatibility tests; **Reject** equating Job Objects or WSL with full isolation.

### Agent sandbox implementations
- Anthropic's `srt` is a beta research preview, offered as CLI and TypeScript library; it uses Seatbelt on macOS, bubblewrap and a network namespace on Linux, and a dedicated account, restricted token and Windows Filtering Platform on Windows ([upstream README](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)).
- `srt` documents unrestricted reads by default, restricted writes and network proxy policy; Windows has important cross-session read-grant, token and network-filter limitations ([README](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)).
- In particular its Windows documentation states Schannel clients, including `dotnet restore`, `winget` and Windows cargo, cannot complete HTTPS inside that restricted token ([README](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)).
- Gemini CLI documents optional Seatbelt, Docker/Podman, native Windows, gVisor and experimental LXC; its default macOS Seatbelt profile permits broad reads and network access ([Gemini sandbox](https://geminicli.com/docs/cli/sandbox.md)).
- Codex uses bubblewrap/seccomp on current Linux; this supersedes a blanket claim that Landlock is its present default ([Codex Linux helper](https://github.com/openai/codex/blob/main/codex-rs/linux-sandbox/README.md)).
- Integration/verdict: **Adapt** techniques, not a claim of interchangeable guarantees; verify a pinned version and adversarial tests.

### System package managers
- apt: Debian-family CLI; `apt-get -y install PACKAGE`, inspect with `dpkg-query`; system changes normally need root ([apt-get](https://manpages.debian.org/unstable/apt/apt-get.8.en.html), [dpkg-query](https://manpages.debian.org/unstable/dpkg/dpkg-query.1.en.html)).
- dnf: RPM-family CLI; `dnf -y install PACKAGE`, query installed RPMs with `rpm -q` ([dnf](https://dnf.readthedocs.io/en/latest/command_ref.html), [rpm](https://rpm.org/docs/)).
- pacman: Arch-family CLI; `pacman --noconfirm -S PACKAGE`, detect with `pacman -Q PACKAGE` ([pacman](https://man.archlinux.org/man/pacman.8.en)).
- zypper: SUSE CLI; `zypper --non-interactive install PACKAGE`, inspect with `zypper search --installed-only` ([zypper](https://en.opensuse.org/SDB:Zypper_manual)).
- Homebrew: macOS/Linux; `brew install FORMULA`, detect via `brew list --versions FORMULA`; it is generally installed for user operation rather than run under sudo ([brew](https://docs.brew.sh/Manpage)).
- winget: Windows; `winget install --id ID --exact --silent --accept-package-agreements --accept-source-agreements`, inspect `winget list --id ID --exact`; individual installers can still require elevation or reject silent operation ([install](https://learn.microsoft.com/en-us/windows/package-manager/winget/install), [list](https://learn.microsoft.com/en-us/windows/package-manager/winget/list)).
- Chocolatey: Windows; `choco install PACKAGE -y`, inspect `choco list --local-only` subject to installed CLI version; package scripts and installation scope determine elevation ([install](https://docs.chocolatey.org/en-us/choco/commands/install/), [list](https://docs.chocolatey.org/en-us/choco/commands/list/)).
- Scoop: Windows user-oriented buckets/manifests; `scoop install PACKAGE`, `scoop list PACKAGE`; global installations and individual installers have different privilege needs ([Scoop](https://github.com/ScoopInstaller/Scoop)).
- Privilege handling: `pkexec` uses polkit authorization; `sudo` runs commands under another identity; Windows UAC obtains consent for elevated execution ([pkexec](https://www.freedesktop.org/software/polkit/docs/latest/pkexec.1.html), [sudo](https://www.sudo.ws/docs/man/sudo.man/), [UAC](https://learn.microsoft.com/en-us/windows/security/application-security/application-control/user-account-control/)).
- Integration/verdict: **Adapt** CLIs, argv not shell strings, explicit package IDs, preflight and post-install detection; never assume `-y` handles EULAs, configuration prompts or privilege.

### Android SDK and NDK
- Google `sdkmanager` lists/installs versioned SDK packages; package paths such as `"ndk;VERSION"` are distinct from SDK platforms/build-tools ([sdkmanager](https://developer.android.com/tools/sdkmanager), [NDK](https://developer.android.com/studio/projects/install-ndk)).
- `sdkmanager --licenses` presents required licences for review; acceptance is a user/legal decision, not an invisible background default ([sdkmanager](https://developer.android.com/tools/sdkmanager)).
- Platform: command-line tools must match host OS and require a compatible Java runtime; Android SDK components have individual licences.
- Integration/verdict: **Adopt** a CLI provider; record SDK root, package revisions and licence acceptance separately.

### Flutter SDK
- Flutter distributes platform-specific SDK bundles and documents environment setup and `flutter doctor` checks ([install](https://docs.flutter.dev/install)).
- Platform: target support and prerequisites differ; iOS builds require Apple tooling on macOS ([iOS setup](https://docs.flutter.dev/platform-integration/ios/setup)).
- Licence: Flutter framework is `BSD-3-Clause`; bundled dependencies have separate terms ([licence](https://github.com/flutter/flutter/blob/master/LICENSE)).
- Integration/verdict: **Adapt** pinned SDK archive plus `flutter doctor`; do not promise the bundle installs Android SDK or Xcode.

### .NET SDK
- Microsoft supplies PowerShell and shell install scripts for Windows and Linux/macOS respectively; they support explicit version and install directory without administrator access ([install script](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-install-script)).
- Scripts target CI and do not persistently update PATH or Windows registry; vendor recommends standard installers for normal development environments ([same](https://learn.microsoft.com/en-us/dotnet/core/tools/dotnet-install-script)).
- Integration/verdict: **Adapt** script for isolated user-managed SDK mode; verify script authenticity and resolved artifacts independently.

### JDK distributions and Adoptium
- Adoptium documents API-based CI download selection by release, OS, architecture, image type and JVM ([CI scripts](https://adoptium.net/installation/ci-scripts/), [API](https://api.adoptium.net/)).
- Different JDK distributors and builds carry different licences and support policies; version alone does not identify a distribution.
- Integration/verdict: **Adopt** vendor/distribution-aware provider; record exact artifact URL, checksum and vendor, not just `java=21`.

### Node, Python and Go artifact manifests
- Node publishes version directories with `SHASUMS256.txt` and signed checksum material; verify release signature against trusted keys before trusting checksums ([Node release verification](https://github.com/nodejs/node#verifying-binaries)).
- Go publishes a machine-readable download endpoint with OS, architecture, filename and SHA-256 for artifacts ([Go downloads API](https://go.dev/dl/?mode=json)).
- Python's official downloads include per-file release metadata and verification information, but do not assume one uniform cross-platform binary JSON manifest equivalent to Go's ([Python downloads](https://www.python.org/downloads/), [source releases](https://www.python.org/ftp/python/)).
- Integration/verdict: **Adopt** official metadata where verified; Python platform packaging gaps require an explicit provider or specialist manager.

### VS Code tasks and problem matchers
- Tasks describe shell/process invocations and problem matchers map output using patterns, file locations and severity; they are a useful text fallback rather than a universal structured compiler protocol ([tasks](https://code.visualstudio.com/docs/debugtest/tasks), [schema](https://code.visualstudio.com/docs/reference/tasks-appendix)).
- Integration/verdict: **Adapt** compatible task concepts and matchers, preserving raw output and documenting unsupported fields.

### Zed and JetBrains tasks
- Zed documents task configuration with labels, commands and variables ([Zed tasks](https://zed.dev/docs/tasks)).
- JetBrains IDEs expose run/debug configurations and build tool integrations; avoid claiming a universal independent JSON diagnostic protocol for all products ([IntelliJ run configurations](https://www.jetbrains.com/help/idea/run-debug-configuration.html)).
- Integration/verdict: **Adapt** user-experience patterns; prefer actual build-tool diagnostics for machine consumption.

### Bazel Build Event Protocol
- BEP emits structured build events with identifiers and relationships, suitable for progress and artifact reporting; it is distinct from Remote Execution API ([BEP](https://bazel.build/remote/bep)).
- Compiler diagnostics can still be emitted as action output rather than a universal normalized diagnostic object.
- Integration/verdict: **Adopt** event streaming/file adapter for Bazel, plus tool-specific diagnostic parsing.

### Cargo JSON
- `cargo build --message-format=json` emits newline-delimited compiler messages, artifacts, build-script and build-finished records ([Cargo external tools](https://doc.rust-lang.org/cargo/reference/external-tools.html)).
- Cargo warns other tools or procedural macros can emit non-JSON output; `cargo metadata --format-version` has its own versioned schema ([same](https://doc.rust-lang.org/cargo/reference/external-tools.html)).
- Integration/verdict: **Adopt** streaming parser and retain raw unmatched output; `cargo_metadata` is a Rust parser option.

### CMake File API and presets
- CMake File API exposes structured replies including codemodel data; query/reply files are not a compiler diagnostic stream ([File API](https://cmake.org/cmake/help/latest/manual/cmake-file-api.7.html)).
- `CMakePresets.json` describes configure/build/test presets and supports project/user configuration separation ([presets](https://cmake.org/cmake/help/latest/manual/cmake-presets.7.html)).
- Integration/verdict: **Adopt** presets and codemodel; combine with compiler output for diagnostics.

### Gradle Tooling API and build scans
- Gradle Tooling API embeds Gradle operations and exposes models/progress to clients; it is a Java-facing integration, not a generic Rust library ([Tooling API](https://docs.gradle.org/current/userguide/tooling_api.html)).
- Build scans record build data and may involve publishing data to a service; confirm consent and deployment policy ([build scans](https://docs.gradle.org/current/userguide/build_scans.html)).
- Integration/verdict: **Adapt** a versioned JVM bridge or CLI; do not equate scan publication with local diagnostics.

### SwiftPM and xcodebuild
- SwiftPM provides package description and build commands; exact structured diagnostic options vary by toolchain ([SwiftPM](https://docs.swift.org/swiftpm/documentation/packagemanagerdocs/)).
- `xcodebuild` can write result bundles with `-resultBundlePath`; `xcresulttool` is the Apple tool for inspecting them ([Apple xcodebuild reference](https://developer.apple.com/library/archive/technotes/tn2339/_index.html), [Xcode release notes](https://developer.apple.com/documentation/xcode-release-notes)).
- Platform: Xcode tooling requires macOS and an installed Xcode; result-bundle schema and xcresulttool JSON versions must be probed.
- Integration/verdict: **Adapt** result-bundle importer; preserve raw output and test Xcode versions.

### Clang and GCC diagnostic formats
- GCC documents `-fdiagnostics-format=json` and SARIF variants, with version-dependent support ([GCC diagnostic options](https://gcc.gnu.org/onlinedocs/gcc/Diagnostic-Message-Formatting-Options.html)).
- Clang supports structured diagnostic output options including SARIF in documented versions; capability-check the installed compiler rather than assuming GCC and Clang accept identical flags ([Clang user manual](https://clang.llvm.org/docs/UsersManual.html)).
- Integration/verdict: **Adopt** per-compiler probing and structured parsers; retain caret/text fallback.

### Remote execution and caches
- SSH workers: execute commands on a user-selected remote machine; transport authentication and host-key checks do not themselves make builds hermetic ([OpenSSH](https://www.openssh.com/manual.html)).
- Bazel Remote Execution API separates content-addressed inputs, action execution and cache protocols; a compatible server and matching toolchains are required ([REAPI](https://github.com/bazelbuild/remote-apis), [Bazel remote execution](https://bazel.build/remote/rbe)).
- distcc distributes supported compilation jobs; its security/network configuration and preprocessing mode matter ([distcc](https://github.com/distcc/distcc)).
- sccache is a compiler cache with optional remote storage and distributed compilation facilities; caching is not the same as general remote action execution ([sccache](https://github.com/mozilla/sccache)).
- Integration/verdict: **Adapt** distinct providers; model filesystem sync, toolchain identity, trust and output-path translation explicitly.

## Architectural implications for Axiom
- Split **environment resolution** from **process isolation**: a reproducible Nix shell, container, or mise environment is not automatically a secure sandbox.
- Provider contract: `discover`, `capabilities`, `list_versions`, `resolve`, `plan`, `install`, `detect`, `environment`, `uninstall`; each may report unsupported, interactive or privileged.
- Treat this as a proposed Axiom contract, not a claim that existing CLIs implement these methods.
- Store requested range separately from resolved exact version, platform triple, source revision and artifact digest.
- Model actual execution host separately from GUI host: native Windows, WSL Linux, container Linux, SSH Linux, and macOS are different targets.
- Proposed provider manifest, inspired by [mise registry](https://mise.jdx.dev/registry.html), [aqua registry](https://aquaproj.github.io/docs/reference/registry-config/) and [proto](https://moonrepo.dev/docs/proto/config):
```yaml
schema: axiom.tool-provider/v1 # proposed, not an existing standard
id: example-tool
provider: official-http
license: UNKNOWN # metadata licence; do not infer artifact licence
versions:
  discovery: { kind: json, url: 'https://example.invalid/releases.json' }
  selection: semver
artifacts:
  - os: linux
    arch: x86_64
    libc: glibc
    url: 'https://example.invalid/tool-{version}.tar.gz'
    archive: tar.gz
    checksum: { algorithm: sha256, value_from: signed-manifest }
    signature: { kind: detached, trust_key: publisher-key-id }
install:
  - { op: download, to: staging }
  - { op: verify, required: true }
  - { op: extract, strip_components: 1 }
  - { op: atomic_move, to: install_root }
env:
  prepend_path: ['${install_root}/bin']
  set: { EXAMPLE_HOME: '${install_root}' }
detect:
  executable: '${install_root}/bin/example'
  args: ['--version']
```
- The example URL is intentionally nonfunctional; `value_from`, `trust_key` and template grammar require actual schema definitions and validation.
- Add platform selectors for OS/version, CPU, libc, ABI, target versus host, archive format, prerequisites and unsupported combinations.
- Add provenance: source URL, publisher, metadata revision, checksum source, signature key fingerprint, verification result and fetch timestamp.
- Separate user approval for executable install hooks, licence acceptance, network hosts, system elevation and permanent environment changes.
- Resolve to a lock record with selected artifact and digests; never treat a version range or mutable image tag as a lock.
- Extraction must reject traversal, unsafe links and unexpected archive entries; install via staging and atomic publication when filesystem semantics permit.
- Treat registry templates, plugin code, archive contents, package scripts and container lifecycle commands as untrusted.
- Environment exports should be structured maps and path operations, not blindly evaluated shell output; offer explicit shell-compatibility mode where needed.
- Capture build events in a normalized model: run ID, origin, severity, message, code, spans, related locations, artifacts, progress and raw event.
- Include path-mapping from remote/container/WSL filesystem to local workspace, including case, symlink and URI normalization.
- Preserve original diagnostic JSON and source provenance so future parsers can reprocess it.
- Build provider adapters and UI orchestration; reuse existing managers, registries, protocols, parsers and vendor installers rather than rebuilding every ecosystem.

## Risks
- A registry checksum without a trusted signature or independently authenticated source detects corruption but does not establish publisher identity.
- Plugin or project configuration can run code during install or startup; `initializeCommand` explicitly runs on the host ([Dev Containers reference](https://containers.dev/implementors/json_reference/)).
- Agent sandbox policies with broad reads or network access can leak secrets; writable workspace files can also be used to influence future trusted runs.
- Unix sockets, Docker sockets, privileged containers and broad host mounts can undermine otherwise meaningful isolation.
- Linux user namespace policy, kernel ABI, nested containers and macOS/Windows OS updates can prevent a sandbox from starting.
- Anthropic documents substantial Windows sandbox limitations, including retained access grants and Schannel incompatibility ([srt README](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)).
- Reproducibility depends on pinned dependencies, platform, SDKs and external services; a lockfile is necessary but not sufficient.
- System package removal or rollback may affect unrelated applications; elevation must be limited to an auditable operation.
- Remote logs, build scans and caches can expose source, paths, credentials or proprietary artifacts.
- Licence obligations differ for registry data, manager, runtime, package, image, SDK and distribution; do not propagate one licence to the whole stack.

## Unverified / needs experiment
- Fetch failure: Codex security-page request redirected cross-origin and was not followed; exact current macOS/Windows Codex sandbox behavior is not independently confirmed here.
- Apple command manual linked above is a third-party mirror of the Apple manual, not an Apple-hosted primary source; confirm current Apple-hosted deprecation evidence and replacement before architectural commitment.
- Confirm exact Swiftly SPDX expression including Swift exception, native Windows availability, and current integration behavior.
- Confirm exact licence and redistribution scope for devenv, Devbox and Flox, including product/service components.
- Validate all package-manager noninteractive flags and detection output against actual installed versions; Chocolatey list flags in particular have changed across releases.
- Validate current Docker Desktop thresholds and user eligibility against the linked live licence terms before procurement.
- Probe proto/mise/vfox plugin API stability and platform coverage; no stable embeddable Rust library contract was established.
- Determine aqua record coverage for platform-specific checksums, signature validation, templating and version overrides for each targeted tool.
- Confirm Android command-line tools supported Java versions, sdkmanager prompts and individual licence terms for each intended SDK revision.
- Confirm Python's preferred current API/attestation and package source for each supported host; no universal official binary manifest was established.
- Confirm Flutter archive checksum verification and Adoptium API checksum/signature fields with live response samples before implementation.
- Test native Windows sandbox compatibility with compilers, debugger, GUI tooling, proxies, symlink/junctions and NTFS ACL cleanup.
- Test macOS Seatbelt behavior and survivability across supported macOS releases; third-party tool use is not an Apple support commitment.
- Test Podman against the reference Dev Container CLI, Compose, Features, rootless UID mapping, host forwarding and all target operating systems.
- Test xcodebuild result-bundle command syntax and xcresulttool schema across supported Xcode versions.
- Test actual Clang SARIF/JSON option names and stability per supported compiler version; JSON/SARIF are not synonymous.
- Verify `LGPL-2.1-only` against the exact Nix release and bundled components before redistribution.
- No tools were installed and no sandbox escape or end-to-end build experiment was performed for this report.

## Sources
- [mise documentation and backend catalogue](https://mise.jdx.dev/dev-tools/backends/)
- [aqua registry](https://github.com/aquaproj/aqua-registry)
- [Nix installation and WSL2](https://nix.dev/install-nix.html)
- [Dev Container metadata reference](https://containers.dev/implementors/json_reference/)
- [Podman machine documentation](https://docs.podman.io/en/latest/markdown/podman-machine.1.html)
- [Codex Linux sandbox implementation notes](https://github.com/openai/codex/blob/main/codex-rs/linux-sandbox/README.md)
- [Anthropic sandbox runtime upstream README](https://raw.githubusercontent.com/anthropics/sandbox-runtime/main/README.md)
- [Gemini CLI sandboxing](https://geminicli.com/docs/cli/sandbox.md)
- [Cargo external-tool integration](https://doc.rust-lang.org/cargo/reference/external-tools.html)
- [CMake File API](https://cmake.org/cmake/help/latest/manual/cmake-file-api.7.html)
- [Bazel Build Event Protocol](https://bazel.build/remote/bep)
- [Remote Execution API](https://github.com/bazelbuild/remote-apis)
