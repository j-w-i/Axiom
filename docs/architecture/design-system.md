# Design system (Axiom.Ui)

The QML module `Axiom.Ui` (`apps/desktop/qml/Axiom/Ui/`) holds every visual
primitive. Application views (`Axiom.App`) use only these tokens and
controls: **no literal colours, font sizes, spacings or radii outside
`Axiom.Ui`.** Reviewers grep for `#[0-9a-fA-F]{6}` and `font.pixelSize: [0-9]`
in `qml/Axiom/App`.

## Theme singleton

`Theme.qml` (`pragma Singleton`) exposes the tokens below as read-only
properties and one writable property `dark: bool`, set by the shell from the
`appearance.theme` setting (`system` resolves through
`Qt.styleHints.colorScheme`).

### Colour roles

| Token | Light | Dark | Use |
|-------|-------|------|-----|
| `bg` | `#ffffff` | `#1e1f22` | editor area, content |
| `bgSubtle` | `#f5f6f8` | `#25262a` | side bar, panels, tab strip |
| `bgElevated` | `#ffffff` | `#2c2d32` | dialogs, menus, popups |
| `bgHover` | `#e9ebef` | `#33353b` | hovered row/control |
| `bgSelected` | `#dbe4ff` | `#2f3d63` | selected row, active tab marker bg |
| `border` | `#d9dce3` | `#3a3c43` | separators, control outlines |
| `text` | `#1c1e24` | `#e3e5ea` | primary text |
| `textMuted` | `#5f6675` | `#9aa1b0` | secondary text, ignored files |
| `textOnAccent` | `#ffffff` | `#ffffff` | text on accent backgrounds |
| `accent` | `#2f5fe0` | `#6b8cff` | primary buttons, focus ring, active markers |
| `danger` | `#c62f3b` | `#f0717b` | errors, stderr text |
| `warning` | `#9a6700` | `#e0b34f` | warnings, "changed on disk" |
| `success` | `#1a7f4b` | `#57c08a` | succeeded status |

Contrast: `text` on `bg` and `textMuted` on `bgSubtle` meet WCAG AA (4.5:1)
in both themes; keep that property when changing values.

### Spacing, radius, sizes

`space1..space6` = 4, 8, 12, 16, 24, 32. `radiusS` 4, `radiusM` 6.
`controlHeight` 28. `rowHeight` 24. `iconSize` 16. `borderWidth` 1.
`focusRingWidth` 2 (colour `accent`, drawn outside the control).

### Typography

`fontFamily`: platform UI font (`Qt.application.font.family`).
`monoFamily`: first available of `"JetBrains Mono", "Cascadia Code",
"SF Mono", "Menlo", "Consolas", "DejaVu Sans Mono", monospace` (resolved in
C++ and exposed as a context property; the same value is sent to the editor
when `editor.fontFamily` is empty).
Sizes (px): `fontCaption` 11, `fontBody` 13, `fontTitle` 15, `fontHeading` 20.
Weights: regular 400, `fontWeightStrong` 600.

### Motion

`durationFast` 80 ms (hover), `durationNormal` 160 ms (panels). All
animations are disabled when the platform requests reduced motion.

## Controls in M1

Each is a thin wrapper over Qt Quick Controls (Basic style) with tokens
applied: `AxButton` (variants `primary`, `default`, `quiet`), `AxIconButton`,
`AxTextField`, `AxCheckBox`, `AxComboBox`, `AxLabel` (role property:
`body|muted|title|heading|caption`), `AxSeparator`, `AxTabButton`,
`AxPanelHeader`, `AxListRow`, `AxScrollBar`, `AxDialog`, `AxBanner`
(variants `info|warning|danger`, optional action button), `AxSplitHandle`.
Add a control only when a view needs it.

## Accessibility rules (reviewed in every UI task)

- Every interactive item sets `Accessible.role` and `Accessible.name`.
- Everything is reachable with Tab/Shift+Tab; lists and trees support arrow
  keys, Home/End, Enter to activate.
- Focus is always visible (`focusRingWidth`), including inside lists.
- No information by colour alone: stderr lines also carry a marker in the
  gutter; task status has text.
- Text never truncates without a tooltip carrying the full text.

## Layout of the main window (M1)

```
┌ menu bar: File  Edit  View  Run  Help ──────────────────────────────────────┐
├──────────────┬──────────────────────────────────────────────────────────────┤
│ EXPLORER     │ tab strip: [main.rs ●] [lib.rs]                              │
│ ▾ src        │                                                              │
│    main.rs   │                editor (WebEngine view)                       │
│    lib.rs    │        — or the Welcome view when no document is open        │
│ ▸ tests      │                                                              │
│ Cargo.toml   ├──────────────────────────────────────────────────────────────┤
│              │ OUTPUT   [ cargo build            ▾] [Run ▶] [Stop ■] [Clear]│
│              │ output text…                                                 │
├──────────────┴──────────────────────────────────────────────────────────────┤
│ status bar: ● Connected   proj   Ln 12, Col 5   UTF-8        task: failed(3)│
└─────────────────────────────────────────────────────────────────────────────┘
```
Side bar and output panel are resizable (`SplitView`) and toggleable
(View menu, Ctrl+B / Ctrl+J). Minimum window size 720×480; below 900 px wide
the side bar starts collapsed.
