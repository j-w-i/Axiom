# ADR-0022: Workspace file operations (protocol 0.6)

- **Status:** accepted (owner, 2026-11-23)
- **Resolves:** `docs/decisions/proposed/CR-AUD-022-workspace-file-operations.md`
- **Amends:** `docs/protocols/ipc.md` (protocol 0.6, additive), `docs/architecture/security.md`
  §Paths

## Context

Protocol 0.1 froze `fs/*` as `list`, `readFile`, `writeFile` and `watch`. The comprehensive
audit of 751ffc2 (AUD-022, P2) found that a person cannot create, rename or delete a file or
folder from the explorer; only the terminal or the agent could. The shell must not touch
workspace files itself (ADR-0002, ADR-0020), so these have to be daemon methods. The CR was
raised by the remediation task (AUD-REM-001) and approved by the owner, who asked for every
audit issue to be fixed; the owner then asked for every review finding to be resolved, this
ADR included (review N15).

## Decisions

1. **Protocol 0.6, a MINOR change: four new methods; the only change to an existing type is
   the optional `GitStatus.polling` (AUD-012, absent unless the repository is polled).**

   | Method | Params → Result |
   |--------|-----------------|
   | `fs/createFile` | `{workspaceId, path, text?}` → `{version}` |
   | `fs/createDirectory` | `{workspaceId, path, parents?}` → `{}` |
   | `fs/rename` | `{workspaceId, from, to}` → `{}` |
   | `fs/trash` | `{workspaceId, path}` → `{}` |

   `initialize.methods` lists every method whatever minor was negotiated, and methods are not
   gated by it (`ipc.md` §0.6).
2. **Containment.** Every path goes through `Workspace::resolve`; outside the root is
   `permission_denied`. The final entry is not followed: renaming or trashing a symlink moves
   the link.
3. **Nothing is overwritten.** A taken name is `already_exists`. The rename is atomic where the
   OS allows (`renameat2(RENAME_NOREPLACE)` on Linux, `renamex_np(RENAME_EXCL)` on macOS).
4. **Nothing is deleted permanently.** Delete is `fs/trash`: the freedesktop.org trash on Linux
   (home trash or the top directory's), `~/.Trash` on macOS, otherwise `unsupported`. A
   permanent delete stays out of the protocol on purpose.
5. **Between resolving and acting** (added by the review, N14 and R5): each operation resolves
   again inside its per-path lock and refuses a path that now leads elsewhere; on Unix it acts
   relative to a directory opened component by component with `O_NOFOLLOW`, and writes go to a
   temporary file renamed into place (a hard link is replaced, not written through). A path
   that changed is `conflict` without `currentVersion`. What remains is in `security.md` §Paths.
6. **Notifications are unchanged.** The watcher reports the changes as `fs/didChange`.

## Consequences

- The explorer's context menu and File → New File are built on these (the shell's part of
  AUD-022). The agent's tools are unchanged: `fs.applyPatch` already creates and deletes
  through the gate.
- Older clients keep working: the session runs at `min(client, daemon)` minor.
- A client that reads `details.currentVersion` on every `conflict` must allow it to be absent.

## Known limits

- `DeletionDate` in `.trashinfo` is written in UTC (the daemon has no time-zone database).
- macOS trashing is a rename into `~/.Trash`: there is no Finder "Put Back" record, and an
  entry on another volume than the home folder is refused with a clear message (nothing is
  moved). `NSFileManager.trashItem` would need Objective-C and `unsafe`, which the daemon does
  not allow.
- Windows: `unsupported` (Windows is deferred by the owner).

## Alternatives rejected

- **The shell doing file operations itself:** breaks the process split (ADR-0002, ADR-0020)
  and the remote model.
- **A permanent `fs/delete`:** an accidental click or a confused agent must stay recoverable.
- **Save As as a method:** it is `fs/createFile` (or `fs/writeFile` without
  `expectedVersion`) at the new path.
