# M1.6 creative brief

_Status: **approved by ADR-0013** (2026-11-17); **implemented (M1.6, 2026-11-18)**._

## 1. The one-sentence goal

Axiom should feel like a **precision instrument for building software**: dense,
calm and fast enough for eight-hour sessions, with controls that feel engineered
and machine state you can read at a glance, in a visual language nobody will
mistake for VS Code, a web dashboard or an AI-startup landing page.

## 2. Where we start

The M1.5 shell works and is measured (`docs/roadmap/verification/M1.5.md`), and
its visual layer was explicitly modelled on Linear: cool low-chroma greys, one
indigo accent, hairlines, 80/160 ms fades (`docs/migration/qt-to-tauri/design-system.md`
§1). The result is competent but generic. The audit in `concepts.md` §1 lists
eleven specific weaknesses. The main ones: every region shares one surface tone;
the only accent is indigo; nothing distinguishes pressable, editable and
content; motion is limited to fades; and machine state (backend, task) is shown
only as small text in the status bar.

## 3. What we take from FL Studio, and what we don't

FL Studio is the primary reference for **craft**, not for layout. Evidence and
sources are in `research.md`.

| Take | Leave |
|------|-------|
| Controls that look and behave like controls: pressed states, engaged toggles that sit *in*, consistent precision gestures | Default 3D shading on every surface (FL itself makes it optional) |
| Readable machine state: LEDs, meters, LCD-style readouts | Free-floating MDI windows; wallpaper; transparency |
| The **hint bar**: whatever is under the pointer explains itself, with its shortcut | Animation "to entertain" |
| Theming as a few global perceptual controls plus fixed semantic slots | Hand-picked colour matrices |
| Animation as a user-controlled setting, from off upwards | FL's orange, its layout, its assets, its typography |

## 4. Design principles (ranked; they break ties in this order)

1. **The editor is sacred.** Nothing in the redesign may add latency between a
   keystroke and its pixels, steal focus, or animate inside the text area.
2. **Material means something.** *Raised* = you can press it. *Recessed* = it
   holds a value or accepts input (fields, displays, the editor itself). *Flat* =
   content and structure. Depth that does not follow this rule is removed.
3. **Colour is a signal.** Chrome is low-chroma graphite. Colour belongs to code,
   to state (running, failed, modified, waiting for you) and to the single accent.
4. **State is visible without reading.** Backend health, task activity and
   "needs you" have a lit indicator in a fixed place.
5. **Every control explains itself.** The hint line describes the hovered or
   focused control and its shortcut. No control is mouse-only.
6. **Density is a feature.** Rows are 22 px, controls 26 px, padding is tight.
   Nothing is a card that could be a row.
7. **Motion carries continuity, never decoration.** It shows where something came
   from or went, and confirms an action. It is off-able, and loops are rare and
   stepped.
8. **Measured, not asserted.** Every visual change ships with screenshots, and
   every motion with frame-time numbers.

## 5. Constraints (non-negotiable)

- Architecture unchanged: Tauri 2 host + React webview over `axiomd`; IPC 0.1 and
  editor bridge 1 frozen; no business logic in the shell (ADR-0012, governance).
- No placeholder UI in production (governance rule 9). Surfaces for M2–M7 are
  *designed* here and *prototyped* in `experiments/`, but built only by their own
  milestones.
- Tokens only: no literal colours outside `src/styles/tokens.css` (guard test).
  Every interactive element has an accessible name and role.
- WCAG 2.2 AA contrast for text, 3:1 for UI component boundaries and focus
  indicators. Full keyboard operation. `prefers-reduced-motion` honoured.
- Performance budgets of `docs/migration/qt-to-tauri/performance.md` §2 remain
  in force, plus the new ones in `performance.md`.
- Linux, macOS and Windows; WebKitGTK, WKWebView and WebView2 render differently,
  so screenshots are reviewed per OS.
- No heavy component framework or CSS framework (Tailwind/shadcn were evaluated
  and rejected in M1.5; that stands). No JavaScript animation library.
- Licences: fonts OFL-1.1; code MIT/Apache-2.0/BSD/ISC (ADR-0011/0012).

## 6. Anti-patterns (reject on sight in review)

Purple-to-blue gradients; cyan or violet glow borders; glassmorphism and
`backdrop-filter`; large floating cards; radii over 8 px on panels or over 4 px on
controls; padding that pushes rows above 24 px; flat, undifferentiated greys;
shadcn defaults; decorative or looping animation without a state behind it;
animation of `width`/`height`/`top`/`left` or of filters; any animation on the
typing path; recreating VS Code's layout with new colours; FL Studio's orange,
layout or artwork.

## 7. Success criteria (what "approved and done" will look like)

- A reviewer shown a screenshot with the logo hidden can tell it is Axiom, and
  not VS Code, Zed or a JetBrains IDE.
- In a 5-second glance at the workspace, a user can say: which panel has focus,
  whether a task is running, whether the backend is connected, and whether
  anything needs their attention.
- Typing latency, startup and idle CPU stay within budget (`performance.md`), and
  there are no frames over 20 ms during any shell motion on the reference Linux
  machine.
- All M1 and M1.5 end-to-end checks still pass.
