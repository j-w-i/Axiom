# Open questions and risks

Unresolved items, each with the point at which it must be answered. Add to
this list; do not let questions live only in handoffs.

## Must be answered by experiment

| # | Question | Why it matters | Answer by |
|---|----------|----------------|-----------|
| Q1 | Does the E1 flow (WebEngine + custom scheme + key events + Ctrl+S) behave the same on **macOS and Windows**? | M1 exit on those platforms | M1-CI-002 |
| Q2 | IME composition, clipboard, drag-and-drop and screen-reader behaviour of Monaco inside Qt WebEngine | Editor usability and accessibility | M1 review checklist; fixes in M4/M7 |
| Q3 | Can Qt WebEngine's Chromium be driven over **CDP** with the calls E3 used (`DOM.getNodeForLocation`, `CSS.getMatchedStylesForNode`, `Page.captureScreenshot`), via `QTWEBENGINE_REMOTE_DEBUGGING` or the in-process devtools channel? | The web inspector design (ADR-0008) | M5 epic 5.0 — fallback: an injected content script implementing the adapter protocol directly |
| Q4 | Source stamping beyond Vite + JSX: TSX edge cases, Next.js/Turbopack, webpack+SWC, server components, portals, component libraries | Breadth of Level 3 on the web | M5 epic 5.4 (per-bundler tasks) |
| Q5 | QML probe injection into an **unmodified** app on each OS; behaviour with Qt Quick Controls' internal items | QML adapter usability | M6 epic 6.1 |
| Q6 | Windows process-tree termination and ConPTY behaviour | Task cancel (M1), terminal (M4) | M1-EXEC-001 via CI; M4 epic 4.3 |
| Q7 | Which OS sandbox mechanisms pass a self-test on current OS releases (Seatbelt deprecation; Landlock ABI; AppContainer compatibility with toolchains) | ADR-0010 tiers | M6 epic 6.5 |
| Q8 | `monaco-vscode-api` inside Qt WebEngine: does a web extension activate; memory and startup cost | ADR-0004 stage 3 | M7 epic 7.1 |
| Q9 | Memory footprint and installed size of the app with WebEngine on each OS | Packaging, performance budgets | M7 epics 7.2/7.4 |

## Must be answered by decision

| # | Question | Owner / when |
|---|----------|--------------|
| D1 | Terminal UI: xterm.js in a second web view vs. a QML terminal widget | Architect, M4 epic 4.0 |
| D2 | Whether to add a Node extension host (ADR-0004 stage 4) | Architect, after Q8 |
| D3 | Project name, organisation and bundle identifiers (currently "Axiom", `org.axiom.Axiom`) — check trademark/name collisions before any public release | Project owner, before M7 |
| D4 | Final licence confirmation and LGPL compliance procedure, incl. macOS hardened runtime vs. replaceable Qt libraries | Legal review, M7 epic 7.6 |
| D5 | Telemetry/crash reporting policy (default: none) | Project owner, M7 |

## Legal items that research could not close

- Apple's Xcode/SDK licence terms as they apply to using an extracted SDK on
  non-Apple hardware (xtool path): the licence text was not retrievable
  during research (`docs/research/xtool.md` §Unverified). The xtool provider
  must present this to the user as their responsibility; do not ship M6.2
  without reading the current agreement.
- Exact current Microsoft Marketplace terms were not fetched; the decision
  to use Open VSX only (ADR-0011 §6) makes this moot unless that changes.

## Known risks accepted for now

- **Chromium lag in Qt WebEngine** (Chromium 140 in Qt 6.11/6.12): accepted
  for the editor surface (no remote content); to be actively managed for the
  M5 preview (isolated profile, track Qt patch releases).
- **Three toolchains** for contributors (Rust, Qt/C++, Node): mitigated by
  the process split and `scripts/`.
- **Private Qt API** in the QML probe: version-locked builds, public-API
  fallback.
- **No task isolation in M1**: equal to running commands in a terminal;
  stated in `security.md`.
