# UI and UX report

Based on the rendered app (debug build, private compositor, 1440×900 and 1180×760 at 1×, plus
900×560, 1024×700, 800×600, 640×480, dpr 2 and text scale 1.5), the committed design docs
(`docs/design/`, ADR-0013, ADR-0020 §4) and the reviewers' code reading. Screenshot names refer
to `evidence/screenshots/`. Objective defects carry an issue id; design opinions are labelled.

The intended identity — dark, quiet, IBM Plex, a single brass accent, the Tauri shell's
layout (owner's direction: the Flutter shell should look like the Tauri design) — is executed
consistently. Recommendations below refine that identity; none proposes a different one.

## Overall assessment

- **Strengths.** Coherent palette and type in both themes (`02-welcome-dark.png`,
  `02-welcome-light.png`); restrained motion; compact, legible editor and diff
  (`08-editor-main-rs.png`, `26-scm-diff.png`); good accessible naming (0 unlabelled
  actionable semantics nodes in the full exploratory tree); dialogs are clear and keyboard
  operable (`15-close-dirty-prompt.png`).
- **Weaknesses.** Layout robustness: several surfaces break at sizes the window allows
  (AUD-004, AUD-014, AUD-016, AUD-047) or under text scaling (AUD-020). Two visible
  correctness defects in the editor chrome (Git gutter AUD-003, active tab off-screen
  AUD-017). Missing everyday affordances (file create/rename/delete, Edit menu, context menus:
  AUD-022, AUD-073). Empty states explain the architecture rather than guiding the person
  (AUD-110).

## Screen-by-screen

### Title row, menus, status bar
- `03-file-menu.png`: File has Open Folder, Close Folder, Save, Save All, Close Editor — no New
  File, Save As, Recent, Quit (AUD-022). There is no Edit menu; "Preferences" is a top-level
  menu (AUD-111, design opinion). Disabled items keep full-contrast key caps — reads as enabled
  at a glance (P3, part of AUD-084).
- Menus never flip or clamp (AUD-013, code); at 1440×900 they fit.
- Status bar: hint line shows stale text after a control unmounts (`repro-R3-tabs-16.png`:
  "Restart the Backend", AUD-064). At 640 px the title row overflows (below the 900 px minimum,
  so not reachable — recorded only in `explore-results.json`).

### Welcome / no folder (`02-welcome-*.png`, `04-nofolder-*.png`)
- The hero block is left-aligned inside a centred 460 px column, so it reads ~95 px left of
  centre; copy "Axiom opens folders through its backend, axiomd." is implementation detail;
  no recent folders on the main area (the side bar lists recents only once some exist); two
  identical Open Folder buttons (AUD-110, design opinion).
- Source Control, Search, Environments, Inspector without a folder each show a different
  heading treatment (AUD-084).

### Explorer (`05-folder-open.png`, `06-explorer-many.png`, `07-explorer-deep.png`)
- 3 000-entry folder: smooth (frames.sh explorer fling 0 over).
- Deep nesting overflows; the leaf row is blank (AUD-016).
- Every row is a Tab stop (AUD-021). No context menu (AUD-022).
- Git colours and the "U"/"M" letters are clear; the folder dot for changes is subtle but
  readable.

### Editor (`08`, `09`, `10`, `12`, `14`, `18`, `repro-R1*`)
- Git gutter marks offset by half the editor height (AUD-003) — the most visible defect.
- Tabs: ~170 px each, only 6 fit at 1180 px; active tab not scrolled into view, no overflow
  affordance (AUD-017).
- Banners stack above the text (error + "Rust has a language server… Install… / Not now"),
  pushing code down 70 px; the error banner persists across documents (AUD-063).
- Long line (20 000 chars): renders, horizontal scroll works (`12-long-line.png`).
- Binary file: clear message (`13-binary.png`) but as a persistent pane banner.
- Line numbers sit ~2 px above the code baseline (AUD-084).
- Split at small widths throws (AUD-004, `repro-R5-split-min-window.png`).

### Dialogs (`15`, `17`, `repro-R8-save-after-delete.png`, `35-settings.png`)
- Well-composed, with dimmed backdrop; default focus on "Overwrite" in the conflict dialog is
  unsafe (AUD-018); deleted files reuse the "changed on disk" wording (AUD-019).
- Settings: fixed 820×600 sheet; clips at the 900×560 minimum (AUD-047); does not follow its
  own theme switch (AUD-046).

### Palette and Go to File (`19`–`22`)
- Clean and fast at 3 000 files (go-to-file query rendered in < 1 s in debug). Command order is
  arbitrary (AUD-111). No focus trap (AUD-048). Six frames over budget on open/close
  (AUD-062).

### Search (`23`, `24`, `24b`)
- Results are readable; option toggles compact. Include/exclude placeholders in the mono face
  while the query uses Sans (AUD-084). Truncation message unclear (AUD-067). Invalid regex
  handled without crash.

### Source Control and diff (`25`, `26`, `38-scm-light.png`)
- Good side-by-side diff with aligned gaps; light theme diff is pleasant. The diff stays in the
  main area after leaving Source Control (`33-command-output.png`), which surprised during
  testing — consider closing it when switching to Files, or showing it as a tab (design
  opinion). Long lines are cut off (AUD-058).

### Environments (`27-environments.png`)
- Informative but long; offers Install for a toolchain already Ready (AUD-068); side-bar rows
  are inert (AUD-056).

### Chat and AI dock (`04-nofolder-chat.png`, `29-chat.png`, `30-dock.png`, `repro-R4/R5`)
- Setup page: very wide rows with buttons ~900 px from labels; install commands as prose
  (AUD-110). In the dock at its default 380 px — even in a 1440 px window — the status text
  truncates to "Not installed…." and the install instructions are lost (`30-dock.png`,
  `repro-R5-split-min-window.png`); the setup layout needs a narrow variant (stack label,
  status and button).
- Composer is enabled before a model is chosen ("Choose a model" link) — acceptable, but the
  Send action should explain what happens.

### Console, Output, Terminal (`31`–`34`)
- Output shows exit status clearly ("Failed with exit code 3 in 4 ms"). Terminal background
  differs from its panel with a seam (AUD-084). At small widths xterm reflow duplicated the
  prompt line (`40-size-640x480.png`; below the minimum window, not filed).

### Failure states (`48-folder-deleted.png`, `49-backend-died.png`, `50-after-restart.png`)
- "The backend stopped" page is clear, with the exit code, but restarting silently loses unsaved
  work (AUD-001). Folder-deleted message leaks internal wording and misaligns its action
  (AUD-066).

## Interaction problems (summary)
AUD-017 (tabs), AUD-018 (unsafe default), AUD-019 (deleted file), AUD-021 (Tab stops),
AUD-022 (no file ops), AUD-063 (sticky banner), AUD-064 (stale hint), AUD-073 (no context
menus), AUD-074 (console view state lost), AUD-077 (hover docs pointer-only).

## Layout issues (objective)
AUD-004, AUD-014, AUD-016, AUD-020, AUD-047, AUD-053, AUD-058, AUD-059, AUD-078, AUD-079
(header), AUD-013/051/052 (overlay placement).

## Motion and animation
- Motion is restrained and fits the "precision instrument" brief. Rail indicator springs,
  palette fade, dialog scale.
- Reduced motion is incomplete and the OS preference is ignored (AUD-054); "Off" did suppress
  the palette transition in the exploratory run (`51-palette-motion-off.png`).
- Loops do not pause when unfocused (AUD-080); toasts have no motion and stack without limit
  (AUD-081).
- Palette open/close is the only animation measured over budget (AUD-062).

## Accessibility observations
- Names and roles: good coverage (0 unlabelled actionable nodes); `a11y_test.dart` passes in
  both themes.
- Keyboard: Tab stops per row (AUD-021); palette focus not trapped (AUD-048); hover docs
  pointer-only (AUD-077); some controls without focus rings (AUD-070); `Pressable` semantics
  flags (AUD-071).
- Text scaling clips (AUD-020). High-DPI (dpr 2, `41-dpr2.png`) renders crisply.
- Contrast is tested for tokens (`test/ui/tokens_test.dart`), but alpha tints are not
  (AUD-084).
- Screen-reader noise from live regions (AUD-082).

## Prioritised design recommendations

| # | Recommendation | Issues | Kind |
|---|----------------|--------|------|
| R-1 | Make layout space-aware: one resolver for side bar / editor / dock / console with an editor minimum, applied to splitters, split groups, Settings, find bar and dialogs | AUD-004, 014, 047, 053 | Defect |
| R-2 | Fix editor chrome geometry: gutter strip full height; active tab always visible with wheel scroll and an overflow list; explorer indent cap | AUD-003, 017, 016, 059 | Defect |
| R-3 | Welcome: centre the block; replace architecture copy with actions (Open Folder, recent folders with paths, "Clone repository" only if supported), keep a single primary button | AUD-110 | Opinion |
| R-4 | Chat setup: a two-column card grid (agent, status, action together); install commands in copyable mono chips | AUD-110 | Opinion |
| R-5 | Safe defaults and honest wording in dialogs: Cancel default for conflicts, a "Compare" action, a distinct "deleted on disk" dialog | AUD-018, 019 | Defect |
| R-6 | Everyday affordances: explorer context menu (New File/Folder, Rename, Delete to trash, Reveal, Copy Path), Edit menu, text-field context menu | AUD-022, 073 | Product gap (owner decision) |
| R-7 | Scale rows with text scale; honour OS reduced motion | AUD-020, 054 | Defect |
| R-8 | Tokenise the stragglers: one side-panel heading style, one gutter, radii and spacings on the scale, alpha tints in the contrast test, mono only for code/paths | AUD-084 | Opinion (values objective) |
| R-9 | Palette: group by category with recently used first; announce selection | AUD-111, 048, 082 | Opinion + defect |
| R-10 | Banners: per-document, dismissible, collapse the LSP offer into the status bar after "Not now" | AUD-063 | Defect + opinion |

None of these changes the product's identity; R-3, R-4, R-6, R-8, R-9 need the owner's approval
before implementation (the brief forbids broad design changes without it).
