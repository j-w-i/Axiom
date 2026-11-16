# Screenshot sets

Real-app screenshots of the production shell (Tauri + the platform webview),
captured by [`../shots.mjs`](../shots.mjs) and compared by
[`../shots-compare.mjs`](../shots-compare.mjs). Spec: `docs/design/visual-qa.md`.
This folder is git-ignored except for this file: the approved milestone set is
committed under `docs/roadmap/verification/M1.6/`.

## Take a set

```sh
apps/desktop-tauri/scripts/e2e.sh shots.mjs        # builds, then runs on a private Xvfb (Linux)
apps/desktop-tauri/scripts/e2e.sh --no-build shots.mjs   # reuse the existing build
AXIOM_SHOTS_DIR=/tmp/after apps/desktop-tauri/scripts/e2e.sh --no-build shots.mjs   # another folder
```

It writes `<os>/<state>-<theme>[@2x].png` and `<os>/manifest.json` (commit,
OS, engine, window size, per-state status). Window 1440x900 at 100% in dark
and light; S4 also at 200% (`GDK_SCALE=2`, Linux). States the shell cannot show
yet are `pending` in the manifest, with the task that adds them.

The suite never opens a window on your desktop: on Linux the runner starts its
own Xvfb and refuses to run without one.

## Compare two sets

```sh
node apps/desktop-tauri/e2e/shots-compare.mjs <before-dir> <after-dir> [--threshold 0.001] [--out <dir>]
```

Per-pixel, with a tolerance of one channel step (anti-aliasing). Exit code 1
when any state changes by more than the threshold (default 0.1% of pixels),
differs in size, or exists in only one set. Writes `<out>/report.html` (before,
diff, after side by side; default `<out>` is `e2e/shots/compare`) and
`<out>/diff/<state>.png`.

Typical use: run once on the commit before your change, once after, compare,
and explain every changed state in the handoff.

## Determinism

Animations and transitions are off, the caret and Monaco cursor are hidden,
durations in finished-run texts (`Succeeded in 83 ms`) are replaced by `1.2 s`,
the pointer is parked, fonts and two animation frames are awaited, and the
workspace is a fresh copy of `tests/fixtures/visual/` at a fixed path. Two runs
on the same commit must compare with 0 states over threshold; if they do not,
fix the harness, never the threshold.

`AXIOM_SHOTS_EXTRA_CSS` injects extra CSS into every page. It exists only to
prove that the comparer notices a token change, e.g.
`AXIOM_SHOTS_EXTRA_CSS=':root[data-theme] { --bg: #3a3a3a }'`.
