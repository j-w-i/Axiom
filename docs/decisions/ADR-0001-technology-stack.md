# ADR-0001: Technology stack

Status: Accepted (2026-11-11)

## Context
The brief proposes Qt 6/QML, Rust and Monaco and asks for them to be
questioned. Research: `docs/research/qt-rust.md`, `prior-art.md`,
`editor.md`. Experiment E1 exercised the combination end to end.

## Decision
- **Desktop shell:** Qt 6, C++17 and QML (Qt Quick Controls). Minimum Qt 6.8
  LTS; developed and CI-tested against **Qt 6.12 LTS** (pinned in
  `scripts/bootstrap-qt.sh`). Build with CMake + Ninja.
- **Backend:** Rust (stable, edition 2021, MSRV 1.85), Tokio.
- **Editor surface:** TypeScript, Monaco Editor (exact version pinned),
  bundled with Vite, hosted in Qt WebEngine.
- **Package managers:** Cargo, pnpm. No others.

## Alternatives considered
- *Electron / Tauri + web UI.* Would make VS Code workbench reuse easier and
  remove the WebEngine-in-Qt seam. Rejected: the brief's native-quality,
  custom-UI goals and the Qt/QML inspection adapter (which benefits from the
  team living in Qt) favour Qt; Tauri's system-webview differences across
  OSes are a poor base for an embedded editor and a preview engine that needs
  one known Chromium with CDP.
- *Slint or GPUI (pure Rust UI).* Removes C++. Rejected: neither offers a
  Chromium web view, which the editor and the web preview both require.
- *Qt Widgets instead of QML.* Rejected: QML is the better fit for a custom
  design system and animated, responsive panels.
- *Pure C++ backend.* Rejected: Rust's safety and ecosystem (tokio, ignore,
  notify, rusqlite, gix) suit the security-sensitive service layer.

## Consequences
- Three toolchains (CMake/C++, Cargo, pnpm). Mitigated by the process split
  (ADR-0002): each builds independently; `scripts/check.sh` runs all.
- Qt WebEngine is required: Windows builds must use MSVC (no MinGW
  WebEngine), the Mac App Store is not a distribution channel, and the
  deployment is large (measure in M7). Chromium in Qt 6.11/6.12 is 140
  (observed in E1).
- Contributors need a Qt SDK; `scripts/bootstrap-qt.sh` installs one without
  root via aqtinstall (verified on Fedora 44).
