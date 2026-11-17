# Qt → Tauri: design system

_**Superseded for tokens** by `docs/design/design-system.md` (M1.6, ADR-0013): since
M1.6-SYSTEM-001 `tokens.css` is generated from `src/styles/themes.json`, and the names
below survive only as legacy aliases until M1.6-QA-006._

_Status: evaluation. Implemented in the prototype at
`apps/desktop-tauri/src/styles/tokens.css`, `src/styles/app.css`,
`src/ui/primitives.tsx`, `src/ui/icons.tsx`; guarded by
`tests/tokens.test.ts`. The Qt design system
(`docs/architecture/design-system.md`) remains authoritative for `apps/desktop`._

## 1. Goals

1. **Linear-grade polish** — quiet, dense, low-chroma neutrals; one accent;
   hairline borders instead of boxes; motion that confirms, never decorates.
2. **IDE productivity** — keyboard-first (command palette, shortcuts listed
   in menus), dense rows (24 px), monospace where data is code or output,
   virtualised lists.
3. **One vocabulary across shells** — semantic token names are a superset of
   the Qt `Theme` singleton (`bg`, `bgSubtle`, `text`, `textMuted`, `accent`,
   `danger`, …), so specs and reviews mean the same thing in both.
4. **Enforced, not aspirational** — guard tests fail on literal colours,
   missing theme tokens, and contrast below WCAG AA.

## 2. Token architecture

Three layers, all CSS custom properties:

| Layer | Example | Who may use it |
|-------|---------|----------------|
| Primitive | `--gray-850`, `--indigo-600`, `--space-3`, `--radius-m` | only `tokens.css` |
| Semantic (themed) | `--bg`, `--bg-subtle`, `--bg-elevated`, `--bg-hover`, `--bg-selected`, `--border`, `--text`, `--text-muted`, `--text-faint`, `--accent`, `--accent-line`, `--danger`, `--warning`, `--success`, `--shadow-popover`, `--scrim` | components and views |
| Component | `--row-h`, `--tab-h`, `--control-h`, `--titlebar-h` | components |

Themes are `[data-theme="dark"]` (default) and `[data-theme="light"]` on
`<html>`, set from the `appearance.theme` setting (`system` resolves through
`prefers-color-scheme`). Monaco themes `axiom-light`/`axiom-dark` are
**generated at runtime from the same tokens** (`src/editor/surface.ts`), so
the editor gutter, widgets and focus border match the shell exactly.

### Mapping from Qt tokens

| Qt `Theme` | Web token | Change |
|------------|-----------|--------|
| `bg`, `bgSubtle`, `bgElevated`, `bgHover`, `bgSelected` | `--bg`, `--bg-subtle`, `--bg-elevated`, `--bg-hover`, `--bg-selected` | dark palette deepened and de-saturated (Linear-like); selected uses translucent accent |
| `border` | `--border`, `--border-strong` | split into hairline and control borders |
| `text`, `textMuted`, `textOnAccent` | `--text`, `--text-muted`, `--text-on-accent` | + `--text-faint` for ignored files, hints, line numbers |
| `accent` | `--accent` (fills), `--accent-line` (rings, markers) | split so white-on-accent meets AA in dark (4.97:1) while rings stay bright |
| `danger`, `warning`, `success` | same names | + `-soft` tints for banners |
| `space1..6`, `radiusS/M`, `controlHeight`, `rowHeight`, `iconSize`, `focusRingWidth` | `--space-1..6`, `--radius-s/m`, `--control-h`, `--row-h`, `--icon`, `--focus-w` | identical values; + `--radius-l` (dialogs), `--space-0-5` |
| `fontCaption/Body/Title/Heading` = 11/13/15/20 | `--text-caption/body/title/heading` | identical |
| `durationFast/Normal` = 80/160 ms | `--dur-fast/normal` | identical; zeroed under `prefers-reduced-motion` |

## 3. Components

| Component | Implementation | Accessibility |
|-----------|----------------|---------------|
| `Button` (default/primary/quiet/danger, `sm`) | `<button>` | native |
| `IconButton` | `<button aria-label title>` — label mandatory by type | name + tooltip |
| `Kbd` | `<kbd>` | — |
| `Banner` (info/warning/danger, optional action) | `role="alert"` for danger, `status` otherwise | announced |
| `Dialog` (explicit buttons, dismiss action) | Radix Dialog | focus trap, Escape, `aria-modal`, title |
| `Sheet` (settings/about) | Radix Dialog | same |
| `Menu` | Radix DropdownMenu | roving focus, typeahead, Escape |
| `Segmented` | `role="radiogroup"` + `role="radio"` | checked state |
| `Splitter` | `role="separator"`, arrow keys resize | value now/min/max |
| `Explorer` tree | `role="tree"`/`treeitem`, `aria-level/expanded/selected`, `aria-activedescendant` | full keyboard model (arrows, Home/End, PgUp/PgDn, Enter/Space, Left→parent) |
| Tabs | `role="tablist"`/`tab`, `aria-selected`, close buttons named "Close X (unsaved)" | dirty state not by colour alone |
| Output | `role="log"`, `aria-live="polite"`; stderr has a gutter glyph and a visually-hidden "stderr:" prefix | not by colour alone |
| Command palette | Radix Dialog + `combobox`/`listbox` with `aria-activedescendant` | keyboard only |
| Icons | 16 px stroke SVGs, `aria-hidden` | decorative |

Rules carried over from Qt (`design-system.md` §Accessibility): every
interactive element named; everything reachable by keyboard; visible focus;
no information by colour alone; truncated text has a tooltip (`title`).

## 4. Library evaluation

| Option | What it gives | Cost / risk | Decision |
|--------|---------------|-------------|----------|
| **Radix Primitives** (MIT) | Unstyled, WAI-ARIA-conformant dialogs, menus, popovers, tooltips; focus management | ~30–40 kB per used primitive (gzip small); React-only | **Adopt** for overlay/menu primitives. Hand-rolled focus traps and menu keyboard models are where accessibility regressions hide. |
| **Tailwind CSS v4** (MIT) | Utility classes, design-token theme config, fast iteration | Duplicates the token vocabulary into class names; makes "no literal values in views" unenforceable (`bg-[#123]`, `p-[13px]`); Monaco theming still needs CSS variables; extra build step | **Do not adopt.** Plain CSS over custom properties is enough for a single app with a small component set, and keeps one source of truth that Monaco also reads. Revisit if the component count grows past ~60. |
| **shadcn/ui** (MIT) | Copy-in React components built on Radix + Tailwind | Not a dependency (source is copied and owned), and assumes Tailwind | **Do not adopt as a dependency.** Use as a reference for component anatomy and Radix usage only. |
| **Headless UI / React Aria** | Alternatives to Radix | React Aria is stronger on a11y breadth but larger and more opinionated | Not needed for M1 scope; React Aria is the fallback if Radix lacks a primitive (e.g. virtualised accessible listbox). |
| **Icon libraries** (Lucide etc.) | Hundreds of icons | dependency for ~17 icons | Not now; local SVG set. |

## 5. Visual language (as built)

- **Density:** 24 px rows, 28 px controls, 34 px tabs/section headers, 38 px
  title bar, 24 px status bar.
- **Surfaces:** `--bg-subtle` for chrome (title bar, side bar, tab strip,
  status bar), `--bg` for content, `--bg-elevated` + `--shadow-popover` for
  overlays. Separation by 1 px `--border` hairlines, no panel boxes.
- **Type:** UI font stack (Inter if installed → system UI), 13 px body,
  uppercase 11 px tracked section labels; mono stack identical to Qt's list.
- **Accent:** indigo, used for primary action, active tab marker, focus ring,
  selection tint. Status colours only for status.
- **Motion:** 80 ms hover, 160 ms overlay fade/scale; no layout animation of
  the editor or lists.

Screenshots from the E2E run: see [prototype-plan.md §5](prototype-plan.md#5-results).

## 6. Guards (CI-enforceable)

`apps/desktop-tauri/tests/tokens.test.ts`:
1. No `#rgb/#rrggbb/rgb()/rgba()` literals in `src/**` outside `tokens.css`
   (icons exempt — they use `currentColor`; Monaco's transparent constants
   allow-listed).
2. Light and dark define the identical set of semantic tokens.
3. Contrast ≥ 4.5:1 for text/bg, text/bg-subtle, muted/bg, muted/bg-subtle,
   danger/bg, on-accent/accent in both themes. (This guard caught a real
   defect during the prototype: white on the dark accent was 3.5:1.)

## 7. Known gaps

- No high-contrast theme (Qt has none either; M7.5).
- Not tested with OS font scaling / HiDPI at 125–200 %.
- Inter is not bundled (falls back to the system UI font); bundling it is a
  licence-clean (OFL) option to make all OSes identical.
