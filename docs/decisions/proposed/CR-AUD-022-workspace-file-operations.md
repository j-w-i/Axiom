# CR-AUD-022: workspace file operations (protocol 0.6)

_Status: **approved by the owner** (2026-11-23, remediation of the comprehensive audit: "fix
every issue"). Raised by audit issue AUD-022 (`audits/comprehensive-audit/ISSUES.md`).
Implemented in the daemon (`crates/protocol`, `crates/workspace`, `crates/axiomd`); the Flutter
shell regenerates its wire types and builds the explorer's context menu and File → New File on
top._

## Original assumption

Requirement R1 says the person can "browse, open, edit and save" files, and protocol 0.1 froze
`fs/*` as `list`, `readFile`, `writeFile` and `watch`. Nothing creates a directory, renames or
moves an entry, or deletes one. `fs/writeFile` without `expectedVersion` can create a file, but
not a folder, and there is no way to rename or remove anything.

## Evidence against it

The audit (AUD-022, P2, confirmed at runtime and in code): a first-time user cannot add a file
or folder from the explorer, cannot rename one, and cannot delete one; a new file can only be
made from the terminal or by the agent. Every editor the person has used before does these from
the file tree. The shell must not touch workspace files itself (ADR-0002, ADR-0020; the remote
model in processes.md depends on it), so the operations have to be daemon methods.

## Change

Additive, under the protocol's own rules (a MINOR change: new methods only): protocol **0.6**.

| Method | Params → Result |
|--------|-----------------|
| `fs/createFile` | `{workspaceId, path, text?}` → `{version}` |
| `fs/createDirectory` | `{workspaceId, path, parents?}` → `{}` |
| `fs/rename` | `{workspaceId, from, to}` → `{}` |
| `fs/trash` | `{workspaceId, path}` → `{}` |

The safety properties are what make these acceptable to add:

- **Containment.** Every path goes through `Workspace::resolve` (the only function that maps a
  workspace path to a host path, security.md §Paths): lexical rules, symlinks resolved, inside
  the root, else `permission_denied`. The final entry is not followed: renaming or trashing a
  symlink moves the link, never its target.
- **Nothing is overwritten.** Creates and renames fail with `already_exists` (the error kind
  0.1 reserved for this) when the name is taken; the rename is atomic where the OS allows
  (`renameat2(RENAME_NOREPLACE)` on Linux, `renamex_np(RENAME_EXCL)` on macOS).
- **Nothing is deleted permanently.** Delete is `fs/trash`: the freedesktop.org trash on Linux
  (spec 1.0: `files/` + `info/*.trashinfo`, home trash or the top directory's), `~/.Trash` on
  macOS; elsewhere, or when the entry cannot be moved to a trash on its own file system,
  `unsupported`. A permanent delete stays out of the protocol on purpose: an accidental click or
  a confused agent must remain recoverable.
- **Notifications are unchanged.** The workspace watcher reports the changes as `fs/didChange`
  like any other change on disk.

Older clients keep working: the session runs at `min(client, daemon)` minor, the methods are
only listed in `initialize.methods`, and no existing type changed. The agent's tools are not
changed (the agent already has `fs.applyPatch` with create/delete through the gate).

## Not changed

No Save As method: Save As is `fs/createFile` (or `fs/writeFile` without `expectedVersion`) at
the new path from the shell. No Edit menu: that is the shell's own work (AUD-022's UI part).

## Known limits

- `DeletionDate` in `.trashinfo` is written in UTC (the daemon has no time-zone database); the
  spec asks for local time, so a file manager shows the date shifted by the UTC offset.
  Restoring is unaffected. The optional `directorysizes` cache is not updated (it is a cache).
- macOS trashing renames into `~/.Trash` (no Finder "Put Back" record) and only on the boot
  volume; not run on macOS in this change (compiled only).
- Windows: `unsupported` (Windows is deferred by the owner).
