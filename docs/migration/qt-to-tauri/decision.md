# Qt → Tauri: evaluation gate and recommendation

_Status: **recommendation to the project owner; not an accepted decision.**
If accepted, it is recorded as ADR-0012 by roadmap task T-01. Until then
ADR-0001/0002/0004 stand and the Qt shell remains the product._

## 1. Recommendation

**Proceed to a gated migration (milestone M1.5), not a switch.**
Build the Tauri shell to full parity *alongside* Qt, and make it the default
only after the parity gate (roadmap T-12) passes on Linux, macOS and Windows.
Do **not** remove Qt until the removal gate (roadmap §5) passes, including
an answer for the M5 preview engine.

Confidence: **moderate**. The evidence is strong on Linux and for the
architecture; it is absent for macOS and Windows, which is where the
largest Tauri-specific risks live (two more web engines). If the T-09/T-12
cross-platform results fail the budgets in performance.md or Monaco behaves
differently on WKWebView/WebView2 in ways that cannot be fixed, **retain
Qt** — the prototype is isolated and can be dropped at no cost to M1.

## 2. Comparison

| Dimension | Qt (M1, as shipped) | Tauri (prototype) | Verdict |
|-----------|---------------------|-------------------|---------|
| **Feature parity** | Complete for M1; 115 Qt tests + offscreen smoke | All 9 M1 exit criteria demonstrated on Linux (E2E 18/18 ×2 runs; stores 13/13 ×20; bridge 12/12). Gaps: renderer-crash recovery (**missing**), a few shortcuts/menus, untested branches (feature-parity.md) | Qt ahead; gaps are bounded and known |
| **Performance** | Startup to explorer 866 ms cold | 421 ms cold; typing p95 18.8 vs 22.8 ms; explorer equal; Qt faster on 50k-line output (45 vs 99 ms) | Tauri ahead on Linux; unmeasured elsewhere |
| **Memory** | 446 MiB idle, 494 MiB with Monaco (PSS) | 282 / 307 MiB | Tauri −37 % on Linux |
| **Cross-platform behaviour** | One bundled Chromium everywhere: **identical engine** on all OSes. Windows CI not green (KD-WIN-1) | Three engines (WebKitGTK / WebView2 / WKWebView). Linux only verified. Known WebKitGTK GPU issues on NVIDIA/Wayland (Tauri docs) | **Qt ahead in predictability**; this is the main risk |
| **Visual quality** | Clean, consistent, Basic-style controls | Denser, Linear-like, command palette, generated Monaco themes, AA-checked tokens | Tauri ahead (subjective; screenshots in prototype-plan.md) — **not a reason to migrate on its own** |
| **Development complexity** | C++ + QML + TS + Rust; CMake + 4 GB Qt SDK; MSVC-only on Windows for WebEngine | Rust + TS; Cargo + pnpm; WebKitGTK dev packages on Linux | Tauri simpler (one fewer language, no Qt SDK); UI work uses the web stack Monaco already requires |
| **Long-term maintainability** | Shell logic split C++/QML; WebEngine Chromium lags (140 at Qt 6.12); Qt licensing constraints (LGPL module list, no static linking) | Shell logic in TS stores (ports of the same models); engines patched by OS vendors; Tauri MIT/Apache | Tauri ahead, *conditional on* engine divergence staying manageable |
| **Testing** | Qt Test against real daemon, offscreen GUI smoke (Linux/macOS/Windows build) | Rust bridge tests and Vitest store tests against the real daemon (no GUI needed); WebDriver E2E on Linux/Windows; **macOS E2E needs WebdriverIO embedded driver (untried)** | Comparable on Linux/Windows; macOS automation weaker |
| **Packaging** | ≈ 337 MB of Qt + WebEngine to ship; macOS notarisation vs LGPL open question (ADR-0011) | 8 MB + system webview on Linux (.deb/.rpm); WebView2 Evergreen on Windows; dmg on macOS; updater plugin | Tauri far smaller; LGPL relinking question disappears for Qt (WebKitGTK is a system library) |
| **Security** | Editor page limited to 7 bridge slots; Chromium 140 (lags) | Whole UI is the page and can call any protocol 0.1 method through the host; strict CSP, no remote content, 7 commands; `freezePrototype` off (Monaco). Engines get OS security updates | Mixed: **narrower page privilege in Qt**, **fresher engines in Tauri**. Editor isolation spike (T-11) can restore the narrower model |

## 3. Regressions and missing functionality (Tauri vs Qt)

1. **Renderer-crash recovery** — Qt recovers unsaved text after a renderer
   crash; the prototype does not (designed, roadmap T-08). *Blocking for
   default status.*
2. **Single engine guarantee lost** — Monaco and the UI run on three engines.
   Untested on two.
3. **M5 preview**: ADR-0001 chose Qt partly so previews could run in a known
   Chromium with CDP. Tauri has no equivalent on Linux/macOS. *Blocking for
   Qt removal* (roadmap §5.4 and spike M5-pre).
4. **Page privilege** — the UI page can issue any protocol method (in Qt only
   C++ could). Mitigated by bundling and CSP; restorable via T-11.
5. **Streaming output** — ~2× slower to settle for 50 000 lines (still < 0.1 s).
6. **Small UI gaps** — Ctrl+W, Ctrl+PgUp/PgDn, output context menu, View/Help
   menus, `--version` headless (feature-parity.md).
7. **Tauri hardening** — `freezePrototype` disabled for Monaco 0.57.

## 4. What would change this recommendation

| Finding during M1.5 | Then |
|---------------------|------|
| Monaco on WKWebView or WebView2 shows IME, clipboard or input defects that need engine patches | Retain Qt; close M1.5 |
| Any OS misses typing p95 ≤ 25 ms or memory ≤ Qt on CI + real hardware | Retain Qt unless a fix is demonstrated |
| Linux real-GPU runs show blank windows/flicker needing env-var workarounds by default | Retain Qt on Linux at minimum, or ship with `WEBKIT_DISABLE_DMABUF_RENDERER` and measure |
| M5-pre spike finds no acceptable preview engine | Keep Qt until M5 is redesigned; do not remove Qt |
| Renderer-crash recovery cannot be made equivalent | Do not make Tauri the default |

## 5. Approval required

Per the brief, no irreversible migration work begins without approval.
The owner is asked to choose one:

1. **Approve M1.5** as specified in implementation-roadmap.md (T-01 writes
   ADR-0012 and the task files; Qt untouched until T-14).
2. **Approve a cross-platform spike only** (T-09 subset: run the prototype's
   bridge tests, store tests and E2E on macOS and Windows CI) before
   deciding — the cheapest way to retire the biggest risk.
3. **Retain Qt**: archive `apps/desktop-tauri/` as an experiment and record
   the evaluation in an ADR with status "Rejected".

The author's preference is **option 2 immediately, then option 1** if it
passes: it costs days, not weeks, and answers the question this Linux-only
evaluation cannot.

## 6. Evidence index

- Audit and design: [architecture.md](architecture.md)
- Parity: [feature-parity.md](feature-parity.md)
- Measurements: [performance.md](performance.md), raw JSON in
  `apps/desktop-tauri/bench/results/`
- UI system: [design-system.md](design-system.md)
- Prototype, tests, defects found, screenshots: [prototype-plan.md](prototype-plan.md)
- Plan: [implementation-roadmap.md](implementation-roadmap.md)
