# E4 — Preview rendering + CDP inspection without Qt WebEngine

Resolves `docs/migration/qt-to-tauri/architecture.md` §4.1 ("open question: M5
preview engine") with a recorded measurement. Task: `docs/tasks/M1.5/M5-PRE-001.md`.

**Question.** Can a preview of a running UI be rendered into the app while being
inspected over the Chrome DevTools Protocol — (a) by an external Chromium
captured frame by frame, or (b) by WebView2's CDP on Windows only?

**Pass criterion (stated before running).** Recommend adoption only if
first frame < 1 s, p95 frame-capture latency < 100 ms, and a stamped element's
source is readable over CDP. Report the numbers even if it fails.

## How to rerun

```sh
node experiments/e4-preview-cdp/probe.mjs           # ~5 s, writes results.json
node experiments/e4-preview-cdp/probe.mjs --frames 120
node experiments/e4-preview-cdp/probe.mjs --chrome /path/to/chrome
```

No dependencies beyond Node 22 (global `WebSocket`, `fetch`). The probe:

1. serves a fixture page over loopback HTTP (a top-level `data:` URL is blocked
   by Chrome, and its target detaches — see §Notes);
2. starts Chrome **headless** (`--headless=new`) with
   `--remote-debugging-port=0` and `--user-data-dir` in a fresh temp directory,
   so no window appears and the real browser profile is never read or written;
3. connects to the browser endpoint, creates a target at the fixture URL,
   attaches to it with `flatten: true`, and scopes every command to that session;
4. measures navigation → first painted frame (polling `Page.captureScreenshot`
   until the image differs from the pre-navigation frame), then 60 sequential
   PNG captures and 20 JPEG captures;
5. reads a stamped element over CDP twice — `DOM.querySelector` +
   `DOM.getBoxModel`, and `Runtime.evaluate` on the element's `data-source`;
6. records PSS and RSS and the process count of the browser group, then removes
   the temp profile and kills anything it started.

## Observed

Three runs, Google Chrome 155.0.8059.84, Fedora 44 / x86_64, headless, Node 22:

| Measure | Run 1 | Run 2 | Run 3 |
|---------|------:|------:|------:|
| Navigation → first painted frame | 149.1 ms | 148.0 ms | 150.0 ms |
| Frame capture p50 (PNG, 60 frames) | 33.9 ms | 33.9 ms | 33.9 ms |
| Frame capture p95 | 51.2 ms | 51.2 ms | 50.7 ms |
| Frame capture max | 52.5 ms | — | 52.2 ms |
| JPEG capture p50 (quality 80) | 34.3 ms | 34.4 ms | 34.4 ms |
| Browser group PSS | 317 MiB | 319 MiB | 318 MiB |
| Browser group RSS | 1232 MiB | — | — |
| Processes in the group | 12 | 12 | 12 |
| Transient capture errors before first frame | 1 | 1 | 1 |
| Stamped element reachable over CDP | yes | yes | yes |

Element → source over CDP worked both ways: `DOM.getBoxModel` on `#stamped`
returned a 1100 × 198 box and its `data-source` read `src/App.tsx:42:7`; the
nested button returned `src/Button.tsx:11:3` with its own rect. That is exactly
the lookup ADR-0008's mapping needs.

**Verdict: option (a) passes its stated criterion** (149 ms < 1 s, p95 51 ms <
100 ms, element lookup works). Option (b) — WebView2's CDP — is **not run: no
Windows host in this environment**; it is not estimated.

## What this does and does not show

- It shows a **preview surface** is feasible: ~34 ms per PNG frame, ~51 ms at
  p95, i.e. a ~20–29 fps preview, and element→source lookups over CDP work.
- It does **not** show that streaming into the Tauri window is cheap: the
  app-side cost of decoding and drawing each frame is not measured here.
- It does **not** measure a real GPU: headless Chrome used its own software
  path. Frame capture on a GPU may be faster or slower.
- The browser group costs **~318 MiB PSS** and 12 processes. That is a real
  budget item: the whole Tauri shell measures 250 MiB idle
  (`performance.md` §3.7).
- Licensing: this used the **already-installed Google Chrome**, whose licence
  does not permit bundling it with Axiom. A product would need a redistributable
  Chromium build (or WebView2/CEF), which changes both size and licence
  posture; nothing here assumes one.

## Notes (found while running)

- A capture issued while a navigation commits fails with
  `Not attached to an active page` because the renderer is being replaced. The
  probe treats it as "not painted yet" and retries; exactly one occurs per run.
- `Emulation.setDeviceMetricsOverride` is harmless here, but choosing the page
  target from `Target.getTargets` is not: Chrome keeps more than one
  `about:blank` page and attaching to the wrong one fails later in the same
  way. Creating the target at the fixture URL and attaching to that id is
  deterministic.
- `--disable-gpu` made the first-frame capture fail consistently
  (`Not attached to an active page`); recorded as observed, not explained.
