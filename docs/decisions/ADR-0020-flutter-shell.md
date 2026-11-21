# ADR-0020: The desktop shell moves to Flutter; Tauri is retired after a gated migration

Status: **Accepted** (2026-11-22) by the project owner ("Switch the entire app's UI to Flutter",
then "Full Flutter rewrite" when asked to choose between a full rewrite, a Flutter shell with the
editor in a web view, and keeping Tauri with a Flutter look).

**Implemented** (2026-11-22, FL-QA-001): the gate of §7 is recorded in
`docs/roadmap/verification/FL.md`. The Flutter shell is the app, and the Tauri shell and `editor/`
are retired; their last commit is tagged `tauri-shell-final`.

Supersedes ADR-0012 decision 1 (shell technology); the process split it kept is unchanged.
Amends:
- ADR-0004: the editor is no longer Monaco, and editor-bridge-1 retires with the Tauri shell;
- ADR-0013: visual identity, see decision 4;
- ADR-0017: Chat's place on the rail, see decision 6;
- ADR-0019 §2: where the M5 preview is shown, see decision 8.

ADR-0003 (IPC), ADR-0005 (contract source of truth), ADR-0006…0011 and ADR-0014…0018 are
unaffected: every daemon service and protocol 0.1–0.4 stays as it is.

## Context
- **The owner's own Flutter work.** The owner builds their other product's UI in Flutter
  (custom widgets, no Material or Cupertino, the "Temporal Tactility" direction), and asked for
  Axiom's UI to combine that direction with the current one.
- **Measured frame drops in the Tauri shell.** These were measured on the owner's machine
  (KDE Wayland, 125 % scale, NVIDIA, WebKitGTK 2.54.2), driving the release app in a private
  KWin on the same GPU at the owner's window size:
  - editor scrolling 44 fps, typing 44 fps;
  - tab switches and sidebar toggles dropping frames;
  - GTK 3 renders at 2× for a 1.25 scale and the compositor scales down, so WebKit paints about
    2.6 times the pixels shown;
  - box-shadows alone cost about 6–10 fps there.
- **What Flutter gives.** It renders the whole UI itself (Impeller or Skia, GPU), and handles
  fractional scales without that over-draw.
- **The editor.** It is the hard part. Monaco is web-only, and Flutter's Linux desktop target
  has no maintained embedded web view, so keeping Monaco would mean a second window. The owner
  chose a full rewrite with a Flutter-native editor.

## Decisions

### 1. The shell is a Flutter desktop app, `apps/desktop-flutter`
- **Language and platforms.** Dart, Flutter stable; Linux and macOS first, Windows later
  (deferred by the owner).
- **What it is.** One process that presents. Like the Tauri shell, it never touches workspace
  files or runs processes itself, except spawning and supervising `axiomd`.
- **Widgets.** It is built on Flutter's `widgets` layer only, with no Material or Cupertino
  controls. Axiom's own controls carry semantics (names, roles, focus) for screen readers and
  keyboard use.

### 2. The boundary is unchanged: JSON-RPC over the daemon's stdio
- **The client.** A Dart client, `lib/daemon/`, does what `crates/desktop-bridge` does for the
  Tauri host:
  - spawns `axiomd`, NDJSON framing, requests, notifications, `$/cancel`;
  - supervision and restart;
  - the 0.1–0.4 handshake.
- **Wire types.** Dart types are generated from `schemas/` (`tool/gen_types.dart`), as the
  TypeScript ones are; the protocol crate stays the single source of truth.

### 3. Editor, terminal, Markdown
- **Editor.** `re_editor` (MIT) for the text model, its layout and input, with `re_highlight`
  (MIT) for syntax colouring. On top of it Axiom adds:
  - tabs and a split;
  - find and replace;
  - the LSP client over `lsp/*` (diagnostics, completion, hover, signature, definition,
    rename, formatting, code actions);
  - Git gutter marks and a side-by-side diff view.

  If `re_editor` cannot meet the frame budget (decision 5) on large files, the fallback is
  Axiom's own editor widget. That needs a new ADR.
- **Terminal.** `xterm` (Dart, MIT) over `terminal/*`.
- **Markdown in Chat.** `markdown` (BSD-3) parsed into Axiom's own widgets; code blocks use the
  editor's highlighter.

### 4. Visual language: "Precision Instrument × Temporal Tactility"
ADR-0013's structure stays: the rail, side bar, editor area, console, status bar with the hint
line, the palette, and IBM Plex. Its surfaces take Temporal Tactility's material grammar:
- **Depth by tone steps, with no blur or glass.** Recessed (wells, the console, the composer's
  bezel), ground, raised (tabs, rows, chips), sheet (menus, dialogs, the palette).
- **Raised surfaces** get a 1 px lit top edge and a 1 px contact shade. There is one soft
  shadow, on sheets only. It is drawn by Flutter, not stacked box-shadows.
- **Dark theme.** Charcoal, not black, with dark grooves between rows.
- **Selection.** On the rail and on tabs, the selected item has a short underline in ink,
  rather than a pill.
- **Colour means one thing each.** The accent marks focus and the primary action; red only
  marks errors and destructive actions. A word or shape always goes with a colour.
- **The composer.** A bezel ring around a lighter inner surface; two-tone labels (model bright,
  effort dim); one ink circle for Send.
- **Numbers.** Tabular figures for every number, time and count.
- **Answer first.** Each page opens with one plain sentence that answers the question you came
  with, before the detail:
  - Chat: "Claude Code is editing 2 files";
  - Source Control: "3 changes on main, 1 ahead";
  - Environments: "Rust ready · Node needs attention".

  It is the only large type on the page.
- **Starting values.** From the owner's Temporal Tactility spec; contrast is checked per token in
  both themes.
  - **Light:** ground `#F2F3F5`, recessed `#E5E7EB`, sheet `#FFFFFF`, bars `#FAFAFB`, ink
    `#0D0E10` / `#5B5F66` / `#868A91`, hairline ink 10 %, lit edge white 75 %, contact shade ink
    8 %, error `#C8322B`.
  - **Dark:** ground `#0E0F11`, recessed `#070809`, sheet `#1B1D21`, bars `#15171A`, ink
    `#F3F4F6` / `#A3A7AE` / `#7D8189`, hairline white 9 %, lit edge white 10 %, contact shade
    black 50 %, error `#FF6259`.
  - **Rules:**
    - hairlines only between surfaces of the same tone;
    - no outer borders on raised surfaces, no glows, no gradients;
    - status dots sit in a cut-out ring (the "older Discord" reference);
    - radii are 10 px for raised items and 14 px for sheets and wells.
- **Motion.**
  - Springs with a slight overshoot (1.5–4 %), only in response to something the person did.
  - No continuous loops: the spinner and the LED are smooth, not stepped, and stop when the
    window is hidden.
  - Reduced and Off settings, as in ADR-0013.

### 5. Performance budget
- **The target.** Every interaction renders at the display's refresh rate: no frame over
  16.7 ms at 60 Hz, measured at the owner's 125 % scale and window size. The interactions are
  scrolling the editor and the tree, typing, switching tabs and rail pages, the palette, the
  splitters, and Chat streaming.
- **How it is measured.** Frames come from Flutter's `FrameTiming` (build and raster)
  in an integration test, against a real daemon, in a private compositor.
- **Startup.** It must not be worse than the Tauri shell's P2 (explorer painted).

### 6. Chat is first-party, under Files, and reaches agent harnesses
- **The rail order:** Files, Chat, Search, Source Control, Environments.
- **Chat is Axiom's own surface.** It talks to Axiom's agent (any configured model) and to
  external agent harnesses through ACP (Claude Code, Codex, Gemini CLI; ADR-0016) in the same
  conversation list and composer.
- **Harness conversations without a folder.** They are allowed: the daemon gives them a
  scratch directory under the data directory. Today they need folder scope.
- **Installing harnesses.** A harness that is missing can be installed from Chat or
  Settings → Models, by the daemon, into the data directory (`agents/<id>/`), using the
  environment's `npm` and showing each step. The person still signs in to the harness
  themselves.

### 7. The migration is gated, as the Qt→Tauri one was
- The Tauri shell stays the default app until the Flutter shell passes, on Linux and macOS:
  - the M1–M4 exit scenarios;
  - the a11y checks;
  - the frame budget;
  - packaging (deb, rpm, app bundle; desktop entry and icon).
- Then `scripts/run.sh` switches. The Tauri shell is tagged `tauri-shell-final` and removed.
  `editor/` (Monaco) and editor-bridge-1 go with it.
- New UI features land in the Flutter shell only from now on. M5 (visual intelligence) waits
  for the switch.

### 8. M5's preview
The Flutter shell has no embedded web view on Linux, so the previewed app opens in the person's
browser (or a browser window the daemon starts). The injected adapter already talks to the
daemon directly (ADR-0019 §1); the Inspector in the shell shows the selection, hierarchy,
properties and source, and drives inspect mode through the daemon. ADR-0019 is otherwise
unchanged.

### 9. Tests
- **Unit and widget tests:** `flutter_test`, next to the code.
- **End-to-end:** `integration_test` against a real `axiomd` with a temporary data directory,
  in a private Xvfb or virtual KWin, never on the developer's desktop (as `headless.sh` does
  today).
- **CI:** Ubuntu and macOS jobs; `scripts/check.sh` runs `flutter analyze` and the tests.

### 10. Licences
Flutter and Dart are BSD-3. Packages must be MIT, BSD or Apache-2.0 (ADR-0011). A check in
`scripts/check.sh` lists every Dart dependency's licence and fails on anything else.

## Consequences
- **About M1.5–M4's UI work is rebuilt.** Every daemon service, contract and daemon test is
  kept. The Tauri e2e suites are the specification for the Flutter ones.
- **The editor loses Monaco's breadth** (multi-cursor details, minimap, language
  configurations). Each gap is either implemented or recorded as a deviation in the parity
  task.
- **Two shells exist until the switch.** Fixes to the Tauri shell are limited to defects.

## Alternatives rejected
- **A Flutter shell hosting Monaco in a web view:** no maintained embedded web view on
  Flutter Linux; a separate editor window is not an IDE.
- **Keep Tauri and apply the look:** fixes the shadows' cost but not the 2× over-draw at
  fractional scales; the owner chose Flutter.
