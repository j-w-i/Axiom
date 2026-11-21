---
id: M4-TERM-002
title: Terminal view in the console (xterm.js), themes, tabs, Mod+`
status: planned
status_note:
depends_on: [M4-TERM-001]
order: 505
scope: apps/desktop-tauri/src/ (console, terminal component, tokens generator), apps/desktop-tauri/package.json, apps/desktop-tauri/e2e/terminal.mjs, apps/desktop-tauri/tests/
---

# M4-TERM-002: Terminal view

## Objective
A **Terminal** view in the console (workspace-spec §5.3): `@xterm/xterm` 6.0.0 + `@xterm/addon-fit`
0.11.0, the code font, an ANSI palette per theme from tokens, several terminals as tabs (new,
close, rename by the shell title), Mod+\` focuses the terminal or creates one, links open through
the host, copy/paste with the platform chords, resize follows the panel.

## Implementation requirements
1. ANSI 16-colour palette tokens for light and dark (generator), contrast-checked against the
   terminal background.
2. Keyboard: the terminal receives keys while focused except the shell's global chords listed in
   workspace-spec (Mod+K, Mod+P, Mod+J, Mod+`); Escape stays in the terminal.
3. Re-attach after a webview reload replays the backlog.
4. Screen-reader mode of xterm on; the terminal has a name ("Terminal 1: zsh").

## Acceptance criteria
1. e2e `terminal.mjs`: Mod+\` opens a terminal; typing `echo $((6*7))` + Enter shows 42; a second
   terminal tab; resizing the console changes `tput cols`; `exit` marks the tab ended; closing the
   app leaves no shell.
2. Typing latency in the terminal: keystroke → echo painted ≤ 50 ms p95 under the bench host.
3. a11y: names, focus visible, contrast of the palette.

## Tests
`pnpm test`; `scripts/e2e.sh terminal.mjs`.

## Failure conditions
Keystrokes leak to the editor; a global shortcut is swallowed by the terminal.

## Documentation updates
`workspace-spec.md` §2.4 (Terminal view), `design-system.md` (ANSI tokens).

## Handoff
Screenshots in both themes.
