# The Flutter shell's design system (FL-DS-001, FL-DS-002)

`apps/desktop-flutter/lib/ui/`. Since FL-DS-002 (2026-11-22) the shell looks like the Tauri shell:
the owner liked that design and asked for it back, in place of FL-DS-001's mix of Precision
Instrument and Temporal Tactility. The reference is the Tauri shell as shipped
(`apps/desktop-tauri/src/styles/` at the tag `tauri-shell-final`; ADR-0013's M1.6 design) and its
screenshots (`docs/roadmap/verification/M1.6/macos/`). The gallery (`lib/ui/gallery.dart`) shows
every control; its golden images, `test/ui/goldens/gallery-{dark,light}.png`, are the visual
record.

## The Tauri design, as code
| Tauri | Flutter |
|-------|---------|
| `--surface-canvas/-chrome/-panel/-editor/-well/-raised-1,-2/-overlay` | `AxiomColors.canvas`, `bar`, `panel`, `ground`, `recessed`, `raisedTop`/`raised`, `sheet` |
| `--line`, `--line-mid`, `--line-input` | `groove`, `hairline`, `lineInput` |
| `--text`, `--text-muted`, `--text-faint` | `ink`, `ink2`, `ink3` (see the deviations) |
| `--accent`, `--accent-hi`, `--accent-lo`, `--primary-border` | `accent`, `accentHi`, `accentLo`, `primaryBorder` |
| `--raised-*` (gradient, lit edge, contact shade) | `Raised`, `RaisedPainter` |
| `--recessed-*` (inner shadow, lit edge) | `Well` |
| `--overlay-shadow` | `Sheet` |
| `.btn`, `.btn--primary/--quiet/--danger` | `AxButton` with `ButtonKind` |
| `.icon-btn[aria-pressed]` | `AxIconButton(selected:)` (the rail passes `engaged: false`) |
| `.input`, `.switch`, `.kbd`, `.glyph` | `AxTextInput`, `AxToggle`, `Keycaps`, `FileGlyph` |
| `.rail`, `.panel__head`, `.tabs`, `.tree-row`, `.console__view`, `.statusbar` | `shell.dart`, `editor_area.dart`, `explorer.dart`, `tabs.dart`, `console.dart` |
| `--r-1/-2/-3`, type 15/13/12/11 px, ease-out | `Metrics`, `AxiomType`, `AxiomMotion` (critically damped) |

## Deviations from the Tauri values
- **The faint ink.** `ink3` is `#A19D94` in dark and `#5D5A54` in light, not `#75726B` and
  `#83807A`: Tauri's faint tier falls under 4.5:1 on several surfaces, and
  `test/ui/tokens_test.dart` checks every text colour on every surface in both themes. Line
  numbers (`#6C6962`, `#8A867E`) and comments (`#8A867E`, `#6B675F`) are lifted for the same reason.
- **The tab's lit edge** does not glide between tabs; each active tab carries its own.
- **Keycaps** read Ctrl / Shift / Alt on Linux and Windows, and ⌘ ⇧ ⌥ on macOS.
- **Text on tints (audit AUD-084, 2026-11-23).** `tokens_test.dart` also checks text on the diff
  tints, `accentSoft`, the selected-row tint and `dangerSoft`. Light `okText` is `#33682B` and light
  `accentText` `#785616`, lifted to reach 4.5:1 on a selected row.
- **Tokens added by the audit remediation:** `diffAdd`, `diffDel`, `approvalBorder`,
  `Metrics.radiusPill`, `radiusHalo`, `sideGutter` (12 px, every side panel), `sideListInset`,
  `rowInset`, and `AxiomType.sectionHeading` (one heading style for equivalent side-panel sections).
- **Rows grow with the system's text scale** (`context.scaled`, AUD-020); fixed heights are the
  heights at scale 1.
- **Reduced motion follows the system:** Standard becomes Reduced when the platform asks for
  fewer animations (`MediaQuery.disableAnimations`, AUD-054).

## Rules the tests enforce
- No literal colour outside `tokens.dart`.
- No Material or Cupertino import.
- Every interactive element has a name and a role.
- Keyboard operation of each control: Enter/Space, menu arrows and Escape (focus returns),
  tab arrows, splitter arrows and Home.
- Toasts are live regions and take no focus.
