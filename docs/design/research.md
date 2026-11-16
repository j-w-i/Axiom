# M1.6 design research: creative-tool and editor UIs

Status: research draft, 2026-11-17. Desk research only: no application was installed or run for this document. All observations come from vendor manuals, release notes, official blogs and, where marked, forums or third-party sources. This document records what those tools do. It does not design Axiom.

**Conventions**

- **Observed (source):** a factual claim with a URL. Vendor documentation unless marked *(third-party)* or *(forum)*.
- **Inference:** my reading or extrapolation. Not verified.
- **Unverified:** looked for and not found. Treat as unknown.

**Version caveat.** Image-Line's online manual tracks the current release, not 21.x. After 21 came "FL Studio 2024"; the manual says there was no 22 or 23 ([new in 21](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_new_21.htm)), and the General Settings page already mentions FL Studio 2026 ([general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm)). Unless a release-note page states a version, FL observations below mean "current manual (2025/2026)". The Resolve interface chapter I could read is from the **18.5** Reference Manual. Resolve 19/20 differences are noted only where the release notes give them.

---

## 1. FL Studio (primary)

### 1.1 Surface, depth and colour

**Observed**
- The theme system arrived in **21.0** (6 Dec 2022) under General Settings > Miscellaneous > Theme ([new in 21](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_new_21.htm)). **21.2** (26 Oct 2023) added waveform colours and a frequency-to-colour map (same page).
- The Theme window (**F10**) has four global knobs: **Hue**, **Saturation**, **Brightness** and **Contrast** (relative brightness between elements). It adds a **Text** brightness knob and *semantic* colour pickers: Selection, Highlight, **Mute LEDs**, **Option LEDs**, odd/even Steps, per-dB-band **Meter** gradients and Waves. It also has a **Light mode** switch that keeps extreme settings legible, **A/B** slots for comparing two themes, and file-based presets ([themes](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_themes.htm)).
- The manual gives **no numeric ranges or defaults** for the knobs (same page). Unverified.
- Toolbar right-click > **Flat buttons** removes shading and highlight colour. The View menu describes it as toggling the "3D shading" on buttons ([toolbar panels](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm), [view menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_view.htm)).
- Toolbar panels can be split by optional **Separators** ([toolbar panels](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)).
- The workspace background can be a colour, an image or (Windows only) an HTML "dynamic wallpaper" ([view menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_view.htm)).
- Peak meters turn orange above 0 dB ([levels & mixing](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/mixer_levelsandmixing.htm)).

**Inference**
- FL's depth model is "bevel optional": shading is the default and flat is one click away. Depth is a user preference, not part of the structure.
- The design move that matters is theming at **two layers**: a few global perceptual controls (hue, saturation, brightness, contrast) plus *semantic* slots (selection, highlight, mute, option, meters). Users can retint everything without breaking state meanings.

### 1.2 Typography

**Unverified.** I found no official statement of FL's UI typeface or sizes. Forum threads say a font ships with the installer ([forum](https://forum.image-line.com/viewtopic.php?p=1003735)), and that Image-Line staff once changed the "kerning" of the browser font ([search result, forum](https://forum.image-line.com/viewtopic.php?p=1371791)). Scaling is documented instead:
- **Main GUI Scaling** follows the system setting. 125–200 % is suggested for 4K–5K displays.
- Pop-ups and toolbars can be scaled separately.
- **Thick lines** applies at ≥200 %.

([general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm))

### 1.3 Panel and window organisation

**Observed**
- F-keys toggle the main windows (Playlist **F5**, Channel rack **F6**, Piano roll **F7**, Plugin picker **F8**, Mixer **F9**, Browser **Alt+F8**). **F12** closes all windows, **Shift+Ctrl+H** restores the default layout, and named layouts (Desktop, Alternate, Tablet, custom) can be saved ([view menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_view.htm)).
- Windows snap together when dragged (Alt prevents it), and editors and plugins can be **Detached** for multi-monitor use ([basics](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm)). Since 21.0.3, detached plugins open on FL's screen ([new in 21](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_new_21.htm)).
- The toolbar is a row of reorderable, hideable panels saved as presets: transport, Global Snap, time, an **Output** panel (scope, spectrum, peak meter), a **CPU & Memory** panel and a Communications panel. **Ctrl+F11** hides it ([toolbar panels](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)).

**Inference**
- FL is a floating-window MDI inside one frame, with snapping and layout recall rather than strict docking. F-keys, magnetic snapping and "realign" make the free-form model usable.
- For an IDE, the always-visible CPU/memory panel is a good precedent for showing **daemon health** in the chrome.

### 1.4 Components and interaction

**Observed:** knob and slider behaviour ([basics](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm)).
- Drag vertically to change a value.
- **Ctrl**-drag for fine adjustment.
- Detents at defaults and other useful points; **Shift** bypasses them.
- **Alt+click** or middle-click resets to default.
- Right-click offers "Type in value", "Set" (a larger control for precision) and controller linking.
- Values can be shown in context units (e.g. dB).
- LCD number displays can be dragged, stepped by clicking the top or bottom edge, or typed into.

**Observed:** animation and refresh ([general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm)).
- **Animations** is a scale from "Don't distract me" (all off) to "Entertain me".
- **Animation refresh rate** has three steps: ¼, ½, or the full monitor refresh rate ("Ultrasmooth").
- A **Smoothing** slider controls GUI and control motion.
- Transparent windows and menus exist but are discouraged by the manual.
- The Theme page notes that drag-animation effects are switched off by the lower animation settings ([themes, via search](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_themes.htm)).
- In 21.0, the Automation "Add target links" button pulses while active ([new in 21](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_new_21.htm)).
- In 2024, tool buttons show their pressed state on hover ([new in 24](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_new_24.htm)).

### 1.5 Discoverability: the hint bar

**Observed** ([toolbar panels](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm))

The Hint Bar sits under the title in the toolbar. It does the following:
1. Hovering any control shows a short description.
2. While a control is dragged, the bar shows its **live value**.
3. Right-click copies the value.
4. Hovering a peak meter reads out the level under the pointer.
5. **Capability icons** say what the hovered item supports: automatable or recordable, MIDI-linkable, has a right-click menu, success or failure, warning, or long operation.

An optional **Extended Hint Panel** floats in the lower-left of the desktop. It is click-through, so it never blocks input.

Related:
- A General Settings "High visibility" option copies menu captions into hints and draws hover rectangles around switches and icons ([general settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm)).
- The Communications panel holds **Gopher**, an LLM assistant trained on the manual; the manual warns it can be wrong ([toolbar panels](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)).
- Gopher arrived in FL Studio 2025 ([MusicRadar, third-party](https://www.musicradar.com/music-tech/fl-studio-2025-introduces-chatgpt-style-ai-assistant-that-offers-music-production-advice-and-helps-you-use-the-daw)). In 2025.2 it is better at writing Piano-roll scripts and sits on the Welcome window ([2025.2 news](https://www.image-line.com/fl-studio-news/fl-studio-2025-2)).

**Inference:** the capability icons are the under-appreciated part. Besides *what* a control is, they say *what else you can do with it*. That is exactly the discoverability gap IDEs have with context menus and keyboard alternatives.

### 1.6 Performance

**Observed**
- **FL Studio 2024** moved UI rendering to **Blend2D**, which Image-Line says gives faster graphics at lower CPU cost ([new in 24](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_new_24.htm)).
- Some plugins received vector UIs in 2024.2 (same page).
- 2025.2 caps open plugin editors at **25** to avoid high CPU use ([2025.2 news](https://www.image-line.com/fl-studio-news/fl-studio-2025-2)).
- I found no independent benchmark of the Blend2D change (search, no result).

**Inference:** Blend2D is a CPU vector rasteriser, so FL's gain comes from cheaper drawing, not from GPU offload. Treat the idle-CPU claims as vendor statements.

### 1.7 Pages a designer can open

- [Toolbar panels](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/toolbar_panels.htm)
- [Theme settings](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_themes.htm)
- [Interface basics](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/basics_interface.htm)
- [View menu](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/menu_view.htm)

### 1.8 FL Studio: Adopt / Adapt / Reject

| Trait | Verdict | Why, for 8+ hour IDE sessions |
|---|---|---|
| Hint bar: hovering any control describes it; dragging shows the live value | **Adapt** | Add a status-bar hint segment showing the hovered control's action, its shortcut and its current value (e.g. a split ratio while dragging). It costs no layout space and never covers content, unlike tooltips. |
| Capability icons in the hint (automatable, has context menu, linkable) | **Adapt** | Show small glyphs for "has context menu", "has shortcut" and "agent can operate this". This teaches hidden affordances without tutorials. |
| Extended Hint Panel is click-through | **Adopt (principle)** | Any overlay Axiom adds (hints, agent progress) must never take input or focus by accident. |
| Theme = 4 global perceptual knobs + semantic colour slots (selection, highlight, mute/option LEDs, meters) + A/B compare | **Adopt** | Fits Axiom's token-only CSS rule. A user can retint the whole shell while state meanings (error, running, modified) stay intact. A/B makes tuning quick. |
| "Light mode" switch that protects legibility at extreme settings | **Adapt** | Clamp contrast in the token generator (see Linear, §8), not as a user-facing toggle. |
| Animation level scale ("Don't distract me" … "Entertain me") + refresh-rate choice | **Adapt** | Offer about three levels (off, subtle, full), default "subtle", and follow `prefers-reduced-motion`. A long session needs motion that can be fully turned off. |
| Fine-drag modifier, Alt-click reset, type-in, detents bypassed with Shift | **Adopt** for Axiom's few numeric controls (font size, split ratios, zoom) | Precision and reset-to-default build trust. |
| F-key per main window (F5–F9) + "realign" + saved layouts | **Adapt** | Give each major surface (editor, inspector, agent, terminal, problems) a stable single-chord toggle, plus a "reset layout" command. |
| Free-floating MDI windows with magnetic snapping | **Reject** as default | Free floating causes window-hunting over long sessions. Prefer docked regions and allow detaching specific panels (e.g. the inspector onto a second monitor). |
| Bevels / 3D button shading as default | **Reject** | It costs pixels and contrast budget, and FL itself makes it optional (Flat buttons). |
| Always-visible CPU/memory toolbar panel | **Adapt** | Show a compact daemon-health / build-load indicator. Keep it numeric and quiet. |
| HTML wallpaper / transparent windows | **Reject** | Distracting and costly. FL's own manual discourages transparency. |
| In-app LLM help trained on the manual (Gopher), labelled fallible | **Adopt (principle)** | Axiom is LLM-native. Help answers should be grounded in Axiom's docs and say plainly that they can be wrong. |

---

## 2. DaVinci Resolve (18.5 manual; 19/20 deltas)

### 2.1 Organisation: the page bar

**Observed** ([Resolve 18.5 Reference Manual, interface chapters, pp. 13–14, 50–56: unofficial mirror of the Blackmagic PDF](https://ltbits.github.io/davinci-resolve-manuals/DR18/DR18-RM01-DaVinciResolveInterface.pdf))
- **Page bar.** Seven workflow pages (media, cut, edit, Fusion, colour, Fairlight, deliver) are switched from always-available buttons at the bottom, in workflow order. The bar can go icons-only, hide single pages, or be removed entirely. **Shift+2…8** switch pages, **Shift+1** opens the Project Manager and **Shift+9** Project Settings.
- **Interface Toolbar.** Each page has one along the top that toggles its panels. It can be icons-only, and each toggle can take a shortcut.
- **Panel focus.** Shared shortcuts go to the focused panel. An optional preference draws a **red line at the top** of it.
- **Layouts.** Single or **dual screen** per page. Layout presets can be saved, updated, exported and imported, and **Reset UI Layout** restores the default. Media Pool bins and scopes can float.
- **Full Screen** is recommended for colourists because the light OS title bar distracts them.
- **Resolve 20** adds vertical-video layouts and dual-screen windows on ultrawide displays ([20.0 readme](https://www.blackmagicdesign.com/us/support/content/readme/a6e2bbb59c294d728d131fa21d18676b)). 20.1 adds a **Shift+Space** search for effects, generators and titles on every page ([third-party tutorial](https://creative-video-tip.squarespace.com/tutorials/davinci-resolve-201-new-features)).

### 2.2 Colour, typography, density

**Observed**
- I found **no user theming** and no in-app UI scaling. Users have asked for both for years. A 2021 forum reply suggests using OS display scaling instead ([forum](https://forum.blackmagicdesign.com/viewtopic.php?p=773979), [forum](https://forum.blackmagicdesign.com/viewtopic.php?p=1009895), [forum](https://forum.blackmagicdesign.com/viewtopic.php?p=894398)). Forum only; not verified for 20.
- The PDF's embedded fonts are Noto Sans (seen in the PDF metadata). That is the manual's font, not the application's. The application's UI font is unverified.

**Inference**
- Resolve's fixed, low-saturation dark grey is deliberate: colour judgement needs a neutral surround, which the manual's Full Screen advice confirms.
- An IDE has a weaker form of the same need. Syntax colours are the "content", and chrome should not compete with them.

### 2.3 Keyboard and performance

**Observed**
- Keyboard Customization offers presets that imitate other NLEs (Premiere Pro, Final Cut Pro, Avid). Import and export are supported ([third-party guide](https://www.tourboxtech.com/en/news/davinci-resolve-keyboard-shortcuts-guide.html)).
- A forum thread says the Premiere preset is not fully faithful ([forum](https://forum.blackmagicdesign.com/viewtopic.php?p=510627)).
- Resolve 20 requires a GPU with ≥4 GB VRAM on Windows and Linux, and Metal or Apple Silicon on macOS ([20.0 readme](https://www.blackmagicdesign.com/us/support/content/readme/a6e2bbb59c294d728d131fa21d18676b)). This covers image processing; UI frame-rate claims are unverified.

### 2.4 Pages a designer can open

- The page bar and Interface Toolbar figures in the mirrored manual PDF above, pp. 13–14 and 50–51.
- [Resolve product page](https://www.blackmagicdesign.com/products/davinciresolve)

### 2.5 Resolve: Adopt / Adapt / Reject

| Trait | Verdict | Why |
|---|---|---|
| Page bar at the bottom switches whole workspaces in workflow order | **Adapt (lightly)** | An IDE's "pages" (edit, inspect running UI, review agent changes) overlap far more than edit and grade do, so full page switches would break context. Use a few saved layouts with a quiet switcher, not a permanent navigation band. Resolve itself lets users hide the bar, which suggests experts don't need it. |
| Shift+digit page shortcuts, also listed in a Switch to Page menu | **Adopt** | Every layout and surface is reachable from the keyboard and discoverable from a menu or palette. |
| Per-page Interface Toolbar of panel toggles, icons-only option | **Adapt** | A compact row of panel toggles with an accessible name and shortcut on each, and no labels by default. |
| Focus indicator (red line on the active panel) | **Adopt (with Axiom's own focus token)** | With many panels, knowing where keystrokes go is essential. Make it on by default, not a preference. |
| Save, export and import layout presets; Reset UI Layout | **Adopt** | Cheap, and helps people with several machines and the team. |
| Neutral grey surround so the content's colour reads true | **Adopt (principle)** | Chrome stays low-chroma so syntax and diagnostics carry the colour. |
| Keymap presets imitating competitor tools | **Adopt** | VS Code and JetBrains keymaps lower the switching cost. Warn about imperfect fidelity, as Resolve's users found. |
| No theming, no UI scaling | **Reject** | Over 8+ hours, eyes, displays and preferences differ. Accessibility requires scaling. |
| Light window title bar in a dark app | **Reject** | Resolve's own manual concedes it distracts. Draw a matching dark title bar where the OS allows. |

---

## 3. Blender (4.x; manual 4.5 LTS)

### 3.1 Surfaces, organisation, density

**Observed**
- **Areas and editors.** The window is tiled into **Areas**, each holding an **Editor** ([areas](https://docs.blender.org/manual/en/4.5/interface/window_system/areas.html)).
  - Rounded corners mark area boundaries. Dragging a corner splits or joins areas, and Esc cancels.
  - Ctrl-drag snaps sizes.
  - The Preference **Border Width** sets the padding between areas, widening hit zones for pen, touch and accessibility ([interface prefs](https://docs.blender.org/manual/en/4.5/editors/preferences/interface.html)).
- **Workspaces.** These are predefined area layouts shown as tabs in the top bar. **Ctrl+PageUp/PageDown** switch between them. Defaults include Layout, Modeling, Sculpting, Shading, Animation, Scripting and others ([workspaces](https://docs.blender.org/manual/en/4.5/interface/window_system/workspaces.html)).
- **Regions.** Each editor has a header, a toolbar (**T** toggles it), a sidebar and an "Adjust Last Operation" panel for tweaking a command after running it. Headers can flip between top and bottom ([regions](https://docs.blender.org/manual/en/4.5/interface/window_system/regions.html)).
- **Hover-scoped shortcuts.** Many shortcuts act on the editor under the **mouse pointer**, not the clicked one ([areas](https://docs.blender.org/manual/en/4.5/interface/window_system/areas.html)).

### 3.2 Colour, typography, scaling

**Observed**
- **Theme editor.** Colours are set per editor, changes show live, and themes save as XML presets and can be installed or reset ([themes](https://docs.blender.org/manual/en/4.5/editors/preferences/themes.html)).
- **Interface preferences:** **Resolution Scale** (relative to detected DPI); **Line Width** Thin/Default/Thick; separate text anti-aliasing, subpixel AA and hinting; replaceable interface **and monospace** fonts; menu hover delays in 1/10 s; pie-menu timeouts in 1/100 s ([interface prefs](https://docs.blender.org/manual/en/4.5/editors/preferences/interface.html)).
- **Blender 4.0:** default UI font changed to **Inter**; outline opacity became themeable (for flat themes); tree views draw hierarchy lines; dropdown and context menus became **searchable with Space**, with recent items first ([4.0 UI notes](https://developer.blender.org/docs/release_notes/4.0/user_interface/)).
- Per-widget theme fields (inner, outline, roundness, shading) exist in the app's Theme > User Interface section. **Unverified:** the 4.5 manual page I read does not document their ranges.

### 3.3 Discoverability, keyboard, performance

**Observed**
- **Status bar:** the left shows **mouse-button actions and the active tool's keymap** for the current context (tapping **Alt** shows tool hotkeys); the middle shows task progress (estimate on hover, cancel button) and fading reports that open the Info editor; the right shows memory, scene statistics and version, chosen via right-click ([status bar](https://docs.blender.org/manual/en/4.5/interface/window_system/status_bar.html)).
- Tooltips explain the control and show its hotkey. **Python Tooltips** add the API path ([interface prefs](https://docs.blender.org/manual/en/4.5/editors/preferences/interface.html)).
- **Vulkan UI drawing:**
  - experimental in **4.3** ([Phoronix](https://phoronix.com/news/Blender-4.3-Beta));
  - the backend draws the whole UI with either OpenGL or Vulkan, never both ([Vulkan dev docs](https://developer.blender.org/docs/features/gpu/vulkan/));
  - described as production-ready by **4.5 LTS** ([CG Channel, third-party](https://www.cgchannel.com/?p=168478)).
- I found no source on idle redraw or CPU behaviour.

### 3.4 Blender: Adopt / Adapt / Reject

| Trait | Verdict | Why |
|---|---|---|
| Status bar shows what each mouse button and modifier does *right now* | **Adopt** | The keyboard counterpart of FL's hint bar: it teaches shortcuts during real work. Pair it with FL-style hover hints in one status segment. |
| "Adjust Last Operation" panel | **Adapt** | After an agent edit or refactor, show a small panel to tweak parameters or undo, instead of a modal dialog. This fits Axiom's verified AI edits. |
| Space-searchable menus, recent-first search | **Adopt** | Large menus stay usable without memorising them. Same idea as a command palette, but scoped to the open menu. |
| Separate interface and monospace font settings; Resolution Scale; Line Width Thin/Default/Thick | **Adopt** | Code (mono) and chrome (sans) need separate tuning. A thickness option matters on HiDPI and for low vision. |
| Border Width setting = gutter between areas, framed as accessibility | **Adopt** | Panel gaps are a token, and larger gaps enlarge resize hit zones. |
| Corner-drag split/join of areas | **Reject** | Powerful but hard to discover, and it often triggers by accident. Use explicit split commands and drop-zones. |
| Shortcuts scoped by hover position | **Reject** for the editor | In a text-heavy IDE, keystrokes must follow keyboard focus. Hover scoping causes errors when the pointer rests over a different panel. |
| Full per-editor theme matrix (hundreds of colours) | **Reject** as the primary theming UI | High authoring cost, and themes go stale when new UI appears. Generate from a few inputs (§8) and allow overrides. |
| Workspace tabs (task layouts) | **Adapt** | Pairs with Resolve's pages: named layouts with Ctrl+PageUp/Down cycling. |

---

## 4. Zed

### 4.1 Rendering and performance

**Observed**
- **GPUI** targets 60–120 fps (an **8.33 ms** frame budget at 120 Hz) with one instanced shader per primitive (rect, shadow, glyph, icon, image). Rounded rects use signed distance functions, shadows a closed-form Gaussian approximation, and text is OS-shaped, cached and atlas-packed with up to 16 subpixel variants ([Zed blog, Mar 2023](https://zed.dev/blog/videogame)).
- **Frame pacing:** frames render in **under 4 ms**, with stable 120 fps on ProMotion. Zed redraws only when the scene is dirty, keeps presenting for **1 s** after the last input so the display stays at its high refresh rate, and pauses the display link for inactive windows ([Zed blog, Feb 2024](https://zed.dev/blog/120fps)).

### 4.2 Colour, typography, layout, keyboard

**Observed**
- **Themes** ([themes docs](https://zed.dev/docs/themes)):
  - themes are JSON files, and a theme selector previews each theme live as you move through it;
  - `theme` takes `mode: system|light|dark` with separate light and dark choices;
  - `theme_overrides` patches individual tokens;
  - a web Theme Builder exports JSON.
- **Defaults** ([visual customization](https://zed.dev/docs/visual-customization)):

  | Setting | Default |
  |---|---|
  | `ui_font_family` | `.SystemUIFont` (`.ZedSans`, currently IBM Plex, is bundled) |
  | `ui_font_size` | 16 |
  | `buffer_font_size` | 15 (`.ZedMono`, currently Lilex) |
  | `buffer_line_height` | "comfortable", 1.618 |
  | Terminal | 15 px, line height 1.3 |
  | Project panel | 240 px wide, indent 20 |
  | Agent panel | 640 px |
  | `tabs.show_close_button` | "hover" |
  | `active_pane_modifiers.inactive_opacity` | 1.0 (off) |
  | `unnecessary_code_fade` | 0.3 |

- `ui_font_size` accepts 6–100 px ([docs commit, third-party mirror](https://git.secluded.site/zed/commit/0f4ebdfbca721614f3cadafc3b44e4fbf099afda)).
- **Keymaps** ([key bindings](https://zed.dev/docs/key-bindings)):
  - JSON keymaps with a context expression language;
  - base keymaps for VS Code, JetBrains, Sublime, Emacs and others, plus Vim and Helix modes;
  - a keymap editor;
  - when a chord is incomplete, the **status bar shows pending keys with a timeout** and the candidate bindings;
  - a configurable which-key menu;
  - about 1 s wait when one chord prefixes another.

**Inference**
- Zed's restraint is the lesson: almost no gradients, separation by tone, and colour reserved for git, diagnostics and selection.
- Its performance budget comes from owning the renderer. A Tauri webview cannot copy that. What Axiom *can* copy is the discipline: redraw only when dirty, put nothing on a timer while idle, and keep animations off the input path.

### 4.3 Zed: Adopt / Adapt / Reject

| Trait | Verdict | Why |
|---|---|---|
| 8.33 ms frame budget; redraw only when dirty; no idle animation | **Adopt (as a budget)** | Set Axiom's targets for webview frames and for idle CPU, both measurable with the existing benchmark harness. Zed shows that idle means idle. |
| GPU-native custom renderer | **Reject** | Out of scope under ADR-0012 (Tauri + React). Use the webview's compositor: animate only `transform` and `opacity`. |
| Separate UI vs buffer font sizes (16/15), golden-ratio buffer line height | **Adopt (structure)** | Separate tokens for UI and code. Exact values are for Axiom to choose. |
| Theme = JSON + overrides + live-preview selector + system light/dark follow | **Adopt** | Fits a token file. Live preview while moving through the selector is a cheap but strong touch. |
| Pending-chord indicator and which-key | **Adopt** | Shows the keyboard state on screen, the keyboard counterpart of the hint bar. |
| Close buttons shown on hover only | **Adapt** | Less noise, but keep a keyboard path and make hover targets large enough. |
| `unnecessary_code_fade` (dim unused code) | **Adopt (principle)** | Lower contrast as a semantic signal instead of adding colour. |
| Base keymaps imitating other editors | **Adopt** | Same reasoning as Resolve. |

---

## 5. Ableton Live 12

**Observed**
- **Info View** shows the name and function of the UI element under the mouse. It is toggled with the **?** key, and users can write their own **Info Text** on tracks, clips and devices, saved in the Set. A **Learn View** offers lessons inside the app ([first steps](https://www.ableton.com/en/live-manual/12/first-steps/)).
- Single window: **Tab** toggles Session and Arrangement (Alt+1/Alt+2 when Tab moves focus); optional second window; stacked Clip/Device detail views (Ctrl+Alt+3/4); a Status Bar for errors and note details; purple consistently marks scale awareness ([Live concepts](https://www.ableton.com/en/live-manual/12/live-concepts/)).
- **Theme & Colors:** theme choice or follow the OS; a **warm, cool or neutral tone**; **high contrast**; grid-line opacity, brightness, colour intensity and hue; "Reduced Automatic Colors". **Display & Input:** zoom, **"Outline View in Focus"** (border around the focused view), Tab focus navigation, hideable labels ([first steps](https://www.ableton.com/en/live-manual/12/first-steps/)).
- **Accessibility:** Live 12 added screen-reader support (VoiceOver, NVDA, JAWS, Narrator), Tab navigation nearly everywhere, a Navigate menu (Alt+N) and high-contrast themes ([accessibility chapter](https://www.ableton.com/en/live-manual/12/accessibility-and-keyboard-navigation/), [Live 12 release notes](https://www.ableton.com/en/release-notes/live-12/)).
- Zoom percentages and UI font: **unverified**.

**Inference**
- Live 12 is the best evidence here that a dense, mouse-heavy creative tool can add full keyboard and screen-reader coverage late, and that theming can be framed as *tone* (warm, cool, neutral) rather than raw colour pickers.

| Trait | Verdict | Why |
|---|---|---|
| Info View: hover description panel toggled with "?" | **Adapt** (merge with FL's hint bar) | One hint mechanism for Axiom, in the status bar, toggled by a shortcut. |
| User-authored Info Text on objects | **Adapt** | Notes on workspace items (files, inspected elements, agent tasks) appear in the hint area. This could also feed agent context. Product decision, flag only. |
| Theme tone warm/cool/neutral + high-contrast + grid opacity | **Adopt** | Small, perceptual controls. The tone setting maps to FL's hue knob with safer limits. |
| "Outline View in Focus" | **Adopt (default on)** | Same as Resolve's focus line: it must be visible. |
| Tab = focus navigation everywhere; Navigate menu | **Adopt** | Axiom's rule that every control has an accessible name and role needs a real keyboard traversal model to back it. |
| One accent hue per meaning (purple = scale) | **Adopt** | Give each semantic system its own hue and use it everywhere (e.g. agent activity). |
| Learn View lessons inside the app | **Reject** for now | Costly content upkeep. Hints, palette and docs cover discovery. |

---

## 6. Bitwig Studio (5.x, 6.0)

**Observed**
- **Interaction rules:** drag vertically, Shift for fine (even after the click), Ctrl/Cmd-click to type, double-click to reset. An **orange tint marks active** controls; inactive ones are neutral. **One panel has focus**, shown by a silver-tinted rounded outline, and panel shortcuts only work there. Panel toggles work mid-drag. Shortcuts are searchable and editable, with swappable mapping sets ([user interfacing](https://www.bitwig.com/userguide/latest/user_interfacing)).
- **Window layout:** a central panel that cannot be hidden, a secondary area below, an access area on the right (browser, project, mappings) and an Inspector on the left. Layout and footer vary by **display profile** ([window body](https://www.bitwig.com/userguide/latest/the_window_body)).
- **Browser** ([browsers](https://www.bitwig.com/userguide/latest/browsers), via search): opening it often puts focus straight in the search field, and **S** returns there from anywhere in the browser.
- **5.1** ([release notes](https://downloads.bitwig.com/5.1/Release-Notes-5.1.html)): grid-line strength control, narrower tracks, freely draggable dialogs, and shortcut zoom centred on the selection.
- **5.2** ([Bitwig story, Jul 2024](https://www.bitwig.com/stories/bitwig-studio-5-2-304/)): a rewritten graphics engine **painted on the GPU** to free CPU for audio, with fewer draw commands; requires DX12, Vulkan or OpenGL.
- **6.0** ([release notes, 11 Mar 2026](https://downloads.bitwig.com/6.0/Release-Notes-6.0.html)): right-clicking the top of the window sets UI-wide **midtone and black level**; grid darkness and timeline brightness are adjustable; clip headers are smaller and track headers flatten to one line on small lanes; keys 1–8 select tools (hold for a temporary switch); back/forward through editor states.
- **Panel transitions and animation: unverified.** I found no documentation of animated panel transitions or their durations.

| Trait | Verdict | Why |
|---|---|---|
| Orange = active, neutral = inactive: one accent for "on" | **Adopt (principle)** | A single "engaged" colour across toggles helps the eye. Axiom's own accent, not orange. |
| Focus = subtle rounded outline on one panel | **Adopt** | Lighter than Resolve's red line. Fits a calm chrome. |
| UI-wide midtone/black-level sliders | **Adopt** | Two perceptual sliders let users fit the UI to their room lighting. |
| Header that flattens to one line when space is tight | **Adapt** | Responsive density for panel headers, e.g. tab bars in narrow splits. |
| Browser opens with focus in search; one key returns to search | **Adopt** | Every list-heavy panel (files, symbols, agent history) should open ready for typing. |
| Back/forward through editor *states* | **Adopt** | Navigation history across views is a must in an IDE. |
| Hold-to-temporarily-switch tool | **Reject** for text editing | Quasi-modes clash with typing. Fine for the inspector's pick mode. |
| Display profiles (layout presets per screen setup) | **Adapt** | Combine with Resolve's dual-screen idea: a layout preset per monitor configuration. |

---

## 7. Contrast case: JetBrains Islands (2025)

**Observed**
- "Islands" was trialled in the 2025.2 EAP as One Island and Many Islands variants ([JetBrains blog, Jun 2025](https://blog.jetbrains.com/platform/2025/06/testing-a-fresh-look-for-jetbrains-ides/)). It became the **default in 2025.3** ([JetBrains blog, Dec 2025](https://blog.jetbrains.com/platform/2025/12/meet-the-islands-theme-the-new-default-look-for-jetbrains-ides/)).
- The design: rounded corners, more spacing, clearer tool-window boundaries and a more visible active tab ([heise](https://heise.de/-11108218)).
- An option gives tool windows a different background to emphasise the editor (Dec 2025 blog, via search summary; the blog returned HTTP 403 to my fetcher).

**Inference:** Islands separates surfaces with **gaps and tone** instead of borders. That is the same direction as Zed and Blender's Border Width. The cost is lost pixels around every panel.

| Trait | Verdict | Why |
|---|---|---|
| Panels as tone-separated "islands" with gaps | **Adapt** | Use a single small gap token, with the gap carrying a darker base tone. Avoid large radii and wide margins, which cost density on 13–14″ laptops. |
| Editor emphasised by a different background from tool windows | **Adopt** | The editor is the "canvas"; the chrome recedes. |

## 8. Contrast case: Linear

**Observed** ([Linear, "How we redesigned the Linear UI (part II)", Mar 2024](https://linear.app/now/how-we-redesigned-the-linear-ui))
- Themes are generated from **three inputs (base colour, accent, contrast)** instead of **98** hand-set variables.
- Generation happens in **LCH**, so equal lightness looks equal and elevation steps (background, panel, dialog, modal) stay even.
- The accent's influence on neutrals is limited.
- Inter Display is used for headings, Inter for the rest.

| Trait | Verdict | Why |
|---|---|---|
| Generate every surface/text token from base + accent + contrast in a perceptual space | **Adopt** | This is how to deliver FL-style hue/saturation/brightness/contrast knobs without breaking contrast: the generator enforces minimum contrast (WCAG) per tier. It fits `tokens.css`. |
| Display-cut face for headings | **Reject** | An IDE has almost no headings. One UI face plus one mono face is enough. |

---

## 9. Cross-application synthesis

### 9.1 Design principles for Axiom

1. **One hint channel that explains what's under the pointer and what the keyboard is doing.** Show the hovered control's name, action, shortcut and live value, plus capability glyphs, in a fixed status-bar segment, and show pending chords there too. Sources: FL hint bar and capability icons, Ableton Info View, Blender status-bar keymap, Zed pending-keystroke indicator.
2. **Theme from a few perceptual inputs, enforced by a generator.** Base hue, tone (warm/cool/neutral), brightness, contrast and accent produce every token in a perceptual colour space, clamped to minimum contrast. Overrides are allowed, and A/B comparison is supported. Sources: FL 21 theme knobs and A/B; Ableton tone and high contrast; Bitwig 6 midtone/black level; Linear three-variable LCH generator; Zed `theme_overrides`.
3. **Semantic colour slots are separate from the base theme.** Selection, focus, running, modified, error/warning and agent activity each own a hue that retinting cannot remove. Sources: FL's Selection, Highlight, Mute/Option LED and Meter slots; Bitwig orange-equals-active; Ableton purple-equals-scale.
4. **Low-chroma chrome; colour belongs to content and state.** Sources: Resolve's neutral surround for colour judgement; Zed's restrained palette; JetBrains editor emphasis.
5. **Separate surfaces by tone and gap, not by borders or bevels.** Use about three surface tiers, with gap width as a token that doubles as resize hit-area. Sources: Islands; Blender Border Width; FL's optional Flat buttons and Separators.
6. **Focus is always visible.** Exactly one panel owns the keyboard, marked by a quiet but unmistakable indicator, on by default. Sources: Resolve red focus line; Bitwig silver outline; Ableton "Outline View in Focus".
7. **Every surface has a one-chord toggle, and layouts are named, saveable and resettable.** Sources: FL F5–F9 and realign; Resolve layout presets, Reset UI Layout and Shift+digit pages; Blender workspaces with Ctrl+PageUp/Down; Bitwig display profiles.
8. **Keyboard parity everywhere.** Use Tab traversal, searchable menus and palette, keymap presets borrowed from other editors, and lists that open with focus in search. Sources: Ableton 12 accessibility; Blender searchable menus; Zed base keymaps and keymap editor; Resolve NLE presets; Bitwig browser search focus.
9. **Motion is optional and never in the input path.** Offer about three levels, honour reduced-motion, animate only compositor-friendly properties, and run nothing while idle. Sources: FL animation scale and refresh-rate choice; Zed redraw-when-dirty and 1 s present hold.
10. **Performance is a stated budget.** Budget frame time for input-to-paint and near-zero idle CPU, measured in CI. Sources: Zed 8.33 ms / <4 ms frames; Bitwig 5.2 GPU repaint to free CPU; FL 2024 Blend2D for lower UI CPU.
11. **Precision controls behave consistently.** Use one modifier for fine adjustment, one gesture for reset to default, type-in on every numeric control, and detents at defaults. Sources: FL knobs; Bitwig Shift-fine and double-click reset.
12. **After an operation, let users adjust it in place.** Show a non-modal panel to adjust or revert the last operation, which suits verified AI edits. Source: Blender Adjust Last Operation.

### 9.2 Anti-patterns observed

- **Default skeuomorphism that costs density and contrast.** FL's 3D button shading is optional for good reason. Bevels and gloss eat the contrast budget that state colours need.
- **Animation as entertainment.** FL's top level, "Entertain me", and transparent windows and menus, which FL itself discourages, are wrong for long sessions.
- **No theming or scaling at all.** Resolve users have asked for years; the workaround is OS scaling.
- **Mismatched OS chrome.** Resolve's light title bar distracts colourists, by the manual's own account.
- **Hover-scoped shortcuts.** Blender routes keys by pointer position, which suits viewports but misfires in text-first tools.
- **Hand-maintained mega-theme matrices.** Blender's per-editor theme and Linear's former 98 variables do not scale; generated tokens do.
- **Hidden gestures as the only path.** Blender's corner-drag split/join needs a visible, keyboard-reachable alternative.
- **Big rounded islands on small screens.** Islands-style spacing costs rows on laptops unless the gap is a tight token. This is inference; I found no measurements.
- **Keymap emulation sold as faithful.** Resolve's Premiere preset drew complaints; label emulations as approximate.

### 9.3 Concrete numbers found

| Item | Value | Source |
|---|---|---|
| FL animation refresh | ¼, ½ or full monitor rate | [FL general](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_general.htm) |
| FL UI scaling advice | 125–200 % on 4K–5K displays; "Thick lines" at ≥200 % | same |
| FL small scrollbars | 50 % width | same |
| FL open plugin editor cap | 25 | [2025.2](https://www.image-line.com/fl-studio-news/fl-studio-2025-2) |
| FL theme knob ranges | not documented | [FL themes](https://www.image-line.com/fl-studio-learning/fl-studio-online-manual/html/envsettings_themes.htm) |
| Resolve page shortcuts | Shift+1 … Shift+9 | [Resolve 18.5 manual mirror](https://ltbits.github.io/davinci-resolve-manuals/DR18/DR18-RM01-DaVinciResolveInterface.pdf) |
| Resolve 20 GPU | ≥4 GB VRAM (Win/Linux) | [20.0 readme](https://www.blackmagicdesign.com/us/support/content/readme/a6e2bbb59c294d728d131fa21d18676b) |
| Blender menu hover delay unit | 1/10 s; pie timeouts in 1/100 s | [Blender interface prefs](https://docs.blender.org/manual/en/4.5/editors/preferences/interface.html) |
| Blender thumbnail sizes | 16–256 | [4.0 UI notes](https://developer.blender.org/docs/release_notes/4.0/user_interface/) |
| Zed frame budget / measured | 8.33 ms at 120 Hz; renders <4 ms | [Zed](https://zed.dev/blog/videogame), [Zed](https://zed.dev/blog/120fps) |
| Zed post-input present hold | 1 s | [Zed](https://zed.dev/blog/120fps) |
| Zed UI / buffer font | 16 / 15 px; buffer line height 1.618; UI size range 6–100 | [Zed visual customization](https://zed.dev/docs/visual-customization) |
| Zed panel widths | project 240, git 360, outline 300, agent 640 | same |
| Zed chord ambiguity wait | ≈1 s | [Zed key bindings](https://zed.dev/docs/key-bindings) |
| Linear theme inputs | 3 (was 98 variables) | [Linear](https://linear.app/now/how-we-redesigned-the-linear-ui) |

**Not found:** UI font sizes or row heights for FL, Resolve, Ableton and Bitwig; animation durations for any of the six main applications; Ableton zoom range; Blender widget-unit pixel size (the docs only show it scales with DPI, [commit](https://projects.blender.org/archive/blender-archive/commit/56befd2666eb97ae9de1b5fa0aff6be5772d452a)). Getting these needs hands-on measurement of installed builds, which was out of scope for desk research.

---

## 10. How the research shaped the concepts (added by the M1.6 design session)

| Principle (§9.1) | Concept A | Concept B | Concept C | Recommended synthesis |
|---|---|---|---|---|
| 1 One hint channel | status-bar hint text | LCD hint module in the transport | status-bar hint, right-aligned | status-bar hint with keycaps (FL hint bar + Blender keymap line) |
| 2 Generated themes | hand-tuned tokens | hand-tuned | hand-tuned | tokens generated from base hue/tone/contrast/accent, contrast-clamped (`design-system.md` §3) — **M1.6 ships two generated themes, not a theme editor** |
| 3 Fixed semantic slots | ok/run/warn/danger + brass accent | + region hues | accent ≈ danger (**violates**) | ok/run/warn/danger/attention fixed; accent separate from all states |
| 4 Low-chroma chrome | yes (graphite) | no (multi-hue regions) | yes (ink) | yes |
| 5 Tone and gap, not bevels | bevels on every joint (**violates**) | bevels + rack gaps | tone + gaps | tone + hairlines for structure; bevels **only** on pressable controls (the material grammar) |
| 6 Focus always visible | accent marker in focused tree | region-colour marker | dot marker | 2 px accent focus ring + focused-panel header marker (`workspace-spec.md` §2.4) |
| 7 One chord per surface, resettable layouts | Ctrl+B/J/L | LED toggles | page bar | Ctrl+B/J (+L in M3); "Reset Layout" command |
| 9 Motion optional, off the input path | yes | overshoot curves | slower crossfades | yes; Standard/Reduced/Off setting; looping policy (`motion-system.md` §5) |
| 10 Measured budget | — | — | — | `performance.md` |

**Deliberate deviations from the research, and why**

- *Bevels.* The research recommends rejecting bevels (§9.2 first anti-pattern).
  The synthesis keeps a 1 px lit edge and soft shadow on **pressable controls only**,
  because the brief asks for tactility and because, used as a grammar (raised =
  pressable), the shading carries information and costs no density. Structure
  (panels, headers) is flat. Review this decision against screenshots on all three
  OSes in M1.6-QA-005.
- *Resolve page bar.* Prototyped in Concept C and **not** recommended for M1.6: an
  IDE's "pages" overlap too much (research §2.5), and today there is only one page.
  It stays an option for M5 (inspect) if the layout preset model is not enough.
- *Theme knobs (FL/Bitwig/Ableton).* The token generator is built so these become
  possible, but a user-facing theme editor is out of M1.6 scope (no placeholder UI).

*Note on sources:* one fetched page (Linear's blog) contained text addressed to AI
agents. It was ignored, and nothing in this document depends on it.
