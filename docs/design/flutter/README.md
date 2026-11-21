# The Flutter shell's design system (FL-DS-001)

ADR-0020 §4 as code: `apps/desktop-flutter/lib/ui/`. The gallery (`lib/ui/gallery.dart`) shows
every control; its golden images, `test/ui/goldens/gallery-{dark,light}.png`, are the visual
record.

## Where the two directions meet
| Taken from | What | Where |
|------------|------|-------|
| Temporal Tactility | Depth by tone steps (recessed, ground, bar, raised, sheet); no blur or glass | `AxiomColors`, `surfaces.dart` |
| Temporal Tactility | A 1 px lit edge on raised surfaces, a 1 px contact shade, one soft shadow on sheets only | `Raised`, `Sheet` (painted, not stacked shadows) |
| Temporal Tactility | Charcoal dark theme, dark grooves between rows | `AxiomColors.dark`, `Groove` |
| Temporal Tactility | Ink underline under the selected tab or rail item | `AxTabs` (it springs to its place) |
| Temporal Tactility | One ink circle for the primary action; the composer's bezel ring and two-tone labels | `InkCircleButton`, `AxTextInput(bezel: true)` |
| Temporal Tactility | Answer first: one plain sentence per page, the only large type | `AnswerLine`, `AxiomType.answer` |
| Temporal Tactility | Status dots in a cut-out ring | `StatusDot` |
| Temporal Tactility | Springs with 1.5–4 % overshoot, only in answer to the person | `AxiomMotion.settle`, `.snap`, `.expressive` |
| Precision Instrument | Brass accent for focus, the running state and links; IBM Plex; the hint line; Mod-written shortcuts as keycaps | `accent`, `focus`, `HintScope`, `Keycaps` |
| Precision Instrument | Standard / Reduced / Off motion | `MotionMode` |

## Deviations from the owner's starting values
- **Tertiary ink.** The tertiary ink (`ink3`) is `#63676E` in light and `#8F939B` in dark,
  instead of `#868A91` and `#7D8189`. The original values fall under 4.5:1 for desktop-sized
  text on bars, wells and hovered rows. `test/ui/tokens_test.dart` checks every text colour on
  every surface in both themes.
- **The sheet tone in dark.** The dark sheet is `#1F2125`, one step above the raised tone
  `#1B1D21`, so menus stand off rows without a border.
- **Keycaps.** They read Ctrl / Shift / Alt on Linux and Windows, and ⌘ ⇧ ⌥ on macOS.

## Rules the tests enforce
- No literal colour outside `tokens.dart`.
- No Material or Cupertino import.
- Every interactive element has a name and a role.
- Keyboard operation of each control: Enter/Space, menu arrows and Escape (focus returns),
  tab arrows, splitter arrows and Home.
- Toasts are live regions and take no focus.
