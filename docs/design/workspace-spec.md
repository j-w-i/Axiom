# Surface specifications

_Status: **approved by ADR-0013** (2026-11-17). Tokens and
components: `design-system.md`. Motion: `motion-system.md`. Every number is a token
value at 100% zoom. Screens: `concepts/r-*.png`._

Scope legend: **M1.6** = built in this milestone (uses existing M1 functionality
and data). **Designed** = specified and prototyped here, built by its own
milestone. Nothing marked Designed may appear in the production shell before then
(governance rule 9).

## 1. Window and layout model

```
┌ title row 38 ─────────────────────────────────────────────────────────────────┐
│ ▲ Axiom  File Edit View Run Help │ axiom ~/src/axiom ▾ ··· [⌕ Search files, run commands  Ctrl K] ··· [● cargo test  00:12.4 ▮▮▯ ↻ ■] [▣][▤][▥] │
├ side bar (180–480, default 252) ┬─────────────── editor column ───────────────┬ dock (M3) ┤
│ Files · axiom              ⌄  │ tab strip 32                                │          │
│ tree, rows 22                 │ breadcrumbs 24                              │          │
│                               │ Monaco (deepest well)                       │          │
│                               ├──────────── console (120–520, def. 232) ────┤          │
│                               │ header 30: [Output] ● task · time   ⧉ ⌫ ✕  │          │
│                               │ output rows 18                              │          │
│                               │ $ prompt 30                         ■ Stop  │          │
└───────────────────────────────┴─────────────────────────────────────────────┴──────────┘
  status bar 24: ● axiomd 0.1.0 │ ◌ task summary │ hint line ……… │ ⚠ 1 │ Ln 84, Col 17 │ Spaces: 4 │ UTF-8 │ LF │ Rust
```

- Minimum window 720×480 (existing). Below 900 px wide, the side bar starts hidden
  (existing rule) and the command field shrinks to an icon button (≤ 1040 px).
- Layout state (side bar width and visibility, console height and visibility; later
  the dock) stays **shell-local** in `localStorage` key `axiom.layout.v1`, extended
  additively (`axiom.layout.v2` with migration if keys change). "Reset Layout"
  (palette, View menu) restores defaults.
- **As built (M1.6-SHELL-002).** Limits unchanged: side bar 180–480 (default 252), console
  120–520 (default 232), both also clamped to the window (side bar ≤ window − 400, console ≤
  window height − 260). `axiom.layout.v2` holds the same four keys as v1; v1 is migrated once and
  removed. Splitters: arrows ±16, Shift ±48, Home and double-click reset to the default. The
  layout lives in a small store (`shell/layout.ts`) so a resize commit re-renders only the
  workbench (one React commit per drag). The focused-region rule marks the side bar header,
  the editor's tab strip and the console header.
- One region owns keyboard focus. Its header shows the 2 px brass top rule
  (`design-system.md` §8). Ctrl+0 focuses the side bar (if visible), Ctrl+1 the
  editor, and Ctrl+\` the console prompt. These are new, conflict-checked against
  Monaco's bindings in EDITOR-002.

## 2. Main workspace (M1.6)

### 2.1 Title row (SHELL-001)

| Element | Spec | Data / action |
|---------|------|---------------|
| Mark + "Axiom" | brass mark 17 px, 13/600 | — |
| Menus | File, Edit, View, Run, Help as flat text buttons (12 px, muted → text on hover); Radix DropdownMenu | existing menu actions (`runMenuAction`) |
| Workspace switcher | name 13/600 + path mono 11 faint + chevron; opens the recent-folders menu and "Open Folder…" | `ws.name`, `ws.root`, `ws.recent` |
| Command field | recessed, `min(380px, 32vw)`, placeholder "Search files, run commands", `Ctrl K` keycaps; click opens the palette (commands) | palette |
| Task transport | recessed display: LED · mono label (ellipsis at 220) · elapsed (brass, tabular) · rate meter (16 × 2 px bars, 4 Hz) · Rerun · Stop. Idle: LED off, label "No task", elapsed hidden, Rerun shows the last command in its tooltip; Stop disabled | `tasks` store: `running`, `label`, `lastOutcome`, `history`; **new store fields** `startedAt` (from `TaskInfo.started_ms` of `task/run`) and `outputSeq` (row counter) — no protocol change |
| Layout toggles | IconButtons (toggle): side bar Ctrl+B, console Ctrl+J; engaged = recessed | layout state |
| macOS (D3) | `titleBarStyle: "Overlay"`, `hiddenTitle: true`; 72 px left inset for the traffic lights; the title row is the drag region (`data-tauri-drag-region` on non-interactive parts) | host config |
| Windows/Linux (D3) | native decorations; native title bar themed to match (Tauri `set_theme`) | host config |

**As built (M1.6-SHELL-003).** `startedAt` comes from `task/list`'s `TaskInfo.startedMs`
(`task/start` answers with the task id only in IPC 0.1; no protocol change). The elapsed time
shows `mm:ss` in 1 s steps (tenths would need a faster timer than the loop policy allows). The
menus are ordered File, Edit, View, Run, Help. The Settings icon button stays next to the layout
toggles. Narrow windows: at ≤ 1040 px the command field becomes an icon button and the
workspace path is hidden; at ≤ 860 px the rate meter is hidden and the task label shortens to
110 px, so every control stays reachable at the 720 px minimum. The status bar's spinner is static
while the transport is visible (one running indicator). Both timers skip their updates while the
window is idle (the loop gate's `data-idle`); the meter renders only when its bars change.

### 2.2 Side bar and explorer (SHELL-002, SHELL-003)

- Panel header: "Files" (12/600) + workspace name (muted) + tools (Collapse All,
  Refresh). Flat; 1 px `--line-mid` below.
- Rows 22 px; indent 14; 1 px indent guides at `--tree-guide`; chevron 13 px,
  rotating; folder icons neutral `--icon-folder`; files as glyph chips; ignored rows
  `--text-faint`.
- Selected: `--selected` + 2 px left marker (faint; brass when the tree is focused).
  Keyboard focus: inset 1 px accent ring on the focused row.
- Trailing status: unsaved dot for documents dirty in the editor (from the
  documents store: no new data). Problem counts arrive with M2.
- Empty and error states: §6.

### 2.3 Editor column (EDITOR-001, EDITOR-002)

- **Tab strip** (32, chrome tone): inactive tabs muted text on chrome, separated by
  `--line-mid`; close glyph visible on hover, focus or when active; dirty = dot
  (replaced by the close glyph on hover). The **active tab** is raised and merges
  into the breadcrumb/editor tone, with the lit brass edge (1.5 px line + 8 px soft
  falloff) gliding between tabs. Overflow: horizontal scroll with wheel, and the
  active tab is always scrolled into view. Reorder by drag (D7, INTERACTION-004).
- **Breadcrumbs** (24): path segments of the active document (12 px muted,
  last segment `--text`). Symbol segments require LSP (M4) and are not shown.
- **Monaco**: background `--surface-editor`; gutter same; line numbers faint /
  active muted; current line `--editor-line` (very low contrast); selection brass
  20%; cursor brass; indent guides `--editor-guide`; minimap kept (existing default);
  syntax from `--syn-*`. Font: the user's setting, or Plex Mono 13/20.
- **Empty editor** (workspace open, no document): §6.

### 2.4 Console (SHELL-006)

- Header (30): view tabs. **Output** is the only view in M1, and the strip is
  hidden while it is the only one (no placeholder Problems/Terminal; they arrive with
  M2/M4). Then a separator, the task LED, mono label and elapsed time (muted), and
  tools: Copy all, Clear, Hide (Ctrl+J).
- Output rows 12/18 mono, virtualised (existing). stdout `--text`; stderr
  `--text-muted` with a 1 px gutter glyph in `--warn` at 55% and the hidden
  "stderr:" prefix (D6); success status line `--ok-text` ✔; failure status line
  `--danger` ✖; notices faint.
- Prompt (30, recessed): `$` sigil in brass, mono input, history (↑/↓), Enter
  runs; while running, a quiet danger **Stop** at the right; otherwise **Run**
  (quiet). Focus-within draws an inset accent hairline.
- Context menu: Copy selection / Copy all (existing), restyled.

**As built (M1.6-SHELL-006).** The header has no view strip (Output is the only view); its meta
appears only when there is a task: while running, LED · label · elapsed (`mm:ss`, 1 s steps,
skipped while the window is idle); afterwards the last command with an on/err LED. The header
LED is static: the transport's LED is the one running indicator that pulses (motion-system.md
§5.3). Tools: Copy all output, Clear, Hide console (Mod+J). Empty output reads "Run a command in
this workspace". The header and its tools are memoised so an output batch re-renders only the
rows (P12). D6 adopted: stderr `--text-muted`, gutter `▏` in `--warn` at 55%; `--danger` is used
only by the failure status line and error rows. CSS in `styles/components/console.css`.

### 2.5 Status bar (SHELL-004)

Segments separated by `--line-mid` hairlines, 12 px muted text, hover overlay on
interactive segments:

| Segment | Content | Action |
|---------|---------|--------|
| Backend | LED (on/run while starting/err) + "axiomd 0.1.0" / "Connecting…" / "Backend stopped" | click: restart if stopped |
| Task | while running: stepped spinner + label; afterwards the last outcome (existing `statusText`, e.g. "✖ Failed with exit code 101 in 4.8 s"). Only one of the transport and this segment animates (`motion-system.md` §5.3): the spinner here is static when the transport is visible | click: show the console |
| **Hint line** (flex) | hovered or focused control's `data-hint`: "Action — Shortcut", shortcut as keycaps; idle: empty | none; `aria-live="off"` |
| Cursor | Ln, Col | (M4: go to line) |
| Encoding, language | "UTF-8" (as today) and the Monaco language of the active document; an EOL segment needs line-ending data the documents store does not expose yet, so it is omitted (no new store field in M1.6) | — |

**As built (M1.6-SHELL-005).** EOL and language segments are omitted: the documents store
exposes neither (no store change). The backend segment's accessible text keeps the prefix
"Connected — " (visually hidden) before "axiomd <version>". Only the backend and task segments
carry `role="status"`. Hints are written with `Mod` and rendered per platform (`ui/hint.ts`);
an `IconButton` without an explicit hint shows "Label — Shortcut". Final hint texts:

| Control | Hint |
|---------|------|
| File / Edit / View / Run / Help menus | "Open, save and close; settings" / "Undo, find and select in the editor" / "Panels, layout and theme" / "Run, rerun and stop commands" / "About Axiom; restart the backend" |
| Workspace switcher | "Switch to a recent folder or open another — Mod+O" |
| Command field (and its compact icon) | "Search files, run commands — Mod+K" |
| Transport Rerun / Stop | "Rerun the last command — F5" / "Stop the task — Shift+F5" |
| Layout toggles, Settings | "Toggle side bar — Ctrl+B", "Toggle panel — Ctrl+J", "Settings — Ctrl+," (⌘ on macOS) |
| Status: backend (stopped / ready), task | "Restart the backend" / "The Axiom backend is connected", "Show the console" |
| Panel tools | "Refresh explorer"; console "Copy all output", "Clear output" |
| Splitters | "Drag to resize — double-click resets" |
| Tabs, tab close | "<path> (unsaved) — Mod+Tab", "Close the document — Mod+W" |
| Console prompt, Run, Stop | "Run a command in the workspace — Enter", "Run the command — Enter", "Stop the task — Shift+F5" |
| Monaco | none (the hint clears while typing) |

The task segment shows only what the store knows; parsing test counts out of
output is **not** done (that is M2 diagnostics).

## 3. Command palette (SURFACES-001)

- Opens with Ctrl/⌘+K (commands) or Ctrl/⌘+P (files). Top 12% of the window,
  `min(600px, 100% − 32px)`, overlay material, scrim `--scrim` without blur.
- Input row 44: search icon, 15 px input, a mode chip at the right ("Commands" /
  "Files").
- Rows 32: group (60 px, faint) · label · keycaps right-aligned. File mode: glyph
  chip · name · directory (faint) with the matched span in brass/600.
- Selection: one highlight element gliding (`translateY`, 90 ms) with a 2 px brass
  left marker; mouse movement selects; Enter runs; Escape closes; Tab is trapped.
- Footer 30 (chrome): `↑↓ move`, `↵ run`, `Esc close`, and the other mode's key.
- Empty: "No matching commands" + "Try fewer letters"; file mode with no
  workspace: "Open a folder to search files" + the Open Folder… command row.
- Recent commands first (shell-local MRU list, max 8) when the query is empty.
- Budget: open → first painted frame ≤ 1 frame after the keystroke; filtering
  5,000 listed files ≤ 8 ms per keystroke (existing scorer; measured in QA-004).

## 4. Settings (SURFACES-002)

Two-pane dialog (`min(820px, 100% − 64px)` × `min(600px, 100% − 96px)`): section
nav (200) + body (max 640). Sections in M1.6: **Appearance** (theme
System/Light/Dark segmented; motion Standard/Reduced/Off), **Editor** (font size
stepper 8–32, font family field with the face's name as placeholder, word wrap
switch), **Workspace** (reopen last workspace switch). About moves to a section of
its own (versions, licences link). Each setting row: label 13/500, description
12 muted, control right-aligned; rows divided by `--line-mid`; min height 52.
Changes apply immediately (existing `settings.setValue`), errors show inline
under the row. Esc or "Done" closes. Agents (M3) and Environments (M2) sections are
**not** shown before their milestones.

## 5. Designed surfaces for later milestones (not built in M1.6)

### 5.1 Environment Manager (M2 epic 2.7) — `concepts/r-environments-dark.png`

- Opens as an editor-area page from the rail's Environments destination (the rail
  arrives with it, D4).
- Toolchain table: name + tools (faint), *required by project* (mono, with the file
  that requires it), detected version, source (managed / system / delegate path), and a
  state column: Ready (sage check), **Plan install…** (primary), or install progress
  (determinate meter + current step, e.g. "Verifying SHA-256 · cmake-…tar.gz").
- Install plans open as a dialog listing every step (downloads with size and
  checksum source, elevation required or not) with **Install** / Cancel. Nothing
  installs without this approval (M2 contract).
- Diagnostics list below: severity glyph · message · code · `file:line:col`
  (click → editor). The same rows become the console's **Problems** view.
- Builds run through the same transport as tasks.

### 5.2 AI workspace (M3 epic 3.8) — `concepts/r-workspace-dark.png` (right dock)

- Dock on the right (`--dock-w` 352, resizable 300–640), toggled with Ctrl+L.
- Session tabs: LED (waiting = attention) + title + badge for pending approvals.
  Concurrent sessions are tabs, and a session waiting on you raises the rail/dock
  badge.
- Log: the user's message (recessed well); the assistant's prose (flat); **tool
  rows** as one-line recessed strips: state (stepped spinner → check/cross),
  tool name in mono (`fs.read`, `task.run` …, names from `docs/protocols/agents.md`),
  target, and meta (lines, matches, the policy rule that allowed it).
- **Checkpoint** markers: hairline · flag · "Checkpoint 3 · before patch · 14:02"
  · Restore.
- **Diff blocks**: file header (mono) + stat; rows with line numbers; additions
  sage 11% / deletions brick 11%; "Open in diff editor" (M4 diff view).
- **Approval card**: the only brass-tinted card in the log (attention). It shows the
  shield icon, "Allow `fs.applyPatch` on `executor.rs`?", effect + policy + checkpoint
  status, and **Allow once** (primary, Enter) / **Allow for session** / **Deny**
  (Esc). It is focused when it appears and announced via `role="alert"`.
- Composer: recessed field; context chips (attached selection, model); Send (primary
  icon).
- Streaming: text appended per frame; one stepped caret; **no shimmer, no
  typewriter delay** (text shows as fast as it arrives).
- Running tool calls drive the transport (one place for "something is executing").

### 5.3 Later (M4–M7), for layout fit only

Terminal (console view, M4); Git status in tree rows and tab strip (M4, colour
from fixed semantic slots); Inspector (M5) as a dock page with preview in the
editor area; debug toolbar inside the transport (M6); user theming from the
generator inputs (M7). None needs a new region.

## 6. Empty, loading and error states (SHELL-005)

| State | Where | Design |
|-------|-------|--------|
| Backend starting | status LED pulsing (stepped) + "Connecting…"; Welcome actions disabled with tooltip "Waiting for the backend" | no spinner overlay; window usable |
| Backend failed | danger banner under the title row: message + **Restart backend**; LED err | existing data |
| No workspace (Welcome) | editor area: mark (32 px, brass), "Open a folder to start" (15/600), primary **Open Folder…** with keycaps, Recent list (rows 28: name 13/500 + path mono faint) ≤ 8; drop a folder here hint only if DnD is implemented | existing |
| Opening workspace | tree area: 6 skeleton rows (static `--hover` bars, no shimmer) for ≤ 1 s, then "Still listing…" | existing `ws.opening` |
| Empty folder | "This folder is empty" + Refresh | |
| Listing error | inline row with danger glyph + message + Retry | existing `tree-row--error` |
| No document open | editor well: four shortcut rows (Command palette, Go to file, Run command, Toggle console) as label + keycaps, faint | existing |
| Output empty | "Run a command in this workspace" + prompt focus hint | existing |
| File too large / binary | banner in the editor area with the daemon's message (existing E-4) | |
| Conflict on save | dialog (existing) restyled: **Reload** / **Overwrite** / Cancel, with the path in mono | |
| Unsaved on quit or close | dialog (existing): list of files (mono), **Save all** / Don't save / Cancel | |

All error copy states what happened, what it affects and the one action that fixes
it, without exclamation marks.

**As built (M1.6-SHELL-007).** Final copy and behaviour:

| State | Shipped |
|-------|---------|
| Welcome | mark 32 px; "Open a folder to start"; "Axiom opens folders through its backend, axiomd."; primary **Open Folder…** with `Ctrl`/`⌘` `O` keycaps; "Recent" (12/600 muted) and ≤ 8 rows of 28 px (name, home-shortened mono path). Column top at 22% of the area's height. |
| Backend starting | Open Folder… is `aria-disabled` (still focusable) with the tooltip and hint "Waiting for the backend"; recent rows are disabled. No overlay. |
| Backend failed | danger banner with a ✖ glyph (generated content, not announced), the backend's message and **Restart backend**; `role="alert"`. All banners carry a tone glyph (✖ danger, ▲ warning, ● info). |
| Opening workspace | six static skeleton bars (`--hover`, 40–70% wide) for 1 s, then "Still listing…" (faint); `role="status"`. |
| Empty folder | "This folder is empty" + quiet **Refresh** (re-lists the folder). |
| Root listing error | ✖ "The folder could not be listed, so its files are not shown." + quiet **Retry** (opens the folder again); the daemon's message stays in the warning banner above. |
| Folder listing error | a tree row: ✖, the daemon's message (full text in its tooltip), **Retry** (re-expands the folder, which lists it). The row takes its own 22 px slot; before SHELL-007 it was drawn over the next row. |
| No document | four rows, label faint 13 px, keycaps right: Command palette, Go to file, Run command, Toggle console. |
| Existing banners | copy unchanged (none had exclamation marks). |
