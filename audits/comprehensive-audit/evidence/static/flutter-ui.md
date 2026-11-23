# Flutter shell: widget and visual layer, static review

Scope: `apps/desktop-flutter/lib/ui/*.dart`, `lib/shell/{shell,palette,settings_page}.dart`,
`lib/editor/{editor_area,code_view}.dart`, `lib/editor/lsp/{lsp_view,underline,semantic_tokens}.dart`,
`lib/chat/{chat_page,markdown_view,re_highlight_bridge}.dart`, `lib/console/*.dart`, `lib/env/env_page.dart`,
`lib/files/explorer.dart`, `lib/scm/scm_page.dart`, `lib/search/search_page.dart`,
`lib/inspector/inspector_page.dart`. I also read `main.dart`, `linux/runner/my_application.cc`, the
vendored `third_party/re_editor` and the Flutter SDK (`~/flutter`) where a finding depended on them.

Method: I read the code only, at commit `751ffc2` on `main`. Nothing was built, run or tested. I
reasoned about the "Confirmed" findings from the code and, where cited, the SDK source. None was
reproduced in a running app. The "Visual checks" list at the end says how to reproduce each one.

Confidence: **Confirmed** means the code path is unambiguous. **Suspected** means the code points to
the defect, but the impact depends on runtime data or engine behaviour I did not observe.
**Unverified** means plausible but not established. Findings marked "Design opinion" are subjective
and are kept apart from defects.

Priorities: P0 (data loss or crash in a common flow) to P3 (polish).

---

## P1

### FUI-01: Splitting the editor throws during build when the editor area is under 480 px wide
- **Category:** GUI (crash or error widget). **Priority:** P1. **Confidence:** Confirmed (by code; not reproduced).
- **Where:** `lib/editor/editor_area.dart:78`, `:83-90`; `lib/ui/splitter.dart:51`, `:62-63`.
```dart
final w = (_leftWidth ?? box.maxWidth / 2).clamp(240.0, box.maxWidth - 240);
...
AxSplitter(min: 240, max: box.maxWidth - 240, ...)   // splitter.dart builds clamp(min, max) too
```
- **Why it matters:** Dart's `num.clamp` throws `ArgumentError` when the lower limit is greater
  than the upper one. That happens whenever `box.maxWidth < 480`. The editor column is
  `window − rail 44 − groove 1 − side bar − splitters − dock`. With the default side bar (260) and AI
  dock (380), it falls under 480 px in any window narrower than about 1175 px. Common cases are a
  half-screen window on a 1920 px display, or a 1366 px laptop with the side bar widened. In that
  state, Mod+\ ("Split Right") makes `EditorArea`'s build throw. The editor area then becomes
  Flutter's error widget (a grey box in release), and it throws again on every rebuild until the
  split is closed. `AxSplitter.build` (`splitter.dart:62-63`) has the same inverted-range clamp.
- **Fix:** If `box.maxWidth < 2 * 240 + splitterThickness`, either don't split (show only the active
  group) or derive the limits safely: `final lo = math.min(240.0, box.maxWidth / 2); final hi =
  math.max(lo, box.maxWidth - 240);`. In `AxSplitter`, assert `min <= max` and clamp with
  `math.min/max` so a degenerate range can't throw. See also FUI-05, which should keep the editor
  column above a minimum width.
- **Verification:** Add a widget test that pumps `EditorArea` with a split in a 400 px-wide box and
  expects no exception. Manually: a 1000 px window with the side bar and dock open, then Mod+\.

### FUI-02: Menus always open downward and to the right, with no flip, clamp or scroll, so they run off the window
- **Category:** GUI/Usability. **Priority:** P1. **Confidence:** Confirmed.
- **Where:** `lib/ui/overlays.dart:187-203` (follower anchored at `bottomLeft`, offset `(0,4)`),
  `:292-322` (an unbounded `Column` of entries in an `IntrinsicWidth`, with no `maxHeight` and no
  scroll). Callers: `lib/chat/chat_page.dart:883` (`ModelPicker`, placed in the composer's trailing
  slot at `:719-723`, which is at the bottom of the window); `lib/shell/shell.dart:869-892` (the View
  menu has 22 commands, so 22 × 26 px plus padding is about 580 px, starting about 42 px from the top);
  `chat_page.dart:202` (row "More" menus); `chat/models.dart:581`.
- **Why it matters:** The model picker is how a person picks who answers a new chat. Its trigger sits
  about 40 px above the window's bottom edge, so a menu of provider headings and models opens almost
  entirely below the window and is clipped. Arrow keys move focus onto items that can't be seen. In
  the dock, the trigger is at the right edge and the menu is at least 180 px wide, so it is also
  clipped on the right. The View menu is cut off in windows under about 620 px tall, and the window
  minimum is 560 px (`linux/runner/my_application.cc:41-42`).
- **Fix:** Lay the menu out with a `CustomSingleChildLayout` against the overlay's size, as
  `_AtText` already does for completion in `lsp_view.dart:1524-1549`. Open upward when the menu
  doesn't fit below, shift left to stay inside the right edge, and cap the height at the available
  space with a scrolling `ListView` (keep `_move` working with `Scrollable.ensureVisible` on the
  focused node).
- **Verification:** Add a widget test with the trigger near the bottom-right of an 800×560 surface
  that asserts the sheet's global rect is inside the window. Manually: open the model picker in Chat,
  and open the View menu in a short window.

---

## P2

### FUI-03: Dialogs freeze the theme captured when they opened, so Settings doesn't follow its own Theme or Motion choice
- **Category:** Visual. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/ui/overlays.dart:399-419`.
```dart
final t = context.axiom;           // captured once
...
pageBuilder: (context, _, _) => ... AxiomTheme(data: t, child: Sheet(...)),
barrierColor: t.colors.scrim, transitionDuration: t.motion.medium,
```
- **Why it matters:** The dialog route already sits under the app-level `AxiomTheme`
  (`main.dart:144-151`), but this wrapper overrides it with the snapshot. If you open Settings and
  choose Theme: Light, the window behind turns light while the Settings sheet and scrim stay dark (and
  the reverse) until you reopen it. The same applies to Motion, and to any dialog that is open when the
  OS theme changes under "System".
- **Fix:** Remove the `AxiomTheme(data: t, ...)` wrapper. If a theme is needed because a caller's
  context sits outside the app theme, read it inside `pageBuilder` with `context.axiom`. Read
  `barrierColor` and the motion at call time only where they can't change.
- **Verification:** Add a widget test: open Settings, tap "Light", and expect the `Sheet`'s colour to
  equal `AxiomColors.light.sheet`.

### FUI-04: The Settings dialog (820×600) is taller than the smallest window (560) and doesn't scroll as a whole
- **Category:** GUI. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/shell/settings_page.dart:56-58` (`SizedBox(width: 820, height: 600)`);
  `linux/runner/my_application.cc:41-42` and `macos/Runner/MainFlutterWindow.swift:19` (minimum
  900×560).
- **Why it matters:** At 560–630 px window heights (plus the dialog's `SafeArea`), the centred sheet
  overflows top and bottom. The "Close" button at the bottom of the left column and the top of the
  sections are clipped. With large text it gets worse.
- **Fix:** Use
  `ConstrainedBox(maxWidth: 820, maxHeight: 600)` inside a `LayoutBuilder` that also caps at
  `constraints.maxHeight - 2*Metrics.s7`, and let the left column scroll.
- **Verification:** Add a widget test at 900×560 with no overflow. Manually: shrink the window to its
  minimum and open Settings.

### FUI-05: Persisted side bar, dock and console sizes aren't constrained by the window, so the main row and column overflow
- **Category:** GUI. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/shell/shell.dart:721-788` (fixed `SizedBox` widths in a `Row` and a fixed console
  height in a `Column`); splitter limits `:739` (side ≤ 520), `:782` (dock ≤ 720), `:763` (console ≤
  640); the values persist in `shell-local.json` (`app/app_state.dart:198-216`).
- **Why it matters:** The side bar (520) plus the dock (720) plus the rail come to 1295 px, which is
  more than a 900–1280 px window. The editor's `Expanded` collapses to 0 and the row overflows, so the
  dock is clipped at the right edge (and FUI-01 can trigger). A console dragged to 640 px in a tall
  window and then shown in a short one (another monitor, or un-maximised) gives the editor 0 px and
  overflows the column, pushing the status bar out.
- **Fix:** Wrap the row in a `LayoutBuilder` and compute effective sizes. Clamp the side bar and dock
  so the editor keeps at least about 320 px (shrink the dock first). Cap the console at
  `maxHeight - title - status - minEditor`. Pass these effective maxima to the splitters too, so the
  keyboard nudges and the announced values agree.
- **Verification:** Add a widget test at 900×560 with stored side 520, dock 720 and console 640:
  expect no `RenderFlex` overflow and an editor width of at least 320 px.

### FUI-06: The command palette doesn't trap focus, and the background stays reachable
- **Category:** Accessibility/Usability. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/shell/palette.dart:117-209`. The palette is a `Positioned.fill` child of the shell's
  `Stack` (`shell.dart:817-843`), not a route, and has no `FocusScope` or `BlockSemantics`. The
  barrier is only a `GestureDetector`.
- **Why it matters:** Pointer input is blocked, but Tab and Shift+Tab move from the query field
  through the result rows and into the title row, rail and editor behind the open palette. The
  palette stays open while focus is elsewhere. Screen readers also keep the whole window in the
  semantics tree: `scopesRoute` names the sheet but doesn't hide the rest. The "Close Command
  palette" barrier node has a label but no tap action.
- **Fix:** Wrap the palette in `FocusScope(autofocus: true)` with a `FocusTraversalGroup`, make Tab
  cycle inside it (or close it, as VS Code does), and add `BlockSemantics` around the sheet plus
  barrier. Alternatively, show it as a `RawDialogRoute` / `showAxDialog` with a top alignment. Give
  the barrier `onTap: _close` in its `Semantics`.
- **Verification:** Extend `a11y_test` so that with the palette open, pressing Tab ten times leaves
  `primaryFocus` inside the palette.

### FUI-07: The palette re-ranks every item on every rebuild, including each arrow key (Go to File with up to 20 000 files)
- **Category:** Performance. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/shell/palette.dart:80-85` (`_results` is a getter that calls `rankFuzzy`), `:115`
  (called in `build`), `:98-100` (`_move` → `setState`); `lib/ui/fuzzy.dart:128-145` (scores and
  sorts every item); `lib/shell/shell.dart:202` (`limit: 20000`).
- **Why it matters:** Each ↑/↓ press re-runs the dynamic-programming fuzzy match over every file and
  then sorts the full match list, even though the query hasn't changed. With an empty query every
  item matches, so the whole list is sorted. Over 20 000 paths that is far more than the per-frame
  budget (`performance.md` P6: filtering 5 000 files should take 8 ms or less per keystroke), so
  keyboard navigation stutters in large repositories.
- **Fix:** Cache the results keyed on `(query, identical(source))`, recompute only in the query
  listener and when `_loaded` arrives, and keep `_move` to a `setState` of `_active` only. For large
  lists, debounce or move ranking to an isolate.
- **Verification:** Add a micro-benchmark of `_move` with 20 000 items, or use `frames.sh` with Go to
  File in a large repository.

### FUI-08: Search results and Source Control changes are built eagerly, not lazily
- **Category:** Performance. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/search/search_page.dart:227-257` (`ListView(children: [for group: Column(children:
  [for match ...])])`); `lib/scm/scm_page.dart:96-140` (the same pattern for the
  Conflicts/Staged/Changes/Untracked sections); `_flat()` and `flat.indexOf` are recomputed per build
  and per key (`search_page.dart:64-75`, `scm_page.dart:72-78`).
- **Why it matters:** Each group is one `ListView` child whose `Column` builds every row, so a search
  with thousands of hits, or a status with thousands of untracked files (for example an unignored
  `node_modules`), builds and lays out every row on each store notification. Search results stream
  in, so that happens repeatedly. The explorer, Problems and Output already use
  `ListView.builder(itemExtent:)`; these two don't (`performance.md` P11).
- **Fix:** Flatten to a list of header and row items and use `ListView.builder` with a fixed extent
  per kind (or `SliverList` with `SliverChildBuilderDelegate`). Keep an index map from id to position
  for the arrow keys.
- **Verification:** Run a frame-timing test with 5 000 results. Manually: search for `e` in a large
  repository and scroll.

### FUI-09: Tooltips are centred under their control with no edge handling, so rail and title-bar tooltips are clipped
- **Category:** Visual/GUI. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/ui/overlays.dart:62-92` (`targetAnchor: bottomCenter, followerAnchor: topCenter`,
  no clamping); rail buttons at `lib/shell/shell.dart:1157-1169`, `:1181-1191` (x ≈ 5–39 px);
  the gear button at the far right of the title row (`:1052-1057`).
- **Why it matters:** A "Source Control Ctrl Shift G" tooltip is about 190 px wide, centred on a
  34 px button at the window's left edge, so about 75 px fall outside the window. Tooltips on the
  right edge (Settings, Toggle console) are cut on the right. Rail tooltips also open downward over
  the next rail icon; side tooltips are the norm for a vertical rail.
- **Fix:** Position tooltips with a layout delegate that clamps to the overlay's bounds and flips
  above when there is no room below. Let `AxIconButton` take a preferred side (`right` for the rail).
- **Verification:** Hover each rail icon and the right-most title-row button, in a golden or by
  inspection.

### FUI-10: The LSP hover card is clipped at the editor's right and bottom edges, and the completion popup in narrow groups
- **Category:** GUI. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/editor/lsp/lsp_view.dart:1469-1497` (`Positioned(left: (x-40).clamp(0,∞), top:
  y+18)`, up to 560×320, inside a `Stack` that clips to the editor view); compare `_AtText`
  (`:1524-1549`), which completion uses and which does flip and clamp. Also `:1585-1588` (the
  completion popup is at least 320 px wide).
- **Why it matters:** Hovering a symbol near the right edge or the last visible lines gives a card
  that is cut off and can't be scrolled into view. In a split group (240 px minimum) or beside a wide
  dock, the card and the 320 px completion popup are wider than the view. The hover card also stays
  at the old pointer position when the editor scrolls under a stationary pointer.
- **Fix:** Place the hover card with `_AtText` (above when there is no room below, clamped
  horizontally), cap its width at `min(560, size.width)`, and hide it on scroll. Cap the completion
  width at the stack width.
- **Verification:** Hover a symbol on the last visible line near the right edge.

### FUI-11: The editor tab strip doesn't scroll the active tab into view, and a mouse wheel can't scroll it
- **Category:** Usability. **Priority:** P2. **Confidence:** Confirmed (no `ensureVisible` or
  `onPointerSignal` anywhere in `lib/`).
- **Where:** `lib/editor/editor_area.dart:164-201` (a horizontal `SingleChildScrollView`, no
  controller, no scrollbar).
- **Why it matters:** With more tabs than fit, Ctrl+Tab, opening a file from the explorer or palette,
  or a reveal from Search activates a tab that stays off-screen. On a mouse without horizontal
  scroll, Flutter's horizontal `Scrollable` ignores vertical wheel deltas, so the hidden tabs can't be
  reached with the pointer at all.
- **Fix:** Give each tab a key and call `Scrollable.ensureVisible` on the active one after it changes.
  Map vertical wheel deltas to horizontal scrolling with a `Listener(onPointerSignal:)`. Consider a
  thin scrollbar or an overflow menu.
- **Verification:** Open 30 files and press Ctrl+Tab repeatedly; scroll the strip with a wheel.

### FUI-12: The find bar is a fixed-width `Row` that overflows in narrow editor groups
- **Category:** GUI. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/editor/editor_area.dart:817-858` (`SizedBox(width: 280)` field + 2 toggle buttons
  + count + next + Replace… + Spacer + close), `:864-885` (replace row).
- **Why it matters:** That content needs about 700 px or more. A split group can be 240 px, and the
  editor column at the minimum window size with the side bar and dock open is about 200–500 px, so
  the row overflows. In release the overflowing part is clipped, which hides the result count, Next
  and the Close button.
- **Fix:** Make the field `Flexible` with a 120–280 px range, use icon toggles (as Search does with
  `Aa`, `.*`), and wrap or collapse the secondary actions under a width threshold. Add a Previous
  button for parity with Shift+Enter.
- **Verification:** Split the editor in a 1000 px window and press Ctrl+F in the right group.

### FUI-13: Reduced motion is incomplete, and the OS reduced-motion preference is ignored
- **Category:** Accessibility. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/ui/tokens.dart:175-201` (Reduced only caps durations; `animates` is true for
  Reduced); `lib/main.dart:115-125` (the theme reads only the stored setting; nothing reads
  `MediaQuery.disableAnimationsOf` or `PlatformDispatcher.accessibilityFeatures`);
  `lib/ui/tabs.dart:218-224` and `lib/shell/shell.dart:1105-1109` (spring glides run whenever
  `animates`); `lib/ui/controls.dart:212-218` (spring on Send); `:577-587` (the spinner turns at
  Reduced); `lib/ui/overlays.dart:420-426` (dialog scale at Reduced).
- **Why it matters:** `motion-system.md` §2 says "`prefers-reduced-motion: reduce` forces at least
  `reduced`". §6 says that at Reduced, glides jump, loops are static, presence is fade only and press
  has no translate. People who set reduced motion in the OS get the full Standard motion. People who
  choose Reduced in Settings still get springs (the springs have no duration cap), a rotating spinner
  and scaling dialogs. `AxToggle` also animates `alignment` (layout), and the tab underline animates
  `Positioned.left`, which goes against motion-system §3 rule 1 (transform and opacity only).
- **Fix:** Add `bool get glides => mode == MotionMode.standard` and `bool get loops => mode ==
  MotionMode.standard` to `AxiomMotion`, and use them in `AxTabs._place`, `_RailState`,
  `InkCircleButton` and `Spinner` (draw a static arc). Drop the `ScaleTransition` unless Standard. In
  `_AxiomAppState._theme()`, take `max(setting, MediaQuery.disableAnimations ? reduced : standard)`.
- **Verification:** Add widget tests at `MotionMode.reduced`: switching tabs leaves no running
  ticker, and the spinner's painter angle stays constant. Run the a11y e2e test with
  `disableAnimations: true`.

### FUI-14: Semantic-token colours keep the old theme after a theme switch until the next typed edit
- **Category:** Visual. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/editor/lsp/lsp_view.dart:230-236` (decoded with `context.colors.syntax` at request
  time, stored as concrete `TextStyle(color:)`), `:303` (re-requested only `if (typed)`);
  `lib/state/documents.dart:213-218`. The grammar colours do update (`editor_area.dart:689`).
- **Why it matters:** After you switch Dark → Light (or the OS switches under "System"), identifiers
  coloured by the language server (types, functions, parameters: most of a Rust or TypeScript file)
  keep the dark palette on the light ground until you type in that document. Every other open
  document stays wrong until it is edited. The dark-theme hues weren't contrast-checked on the light
  ground.
- **Fix:** Store the token *role* (type index or modifier) instead of a colour, and resolve to
  `SyntaxColors` at paint time. Alternatively, re-run `_structure()` from `didChangeDependencies` when
  `context.colors` changes.
- **Verification:** Open a Rust file with rust-analyzer ready, switch the theme, and compare a type
  name's colour with `AxiomColors.light.syntax.type`.

### FUI-15: Controls that look interactive but do nothing
- **Category:** Usability. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/env/env_page.dart:713` (each Environments side-bar row is an `AxListRow` with
  `onPressed: () {}`); `lib/console/console.dart:209-217` (the status bar's language-server segment
  is a `Pressable` with `onPressed: () {}` unless the server failed).
- **Why it matters:** The rows show hover and pressed states and a click cursor, take a Tab stop and
  are announced as buttons, but clicking or pressing Enter does nothing. The project rules forbid
  placeholder UI. The language-server segment in the "not installed" state should lead to the
  install offer and doesn't.
- **Fix:** For the Environments rows, open the Environments page and scroll to or focus that row
  (or make them non-interactive text rows). For `LspStatus`, act per state: missing → the
  install offer or plan, ready → open Output/logs or Environments, otherwise pass `onPressed: null`
  and drop the click cursor.
- **Verification:** Click each row and the status segment and observe an effect.

### FUI-16: The floating approvals stack can grow beyond the window and overlaps the toasts
- **Category:** GUI. **Priority:** P2. **Confidence:** Confirmed (geometry from code).
- **Where:** `lib/shell/shell.dart:799-816` (`Positioned(right:16, bottom:36, width:440)`, no height
  limit); `lib/chat/chat_page.dart:1653-1700` (up to 3 cards); a card contains a diff preview of up
  to 260 px (`:1330-1335`) plus title, meta, external note and buttons, about 380–420 px each. The
  toasts are positioned at `right:16, bottom:48` (`lib/ui/overlays.dart:465-467`).
- **Why it matters:** Three pending approvals with diffs need about 1 200 px. In a 560–900 px window
  the column grows above the top of the window, so the oldest card's Allow/Deny buttons are
  unreachable. Toasts render above it (`ToastHost` is above the Navigator) in the same corner and
  cover its buttons for 5 s.
- **Fix:** Constrain the stack to `maxHeight = available − margins`, make it scroll, collapse cards
  after the first (show a summary line), and move the toasts above the stack or to another corner
  while approvals are shown.
- **Verification:** Have an agent request three file edits from a conversation that isn't shown, in
  a 768 px-tall window.

### FUI-17: Long output and diff lines are cut off with no wrap and no horizontal scroll
- **Category:** Usability. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/console/output.dart:221-227` (`maxLines: 1, softWrap: false, overflow: clip`,
  `itemExtent: 19`); `lib/scm/scm_page.dart:403-410` (`_SideBySide` text, same pattern).
- **Why it matters:** Compiler errors, test failures and stack traces often exceed the console width.
  The end of the line (often the file:line or the actual message) is invisible, and there is no way
  to scroll to it. The lazy list also limits selection to built rows, so copying a long log is
  unreliable.
- **Fix:** Offer wrap (a toggle in the Output head, defaulting to on). With a fixed `itemExtent`,
  either measure wrapped heights (use `ListView.builder` without `itemExtent`, since rows are
  homogeneous text) or put the list in a horizontal `Scrollable` sized to the longest visible line.
  Add "Copy all" for the output.
- **Verification:** Run `printf '%0300d\n' 0` in the run prompt.

### FUI-18: `AxTabs` is a non-scrolling `Row` that overflows with many or long tabs
- **Category:** GUI. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** `lib/ui/tabs.dart:267-308` (`Row(mainAxisSize: min)` with unbounded children, no
  ellipsis); used for terminals (`lib/console/terminal_view.dart:146-159`, inside `Flexible`) and for
  chat scopes (`lib/chat/chat_page.dart:124-133`, with the folder name as a label).
- **Why it matters:** With about 6 or more terminals ("bash 1"… plus the New and Close buttons) the
  tab row overflows its `Flexible` slot. Later tabs are clipped and can't be clicked (arrow keys
  still reach them). In the chat side bar (180 px minimum), "General" plus a long folder name
  overflows.
- **Fix:** Let tabs shrink with ellipsis (`Flexible` plus `TextOverflow.ellipsis` and a minimum
  width), or make the strip horizontally scrollable with the selected tab kept visible (shared with
  FUI-11).
- **Verification:** Open 8 terminals in a 1000 px window; open a folder with a 40-character name and
  show Chat.

### FUI-19: Lists put a Tab stop on every row (no roving focus)
- **Category:** Accessibility/Usability. **Priority:** P2. **Confidence:** Confirmed.
- **Where:** Every row is a focusable `Pressable`: `lib/files/explorer.dart:235-241`,
  `lib/console/console.dart:145` (Problems), `lib/search/search_page.dart:279-283`,
  `lib/scm/scm_page.dart:173-177`, `lib/chat/chat_page.dart:171` (conversations),
  `lib/shell/palette.dart:247-251`. Also each inactive editor tab's invisible close button
  (`editor_area.dart:297-306`, `Opacity(0)`, still focusable).
- **Why it matters:** The explorer, Search and SCM implement ↑/↓ themselves, but Tab still walks
  through every built row, so leaving a 100-row tree takes up to 100 presses. Problems has no arrow
  navigation at all. On editor tabs, Tab lands on close buttons that are drawn at opacity 0, so the
  focus ring is invisible.
- **Fix:** Use a roving tab index: only the current row has `skipTraversal: false`, others `true`,
  and the arrow keys move it. Give Problems the same ↑/↓/Home/End. Exclude the hidden close buttons
  from traversal (`ExcludeFocus` when not `showClose`), relying on Mod+W and middle-click.
- **Verification:** In `a11y_test`, count the Tab presses needed to cross the explorer with 50 rows
  (expected: 1).

---

## P3

### FUI-20: `_focusedDoc` relies on `FocusNode.debugLabel`, which is always null in release builds
- **Category:** Architecture/GUI. **Priority:** P3 (reachability low). **Confidence:** Confirmed for
  the mechanism; Suspected for user impact.
- **Where:** `lib/shell/shell.dart:227-231`, used by the "Save" command at `:455-456`; labels set in
  `lib/editor/editor_area.dart:43`. Flutter: `~/flutter/packages/flutter/lib/src/widgets/focus_manager.dart:697-704`
  ("Will always return null in release builds").
```dart
final f = FocusManager.instance.primaryFocus?.debugLabel ?? '';
if (f.startsWith('editor 2:') && d.split != null) return d.doc(d.split!.active);
```
- **Why it matters:** In release builds (`scripts/run.sh` and the packages) the right-group branch is
  dead, so the shell's Save command always targets the left group's document. In practice, Mod+S
  inside an editor is handled first by re_editor's save intent (`code_view.dart:182-191`), which
  masks the bug. `integration_test/editor_test.dart:150` asserts on `debugLabel`, so it passes only
  in debug.
- **Fix:** Track the focused group explicitly (a `ValueNotifier<int>` set from each `_DocView`'s
  focus listener, or compare `primaryFocus` with `EditorAreaState.focusFor(...)` nodes). Don't use
  `debugLabel` for logic or for e2e assertions.
- **Verification:** Run the split tests under `--profile`.

### FUI-21: The hint line keeps a stale hint when a hovered or focused control disappears
- **Category:** Usability. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/ui/pressable.dart:110-136` (no hint cleanup in `dispose`);
  `lib/ui/splitter.dart:72-83`; `lib/ui/text_input.dart:91-112`. Flutter documents that `onExit`
  isn't called when a hovered `MouseRegion` unmounts (`widgets/basic.dart:7356-7358`).
- **Why it matters:** If you click a menu item, a palette row or a dialog button, the control unmounts
  under the pointer and the status bar keeps saying, for example, "Rename… " until something else is
  hovered.
- **Fix:** In `dispose`, `if (h.value == (widget.hint ?? widget.label)) h.value = null` (cache the
  `HintScope` notifier in `didChangeDependencies`, because `context` lookups in `dispose` are unsafe).
- **Verification:** Open the File menu, click an item, and check the status bar.

### FUI-22: Status dots' cut-out ring doesn't match the surface they sit on
- **Category:** Visual. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/ui/controls.dart:379` (the default ring is `c.bar`); mismatches at
  `lib/chat/chat_page.dart:188-191` (`on ? c.raised : c.bar`, but rows are `c.selected` or the side
  bar's `c.panel`); `lib/console/console.dart:156` (`c.recessed` on the `c.panel` console);
  `lib/env/env_page.dart:718`, `lib/inspector/inspector_page.dart:151,281,372,401` (default `bar` on
  `panel`).
- **Why it matters:** In dark mode `bar` is #141517 and `panel` is #1E1F22, so each dot shows a
  visible dark halo. This is the "older Discord" mark done wrong, and it varies from surface to
  surface.
- **Fix:** Make the ring optional (`null` = no ring) and pass the actual surface colour at each call
  site, or provide the surface colour through an `InheritedWidget` that regions set.
- **Verification:** Zoom on a Problems row, a conversation row with a running chat, and an Inspector
  source row in both themes.

### FUI-23: Some focusable controls have no visible focus ring
- **Category:** Accessibility. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/chat/chat_page.dart:207-214` (the row "More" menu trigger ignores `st.focused`);
  `lib/console/console.dart:217-227` (`LspStatus`); `lib/shell/shell.dart:1706-1734` uses
  `FocusRing`, so the status bar is inconsistent with itself; the invisible editor-tab close buttons
  (FUI-19).
- **Fix:** Wrap every `AxMenu.trigger` and `Pressable` builder output in `FocusRing(visible:
  s.focused)`. Consider having `Pressable` draw the ring itself unless the builder opts out.
- **Verification:** Tab through Chat's conversation list and the status bar.

### FUI-24: `Pressable` drops the focusable and focused semantics flags
- **Category:** Accessibility. **Priority:** P3. **Confidence:** Suspected (effect depends on the
  platform bridge).
- **Where:** `lib/ui/pressable.dart:150-167` (an outer `Semantics(excludeSemantics: true)` wraps
  `FocusableActionDetector`, whose inner `Focus` semantics carry `focusable` and `focused`).
- **Why it matters:** Assistive technology that follows the focused flag (VoiceOver on macOS) may not
  track keyboard focus on Axiom's buttons, tabs and rows.
- **Fix:** Add `focusable: _enabled, focused: _focused` to the outer `Semantics`.
- **Verification:** Run `tester.getSemantics(find.byType(Pressable))` after focusing it and check
  `SemanticsFlag.isFocused`.

### FUI-25: Rebuild scope is too wide for shell-level state
- **Category:** Performance. **Priority:** P3. **Confidence:** Confirmed (cost measured only in
  aggregate: `docs/roadmap/verification/FL.md` reports the highest build as 14.2 ms, when opening the
  palette).
- **Where:**
  - `lib/shell/shell.dart:183-187`, `:821`, `:831`, `:842`: opening or closing the palette or a
    picker is a `setState` on `_ShellState`. That rebuilds the title row, re-runs the
    `ListenableBuilder` at `:694` (rail, `_SideBar` → `FilesPage`, `_Main` → `_Work` → `EditorArea`,
    which aren't const) and the approvals builder. `commands()` (a list of more than 40 entries,
    including tasks) is rebuilt for every shortcut press (`:654`).
  - `:694-696`, `:742`, `:765`, `:785`: every splitter pointer move writes `prefs`
    (`app_state.dart:199`), which notifies and rebuilds the same subtree plus `ConsoleRegion`'s head.
  - `lib/ui/tabs.dart:195-200`: each parent rebuild schedules a post-frame `_place` that always calls
    `setState`, so every `AxTabs` builds twice per parent rebuild (the console head does this on every
    splitter move).
  - `lib/editor/lsp/lsp_view.dart`: `setState` for popup-only state (completion keystrokes and
    arrows, hover, signature) re-runs `widget.builder`, rebuilding `CodeView`/`CodeEditor` with a new
    overrides map.
  - `lib/chat/chat_page.dart:472` and `:108-110`: each streamed delta rebuilds the Chat header,
    composer, approval cards and (when visible) the conversation list with `a.filtered()`.
- **Fix:** Hold palette and picker state in a `ValueNotifier` with its own `ListenableBuilder` above
  the `Stack` children (or use `OverlayPortal`). Separate layout prefs (sizes) from other prefs and
  rebuild only the `SizedBox` that changes (pass child widgets through `ListenableBuilder.child`). In
  `AxTabs`, only `setState` when `_placed` flips. Put the completion, hover and signature overlays in
  their own `StatefulWidget` layered above `widget.builder(...)`. Split Chat's
  `ListenableBuilder` into the transcript and the chrome.
- **Verification:** Use `debugPrintRebuildDirtyWidgets` or DevTools rebuild counts while dragging the
  side splitter and pressing ↓ in the palette.

### FUI-26: Text fields rebuild on every keystroke and have no context menu
- **Category:** Performance/Usability. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/ui/text_input.dart:103` (`_onText` → `setState` for any controller change,
  including selection); `:142` (`selectionControls: null`, no `contextMenuBuilder`).
- **Why it matters:** The placeholder only needs a rebuild when emptiness changes. More visibly, a
  right-click in any field (palette, composer, search, run prompt) does nothing. Desktop users expect
  Cut/Copy/Paste there, and keyboard shortcuts are the only way.
- **Fix:** `setState` only when `_controller.text.isEmpty` changes. Add a token-styled
  `contextMenuBuilder` built on `AxMenu`'s sheet (widgets layer, no Material).
- **Verification:** Right-click in the chat composer.

### FUI-27: Switching console views throws away the Output and Terminal views' state
- **Category:** Usability. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/console/console.dart:97-101` (a `switch` returns a fresh `OutputView`,
  `TerminalPane` or `ProblemsView`; nothing stays mounted, unlike the side bar and main area, which
  use `Offstage`).
- **Why it matters:** A half-typed command in the run prompt (a controller in `OutputViewState`), the
  output scroll position, the history cursor and the terminal's selection and scroll-back position are
  lost each time you look at Problems.
- **Fix:** Keep the three views mounted in an `IndexedStack` or in `Offstage` + `TickerMode`, as
  `_SideBar` does.
- **Verification:** Type in the run prompt, switch to Problems and back.

### FUI-28: The Environments page calls the store from `build`, which can loop
- **Category:** Architecture/Performance. **Priority:** P3. **Confidence:** Confirmed (only an edge
  case loops).
- **Where:** `lib/env/env_page.dart:78` (`unawaited(s.loadProviders())` inside the builder);
  `lib/state/environments.dart:231-242` (guards on `providers.isNotEmpty`, then assigns and notifies
  even when the daemon returns an empty list).
- **Why it matters:** Each rebuild issues another `env.providers` RPC while one is in flight. If the
  daemon returns zero providers (a broken install or missing built-ins), each response notifies,
  which rebuilds and calls again: an endless RPC and rebuild loop while the page is shown.
  `_ItemView` does the same with `app.agent.loadImage` (`chat_page.dart:992-994`).
- **Fix:** Move the call to `initState` or a post-frame callback, as `_loadSoon` does in Chat. In the
  store, add an "in flight or loaded" flag and only notify on change.
- **Verification:** Run a unit test with a fake client that returns `providers: []`.

### FUI-29: The composer changes its widget tree shape when attachments or notes appear
- **Category:** Usability. **Priority:** P3. **Confidence:** Suspected.
- **Where:** `lib/chat/chat_page.dart:756-810` (`return input;` versus `Column([..., input])`).
- **Why it matters:** When `attached`, `note` or `context_` goes from empty to non-empty, the
  `AxTextInput` element is re-created under a new parent. Its `Focus` and `EditableText` are
  unmounted and remounted, which can drop keyboard focus and the IME composition while you are typing.
  For example, an attach note appears, or the Inspector adds context to an open dock.
- **Fix:** Always return the `Column` (empty children collapse), or key the input with a
  `GlobalKey`.
- **Verification:** Focus the composer, add context from the Inspector, and keep typing.

### FUI-30: Hover documentation is pointer-only
- **Category:** Accessibility. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/editor/lsp/lsp_view.dart:608-614` (shown only from `MouseRegion.onHover`); the
  shortcut map (`:1389-1420`) has no "show hover".
- **Fix:** Bind Ctrl+K Ctrl+I (VS Code) or a single chord to `_hoverAt` at the caret's position, with
  Escape to close.

### FUI-31: Status bar segments with long text aren't ellipsised (unbounded width)
- **Category:** GUI. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/shell/shell.dart:1610-1630` (branch name), `:1674-1686` (workspace root with
  `overflow: ellipsis`, but inside a non-flex `Row` slot, so ellipsis can't apply).
- **Why it matters:** A long branch (`feature/…`) or root path in a narrow window pushes past the hint
  line's `Expanded` and overflows the status bar.
- **Fix:** Wrap those segments in `Flexible` with `maxWidth` constraints.

### FUI-32: The side-by-side diff runs a quadratic alignment on the UI thread
- **Category:** Performance. **Priority:** P3. **Confidence:** Suspected.
- **Where:** `lib/scm/scm_page.dart:269-273` → `lib/state/git.dart:125-140` (LCS up to 4 M cells,
  a 16 MB `Uint32List`); the gutter path moves the same work to an isolate above 2 000 lines
  (`editor_area.dart:610-612`), but `DiffView` doesn't. The header `Text(widget.path)` at
  `scm_page.dart:310` isn't flexible and overflows on long paths.
- **Fix:** Use `Isolate.run` for large inputs, as the gutter does. Make the path `Flexible` with
  ellipsis.

### FUI-33: Loops don't pause when the window is unfocused, and there can be more than 3
- **Category:** Performance. **Priority:** P3. **Confidence:** Confirmed (no lifecycle or focus hook;
  `TickerMode` is only used for hidden pages).
- **Where:** `lib/ui/controls.dart:566-587` (the comment says hidden windows stop it "by TickerMode",
  but nothing ties `TickerMode` to window state); `lib/console/output.dart:276` (4 Hz transport
  timer); `lib/files/explorer.dart:278-279` (a spinner per loading folder).
- **Why it matters:** `performance.md` P9 requires loops to pause when the window is unfocused, and
  motion-system §3 caps visible loops at 3. Flutter keeps producing frames in the `inactive`
  (unfocused) state, so each visible spinner repaints the whole window at 60 Hz (see the FL-PERF-001
  note in `tokens.dart:206-209`).
- **Fix:** Add an `AppLifecycleListener` in `AxiomApp` that wraps the tree in `TickerMode(enabled:
  state == resumed)` and pauses the transport timer. Make the spinner stepped (about 8 frames per
  second), as the spec says.

### FUI-34: Toasts stack without limit and don't animate
- **Category:** GUI/Visual. **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/ui/overlays.dart:451-457`, `:462-506` (an unbounded `Column`; each toast gets its
  own 5 s timer; no enter or exit motion, although motion-system §6 specifies slide plus fade).
- **Fix:** Cap at about 3 (merge duplicates), and add an `AnimatedSwitcher`-style fade honouring
  `motion`.

### FUI-35: Live regions that announce too much
- **Category:** Accessibility. **Priority:** P3. **Confidence:** Suspected.
- **Where:** `lib/inspector/inspector_page.dart:63-73` ("Under the pointer: …" re-announced on every
  pointer move in the inspected page); `lib/editor/lsp/lsp_view.dart:1483-1486` (the whole hover text
  as a live label on each hover); `:1601-1604` (the completion list container is a live region and
  is re-announced on every keystroke); `palette.dart` has the opposite problem (the active row isn't
  announced while typing).
- **Fix:** Announce only the selected or active item (`SemanticsService.sendAnnouncement`, throttled).
  Drop `liveRegion` from containers.

### FUI-36: Unbounded per-path `FocusNode` maps, and `TextPainter`s created in `build`
- **Category:** Performance (memory). **Priority:** P3. **Confidence:** Confirmed.
- **Where:** `lib/files/explorer.dart:44-45`, `lib/scm/scm_page.dart:30-32`,
  `lib/search/search_page.dart:32-34` (nodes are only disposed when the page goes away; scrolling a
  50 000-file tree retains 50 000 nodes); `lib/editor/editor_area.dart:416-424` (one `TextPainter`
  per breadcrumb segment per build, never `dispose()`d).
- **Fix:** Prune nodes for rows no longer in `_rows` or results (as `EditorAreaState` does at
  `:67-72`). Measure breadcrumbs once per path and theme, and dispose the painters.

### FUI-37: Visual inconsistencies between equivalent elements
- **Category:** Visual. **Priority:** P3. **Confidence:** Confirmed (the values); the judgement is a
  Design opinion.
- **Section headings in side panels** use three different styles: Inspector `_Heading` uses
  `type.label` (12/w500/ink) (`inspector_page.dart:94-103`); SCM groups use `type.tiny` (11/ink2)
  plus a badge (`scm_page.dart:130`); Search file groups use `type.label`; the "Recent" header on the
  welcome page and the side-bar head use `small` w600.
- **Content gutters in the side bar** differ: Inspector 12 px (`s5`), Search and SCM 4–6 px plus
  8 px of row padding, Chat 8 px, Environments 4 px.
- **Radii and spacings off the token scale:** `BorderRadius.circular(7)` (badge,
  `controls.dart:335`), `9` (switch, `:474`), `3` (glyph and meter, `glyph.dart:44`,
  `env_page.dart:523,530`), focus-halo `radiusControl + 2` (`text_input.dart:193`); 29 literal
  paddings and gaps of 5, 7, 9, 10, 14, 18 or 20 px outside `Metrics.s1–s7`.
- **Derived tints outside the tokens and the contrast test:** diff rows
  `c.danger/ok.withValues(alpha: 0.12)` (`scm_page.dart:383-385`), approval border
  `c.accent.withValues(alpha: 0.5)` (`chat_page.dart:1283`). `tokens_test` doesn't check
  `dangerText`/`okText` on these tints.
- **About** hard-codes "Axiom (Flutter) 0.1.0" (`settings_page.dart:374`). It matches `pubspec.yaml`
  today but will drift.
- **Explorer selection marker:** it is meant to be brass "while the tree has focus", but it uses
  `s.focused`, which is only true for keyboard focus (`explorer.dart:309`), so after a click it shows
  `ink3`.
- **Fix:** Add `AxiomType.sectionHeading` and a `Metrics.sideGutter`, add `radiusPill`/`radiusChip`
  tokens, move the derived tints into `AxiomColors` (`diffAddBg`, `diffDelBg`, `approvalBorder`) so
  the contrast test covers them, and read the version from the build (`--dart-define` or
  `package_info`-style generated code).

---

## Considered and not reported
- **re_editor re-highlighting on every `CodeView` build** (a new `CodeHighlightTheme` each build).
  `CodeHighlightTheme` and `CodeHighlightThemeMode` have value equality
  (`third_party/re_editor/lib/src/code_theme.dart:25-37`, `:71-81`), so no re-highlight. Not a
  defect.
- **Chat transcript non-lazy:** it uses `ListView(children:)`, which still builds elements lazily, and
  `_itemViews` reuses widget instances; Markdown pieces are cached (`markdown_view.dart:96-140`).
  Acceptable. Only the streaming last piece is re-parsed and re-highlighted per delta (quadratic over
  a very long code block); I'd note it only if profiling shows it.
- **Dialog focus restore:** this relies on Navigator/`FocusScopeNode` memory and is consistent with
  the a11y test. Escape closes `showAxDialog` through `ModalRoute`'s dismiss action.
- **Literal colours outside `tokens.dart`:** none found in scope (only `withValues` derivations,
  listed under FUI-37).
- **No Material or Cupertino imports** in scope.

## Areas reviewed and not reviewed
**Reviewed in full:** `lib/ui/{tokens,theme,pressable,controls,overlays,splitter,tabs,text_input,
surfaces,icons,glyph}.dart`; `lib/shell/{shell,palette,settings_page}.dart`;
`lib/editor/{editor_area,code_view}.dart`; `lib/editor/lsp/semantic_tokens.dart`;
`lib/console/{console,output,terminal_view}.dart`; `lib/files/explorer.dart`;
`lib/scm/scm_page.dart`; `lib/search/search_page.dart`; `lib/inspector/inspector_page.dart`;
`lib/env/env_page.dart` (page, rows, side; `PlanDialog` skimmed).

**Reviewed in part:** `lib/editor/lsp/lsp_view.dart` (lifecycle, hover, signature, completion popup,
shortcut map, layout delegate; not the request and edit logic of each LSP feature);
`lib/editor/lsp/underline.dart` (skimmed); `lib/chat/chat_page.dart` (side, view, composer, model
picker, item view head, approvals; not `DiffBlock` rendering details or `_RestoreConflict`);
`lib/chat/markdown_view.dart` (piece cache, table, code block; not every inline case);
`lib/chat/re_highlight_bridge.dart`; `lib/ui/fuzzy.dart` (ranking only); `lib/main.dart` (theme
and lifecycle).

**Not reviewed:** `lib/ui/gallery.dart` (a development surface behind `--gallery`), `lib/ui/icon_data.dart`
internals, `lib/chat/models.dart` (except confirming that it uses `AxMenu`), `lib/state/*` and
`lib/daemon/*` (another reviewer's scope; only read where a UI finding depended on them),
tests and golden images, and platform runners (except window minimum sizes).

**Not done:** nothing was run (no flutter, dart or cargo). No screenshots of the Flutter app were
taken. The `docs/assets/tauri` reference screenshots predate the brass palette, so I didn't use them
for pixel comparisons.

## Visual checks worth doing in the running app
1. **FUI-01:** set the window to about 1000 px wide with the side bar and dock (Mod+L) open, open a
   file, and press Mod+\.
2. **FUI-02:** Chat (rail) with no conversation open: click the model name at the right of the
   composer. Also File → View menu in a window about 600 px tall.
3. **FUI-03:** Settings (Mod+,) → Appearance → toggle Light/Dark while the dialog is open.
4. **FUI-04:** shrink the window to its minimum (900×560) and open Settings.
5. **FUI-09:** hover rail icons (Source Control, Environments) and the gear at the top right.
6. **FUI-10:** with a language server ready, hover a symbol near the right edge or bottom of the
   editor, and in a split right group.
7. **FUI-11:** open about 25 files (Mod+P), then Ctrl+Tab through them, and try the mouse wheel on the
   tab strip.
8. **FUI-12:** split the editor (Mod+\), focus the right group, Ctrl+F, then Replace….
9. **FUI-14:** open a `.rs` file with rust-analyzer ready, switch the theme from the palette ("Theme:
   Light"), and look at type and function colours without typing.
10. **FUI-17 and FUI-27:** in Output, run `printf '%0300d\n' 0`, then switch to Problems and back
    (the scroll and prompt are lost, and the long line is clipped).
