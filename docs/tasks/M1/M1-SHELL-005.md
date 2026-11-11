---
id: M1-SHELL-005
title: Settings — theme, editor font, reopen-last
status: planned
status_note:
depends_on: [M1-SHELL-003]
order: 44
scope: apps/desktop/
---

# M1-SHELL-005: Settings

## Objective
A settings dialog for the known settings, applied live to the shell and the
editor and persisted by the daemon.

## Context
Read: `docs/protocols/ipc.md` §Settings (known keys),
`crates/protocol/src/settings.rs`, `docs/protocols/editor-bridge.md`
(`configure`).

## Implementation requirements
1. `src/models/SettingsStore` (exposed as `settings`): loads all values with
   `settings/get` at startup; exposes typed properties `theme`,
   `editorFontSize`, `editorFontFamily`, `editorWordWrap`, `reopenLast` with
   NOTIFY signals; setters call `settings/set` and update on the daemon's
   `settings/didChange` (single source of truth: the property changes when
   the notification arrives, not optimistically). `invalid_params` → revert
   and show the message.
2. `Theme.dark` binds to: `theme == "dark"`, or `theme == "system"` and the
   OS scheme is dark. Remove the temporary binding from M1-SHELL-001.
3. Whenever theme or editor settings change (and after `ready`), emit
   `configure` to the editor with resolved values (`fontFamily` empty →
   `Theme.monoFamily`).
4. `SettingsDialog.qml` (File ▸ Settings…, Ctrl+,): sections *Appearance*
   (Theme: System/Light/Dark) and *Editor* (Font size spin 8–40, Font family
   text field with placeholder showing the resolved default, Word wrap
   checkbox) and *Workspace* (Reopen last folder at startup). Changes apply
   immediately; the dialog has only **Close**. Each control has a label
   associated for accessibility.
5. Help ▸ About shows the app version, daemon version, protocol version
   and Qt version, and the data directory: the path passed with
   `--data-dir`, or the text "default location" when none was passed
   (protocol 0.1 has no method that reports the daemon's data directory; do
   not add one in this task).

## Constraints
Only the settings in the known-keys table. Do not add settings for
unimplemented features.

## Acceptance criteria
Tests: changing `theme` flips `Theme.dark` and triggers `configure` with the
resolved theme; an out-of-range font size is rejected and the property keeps
its value; values persist across an application restart (same data dir);
two `SettingsStore`s on two daemon connections are not required (single
connection in M1).
Manual/smoke: dialog opens, live application visible in a grabbed
screenshot for light and dark.

## Tests
`scripts/check.sh`.

## Failure conditions
None specific.

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
Screenshots (paths) of the dialog in both themes.
