# M1.6 visual QA

_Status: **approved by ADR-0013** (2026-11-17); **implemented (M1.6, 2026-11-18)**. A passing unit test is not evidence of a polished
interface. Every M1.6 task that changes pixels attaches screenshots and passes the
review below._

## 1. Principles

- **Real app, real engine.** Reference screenshots come from the production shell
  (Tauri + WebKitGTK on Linux; WKWebView and WebView2 when hardware exists), driven
  by the existing e2e WebDriver harness. Prototype screenshots
  (`docs/design/concepts/`) are the *design references*, not the evidence.
- **Deterministic states.** Fixed window size, fixed fixture workspace, fixed
  data, animations disabled (`appearance.motion = "off"`), caret hidden, clock
  frozen (elapsed timers read from an e2e override).
- **Both themes, two scales.** Every state at 1440×900 in dark and light at 100%,
  and a subset at 200% (HiDPI).
- **Human review plus automated diffing.** Automation catches change; people
  judge quality.

## 2. The screenshot harness (QA-001)

`apps/desktop-tauri/e2e/shots.mjs`, run through `scripts/e2e.sh shots` (under
`headless.sh` on Linux):

1. Builds the `e2e` feature app, starts it with a temporary `AXIOM_DATA_DIR`, and
   opens the fixture workspace `tests/fixtures/visual/` (copied from a small,
   committed snapshot of real Axiom files: `crates/exec/src/*.rs`, a README, a
   Cargo.toml, an ignored `target/`).
2. Drives each state in §4 through WebDriver (keyboard and pointer), waits for
   `document.fonts.ready` and two animation frames, then calls `takeScreenshot`.
3. Writes `apps/desktop-tauri/e2e/shots/<os>/<state>-<theme>[@2x].png` plus a
   `manifest.json` (app commit, OS, engine version, scale, window size).
4. `--compare <dir>` produces a per-pixel diff (`pixelmatch`-style, implemented
   locally without a new dependency) with a threshold of 0.1% changed pixels per
   state, and writes an HTML report of before / after / diff.

**As built (M1.6-QA-001).** Differences from the plan above:

- Run it as `scripts/e2e.sh shots.mjs` (a bare suite name is looked up in `e2e/`);
  it is not in the default suite set. Compare with the separate
  `node e2e/shots-compare.mjs <before> <after> [--threshold] [--out]`, not a `--compare` flag.
- File names are lowercase state ids: `s4-dark.png`, `s4-light@2x.png`.
  S1 and S3 use stand-in backends (a process that never answers / exits at
  once), so their theme is set on the document instead of through a setting.
- Animations are disabled by injected CSS (there is no `appearance.motion`
  until M1.6-INTERACTION-001). Durations in finished-run texts are replaced by
  `1.2 s` in the page, because they are the only wall-clock text; the shell shows
  no elapsed time for a running task yet, so no clock hook exists.
- The 200% pass is S4 in both themes with `GDK_SCALE=2`; the runner gives its
  Xvfb a 2880x1800 screen for this suite.
- Independent stdout/stderr pipes interleave differently from run to run, so the
  stand-in build tool writes one line per 40 ms; a harness that lets that vary
  fails the two-run determinism check.
- Not yet capturable, listed `pending` in the manifest: S1b-S1d (SHELL-007), S11b
  (SURFACES-002), S16 (SHELL-005), S17 (SYSTEM-003) and the focused-region rule.
  S11 is the single current Settings sheet.
- The pre-redesign baseline is in `docs/roadmap/verification/M1.6/baseline/linux/`.

Baselines are **not** committed for every run. The approved set for each task is
attached to its handoff, and the milestone set lives in
`docs/roadmap/verification/M1.6/` (PNG, 100% only, for size).

## 3. Component gallery

`apps/desktop-tauri/gallery.html` (dev and e2e only; excluded from `dist`, and a
test asserts it is absent from the bundle) renders every `ui/` component in every
state of `design-system.md` §8 in both themes, for close inspection at 200%.

## 4. Required states

| # | State | Notes |
|---|-------|-------|
| S1 | Welcome, backend connecting | |
| S1b | Workspace opening (tree skeleton rows) | SHELL-007 |
| S1c | Workspace empty folder | SHELL-007 |
| S1d | Workspace listing error row | SHELL-007 |
| S2 | Welcome, backend connected, 3 recent folders | |
| S3 | Backend failed banner | |
| S4 | Workspace, 4 tabs (one dirty), executor.rs active, console with a passing run | main reference |
| S5 | S4 with a task running (LED, transport, stderr progress lines) | clock frozen at 00:12.4 |
| S6 | S4 with a failed run | |
| S7 | Command palette, empty query | |
| S8 | Palette, file mode with a query and highlights | |
| S9 | File menu open | |
| S10 | Output context menu | |
| S11 | Settings, Appearance; S11b Editor | |
| S12 | Unsaved-changes dialog; S12b conflict dialog | |
| S13 | Side bar hidden, console hidden (compact) | |
| S14 | Window at its 720×480 minimum | layout must not break |
| S15 | Focus on the tree (focused row ring, focused-region rule) | keyboard |
| S16 | Hint line showing the Rerun hint | hover |
| S17 | Gallery page, each section | |

## 5. Review checklist (per screenshot set)

**Alignment and rhythm.** Baselines align across the title row, tab strip and
panel headers; icons are centred in 26 px boxes; text is optically centred in rows
(no 1 px drift between Plex and the system fallback); 4 px grid respected.
**Typography.** Faces loaded (no fallback); sizes and weights per `design-system.md`
§5; no all-caps; tabular numbers in times and counts; truncation with an ellipsis.
**Colour.** Only tokens (guard test) and only the roles in §2.2; brass appears only
for focus, active, running, primary and attention; no colour-only meaning.
**Material.** Every box is raised, recessed or flat as specified; no bevels on
structure; one popover shadow.
**Hierarchy.** From two metres: the focused region, task state, backend state and
anything needing attention are identifiable within 5 s.
**Contrast.** Spot-check with a picker against §2.3 (the guard test covers tokens,
not composites such as text on `--selected`).
**Artefacts.** No seams doubled at splitters; no clipped focus rings; no blurry 0.5 px
lines at 100% or 200%; scrollbars styled; no flash of unstyled content at start.
**Platform.** Native title bar matches the theme (Windows/Linux); traffic lights
aligned with the title-row baseline (macOS); fonts render with the platform's
smoothing (`visual-qa` review per OS).

## 6. Manual interaction pass (per task that adds motion, and in QA-003/QA-005)

Watch each motion-system §4 interaction at Standard, then at Reduced and Off.
Check that it is interruptible (toggle twice quickly), that the input acts on the
first frame, that hover-delay flicker is absent on splitters, and that tab glides
land exactly on the active tab after a reorder or a scroll. Repeat with a
screen recording attached for the milestone review.

## 7. Visual acceptance criteria template (copy into each task)

```
Visual acceptance
- [ ] States <S…> captured in dark and light at 1440×900 (100%) and S4 at 200%.
- [ ] Compared against docs/design/concepts/r-<…>.png: deviations listed and justified.
- [ ] Checklist §5 passed; reviewer notes attached.
- [ ] Diff report vs the previous baseline attached; every changed state explained.
- [ ] Platforms: Linux verified; macOS / Windows "unverified on <OS>" unless run.
```
