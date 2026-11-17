# Axiom design system — "Precision Instrument"

_Status: **approved by ADR-0013** (2026-11-17); normative for M1.6. It supersedes
`docs/migration/qt-to-tauri/design-system.md` §1–§2 (goals and palette); that
document's component accessibility table (§3) and library evaluation (§4) remain in
force except where this one says otherwise. Reference implementation:
`experiments/e5-visual-concepts/src/concepts/{a,r}.css` (prototype only; production
code is written fresh by the M1.6-SYSTEM tasks)._

## 1. Foundations at a glance

| Foundation | Rule |
|------------|------|
| Palette | Warm-neutral **graphite** (dark) / **aluminium** (light); one **brass** accent; fixed state hues |
| Material | **Raised** = pressable · **Recessed** = holds a value or accepts input · **Flat** = content and structure |
| Hierarchy | Tone steps plus hairlines. The editor is the deepest well; chrome is darkest in dark mode and lightest-grey in light mode |
| Type | IBM Plex Sans (UI), IBM Plex Mono (code, data, paths) — bundled if D2 is approved |
| Density | 4 px grid; 22 px rows, 26 px controls, 32 px tabs, 30 px panel headers |
| Shape | Radii 3 / 4 / 6 px; panels square |
| Signal | LEDs for live state; the accent marks focus, the active item, primary actions and "needs you" |
| Motion | `motion-system.md` |

## 2. Colour

### 2.1 Primitive scales (generated; see §3)

Only `tokens.css` may reference primitives. Values are the approved prototype's,
reproduced by the generator within ΔE2000 < 2.

| Scale | Steps (dark use → light use) |
|-------|------------------------------|
| `graphite-0…900` | 15 role steps shared by both themes (`0` seam, `50` canvas, `100` well, `150` chrome, `175` editor, `200` panel, `250` overlay, `275` raised bottom, `300` hairline, `350` raised top, `400` strong line, `500` input boundary, `600`/`700`/`900` text tiers; light values in the light block). Chroma 0.002–0.013 OKLCH. As built (SYSTEM-001): the approved dark *surfaces* sit at OKLCH hue 248–286 (slightly cool), the text tiers at 85–92 (warm); the generator keeps both as hue offsets from the base input. Key dark stops: 50 `#0b0c0d` canvas, 100 `#111213` well, 150 `#141517` chrome, 175 `#17181a` editor, 200 `#1e1f22` panel, 250 `#26272a` overlay, 300 `#2b2c30` hairline, 350 `#303135` raised, 400 `#3a3b3f` strong line, 600 `#6f6c66`, 700 `#a3a097`, 900 `#e6e3dc` |
| `brass-300…800` | hue 80°, chroma 0.09–0.11: 300 `#ddb96f`, 400 `#d9b672`, 500 `#c9a35b`, 600 `#b88a3a`, 700 `#9a722c`, 800 `#85601c` |
| `sage` (ok) | dark `#8fbf7f` / text `#9fca90`; light `#4f8f45` / text `#3f7a36` |
| `ember` (warn) | dark `#e8904f`; light `#a8520f` (**darkened from the prototype's `#c2621d`, 4.0:1, to ≥ 4.5:1**) |
| `brick` (danger) | dark `#e5645d`; light `#c43c35` |
| `steel` (info, M2+) | dark `#9fb8d8`; light `#3b6a9e` |

### 2.2 Semantic tokens (what components use)

| Token | Role | Dark | Light |
|-------|------|------|-------|
| `--surface-canvas` | behind everything; seams | `#0b0c0d` | `#c4c1bb` |
| `--surface-chrome` | title row, tab strip, status bar | `#141517` | `#e6e4df` |
| `--surface-panel` | side bar, console, dock | `#1e1f22` | `#efede9` |
| `--surface-editor` | editor and page content (deepest well) | `#17181a` | `#fcfbf9` |
| `--surface-well` | fields, displays, pressed toggles | `#111213` | `#ffffff` |
| `--surface-raised-1/2` | top/bottom of pressable controls | `#303135`/`#28292c` | `#ffffff`/`#f1efeb` |
| `--surface-overlay` | menus, palette, dialogs, toasts | `#26272a` | `#fcfbf9` |
| `--line` | seam: structural boundary, control outline | `#0a0a0b` | `#c3c0ba` |
| `--line-mid` | hairline inside a surface | `#2b2c30` | `#dcd9d3` |
| `--line-strong` | dashed/emphasised outline | `#3a3b3f` | `#bdb9b2` |
| `--line-input` | border of value-holding controls (fields, command field, console prompt); solved at ≥ 3:1 (CR-M1.6-SYSTEM-001) | `#6c6962` | `#86827c` |
| `--edge`, `--edge-strong` | lit edge on raised controls only | white 5.5% / 8.5% | white 90% / 100% |
| `--hover`, `--selected`, `--press` | state overlays | white 4.5% / 7.5%, black 28% | ink 5% / 8% / 10% |
| `--text`, `--text-muted`, `--text-faint` | text tiers | `#e6e3dc` / `#a3a097` / `#75726b` | `#1e1d1b` / `#5b5852` / `#84807a` |
| `--accent` (+`-hi`, `-lo`, `-text`, `-soft`, `-line`) | brass | `#c9a35b`, text `#d9b672` | `#b88a3a`, text `#85601c` |
| `--on-accent` | text on brass fill | `#1a1307` | `#1d1505` |
| `--focus` | focus ring | `#d2ac63` | `#9a722c` |
| `--ok`, `--run`, `--warn`, `--danger`, `--attention` | state hues (fixed; never re-tinted) | sage, brass, ember, brick, ember | darker equivalents |
| `--syn-*` (8) | editor syntax (`text, comment, keyword, type, string, number, punct, macro`) | clay, verdigris, olive, lilac, steel | darker equivalents |
| `--editor-*` (9) | Monaco chrome: line, line numbers, selection, cursor, guides | see prototype `a.css` | |
| `--glyph-{rs,md,toml,other}` and `-bg` | file-type glyph chips (§7): text and 12–17% tint | `rs` clay `#dd9e80`, `md` steel `#a9c1df`, `toml`/`lock` verdigris `#95c6b6`, other `#a3a097` | `#a8502e`, `#3b6a9e`, `#2f7a68`, `#5b5852` |
| `--icon-folder`, `--tree-guide` | explorer folder icons and indent guides | `#8e8a82`, white 5% | `#7a766e`, ink 7% |

`--run` equals the accent (activity is the brass signal), and `--attention`
("needs you", e.g. an approval or a dirty-on-quit prompt) equals `--warn`. Both are
separate tokens so they can diverge.

### 2.3 Contrast requirements (enforced by `tests/tokens.test.ts`)

| Pair | Minimum | Prototype (dark / light) |
|------|---------|--------------------------|
| `--text` on editor, panel, chrome, overlay | 7:1 | 13.9 / 16.1 |
| `--text-muted` on editor, panel, chrome | 4.5:1 | 6.3 / 6.0 |
| `--text-faint` on editor, panel | 3:1, **non-essential text only** (line numbers, ignored files, placeholders, hints at rest) | 3.7 / 3.5 |
| `--accent-text`, `--ok-text`, `--warn`, `--danger` on editor and panel | 4.5:1 | 9.2, 9.6, 7.2, 5.3 / 5.5, 5.0, ≥4.5 (after fix), 5.0 |
| `--on-accent` on `--accent` | 4.5:1 | 7.8 / 5.8 |
| `--focus` against both adjacent surfaces | 3:1 | |
| `--line-input` (input boundary) against editor, panel, chrome and well | 3:1 (WCAG 1.4.11); `--line` and hairlines are seams, decorative (CR-M1.6-SYSTEM-001, owner 2026-11-17) | 3.0–3.4 / 3.0–3.8 |

## 3. Theme generation

Themes are **generated**, not hand-edited (research principle 2; Linear's
three-input model). `apps/desktop-tauri/scripts/gen-themes.mjs` (no dependencies)
reads `src/styles/themes.json`:

```json
{ "dark":  { "base": { "hue": 70, "chroma": 0.006 }, "tone": "warm", "contrast": 1.0,
             "accent": { "hue": 80, "chroma": 0.10, "lightness": 0.74 } },
  "light": { "base": { "hue": 75, "chroma": 0.008 }, "tone": "warm", "contrast": 1.0,
             "accent": { "hue": 78, "chroma": 0.11, "lightness": 0.60 } } }
```

and writes the primitive and semantic blocks of `tokens.css` between generated
markers as `#rrggbb` (Monaco and the guard test need hex). Surface tiers are
fixed OKLCH lightness offsets from the base. Text tiers are solved for their minimum
contrast against the worst surface they appear on. State hues are fixed per theme
and only lightness-clamped. A `--check` mode fails when `tokens.css` is stale, and
`scripts/check.sh` runs it, like schema freshness.

**As built (M1.6-SYSTEM-001).** `gen-themes.mjs` expresses each tier as an offset from
the inputs (neutral: absolute OKLCH lightness, chroma × base chroma, hue offset; brass:
offsets from the accent input), solves the text tiers and `--line-input` by contrast, and
clamps the state hues used as text to 4.5:1 on the editor and panel. Calibration against
§2.2 (`tests/tokens.test.ts` asserts ΔE2000 < 2 for every documented value): dark
reproduces all of them exactly; light differs only where contrast moved it:

| Token (light) | Generated | Documented | ΔE2000 | Why |
|---------------|-----------|------------|-------:|-----|
| `--text` | `#1d1d1b` | `#1e1d1b` | 0.74 | solved to its contrast target |
| `--text-muted` | `#5a5852` | `#5b5852` | 0.62 | solved |
| `--text-faint` | `#83807a` | `#84807a` | 0.57 | solved |
| `--ok-text` | `#3e7935` | `#3f7a36` | 0.37 | clamped: the documented value is 4.44:1 on the panel |
| `--danger` | `#c23a34` | `#c43c35` | 0.65 | clamped: 4.44:1 on the panel |

Recorded exceptions and choices: `--warn` (light) is seeded with the documented
`#a8520f`, not the prototype's `#c2621d`; `--run` is `var(--accent)` in both themes
(§2.2 says so; the prototype's dark `--run` was the focus brass `#d2ac63`); the brass
scale has steps 300–800 in dark and 500–800 in light (light needs no lighter steps);
until M1.6-QA-006 the legacy alias `--accent-line: var(--accent)` shadows the
generated translucent `--accent-line`, and `--bg-hover`/`--scrollbar` alias opaque
tiers because `src/editor/surface.ts` appends an alpha byte to their hex.

M1.6 ships two themes. User theming (FL's hue/saturation/brightness/contrast,
Bitwig's midtone/black level) becomes a later task that edits these inputs. **No
theme editor UI in M1.6.**

## 4. Material grammar

Every visible box is exactly one of three materials. Reviewers reject anything
that mixes them.

| Material | Means | Recipe (dark) | Used by |
|----------|-------|---------------|---------|
| **Raised** | You can press it | `linear-gradient(--surface-raised-1, --surface-raised-2)`, 1 px `--line` border, `inset 0 1px 0 --edge-strong`, `0 1px 1px rgb(0 0 0/.22)` | Buttons, keycaps (`Kbd`), the active tab, switch thumbs, the segmented glide |
| **Recessed** | It holds a value or accepts input | `--surface-well`, 1 px `--line-input` border on fields that accept input (`--line` on meters and other displays), `inset 0 1px 2px rgb(0 0 0/.28)`, outer `0 1px 0 --edge` | Fields, the command field, the task transport, the console prompt, the editor (by tone), **engaged toggles**, the switch track, meters |
| **Flat** | Content or structure | Surface tone only; separated by `--line` (between regions) or `--line-mid` (inside) | Panels, headers, lists, rows, the status bar, menus' items |

Pressing a raised control moves it toward recessed: the gradient inverts, the inset
shadow appears, and it translates 0.5 px down over 60 ms. An **engaged** toggle
(`aria-pressed="true"`) stays recessed. Overlays are flat surfaces lifted by the one
popover shadow (`0 12px 32px rgb(0 0 0/.55), 0 2px 6px rgb(0 0 0/.35)`) plus
`inset 0 1px 0 --edge-strong`; no blur.

## 5. Typography

| Token | Face | Size / line | Weight | Use |
|-------|------|-------------|--------|-----|
| `--text-title` | Plex Sans | 15 / 20 | 600 | Dialog titles, page titles |
| `--text-body` | Plex Sans | 13 / 18 | 400, 500 for emphasis | Default UI text, tree rows, menu items |
| `--text-small` | Plex Sans | 12 / 16 | 400–600 | Panel headers (600), metadata, status bar |
| `--text-tiny` | Plex Sans | 11 / 14 | 500 | Badges, counts, keycaps, captions |
| `--code-size` / `--code-leading` | Plex Mono | 13 / 20 (user setting overrides size) | 400 | Editor |
| `--out-size` / `--out-leading` | Plex Mono | 12 / 18 | 400 | Console, diffs, paths in chrome |

**As built (M1.6-SYSTEM-002).** The faces are bundled as `"Axiom Plex Sans"` (400, 500, 600 and
400 italic) and `"Axiom Plex Mono"` (400, 500 and 400 italic): IBM's own unmodified `Latin1`
split WOFF2 files (Plex Sans 1.1.0, Plex Mono 2.5.0; 143,096 bytes in all; provenance and
checksums in `apps/desktop-tauri/src/assets/fonts/ibm-plex/SOURCE.md`), declared in
`src/styles/fonts.css` with IBM's own `unicode-range`. The UI face uses `font-display: block`
and the code face `swap`, after which Monaco remeasures. The `Pi` split (arrows, maths
symbols) is not bundled; those characters (e.g. ✔ ✖ in the console) come from the system
fallback. `--font-ui` and `--font-code` are the families above followed by the fallback
stacks below.

Rules: sentence case everywhere (no all-caps labels); `font-variant-numeric:
tabular-nums` for times, counts and line/column; paths and commands in mono;
truncation with an ellipsis plus `title`/tooltip; the user's editor font
setting applies to the editor and console only. Fallback stacks if D2 is rejected:
`system-ui, "Segoe UI", "Cantarell", "Noto Sans"` / `ui-monospace, "Cascadia Code",
"SF Mono", Menlo, Consolas, "DejaVu Sans Mono"`.

## 6. Space, density, shape

| Token | Value | | Token | Value |
|-------|------:|-|-------|------:|
| `--s-1…5` | 4, 8, 12, 16, 24 | | `--row-h` | 22 |
| `--control-h` / `-sm` | 26 / 22 | | `--indent` (tree) | 14 |
| `--title-h` | 38 | | `--tab-h` | 32 |
| `--head-h` (panel header) | 30 | | `--crumb-h` | 24 |
| `--status-h` | 24 | | `--prompt-h` | 30 |
| `--ibtn` | 26 | | `--icon` | 16 (rail 18) |
| `--r-1` controls | 3 | | `--r-2` small surfaces, fields | 4 |
| `--r-3` overlays, dialogs | 6 | | panels | 0 |
| `--focus-w` | 2 | | `--led` | 7 |

Minimum hit target 22×22 (WCAG 2.5.8 AA 24×24 is met by spacing: rows are
full-width, and icon buttons are 26). Splitters have a 1 px visible seam and a
7 px hit zone.

## 7. Iconography

16 px grid; 1.35 px stroke; round joins; terminals end on the pixel grid; no
fills except play/stop; `currentColor`; `aria-hidden` (the control carries the
name). File types are shown as **glyph chips** (two-letter mono label on a
12–17% tint of a syntax hue: `rs` clay, `md` steel, `tm`/`lk` verdigris,
others neutral). The Axiom mark is a precision triangle with a crossbar notch,
drawn in brass. M1.6 adds only icons used by M1 surfaces; the set lives in
`src/ui/icons.tsx` (no icon library).

## 8. Focus and interaction states

| State | Visual | Applies to |
|-------|--------|------------|
| Hover | `--hover` overlay; raised controls lighten 10% | all interactive |
| Pressed (active) | raised → recessed recipe, `translateY(.5px)`, 60 ms | raised |
| Engaged (`aria-pressed`, `aria-checked`) | recessed, text `--text` | toggles, segmented, switch (brass track) |
| Selected (`aria-selected`) | `--selected` + 2 px left marker (`--text-faint`; `--accent` when the list has focus) | tree rows, palette items, sessions |
| Focus-visible | 2 px `--focus` outline, 1 px offset (controls); inset 1 px `--accent-line` (rows inside a focused list) | everything focusable |
| Focused region | the header of the panel containing focus gets a 2 px brass rule on its top edge | panels |
| Disabled | 42% opacity, no hover, `aria-disabled` or `disabled` | controls |
| Loading/busy | stepped spinner in place of the leading icon, label kept, `aria-busy` | buttons, rows |
| Invalid | `--danger` border + ring, message via `aria-describedby` | fields |
| Running / live | LED `--run` (stepped pulse per `motion-system.md` §5) | transport, status |
| Attention | LED or count in `--attention` | rail/dock badges, tabs with conflicts |

Nothing is conveyed by colour alone: dirty = dot replacing the close glyph and
"(unsaved)" in its name; stderr = gutter glyph plus a hidden prefix; LEDs always
come with text or an `aria-label`.

## 9. Components

"Built in M1.6" follows governance rule 9: a component is implemented only when an
M1 surface uses it. The others are specified here and prototyped in
`experiments/e5-visual-concepts`.

| Component | Purpose | Variants / states | Keyboard and a11y | Built in M1.6 (task) |
|-----------|---------|-------------------|-------------------|----------------------|
| `Button` | Commands | default, primary, quiet, danger, danger-quiet; sm; all §8 states; loading | native `<button>`; primary is the dialog default | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/button.css` |
| `IconButton` | Icon-only commands and toggles | plain, toggle (`aria-pressed`) | label mandatory (type-enforced) → `aria-label` + Tooltip | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/icon-button.css` |
| `Kbd` / `Keys` | Shortcut display | single, chord | `<kbd>`; platform-aware (`⌘`/`Ctrl`) | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/kbd.css` |
| `Badge`, `Count` | Labels, counts | neutral, accent, **mock** (prototype only) | text, never colour only | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/badge.css` |
| `Led` | Live state | off, on (ok), run, warn, err | `role="img"` + `aria-label`, or decorative next to text | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/led.css` |
| `Meter` | Proportion or rate | determinate bar; **rate meter** (16 bars) | `role="progressbar"`/`img` with label | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/meter.css` |
| `Spinner`, `Activity` | Indeterminate progress | stepped spinner; no sweep in production | `aria-busy` on the owner | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/spinner.css` |
| `Field` | Text input | default, mono, invalid, disabled | label or `aria-label`; error via `aria-describedby` | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/field.css` |
| `Switch` | Boolean setting | on/off/disabled | `role="switch"`, `aria-checked`, Space toggles | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/switch.css` |
| `Segmented` | 2–4 exclusive options | gliding thumb | radiogroup; arrow keys move and select | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/segmented.css` |
| `Select` | Choice from a list | button + listbox | Radix Select only if a surface needs it | ❌ (no M1 use) |
| `Stepper` | Numeric setting | − field + | buttons named; field typeable; Up/Down | ✅ built (SYSTEM-003): `src/ui/primitives.tsx`, `src/styles/components/stepper.css` |
| `Menu`, `ContextMenu` | Commands in a popup | items, shortcut, heading, separator, disabled | Radix DropdownMenu (existing); output context menu ported to the same styling | ✅ SYSTEM-004 |
| `Tooltip` | Names and shortcuts for icon controls | 500 ms delay, instant when moving between tooltips | Radix Tooltip (D8); also shown on focus | ✅ SYSTEM-004 |
| `Dialog` | Blocking decision | explicit buttons, dismiss action | Radix Dialog: focus trap, Escape, initial focus on the safe default | ✅ SYSTEM-004 |
| `Sheet` / settings dialog | Non-blocking configuration | two-pane (sections and body) | as Dialog; section nav is a tablist | ✅ SURFACES-002 |
| `Toast` | Transient confirmation or result | ok, error, with an action | `role="status"`/`alert`; never steals focus; 4 s, paused on hover or focus | ✅ (task finished while console hidden; save failed) SYSTEM-004 |
| `Banner` | Persistent problem in context | danger, warning, info + action | `role="alert"`/`status` | ✅ restyle |
| `Panel` | Region frame | header (title, meta, tools), body | `<section aria-label>`; header marks focused region | ✅ SHELL-002 |
| `Splitter` | Resize regions | vertical, horizontal | `role="separator"`, arrow keys ±16 (Shift ±48), Home resets; double-click resets | ✅ SHELL-002 |
| `Tree` | Explorer | rows, guides, glyphs, ignored, selected, focused, dirty dot | full tree keyboard model (existing) | ✅ SHELL-003 |
| `Tabs` (editor) | Open documents | recessed strip, raised active key, lit-edge glide, dirty, hover close | tablist; Ctrl+Tab, Ctrl+PgUp/PgDn, Ctrl+W, middle-click | ✅ EDITOR-001 |
| `Breadcrumbs` | Path of the active document | path segments (symbols need LSP: M4) | `nav` + `aria-label` | ✅ EDITOR-001 |
| `TitleRow`, `CommandField`, `Transport` | App chrome | `workspace-spec.md` §2 | | ✅ SHELL-001 |
| `StatusBar` + `HintLine` | State and discoverability | segments; hint with keycaps | `role="status"` for state; the hint is **not** live (no announcements on hover) | ✅ SHELL-004 |
| `Palette` | Commands and files | modes, groups, glide, keycaps, highlights, footer | combobox + listbox + `aria-activedescendant` | ✅ SURFACES-001 |
| `ActivityRail` | Top-level destinations | icons, active marker, badge | `nav`, `aria-current="page"` | ❌ until M2 (D4) |
| `AgentLog`, `ToolRow`, `ApprovalCard`, `DiffBlock`, `Checkpoint`, `Composer` | AI workspace | prototype | `workspace-spec.md` §5.2 | ❌ M3 |
| `EnvTable`, `InstallProgress`, `DiagnosticRow` | Environment Manager | prototype | `workspace-spec.md` §5.1 | ❌ M2 |

## 10. Usage constraints

- Never more than one primary button per surface. Danger buttons never sit next to
  the primary in the default-focus position.
- The accent never decorates. If removing a brass element loses no information, it
  must be removed.
- Panels are never rounded, carded or shadowed. Overlays are the only elevated
  surfaces.
- Do not stack tinted surfaces (e.g. a soft-accent card inside a soft-accent card).
- Long lists are virtualised (existing explorer and output); restyling must keep
  fixed row heights.
- No `backdrop-filter`, `filter`, `mix-blend-mode` or animated `box-shadow` in production.
