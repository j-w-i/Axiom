# FL verification — the Flutter shell (in progress)

_Started 2026-11-22. Each task adds its evidence; FL-QA-001 completes the record._

## Frame timings (ADR-0020 §5)
`apps/desktop-flutter/scripts/frames.sh`:
- a profile build, in a private virtual KWin on this machine's GPU (NVIDIA RTX 3060 Ti, driver
  620.70);
- the owner's display: 2560×1440 at 125 %;
- the window at 2040×1120 logical, which is 4080×2240 physical, because GTK 3 renders at 2× for
  a 1.25 scale, as it did for the Tauri shell;
- build and raster time of every frame, from the engine's `FrameTiming`.

Two runs on 2026-11-22, after FL-SHELL-001 and FL-FILES-001:

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Explorer fling, 3000 rows | 287 / 301 | 5.5 / 8.3 | 4.8 / 8.3 | 0 / 0 |
| Explorer wheel scroll | 82 / 78 | 6.3 / 11.6 | 3.6 / 4.7 | 0 / 0 |
| Side bar toggle (Mod+B ×8) | 24 / 27 | 12.6 / 12.6 | 4.7 / 4.7 | 0 / 0 |
| Palette open/close ×6 | 37 / 44 | 6.0 / 6.0 | 4.7 / 4.7 | 0 / 0 |
| Splitter drag | 77 / 82 | 6.1 / 6.9 | 4.8 / 5.5 | 0 / 0 |
| Theme switch ×3 | 64 / 56 | 8.2 / 9.0 | 3.7 / 6.3 | 0 / 0 |

**The editor** (FL-EDIT-001), `re_editor`, measured on 2026-11-22.

Alone (`editor_frames_test.dart`), with a 5000-line Rust file filling the window:

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Fling scroll | 210 | 3.2 / 3.4 | 3.9 / 5.7 | 0 |
| Wheel scroll | 104 | 4.4 / 4.4 | 6.0 / 6.8 | 0 |
| Typing (a character every 40 ms) | 86 | 6.6 / 7.6 | 5.9 / 6.1 | 0 |
| Cursor ↓ ×60 | 82 | 6.4 / 8.9 | 4.1 / 4.1 | 0 |

In the shell (`frames_test.dart`), with three 5000-line files open and the explorer listing
3000 more:

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Tab switching (Ctrl+Tab ×12) | 108 | 8.9 / 11.3 | 7.1 / 13.1 | 0 |
| Typing | 75 | 7.0 / 7.1 | 6.5 / 6.9 | 0 |
| Editor fling | 160 | 5.6 / 7.3 | 6.0 / 7.0 | 0 |
| Side bar toggle | 27 | 7.5 / 7.5 | 4.0 / 4.0 | 0 |

The side bar is kept mounted while hidden; before that, one toggle took 20.1 ms to build.

**The first palette open.** Before `SheetWarmup` it took one 33.7 ms raster frame, the first use
of the sheet's blurred shadow. The shell now paints one nearly transparent sheet for its first
three frames; after that the first open rasters within 2.7 ms.

**The Tauri shell, for comparison.** The same private compositor, GPU, scale and window size,
measured with requestAnimationFrame:
- editor scroll 44 fps;
- typing 44 fps;
- explorer scroll 58 fps;
- tab switches dropped 39 of 163 frames.

The editor's own figures come with FL-EDIT-001.

### Chat (FL-CHAT-001)
`AXIOM_FRAMES_TARGET=chat_frames_test.dart scripts/frames.sh`, same setup, against the real
daemon. The scripted model streams a long Markdown answer: 12 sections of prose, lists, tables
and Rust blocks, 444 chunks 12 ms apart. Then the transcript is flung.

| Interaction | Frames | Build p99 / max (ms) | Raster p99 / max (ms) | Over 16.7 ms |
|-------------|-------:|---------------------:|----------------------:|-------------:|
| Streaming the answer | 188 / 191 | 7.4 / 8.3, 7.2 / 7.4 | 4.9 / 7.3, 7.5 / 10.7 | 0 / 0 |
| Transcript fling | 227 / 222 | 0.7 / 0.9, 0.6 / 1.1 | 6.5 / 7.8, 5.5 / 7.4 | 0 / 0 |

**The first version missed the budget:** 5 frames over, build max 36.3 ms. Each delta re-parsed
the whole answer and rebuilt the whole transcript. Three changes fixed it:
- the Markdown is split into top-level pieces, each parsed and built once (`markdownPieces`);
- the transcript is rebuilt only when its events change;
- each item's widget is reused, and the streaming text sits where its finished item will.

After the change, `frames_test.dart` (the shell's interactions) still has no frame over budget.

## End to end
`console_test.dart`: 3/3, in a private virtual KWin and in a private Xvfb, against the real
daemon.
- **Output:**
  - Mod+Shift+R focuses the prompt; a command's stdout and stderr stay apart, and the exit
    status shows ("Failed with exit code 2");
  - Shift+F5 stops a running command within seconds;
  - a missing program is a structured error;
  - the transport shows the label and an advancing time, then success; a failure shows as
    such; Rerun names the last command.
- **Provider tasks:** "Build: cargo build" from the palette builds a broken crate and fails;
  the transport offers "Rerun: Build: cargo build"; the E0308 row reads "Error: mismatched types
  (E0308) at src/main.rs:2:18, from rustc" and opens the file at Ln 2, Col 18.
- **Terminal:**
  - Mod+Shift+` opens a shell in the folder, and `pwd` prints it;
  - `stty size` matches the screen, and hiding the side bar gives more columns;
  - a typed key's echo is on screen within p95 30 / 16 / 33 ms across three runs (the
    criterion is 50 ms);
  - a second terminal is a tab, and Mod+` focuses it;
  - Mod+K from the terminal opens the palette;
  - `exit` marks the tab "(ended)";
  - after quitting, no shell process is left.

`chat_test.dart`: 3/3, in a private virtual KWin and in a private Xvfb, against the real daemon.
- **No folder, the scripted model:**
  - Chat is on the rail under Files and opens with a model chosen;
  - Enter sends; streamed Markdown arrives with a table and a code block coloured per token;
  - the conversation is titled from the first message;
  - ↑ edits the message and replaces the branch; Regenerate replaces the answer;
  - an attached image reaches the model and shows as a thumbnail;
  - F2 renames; Delete asks first, then removes the conversation from the daemon too.
- **A folder, the dock:**
  - Mod+L opens the dock beside the editor, in the folder's scope;
  - an edit asks with a focused card showing the diff, and the rail reads "Chat, 1 waiting for
    you"; Enter allows it and the file changes; Esc denies the next one and the file stays;
  - Restore puts the file back;
  - a file the person changed after the agent is a conflict: it is listed, nothing is
    restored, the person's text stays;
  - Mod+L closes the dock.
- **No folder, no model:**
  - the first-run setup shows;
  - an OpenAI-compatible server (a fake one on loopback) is added with a key. The key never
    shows in the UI and reaches the server as a bearer, and the answer streams;
  - Settings → Models shows "Key stored · 1 model"; removing the provider brings the setup
    back;
  - Claude Code is installed from the setup by a fake npm into `<data>/agents`;
  - in Settings → Models, cancelling a hung install stops it within seconds;
  - the fake ACP agent, picked in the model picker, answers in `<data>/scratch/<session>`. The
    card names it as an external agent, and its write happens only after Allow once.

`apps/desktop-flutter/scripts/e2e.sh shell_test.dart`: 1/1, in a private virtual KWin and in a
private Xvfb. It covers connecting, opening a folder, the ignored entries, the explorer by
keyboard, the hint line, the palette, Mod+B, the splitter's keys, Settings, and a restart that
brings back the folder, the expanded folders, the side bar width and the theme.

`editor_test.dart`: 1/1 against the real daemon. Covers:
- opening from the explorer (the editor, Markdown in the status bar);
- Go to File for a file the explorer never listed;
- Mod+S, with the bytes on disk matching;
- go to line, with the cursor in the status bar;
- a clean reload on an external change;
- dirty plus changed on disk, then the conflict dialog, then Overwrite;
- closing a dirty tab, then Don't Save, with nothing written;
- the split: Mod+\, the same text in both views, Mod+2, typing on the right, Mod+S there,
  Mod+1;
- a restart that brings back the tabs, the active one and the split;
- closing the right group's last tab, which ends the split.
