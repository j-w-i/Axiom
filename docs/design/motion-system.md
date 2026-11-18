# Axiom motion and interaction system

_Status: **approved by ADR-0013** (2026-11-17); **implemented (M1.6, 2026-11-18)**. Measurements:
`concepts.md` §4 and `performance.md`. Reference implementation:
`experiments/e5-visual-concepts/src/base.css` (presence choreography) and
`src/shared.tsx` (`usePresence`)._

## 1. Purpose

Motion is allowed when it does one of three things:

1. **Continuity**: shows where something came from or went (panels, palette, tab indicator).
2. **Confirmation**: acknowledges an input within one frame (press, toggle, save).
3. **Live state**: shows that a process is alive (task running, agent waiting).

Anything else is decoration and is rejected. The editor's text area, the
caret's behaviour and terminal output are never animated by the shell.

## 2. Tokens

| Token | Standard | Reduced | Off |
|-------|---------:|--------:|----:|
| `--dur-press` | 60 ms | 60 ms | 0 |
| `--dur-1` (micro: hover, chevrons, selection glide) | 90 ms | 90 ms (colour only) | 0 |
| `--dur-2` (enter: panels, popovers, palette, switch) | 140 ms | 80 ms fade | 0 |
| `--dur-3` (travel: tab indicator, toast, meter) | 200 ms | 80 ms fade | 0 |
| `--travel` (enter offset) | 8 px | 0 | 0 |
| `--ease-out` | `cubic-bezier(.2, .8, .2, 1)` | | |
| `--ease-in` (exits) | `cubic-bezier(.4, 0, 1, 1)` | | |
| `--ease-in-out` | `cubic-bezier(.45, 0, .2, 1)` | | |

No springs and no overshoot: precision instruments settle and do not bounce (Concept
B's overshoot was tested and rejected as playful). Exits are faster than entrances
(`--dur-1` vs `--dur-2`) so dismissal never delays the next action.

**Setting.** `appearance.motion` = `standard` (default), `reduced` or `off`, stored
as a shell-side settings key (unknown keys are stored unchanged by the daemon: no
protocol change). `prefers-reduced-motion: reduce` forces at least `reduced`. The
level is applied as `data-motion` on `<html>` and selects the token column above.

## 3. Rules

1. Animate only `transform` and `opacity`. Colour transitions are allowed for hover
   and selection (≤ 90 ms). Never animate `width`, `height`, `top`, `left`,
   `grid-template-*`, `box-shadow`, `filter` or `backdrop-filter`.
2. **A layout change happens once.** Panels change size in a single frame. Motion is
   applied to the panel's *content* (transform + opacity), never to its box.
   Monaco therefore re-lays out once per toggle, not per frame.
3. **Input is never delayed by motion.** Focus moves and the action executes on the
   input frame; animation follows. A closing overlay does not block the next
   keystroke (the palette runs its command after the close starts, as today).
4. **Interruptible.** Re-triggering mid-animation retargets from the current state.
   Presence uses CSS animations keyed by `data-phase`, so reversing restarts from the
   rendered state.
5. **No animation during drag or resize.** Splitter drags write a CSS variable per
   animation frame (`requestAnimationFrame`-coalesced, no React render per pointer
   event) and commit to state on release.
6. **Nothing animates while idle** except the live-state indicators in §5.
7. `will-change` only during an animation (set and cleared by the presence helper),
   never permanently.

## 4. Choreography catalogue

| Interaction | Motion (Standard) | Category |
|-------------|-------------------|----------|
| Button / key press | raised → recessed recipe + `translateY(.5px)`, `--dur-press` linear | confirmation |
| Toggle engage (icon toggle, LED toggle) | snaps to recessed; no travel | confirmation |
| Switch | thumb `translateX`, `--dur-2` ease-out; track colour `--dur-2` | confirmation |
| Segmented | glide element `translateX`, `--dur-2` | continuity |
| Tree chevron | `rotate(90deg)`, `--dur-1` | continuity |
| Tree expand | no height animation (virtualised list); new rows appear in one frame | — |
| Side bar show / hide | box toggles in one frame; content `translateX(∓8px)` + opacity, in `--dur-2` / out `--dur-1` | continuity |
| Console show / hide | as the side bar, on Y | continuity |
| Splitter hover | seam brightens to `--accent-line` after a 120 ms hover delay (avoids flicker when the pointer passes over it) | feedback |
| Tab activation | lit-edge indicator glides (`translateX` + `scaleX`), `--dur-3`; tab text colour `--dur-1` | continuity |
| Tab reorder (D7) | dragged tab lifts (overlay shadow, 1.02 scale); siblings shift by `translateX` `--dur-2`; drop settles `--dur-2` | continuity |
| Menu / context menu | opacity + `translateY(-4px)` in `--dur-2`; out `--dur-1` | continuity |
| Command palette | scrim fades `--dur-2`; panel opacity + `translateY(-6px) scale(.985)` → 1 in `--dur-2`; out `--dur-1`; selection highlight glides `translateY` `--dur-1` | continuity |
| Dialog | as the palette, centred | continuity |
| Tooltip | opacity `--dur-1` after a 500 ms delay; 0 delay within 300 ms of the previous one | feedback |
| Toast | `translateY(10px)` + opacity in `--dur-3`; out opacity `--dur-2`; auto-dismiss after 4 s, paused on hover/focus | feedback |
| Task start | transport LED goes to `run`; elapsed time starts; meter begins sampling | live |
| Task end | LED → ok/err (instant); a toast if the console is hidden | live / feedback |
| Output-rate meter | bars update at 4 Hz (250 ms buckets) with no tweening | live |
| Agent streaming (M3) | text appended per animation frame; caret block blinks at 1 Hz stepped; tool rows switch spinner → check instantly | live |
| Workspace switch | editor area crossfades (opacity `--dur-2`) while the tree loads; no slide | continuity |
| Drag-and-drop (tabs, M4 files) | drop zones show a 2 px accent insertion line; invalid targets show nothing (no shake) | feedback |

## 5. Looping-animation policy (measured)

Loops are the only motion that costs CPU while the user does nothing. Measured in
the prototype (software raster; `concepts.md` §4): all indicators smooth = 18% of a
core; spinners alone ≈ 9%; smooth LED breathing ≈ 6–8%; the same LED stepped = 1.6%;
everything under this policy = **4.4%**.

1. A loop exists only while its state is true (task running, agent waiting). There
   are no decorative loops.
2. Loops are **stepped**: `steps(n)` with ≤ 8 updates per second (LED pulse
   `steps(6)` over 1.6 s; spinner `steps(8)` over 0.8 s; caret `steps(1)` at 1 Hz).
3. **At most one loop per region**, and at most three visible in the whole window.
   Duplicate indicators of the same state (the transport and status bar both showing
   "running") animate only one; the other is static.
4. No continuous sweeps or shimmer. Indeterminate progress is a stepped spinner.
5. Loops pause when the window is hidden or unfocused (`visibilitychange`, `blur`
   → `data-idle` on `<html>`, which sets `animation-play-state: paused`).
6. `reduced`: loops become static (LED solid, spinner replaced by a static glyph
   plus text). `off`: same.
7. The budget for every loop running together is in `performance.md` §3, measured on
   the production engine by M1.6-QA-004.

## 6. Reduced-motion behaviour

| Element | Standard | Reduced | Off |
|---------|----------|---------|-----|
| Presence (panels, palette, menus, dialogs) | transform + fade | fade 80 ms | instant |
| Glides (tab, segmented, palette selection) | transform | instant jump | instant |
| Press | translate + recipe | recipe only | recipe only |
| Loops | stepped | static | static |
| Toast | slide + fade | fade | instant |

## 7. How motion is verified

- **Frame times**: the e2e harness records `requestAnimationFrame` intervals during
  each catalogue interaction on the real app (`performance.md` §4.3). Budget: p95 ≤
  17.5 ms and no frame over 25 ms on the reference machine at 60 Hz.
- **Input latency**: typing p95/p99 unchanged within +1 ms of baseline (bench).
- **Idle cost**: process CPU with the task running and the window idle, versus with no
  task (bench `idle-running` scenario).
- **Reduced motion**: e2e runs once with `appearance.motion = "off"` and asserts no
  running CSS animations (`document.getAnimations().length === 0` when idle).
- **Visual**: screenshots are taken with animations disabled; a separate manual pass
  per OS watches each interaction (`visual-qa.md` §5).
