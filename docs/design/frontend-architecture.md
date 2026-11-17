# Frontend architecture for M1.6

_Status: **approved by ADR-0013** (2026-11-17). Applies to `apps/desktop-tauri/src/`. Governing
documents: ADR-0012, `docs/architecture/modules.md` §apps/desktop-tauri,
`governance.md`._

## 1. Verdict on the existing architecture

The M1.5 shell's structure is right for M1.6 and is **kept**:

- external stores (`src/state/*`) own presentation state and protocol sequencing,
  and components subscribe with `useSyncExternalStore`;
- the webview never touches files or processes; it calls host commands;
- plain CSS over custom properties, with a token guard test;
- Radix primitives only where accessibility is non-trivial;
- Monaco themed from tokens at runtime.

M1.6 changes the **presentation layer only**. No change to `src-tauri/` except
window-chrome configuration if D3 is approved, to `src/daemon/`, the protocol,
`editor/src/` or the stores' behaviour. Two small, additive store fields are
allowed (§5).

## 2. Target file layout

```
src/
  styles/
    themes.json          NEW  generator inputs (design-system.md §3)
    tokens.css           REWRITTEN  primitives + semantic (generated blocks) + component + motion tokens
    base.css             NEW  reset, body, focus, scrollbars, reduced-motion and data-motion switches
    materials.css        NEW  .raised / .recessed recipes as shared declarations (no colours)
    components/*.css     NEW  one file per component (button.css, tree.css, tabs.css …), split out of app.css
    shell.css            NEW  layout grid of the window (title row, workbench, status bar)
    app.css              REMOVED (content moves to the files above)
  ui/                    design-system components (no store imports)
    primitives.tsx       Button, IconButton, Kbd/Keys, Badge, Led, Meter, Spinner, Field, Switch, Segmented, Stepper
    overlays.tsx         Menu, ContextMenu, Tooltip, Dialog, Sheet, Toast, Banner
    panel.tsx            Panel, Splitter
    motion.ts            usePresence, useLoopGate (pause loops when hidden/unfocused), motion level
    hint.ts              hint registry: data-hint → HintLine
    icons.tsx            icon set (extended only by what is used)
  shell/                 NEW  app chrome (imports stores, composes ui/)
    TitleRow.tsx         menus, workspace switcher, CommandField, Transport, layout toggles
    StatusBar.tsx        segments + HintLine
    Workbench.tsx        regions, splitters, layout persistence (moved out of App.tsx)
    shortcuts.ts         application key map (moved out of App.tsx, unchanged behaviour)
    commands.ts          palette command list (moved out of App.tsx)
  components/            feature views (existing names kept)
    Explorer.tsx, EditorArea.tsx (tabs + breadcrumbs), OutputPanel.tsx (console),
    CommandPalette.tsx, Dialogs.tsx (+ Settings), Welcome.tsx, MonacoHost.tsx, ErrorBoundary.tsx
  editor/surface.ts      theme generator extended to --syn-* and --editor-* tokens
  App.tsx                composition only (target < 120 lines)
gallery/                 NEW  dev-only component gallery (vite entry `gallery.html`, not in `dist`)
```

**Shell split: done (M1.6-SHELL-001, 2026-11-17).** `App.tsx` is the composition root
(117 lines) and `src/shell/` holds `TitleRow.tsx`, `StatusBar.tsx`, `Workbench.tsx`,
`shortcuts.ts`, `commands.ts`, plus `actions.ts` and `platform.ts`. Deviations from the
tree above: `actions.ts` (the `ShellContext` interface, `runMenuAction` and the
`axiom://menu` hook) and `platform.ts` (`IS_MAC`, `MOD`, `useSystemDark`) are additional
modules; `Workbench.tsx` also exports `useLayout()` (the layout state, shared with the
title row and the shortcuts) and `Banners` (the strip above the workbench);
`commands.ts` exports `allCommands` (with enabled flags), `buildCommands`,
`buildFileCommands` and the memoising `usePaletteCommands`.

**Dependency direction** (enforced by a Vitest import-graph test in
SYSTEM-003): `ui/` imports nothing from `state/`, `shell/`, `components/` or
`@tauri-apps/*`; `components/` and `shell/` may import `ui/` and `state/`; only
`daemon/` and `shell/` may import `@tauri-apps/*`. This keeps the design system
free of Tauri and stores, so a future GPUI or other front end can reuse the
specification and tokens, and tests can render `ui/` in isolation.

## 3. Styling approach

- **Plain CSS + custom properties**, one file per component, imported by that
  component. Class names stay BEM-like (`.tab`, `.tab__close`); state is expressed
  through ARIA and `data-*` attributes (`[aria-selected="true"]`,
  `[data-phase="out"]`), never through extra state classes, so styling and
  accessibility cannot disagree.
- No CSS-in-JS, no Tailwind, no CSS modules (re-evaluated: modules would hide the
  ARIA-attribute selectors behind generated names and complicate the guard test;
  the component count, about 30, does not need them).
- Themes: `[data-theme]` on `<html>` (existing). Motion: `[data-motion]` on `<html>`.
  Window idle: `[data-idle]` on `<html>`.
- The token guard test is extended (SYSTEM-001): no literal colours outside
  `tokens.css` (existing); every theme defines every semantic token (existing); the
  §2.3 contrast pairs of `design-system.md`; no `transition`/`animation` on layout
  properties; no `backdrop-filter`/`filter`; every `animation` uses a duration token.

## 4. Libraries

| Need | Decision |
|------|----------|
| Dialog, DropdownMenu | Radix (existing, MIT) |
| Tooltip | `@radix-ui/react-tooltip` (MIT) if D8 is approved |
| Context menu | `@radix-ui/react-context-menu` (MIT), replacing the hand-rolled output menu, if a second context menu (explorer) is in scope; otherwise keep the existing one, restyled |
| Animation | **CSS only.** Framer Motion / Motion One rejected: JS-driven animation runs on the main thread that also handles typing, and adds 30–60 kB |
| Icons | hand-drawn set (existing approach); no icon library |
| Fonts | IBM Plex Sans/Mono WOFF2 files vendored under `src/assets/fonts/` with their OFL text (D2); not `@fontsource` at runtime |
| Virtualisation | existing hand-written virtual lists (explorer, output) |

Each new dependency is recorded in the task's completion report with its licence
(governance rule 1).

## 5. Store additions (presentation data only)

| Store | Field | Source | Used by |
|-------|-------|--------|---------|
| `TasksStore` | `startedAt: number \| null` | `TaskInfo.started_ms` in the `task/run` response (existing protocol field) | transport elapsed time |
| `TasksStore` | `outputSeq: number` (monotonic row counter including dropped) | incremented where rows are appended | rate meter (sampled by the view at 4 Hz) |

No other store changes. The view computes elapsed time with one `setInterval` of
1 s while a task runs (text only; no animation). The meter samples `outputSeq`
every 250 ms.

## 6. Platform integration

| Concern | Approach |
|---------|----------|
| Window chrome (D3) | macOS: `titleBarStyle: "Overlay"`, `hiddenTitle: true` in `tauri.conf.json`; title row gets `data-tauri-drag-region` on its non-interactive parts and a 72 px left inset when `navigator.platform` is Mac. Windows/Linux: native decorations; `Window.setTheme(dark/light/null)` follows `appearance.theme` (needs the `core:window:allow-set-theme` capability: a capability change, recorded in `security.md`) |
| Fonts per OS | bundled faces make metrics identical; `-webkit-font-smoothing: antialiased` only on macOS (`data-os="mac"`), because WebKitGTK and WebView2 ignore or misapply it |
| Scaling | everything in CSS px, so the OS scale factor applies; screenshots are reviewed at 100% and 200% (`visual-qa.md`) |
| Shortcuts | `MOD` = ⌘ on macOS, Ctrl elsewhere (existing); keycaps render per OS |
| High contrast | Windows `forced-colors: active`: components use system colours via a `@media (forced-colors)` block (focus rings, borders); verified in QA-003 |
| Reduced motion | `prefers-reduced-motion` + `appearance.motion` (motion-system.md §2) |

## 7. Testing hooks

- `data-testid` attributes used by `e2e/*.mjs` are preserved (or renamed together
  with their e2e checks in the same task); the 51 Vitest tests and 51 e2e checks
  must stay green in every task.
- Every component in `ui/` gets a gallery entry (states × themes) for screenshots
  (`visual-qa.md` §3).
- `window.__axiomMotion` (dev and e2e builds only) exposes the motion level and the
  running-animation count for assertions.
