# Candidate libraries

Status: research, 2026-11-24. Versions, dates and licences are from the crates.io and pub.dev APIs queried today; repository
activity from the GitHub API. "Use" means candidate pending prototype, not a decision. **[T]** tested, **[S]** source read,
**[D]** docs, **[I]** inference. Nothing below was built. Toolchain: Rust 1.100, Flutter 3.50.0 / Dart 3.14.0.

## 1. Licence policy (Axiom is MIT OR Apache-2.0)

| Class | Licences | Rule |
|-------|----------|------|
| Allow | MIT, Apache-2.0 (± LLVM-exception), BSD-2/3, ISC, 0BSD, Zlib, Unlicense, Unicode-3.0, BSL-1.0, CC0-1.0*, MPL-2.0† | `cargo-deny` allow-list; NOTICE generated |
| Case by case | CDLA-Permissive-2.0 (webpki-roots data), OpenSSL-style notices (aws-lc), OFL fonts, libgit2 GPL-2.0 **with linking exception** | Record in NOTICE / ADR |
| Reject | GPL, AGPL, LGPL when statically linked, SSPL, BUSL, source-available, no licence | Build fails |

\* CC0 grants no patent licence; `notify` is CC0-1.0. Acceptable **[I]**, but record it.
† MPL-2.0 is file-level copyleft: compatible to distribute unmodified; modifications to *those files* must be published. Needs an
ADR because it is not MIT/Apache. Affects `nucleo-matcher` and Dart `dbus`.

**Rejected on licence:** `birdcage` (GPL-3, archived), `yash-syntax` (GPL-3), `hakoniwa` (LGPL-3), `libseccomp` (LGPL C library),
VTE/GTK terminal widgets and `libvte` (LGPL), Qt-based anything, CodeMirror-GPL extensions **[I]**. LGPL programs that are only
*spawned* (bubblewrap, git GPL-2) are fine; bundling `bwrap` means shipping its source offer.

## 2. Rust

### 2.1 Core, net, storage

| Crate | Version (date) | Licence | Maintenance | Risk / verdict |
|-------|----------------|---------|-------------|----------------|
| `tokio` | 1.54.1 (2026-10-21) | MIT | Excellent | **Use** |
| `reqwest` | 0.13.5 (2026-09-08) | MIT/Apache | Excellent | **Use** for providers; disable redirects with auth (SI-4); rustls backend |
| `hyper` | 1.13.0 (2026-11-23) | MIT | Excellent | **Use** directly for the egress proxy |
| `rustls` | 0.23.46 (2026-10-14) | Apache/ISC/MIT | Excellent; 0.24 in dev | **Use**; Oct 2026 advisory: session Debug leaks secrets, keep ≥ patched |
| `rcgen` | 0.14.10 | MIT/Apache | Good | Use only if TLS interception ships |
| SSE | `eventsource-stream` 0.2.3 (2022), `reqwest-eventsource` 0.6 (2024) | MIT/Apache | **Stale** | **Write our own** SSE decoder (~150 lines, fuzzed, byte-capped) |
| `serde_json` | 1.0.154 | MIT/Apache | Excellent | **Use**; `jiter` 0.19 (MIT) for partial JSON of streaming tool args |
| `schemars` | 1.2.3 | MIT | Good | **Use** for wire schema → Dart codegen |
| `rusqlite` | 0.43.0 (2026-11-15) | MIT (SQLite PD) | Excellent | **Use**, `bundled`; one writer thread, WAL; `rusqlite_migration` 2.9 (Apache) |
| `sqlx` | 0.9.1 (2026-11-18) | MIT/Apache | Good | Async pool over SQLite adds indirection; compile-time DB; Codex carries a patched fork [S]. Second choice |
| `turso` | 0.8.4 | MIT | Fast, pre-1.0 | No |
| `tantivy` | 0.27.0 (2026-10-17) | MIT | Good | Large. SQLite **FTS5 first**; tantivy only if measured insufficient |
| `keyring` | 4.2.0 (2026-08-29) | MIT/Apache | Reorganised into `keyring-core` 1.1 + stores | **Use** 4.x; check headless Linux behaviour (no Secret Service → memory-only, SI-3); Codex still on 3.6 [S] |
| `secrecy` / `zeroize` | 0.10.3 (2024) / 1.9.2 | Apache/MIT | Stable / good | **Use** |
| `interprocess` | 2.4.9 (2026-11-18) | 0BSD/Apache | Active, 1 maintainer | Only if a daemon mode is chosen; tokio has UDS and named pipes already → **prefer tokio** + explicit DACL |
| `notify` + `notify-debouncer-full` | 9.0.0 / 0.8.0 (2026-11-21) | **CC0-1.0** / MIT-Apache | Active, 9.0 three days old | Start on 8.x API-compatible pin or wait for 9.0.1; always reconcile by rescan |
| `similar` | 3.2.1 (2026-10-24) | Apache-2.0 | Good | **Use** for display diffs |
| `imara-diff` | 0.2.0 (2025-06) | Apache-2.0 | Slow | Faster histogram diff; used by gix; consider for huge files |
| `blake3` | 1.8.7 | CC0/Apache/Apache-LLVM | Excellent | **Use** (content hashes) |
| `sysinfo` | 0.40.4 | MIT | Good | Use for process tree stats |
| `process-wrap` | 11.0.0 (2026-11-24) | Apache/MIT | Good; frequent majors | **Use**, or ~300 own lines; `command-group` is superseded |
| `cap-std` | 4.0.3 (2026-08-20) | Apache-LLVM/Apache/MIT | Bytecode Alliance | **Use** for handle-relative FS (SI-9) with own Windows reparse tests |
| `rustix` | 1.1.6 | Apache-LLVM/Apache/MIT | Excellent | **Use** (`openat2`, prctl) |
| `landlock` / `seccompiler` | 0.4.8 / 0.5.0 | MIT-Apache / Apache-BSD | Good / stable | **Use** |
| `mxc-sdk` | 1.1.0 (2026-11-14) | MIT | Microsoft, 7 weeks past 1.0 | **Evaluate**; non-optional telemetry in routed release builds (sandboxing.md) |

### 2.2 Code intelligence, shell, terminal

| Crate | Version (date) | Licence | Maintenance | Risk / verdict |
|-------|----------------|---------|-------------|----------------|
| `portable-pty` | 0.9.0 (2025-02-11) | MIT | wezterm repo active, crate unreleased 21 months | **Use** with thin wrapper (Codex uses 0.9 [S]); plan own ConPTY for job objects and handle lists. Alternative Unix: `pty-process` 0.5.4 (MIT, 2026-10) |
| `ignore`, `globset`, `grep-searcher`, `grep-regex` | 0.4.35 / 0.4.22 / 0.1.18 / 0.1.15 (2026-11-22) | Unlicense/MIT | Excellent | **Use** in-process; avoids spawning `rg` (and its `--pre`, `.ripgreprc`) |
| `nucleo-matcher` | 0.3.1 (2024-02) | **MPL-2.0** | Stale crate | Prefer `frizbee` 0.13 (MIT) |
| `tree-sitter`, `tree-sitter-bash` | 0.27.2 (2026-11-07) / 0.25.1 (2025-12) | MIT | Excellent / slow | **Use**; pin grammar ABI; C code → parse untrusted text in a worker process with timeout |
| `tree-sitter-highlight` | 0.27.2 | MIT | Good | Candidate for engine-side highlighting |
| `syntect` | 5.3.0 (2025-09) | MIT | Active repo | Fallback: many grammars, slower, no incremental tree |
| `async-lsp` | 0.2.4 (2026-04) | MIT/Apache | 1 maintainer, 4 open issues | **Use** for client side |
| `lsp-types` | 0.97.0 (2024-06) | MIT | **Dormant** | Prefer `ls-types` 0.0.6 (MIT, 2026-03) or generate from `metaModel.json` |
| `gix` | 0.91.0 (2026-11-22) | MIT/Apache | Very active, 0.x monthly majors, MSRV 1.95 | **Read-only queries** without executing programs. ≥ 4 trust-model advisories Oct–Nov 2026, last High (GHSA-4c7q) fixed in 0.91 released days ago → pin, update fast |
| `git2` | 0.21.0 (2026-05) | MIT/Apache (libgit2 GPL-2 + linking exception) | Good | C attack surface, feature lag (worktrees, sparse). Second choice |
| git CLI | user's | GPL-2 (spawned) | — | **Use for mutations** (worktree, merge, push: hooks, credentials, LFS, signing) via the hardened sandboxed wrapper (SI-22) |
| `alacritty_terminal` | 0.26.0 (2026-04) | Apache-2.0 | Good | Engine-side emulator option |
| `libghostty-vt` (Rust binding) | 0.2.2 (2026-09-28) | MIT/Apache | Active (repo pushed 2026-11-23) | Predates the C library's 1.0 (2026-11-06); needs Zig toolchain or prebuilt; watch |
| `vt100` | 0.18.1 (2026-10) | MIT | OK | Tests / headless snapshots |
| `brush-parser` | 0.4.0 (2026-05) | MIT | Active, pre-1.0 | Second bash parser for differential fuzzing only |
| `shlex` | 2.0.1 | MIT/Apache | Good | Splitting only; never for security decisions |
| `pulldown-cmark` | 0.13.6 (2026-10) | MIT | Excellent | Engine-side Markdown if parsing moves to Rust; `comrak` 1.2 (BSD-2) for full GFM |

### 2.3 Agent protocols, tokens, policy

| Crate | Version (date) | Licence | Maintenance | Risk / verdict |
|-------|----------------|---------|-------------|----------------|
| `rmcp` | 3.11.0 (2026-11-17) | Apache-2.0 | Official; majors Mar/Jun/Jul 2026, weekly minors | **Use** behind an adapter; ≥ 3.9 required (GHSA-c9f9); 8 advisories since April; own the HTTP client (caps, redirect, SSRF guard) |
| `agent-client-protocol` | 5.5.1 (2026-11-21) | Apache-2.0 | 1.0 in June, 5.0 on 2 Nov | Very fast majors: **wrap**; consider schema crate only (`…-schema` 1.22.0) + own JSON-RPC loop |
| `jsonrpsee` | 0.26.2 | MIT | Good | Not needed: newline-delimited JSON-RPC over stdio is ~300 lines |
| `tiktoken-rs` | 0.12.1 (2026-09) | MIT | Good | Estimates for OpenAI models only; large tables |
| `tiktoken` (goliajp) | 5.3.0 (2026-11-24) | MIT/Apache | New, small user base | Supply-chain caution; not now |
| `tokenizers` | 0.23.2 (1.0 rc) | Apache-2.0 | HF | Only for local models. **Default: provider usage fields + count endpoints, chars/4 pre-flight** |
| `cedar-policy` | 4.15.0 | Apache-2.0 | AWS | See sandboxing.md §7; not now |
| `starlark` | 0.14.3 (2026-11-21) | Apache-2.0 | Meta | Not for config |
| `monty` / `rquickjs` / `v8` / `wasmtime` | 1.6.0 / 0.14.0 / 152.2.0 / 51.0.0 | MIT / MIT / MIT / Apache-LLVM | Active | sandboxing.md §9 |
| `minisign-verify` | 0.3.0 (2026-09) | MIT | Good | Update and catalogue signatures |
| `cargo-packager-updater` | 0.2.3 (2025-07) | Apache/MIT | Slow | Option for Linux/Windows; else Sparkle/WinSparkle |

MSRV note: `monty` 1.98, `wasmtime` 1.97, `grep-*` 1.96, `gix` 1.95. Axiom's effective MSRV is "latest stable"; state it.

## 3. Flutter / Dart

### 3.1 Platform status at 3.50

| Topic | Finding |
|-------|---------|
| **Multi-window** | **Not usable on stable.** `flutter config --list` shows `enable-windowing: (Unavailable)`; the feature is master-only and `_window.dart` says "Do not import this file in production applications" [S, T]. APIs renamed again in 3.50 (`RegularWindow*` → `Window*`, `sizedToContent` → `shrinkWrap`); popups, tooltips, dialogs, satellites land per platform [D]. Plan **single window** + in-app panes; a second window = second engine (`desktop_multi_window` 0.3.1, Apache-2.0: separate isolates/engines, memory cost) only if a prototype proves need |
| **Impeller** | Default on macOS, Windows, Linux since 3.47; opt-out to Skia still possible in 3.50 but documented to be removed [D]. macOS: Metal. Windows: embedder uses **OpenGL ES 3 through ANGLE → D3D11** [S: `windows/egl/manager.cc`, PR 191516], Linux OpenGL [S], although the 3.47 blog says Vulkan. Text: SDF rendering on desktop [D]. 3.50 has many Linux compositor/fence fixes and Windows MSAA fixes → expect driver-specific bugs; need golden + soak on real GPUs |
| **Fragment shaders** | `FragmentProgram` everywhere; `ImageFilter.shader` requires Impeller, so now possible on all three desktops [S]. Guard with `ImageFilter.isShaderFilterSupported`. 3.50 adds `filterQuality` |
| **Accessibility** | macOS: NSAccessibility bridge, most mature **[I]**. Windows: `AccessibilityBridgeWindows` (MSAA/IAccessibleEx; UIA behind a build macro per old doc) [S, search excerpt]. Linux: ATK bridge (GTK3); Ubuntu docs say Flutter "has compatibility issues with AT-SPI" [search excerpt]. 3.50 adds `reduceMotion` in MediaQuery, Windows enabled-state fix, Linux heading text. **Test VoiceOver, NVDA, Orca per milestone**; treat Orca as at risk |
| **Material split** | `material_ui` 1.11.1 / `cupertino_ui` 1.1.7 are now packages; some plugins (`window_manager` ≥ 0.7, `flterm`, `gpt_markdown`) depend on `material_ui` → a widgets-only app will still compile it in |
| **Native menus** | `PlatformMenuBar` works on **macOS only** [S]. Windows/Linux: draw menus in Flutter. Context menus: in-app overlay; `super_context_menu` adds native code (Rust) for little gain |
| **Golden tests** | Built-in `matchesGoldenFile` is enough; `alchemist` 0.15.0 (MIT, 2026-11-17) adds CI-stable text blocks → **use**. `golden_toolkit` discontinued. Impeller vs software raster differ: run goldens on one pinned OS + a tolerance comparator; note `flutter test` does not use the GPU backend **[I]** |
| **Widget previews** | Stable since 3.47 [D]; useful for the design system |
| **Build hooks** | Native assets/code assets are on (`hooks`, `code_assets`), so FFI to a Rust/C library no longer needs plugin glue |

### 3.2 UI ↔ engine transport

| Option | Version | Pros | Cons | Verdict |
|--------|---------|------|------|---------|
| Child process, stdio, newline JSON (or length-prefixed) | `dart:io` | Crash isolation, secrets outside UI process (SI-14), testable headless, no listener (SI-20), same engine for CLI | Serialisation cost; needs batching/back-pressure | **Default** |
| UDS / named pipe daemon | `dart:io` UDS; named pipes on Windows need FFI (`win32` 6.4.0 BSD-3) | Survives UI restart, many UIs | Auth, lifecycle, Windows work; Dart has AF_UNIX on Windows but ACL semantics need proof **[I]** | Later, if sessions must outlive UI |
| `flutter_rust_bridge` | 2.13.0 (2026-08-23), MIT, active | Typed, streams, fast | Same process: an engine crash or parser bug kills the UI, secrets share memory, sandbox parent = UI (TCC) | Only for **pure** helpers (highlighting, Markdown, VT) |
| `rinf` | 8.11.0 (2026-10), MIT | Message passing | Same-process drawbacks | No |
| `json_rpc_2` | 4.1.0 (2026-02), BSD-3 | Dart team | Thin; we need typed codegen anyway | Optional |

### 3.3 Markdown

| Package | Version (date) | Licence | Incremental? | Notes |
|---------|----------------|---------|--------------|-------|
| `markdown` | 7.3.1 (2026-03) | BSD-3 | **No**: `Document.parseLines` re-parses all | Dart team; correct CommonMark/GFM. **Use as the block/inline parser under our own incremental splitter** |
| `flutter_markdown_plus` | 1.0.12 (2026-07) | BSD-3 | No | Successor of discontinued `flutter_markdown`; 114 open issues; default image builder fetches remote images (T23) **[I]** |
| `gpt_markdown` | 1.4.1 (2026-11) | BSD-3 | Partial | Pulls `http`, `flutter_svg`, `material_ui`: violates SI-14 unless forked |
| `markdown_widget` | 2.3.2+8 (2025-04) | MIT | No | Slow maintenance |
| `streamdown`, `flutter_smooth_markdown`, `mixin_markdown_widget`, `flutter_streaming_text_markdown` | 0.1–1.11 (2026) | BSD/MIT | Claimed | < 11k downloads/month each; immature |
| Rust `pulldown-cmark` in engine | 0.13.6 | MIT | Block-resume by us | Parse once, send a typed block tree; keeps hostile parsing out of the UI process |

Verdict **[I]**: own streaming renderer. Split at stable block boundaries; only the tail block is re-parsed per delta; finished
blocks become immutable `RenderObject`s in a lazy sliver list. No remote fetch, no raw HTML. Math (`flutter_math_fork` 0.7.4,
Apache-2.0, 2025-05, slow) and Mermaid are later, isolated features (Mermaid needs a JS runtime: none acceptable in UI today).

### 3.4 Syntax highlighting and code

| Package | Version (date) | Licence | Verdict |
|---------|----------------|---------|---------|
| `re_highlight` | 0.0.3 (2024-02; repo 2025-03) | MIT | Regex (highlight.js port), dormant. Acceptable stop-gap |
| `syntax_highlight` | 0.5.0 (2025-08) | BSD-3 | TextMate-like, few languages, drags in `super_clipboard` 0.9 |
| `highlight`/`flutter_highlight` | 0.7.0 (2021) | MIT | Dead |
| `tree_sitter` (Dart FFI) 0.2.1 / `flutter_tree_sitter` 0.0.8 | 2025 | MIT | Immature bindings |
| **tree-sitter in Rust engine** (spans over IPC) | 0.27.2 | MIT | **Preferred**: incremental, off the UI isolate, crash-contained in a worker, shared with code search |
| `re_editor` | 0.10.0 (2026-07; repo 2026-10) | MIT | Best Flutter code editor; depends on `re_highlight`; use only if editing is in scope |
| `code_forge` | 11.8.0 (2026-11) | MIT | Very fast majors; LSP features; watch |
| `super_editor` | 0.2.7 stable (2024-06), 0.3.0-dev.52 (2026-06) | MIT | Rich-text; stuck on pre-release; not for a composer. **Own composer** on `EditableText` |

### 3.5 Terminal

| Package | Version (date) | Licence | State | Verdict |
|---------|----------------|---------|-------|---------|
| `xterm` | 4.0.0 (2024-02; repo last push 2025-06) | MIT | 111 open issues, dormant | Works, but no owner for security bugs (T42) |
| `kterm` | 1.6.0 (2026-10) | MIT | Fork, 8 stars | Watch |
| **libghostty-vt** (C) | 1.0 (2026-11-06) | MIT; all deps permissive with manifest | Stable C ABI, semver, CI ABI checks; tested Linux/macOS/Windows x64; Windows arm64 builds only; used by JetBrains IDEs [D] | **Preferred core**: state + parser only, we render and own the PTY |
| `libghostty` (Dart) / `flterm` (widget) | 0.0.13 / 0.0.6 (2026-10) | MIT | Pre-1.0 C API pin, 1 maintainer, `flterm` pulls `material_ui` | Reference; write a small `ffigen` binding to 1.0 and our own painter |
| `alacritty_terminal` in engine + cell diffs over IPC | 0.26.0 | Apache-2.0 | Mature | Strong alternative: hostile bytes never parsed in the UI process |
| `flutter_pty` | 0.4.2 (2025-01) | MIT | Slow | Not needed: PTY lives in the engine |

Choice to prototype: VT state in **engine** (alacritty_terminal or libghostty-vt, cell diffs) vs in **UI** (FFI). Engine side is
better for isolation, resume and agent reading of screens; UI side for latency. Measure before deciding.

### 3.6 Window chrome, clipboard, system

| Package | Version (date) | Licence | Verdict |
|---------|----------------|---------|---------|
| `window_manager` | 0.7.1 (2026-11-22) | MIT | Active again, but three releases this month, now rewritten on `nativeapi` 0.6 (FFI), requires Flutter ≥ 3.50, pulls `material_ui`. Use pinned, or write ~300 lines per runner |
| `bitsdojo_window` | 0.1.6 (2023-12) | MIT | **Dead** |
| `macos_window_utils` | 1.9.1 (2026-01) | MIT | Good for NSVisualEffect/titlebar; widgets-only dependency → **use** on macOS |
| `flutter_acrylic` | 1.1.4 (2024-06) | MIT | Stale |
| `super_clipboard` / `super_drag_and_drop` / `super_native_extensions` | 0.9.1 (2025-06; repo active 2026-11) | MIT | Rust built via cargokit at build time; 174 open issues; `device_info_plus <12`. Heavy. Prefer `Clipboard` + `pasteboard` 0.6.2 (Apache-2.0) for images, `desktop_drop` 0.9.2 (Apache-2.0) |
| `file_selector`, `url_launcher` | 1.1.0 / 6.3.3 | BSD-3 | **Use** (first-party); wrap `url_launcher` with the scheme allow-list (T23) |
| `flutter_local_notifications` | 23.0.4 (2026-11-23) | BSD-3 | Good; Linux implementation uses `dbus` (**MPL-2.0**) → ADR or send from the engine |
| `tray_manager`, `hotkey_manager` | 0.9.1 / 0.4.1 (2026-11-22) | MIT | Optional |
| `auto_updater` | 1.0.0 (2024-10; repo 2026-11) | MIT | Sparkle/WinSparkle wrapper; consider native integration instead |
| `flutter_secure_storage` | 11.2.0 | BSD-3 | **Do not use**: secrets belong to the engine (SI-3, SI-14) |
| `drift` / `sqlite3` | 2.35.4 / 3.9.2 | MIT | Not needed: engine owns storage |

### 3.7 State

| Option | Version | Licence | Notes |
|--------|---------|---------|-------|
| Plain `ValueNotifier`/`ChangeNotifier` + `ListenableBuilder` + `InheritedModel` | SDK | BSD | **Default**: engine is the source of truth, UI keeps projections; per-row notifiers keep streaming rebuilds tiny; no dependency |
| `signals` / `signals_flutter` | 7.1.0 (2026-05) | Apache-2.0 | Good fine-grained fit; ≈ 36k downloads/month, 1 main author; majors 6 → 7 within months |
| `riverpod` | 3.5.0 (2026-11) | MIT | Popular, active; DI + async caching we mostly do not need; 3.x minors monthly |
| `bloc` / `provider` | 9.2.1 / 6.1.5 | MIT | No benefit here |

## 4. Highest dependency risks

1. `rmcp` and `agent-client-protocol`: protocol churn plus advisory rate. Isolate behind adapters with recorded-transcript tests.
2. `gix`: frequent trust-model advisories; it parses hostile repositories. Run in a sandboxed worker.
3. `mxc-sdk`: young, telemetry, Windows build gating.
4. `portable-pty`, `xterm`, `lsp-types`, `bitsdojo_window`, `re_highlight`, SSE crates: dormant or long unreleased.
5. Flutter desktop renderer transition (Impeller) and multi-window unavailability: architecture must not need them to change.
6. Every Dart plugin with native code runs in the UI process with full user rights: target **≤ 6** plugins.

## 5. Unverified

No package here was compiled or run. Binary size and start-up figures were not measured. Accessibility statements for Windows and
Linux rest on source layout and search excerpts. Transitive licences were not resolved: run `cargo deny` and the Dart licence
check on the first real lockfiles. `flutter test` raster backend, AF_UNIX ACLs on Windows, and `keyring` 4 on headless Linux need tests.

Sources: https://crates.io/api/v1 · https://pub.dev/api · https://api.github.com ·
https://docs.flutter.dev/release/release-notes/release-notes-3.50.0 · https://flutter.dev/blog/whats-new-in-flutter-3-47 ·
https://docs.flutter.dev/perf/impeller · https://mitchellh.com/writing/libghostty-1-0 · local Flutter SDK source (3.50.0).
