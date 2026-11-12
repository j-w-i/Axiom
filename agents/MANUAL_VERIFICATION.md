# Manual verification protocol (desktop shell)

_Introduced by task T-10. Some behaviours cannot be automated in CI: input
methods, clipboard with other applications, drag and drop, HiDPI scaling,
screen readers, and rendering on a real GPU. `docs/architecture/testing.md`
requires them to be *recorded*, not assumed. This file is the procedure; the
results are recorded per milestone in `docs/roadmap/verification/`._

Run it on one real machine per OS, on the candidate build, with the app started
**normally** (not headless, not over a remote session).

## 0. Record

| Field | Value |
|-------|-------|
| Build | commit + package version |
| OS / desktop | e.g. Windows 11 24H2, macOS 15.2, Fedora 44 / GNOME 49 (Wayland) and X11 |
| Display | resolution, scale (100 % / 150 % / 200 %), GPU |
| Webview | WebKitGTK / WebView2 runtime / WKWebView version |
| Tester, date | |

## 1. Input and editing

1. Type ASCII, then accented Latin, then **Japanese (kana → kanji conversion)**
   in the editor. The candidate window must appear at the caret and the
   committed text must be correct, including composition cancelled with Escape.
2. Type with a dead-key layout (e.g. `us-intl`, `de`) and confirm the
   composed characters are right.
3. Select text with the mouse, drag to move it, and undo (`Ctrl/Cmd+Z`) twice.
4. `Ctrl/Cmd+S` inside the editor saves; `Ctrl/Cmd+S` with focus in the explorer
   saves the active document too.
5. Arrow keys, Home/End, PageUp/PageDown, and `Ctrl/Cmd+W` behave as listed in
   the menus.

## 2. Clipboard

1. Copy text from another application → paste into the editor: the text must
   arrive unmodified (including multi-line).
2. Copy from the editor → paste elsewhere.
3. Copy a selection from the **output panel** via its context menu, and via
   "Copy output"; paste elsewhere.

## 3. Files and windows

1. Drag a folder from the file manager onto the window: the OS default is
   acceptable *only if* it is obvious nothing happened; record what happens.
2. Open a folder, drag a file within the explorer (no behaviour is defined in
   M1; record it).
3. Close the window with unsaved changes → the prompt appears; Cancel keeps the
   window and the text; Save writes and closes.
4. Kill the app from the OS while a document is dirty, relaunch: the crash
   recovery must restore the tab with the unsaved text and show the banner.
5. Move the window between monitors with different scale factors; the UI must
   re-scale without blurring or clipping.

## 4. Accessibility

1. Screen reader (Linux: Orca; Windows: Narrator or NVDA; macOS: VoiceOver):
   navigate title bar → explorer → tabs → editor → output. Every control must be
   announced by name and role; the tree must announce level, expanded state and
   selection.
2. Tab through the whole window: focus must always be visible, and no control
   may be unreachable.
3. High contrast / forced colours: text must remain readable.

## 5. Rendering and performance on a real GPU

1. Scroll the editor and the explorer with 5 000 entries expanded: no tearing,
   no white flashes, no flicker (Linux WebKitGTK/NVIDIA is the known risk).
2. Start the app three times and note the time to the explorer appearing
   (`time` a stopwatch, or read the `app_info.uptimeMs` console value).
3. Watch memory for two minutes idle with a document open (task manager): it
   must not grow without bound.
4. Resize the window across its minimum size; the side bar must collapse below
   900 px without layout breakage. **Drag a corner inwards as far as it goes:
   the window must stop at 720 × 480** (content size). The automated suite can
   only check this on Linux: on macOS and Windows a programmatic resize is not
   bounded the same way as a user's drag (`e2e/geometry.mjs`, check 2).
5. Resize the window, quit with `Ctrl/Cmd+Q`, relaunch: the same size and
   position come back (CI checks this too, `e2e/geometry.mjs` check 4).

## 6. Sign-off

Each item: **pass** / **fail (with what happened)** / **not applicable (why)**.
A failure that touches data integrity (2.3, 3.3, 3.4) blocks the milestone.
Record the result in `docs/roadmap/verification/<milestone>.md` and link the
screenshots.
