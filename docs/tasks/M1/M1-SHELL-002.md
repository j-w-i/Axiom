---
id: M1-SHELL-002
title: Workspace opening and file explorer
status: tested
status_note: 2026-11-11; see handoff
depends_on: [M1-SHELL-001, M1-DAEMON-002]
order: 41
scope: apps/desktop/
---

# M1-SHELL-002: Open Folder, Welcome view, Explorer

## Objective
The user can open a directory, see its files in a lazily loaded tree that
stays in sync with the disk, and reopen recent workspaces.

## Context
Read: `docs/architecture/data-flow.md` §Open a workspace,
`docs/architecture/design-system.md`, `crates/protocol/src/{workspace,fs}.rs`.

## Implementation requirements
1. `src/models/WorkspaceController` (QObject, exposed as `workspace`):
   properties `open: bool`, `name`, `root`, `id`; `recent` (list model of
   `{root, name}`); invokables `openFolder(path)`, `close()`,
   `refreshRecent()`. On open: `workspace/open` → `fs/watch` →
   root listing. Errors surface through an `errorOccurred(message)` signal
   shown in an `AxBanner`.
2. `src/models/FileTreeModel` (`QAbstractItemModel`): roles `name`, `path`
   (workspace-relative), `kind`, `ignored`, `expanded`, `loading`.
   Children are fetched with `fs/list` on first expansion
   (`canFetchMore`/`fetchMore` or an explicit `expand(index)`); a directory
   whose listing failed shows an inline error row with the message.
   On `fs/didChange`: for each expanded directory that is a parent of a
   changed path (or all expanded directories if a change has `path: ""`),
   re-list and apply a minimal diff (`beginInsertRows`/`beginRemoveRows`) so
   expansion and selection survive.
3. QML: `ExplorerView.qml` using `TreeView` with `AxListRow` delegates:
   chevron for directories, ignored entries in `textMuted`, keyboard
   navigation (Up/Down, Left/Right collapse/expand, Enter activates),
   `Accessible` names. Activating a file emits `fileActivated(path)` (handled
   in M1-SHELL-003; until then it only selects).
4. `WelcomeView.qml` in the content area when no workspace is open: an
   **Open Folder…** primary button and the recent list (click opens; entries
   are real, from `workspace/recent`; if empty, the list is absent, not a
   placeholder).
5. File menu: **Open Folder…** (Ctrl+O; `FolderDialog` from
   `QtQuick.Dialogs`), **Open Recent** submenu, **Close Folder**.
   Title bar: `<name> — Axiom`.
6. Persist and restore expanded directories with `workspace/setState` /
   `getState` under key `explorer.expanded` (array of paths), debounced
   500 ms.
7. Startup: if setting `workspace.reopenLast` is true and
   `workspace.lastRoot` exists, open it (read both via `settings/get`).

## Constraints
- The shell never reads the filesystem itself (even for the tree).
- Add `QuickDialogs2` to the Qt components (allowed by ADR-0011).

## Acceptance criteria
- Tests (Qt Test, real daemon, temp copy of `tests/fixtures/basic`):
  model lists root entries in protocol order; expanding `src` lists its
  files; creating a file on disk appears in the model within 3 s without
  collapsing other expanded directories; deleting it removes the row;
  ignored flag exposed; expanded state restored after closing and reopening
  the workspace; opening a non-existent path emits `errorOccurred`.
- Manual/smoke: Open Folder shows the tree; recent list populated on next
  launch; reopen-last works.

## Tests
`scripts/check.sh`.

## Failure conditions
`TreeView` cannot meet the keyboard/accessibility requirements on the
pinned Qt → describe the gap and propose (CR) rather than silently
dropping the requirement.

## Documentation updates
`docs/roadmap/current.md`.

## Handoff
Any Qt `TreeView` workarounds applied, with Qt bug links if known.
