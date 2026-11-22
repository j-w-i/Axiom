# ADR-0012: Desktop shell on Tauri 2 + React; Qt is retired after a gated migration

Status: Superseded by ADR-0020 (FL-QA-001, 2026-11-22): the shell is Flutter
(`apps/desktop-flutter`); the Tauri shell was removed and is kept at the tag
`tauri-shell-final`. The process split stands. Earlier: Accepted (2026-11-12) —
proposed by the evaluation in
`docs/migration/qt-to-tauri/decision.md`, approved by the project owner.

Supersedes ADR-0001 (desktop shell), supersedes ADR-0002 (the "no Rust in the
shell" clause; the process split is unchanged), amends ADR-0004 (editor host)
and ADR-0011 (licensing). ADR-0003 (IPC), ADR-0005 (contract source of truth),
ADR-0006…0010 are unaffected.

## Context

M1 shipped a Qt 6 C++/QML shell (`apps/desktop`) over the Rust daemon. The
shell is sound, but every future UI milestone (M2 Environment Manager and
Problems, M3 agent workspace, M4 terminal/diff, M5 Visual Inspector) would be
built twice if the shell stayed Qt while the evaluation ran.

A complete, isolated prototype of a Tauri 2 + React + Monaco shell was built
and measured against the Qt shell on the same hardware, same daemon binary and
same workloads (`apps/desktop-tauri`, evidence in
`docs/migration/qt-to-tauri/`). On Linux it reached parity on all nine M1 exit
criteria, and was faster to a usable explorer, smaller and lighter on memory.
macOS and Windows were **not** exercised.

ADR-0001 rejected a web shell partly because
- Qt WebEngine gives one known Chromium with CDP, which M5's visual
  inspection relied on;
- system webviews differ per OS, a poor base for an embedded editor and
  preview engine.

Those concerns are real and are **not** dismissed here: they are treated as
gates, not as solved problems (see Consequences and the M5-PRE-001 spike).

## Decision

1. **The desktop shell is Tauri 2 + React + TypeScript**, hosting Monaco in
   the application webview. The shell process is Rust (Tauri host) plus
   TypeScript/React (webview). Qt, C++ and QML are no longer used for new
   work.
2. **The architecture boundary is unchanged.** The shell and `axiomd` remain
   two processes (ADR-0002) speaking frozen IPC protocol 0.1 (ADR-0003). All
   filesystem, workspace, process and task operations stay in the daemon. The
   Tauri host exposes only the commands listed in
   `docs/migration/qt-to-tauri/architecture.md` §2.3 through Tauri
   capabilities; there is no command that reads or writes workspace files or
   starts processes.
3. **The daemon bridge is a Rust library**, `crates/desktop-bridge`
   (`axiom-desktop-bridge`), not JavaScript. Owning the child process,
   framing, correlation, timeouts, cancellation, restart and bounded
   shutdown in Rust keeps protocol handling out of the webview and makes it
   testable against the real `axiomd` without a GUI.
4. **The editor seam (ADR-0004) is preserved.** Monaco stays the editor. The
   editor-bridge-1 *message set* (`docs/protocols/editor-bridge.md`) remains
   the contract between "shell document state" and "editor surface"; in the
   Tauri shell it is an in-process TypeScript interface
   (`EditorPort`/`EditorListener`) implemented over the same
   `editor/src/documents.ts` used by the Qt shell. No message is added,
   removed or weakened. ADR-0004's staged plan (plain Monaco now, language
   servers via the daemon in M4, extension host later) stands.
5. **The web design system replaces the QML one.** Tokens are CSS custom
   properties with the same semantic vocabulary as the QML `Theme` singleton
   (`docs/architecture/design-system.md` is amended, not discarded). Radix
   primitives are used for overlays and menus; Tailwind and shadcn/ui are
   not adopted (rationale: `docs/migration/qt-to-tauri/design-system.md` §4).
6. **The migration is gated.** Milestone M1.5 (between M1 and M2,
   `docs/tasks/M1.5/`) builds the Tauri shell to full parity in parallel with
   the Qt shell. The Qt shell remains the release artifact until **T-14**,
   which may only run after the parity gate **T-12**. The Qt shell is deleted
   only by **T-15**, after T-14 has been the default for a milestone and the
   conditions in `docs/migration/qt-to-tauri/implementation-roadmap.md` §5
   hold. Until T-15, `apps/desktop` keeps building and its tests keep running.
7. **Cross-platform claims are evidence-based.** The shell runs on three web
   engines (WebKitGTK, WebView2, WKWebView). No behaviour may be described as
   verified on an OS where the Tauri job in CI has not run, and the
   platform-specific paths (renderer-crash detection, native menu, packaging)
   must each be tested on every OS they claim.

## Alternatives considered

- **Retain Qt** (do nothing). Rejected by the owner after the evaluation: the
  visual and product goals (Linear-grade UI, IDE-class productivity) and the
  cost of building every future UI twice outweigh the migration cost. This
  ADR does not claim Tauri is better in every dimension — Qt remains ahead on
  engine predictability, page privilege and streaming-output throughput
  (`decision.md` §2–3).
- **Electron.** Rejected: ships a full Chromium (the size and licensing
  problem Qt has, without the Qt inspection adapter), and needs a Node main
  process we would have to harden anyway.
- **Keep Qt for the shell, use a web tech stack in a Qt WebEngine view**
  (i.e. replace QML with React inside Qt). Rejected: keeps the whole Qt
  toolchain and deployment burden while giving up QML's benefits; the
  prototype shows the Tauri host is small (≈ 1.1 k lines of Rust).
- **Move the daemon bridge to JavaScript** (`tauri-plugin-shell`). Rejected:
  it would grant the webview process-spawn permission and put protocol
  framing on the UI thread.
- **Monaco in a separate webview/iframe from the start.** Deferred, not
  rejected: it would restore crash isolation and the narrower editor
  privilege of the Qt shell at the cost of an engine process and an IPC hop
  per message. Spike T-11 decides it with measurements.

## Consequences

- **Three web engines to test.** WebKitGTK (Linux), WebView2 (Windows),
  WKWebView (macOS). Monaco and every UI behaviour must be exercised on all
  three; CI (T-09) is the verifier. WebKitGTK's known Linux GPU problems
  (DMABUF/NVIDIA blank windows and flicker, documented by Tauri) are a
  distribution risk that must be measured on real hardware (T-10).
- **Page privilege widens.** In Qt only C++ could issue protocol requests;
  in the Tauri shell the whole UI page can issue any protocol 0.1 method
  (except `initialize`/`shutdown`, which the host owns). Mitigations: no
  remote content, strict CSP, bundled-only assets, capability-gated command
  set. Spike T-11 may narrow this again by isolating Monaco.
- **`freezePrototype` is disabled** because Monaco 0.57 assigns to built-in
  prototypes at load. Recorded as a deliberate weakening of Tauri's default
  hardening.
- **Streaming output is ~2× slower** for very large outputs (99 ms vs 45 ms
  to settle 50 000 lines on Linux). Acceptable; batching in the host is the
  fix if a future milestone needs it.
- **M5's preview engine is unresolved.** ADR-0001's Chromium/CDP advantage
  disappears on Linux and macOS with Tauri. M5 may not be planned on Tauri
  until spike M5-PRE-001 has a working answer (e.g. an external Chromium
  driven over CDP and rendered into the shell, or WebView2 CDP on Windows
  only). If no acceptable answer exists, M5's preview epic must be redesigned
  or the shell decision revisited — this is the main technical debt this ADR
  creates.
- **The QML inspection adapter (M6.1)** no longer benefits from the team
  living in Qt; it becomes a standalone probe library. It was always planned
  as a separate shared library (ADR-0008, ADR-0011 §4).
- **Toolchain**: Cargo + pnpm + the platform webview SDK. Linux needs the
  distro's WebKitGTK 4.1 development packages (or
  `apps/desktop-tauri/scripts/sysroot.sh` where they cannot be installed);
  Windows needs the MSVC toolchain and the WebView2 runtime; macOS needs the
  Xcode command line tools.
- **Licensing**: Tauri is MIT/Apache-2.0. On Linux the shell links the
  system WebKitGTK (LGPL-2.1) dynamically — the Qt relinking obligation for
  the shell disappears, and the AppImage (which may bundle WebKitGTK) needs
  the same replaceable-library treatment Qt had. On Windows the app depends
  on the Evergreen WebView2 runtime (Microsoft terms) and must not assume it
  is present on Windows 10. ADR-0011 is amended accordingly.
- **Verification debt is explicit**: M1.5 is not complete until
  `docs/roadmap/verification/M1.5.md` records macOS and Windows results.

## Evidence

`docs/migration/qt-to-tauri/`: `architecture.md` (audit + design),
`feature-parity.md` (row-by-row parity incl. gaps), `performance.md`
(interleaved benchmarks, black-box), `prototype-plan.md` (what was built,
tests, defects found), `decision.md` (recommendation). Raw benchmark JSON:
`apps/desktop-tauri/bench/results/`.
