//! Creating, renaming and trashing workspace entries (protocol 0.6: `fs/createFile`,
//! `fs/createDirectory`, `fs/rename`, `fs/trash`; audit AUD-022). Every path goes through
//! [`Workspace::resolve`]; the entry itself is never followed when it is a symlink (renaming or
//! trashing a link moves the link, not what it points to), nothing ever replaces an existing
//! entry, and the change notifications come from the workspace watcher as for any other change.

use crate::error::{display_path, WsError};
use crate::workspace::{join_err, version_of, Workspace};
use axiom_protocol::fs::{FileVersion, MAX_TEXT_FILE_BYTES};
use axiom_protocol::WorkspacePath;
use std::path::{Path, PathBuf};

/// Renames `from` to `to`, failing with `AlreadyExists` if `to` exists, atomically where the
/// platform and file system can (Linux `renameat2(RENAME_NOREPLACE)`, macOS
/// `renamex_np(RENAME_EXCL)`); elsewhere a check right before the rename.
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
    {
        use rustix::fs::{renameat_with, RenameFlags, CWD};
        use rustix::io::Errno;
        match renameat_with(CWD, from, CWD, to, RenameFlags::NOREPLACE) {
            Ok(()) => return Ok(()),
            // The kernel or the file system has no such flag: check, then rename.
            Err(Errno::NOSYS) | Err(Errno::INVAL) | Err(Errno::NOTSUP) => {}
            Err(e) => return Err(e.into()),
        }
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

fn io_err(e: std::io::Error, rel: &str) -> WsError {
    if e.kind() == std::io::ErrorKind::AlreadyExists {
        WsError::AlreadyExists(display_path(rel))
    } else {
        WsError::from_io(e, rel)
    }
}

impl Workspace {
    /// The host path of the entry `path` names, without following the entry itself: its parent
    /// is resolved (contained, symlinks resolved), its own name is appended. Not the root.
    pub(crate) fn resolve_entry(&self, path: &WorkspacePath) -> Result<PathBuf, WsError> {
        if path.0.is_empty() {
            return Err(WsError::InvalidPath(
                "The workspace root itself cannot be created, renamed or moved to the trash."
                    .into(),
            ));
        }
        let (parent, name) = match path.0.rsplit_once('/') {
            Some((p, n)) => (p.to_string(), n),
            None => (String::new(), path.0.as_str()),
        };
        // Lexical rules first (the parent alone could be valid while the whole is not).
        if !path.is_lexically_valid() {
            return Err(WsError::InvalidPath(format!(
                "`{}` is not a valid workspace path.",
                path.0.escape_debug()
            )));
        }
        let parent = self.resolve(&WorkspacePath(parent)).map_err(|e| match e {
            // The message names the path asked for, not its parent.
            WsError::Escapes(_) => WsError::Escapes(display_path(&path.0)),
            other => other,
        })?;
        Ok(parent.join(name))
    }

    /// Creates a new file with `text` (protocol 0.6 `fs/createFile`). Its directory must exist;
    /// `AlreadyExists` if anything (a file, a directory, a symlink) has that name. The file
    /// appears complete or not at all (written to a temp file, renamed without replacing).
    pub async fn create_file(
        &self,
        file: &WorkspacePath,
        text: &str,
    ) -> Result<FileVersion, WsError> {
        if text.len() as u64 > MAX_TEXT_FILE_BYTES {
            return Err(WsError::TooLarge {
                bytes: text.len() as u64,
            });
        }
        let abs = {
            let (ws, file) = (self.clone(), file.clone());
            tokio::task::spawn_blocking(move || ws.resolve_entry(&file))
                .await
                .map_err(join_err)??
        };
        let lock = self.lock_for(&abs);
        let _guard = lock.lock().await;
        let ws = self.clone();
        let file2 = file.clone();
        let bytes = text.as_bytes().to_vec();
        let version = tokio::task::spawn_blocking(move || -> Result<FileVersion, WsError> {
            ws.recheck(&file2, &abs, true, None)?;
            create_file_blocking(ws.root(), &abs, &file2.0, &bytes)
        })
        .await
        .map_err(join_err)??;
        self.inner.ignores.invalidate_for_write(&file.0);
        Ok(version)
    }

    /// Creates a directory (protocol 0.6 `fs/createDirectory`); with `parents`, the missing
    /// directories above it too. `AlreadyExists` if the directory itself, or anything with its
    /// name, is already there.
    pub async fn create_dir(&self, dir: &WorkspacePath, parents: bool) -> Result<(), WsError> {
        let ws = self.clone();
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || -> Result<(), WsError> {
            let abs = ws.resolve_entry(&dir)?;
            let rel = dir.0.as_str();
            // Created by path: a directory above swapped for a symlink in between could still
            // redirect the missing directories `parents` creates (empty ones; review N14). The
            // last one is created in its parent opened without following symlinks (Unix).
            if parents {
                if let Some(parent) = abs.parent() {
                    // `resolve` canonicalised every existing ancestor and checked containment, so
                    // what is missing below them is created inside the root.
                    std::fs::create_dir_all(parent).map_err(|e| match e.kind() {
                        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotADirectory => {
                            WsError::NotADirectory(display_path(rel))
                        }
                        _ => WsError::from_io(e, rel),
                    })?;
                }
            }
            ws.recheck(&dir, &abs, true, None)?;
            #[cfg(unix)]
            {
                let (parent, name) = crate::workspace::split(&abs, rel)?;
                crate::beneath::Dir::open_beneath(ws.root(), parent, rel)?
                    .mkdir(name)
                    .map_err(|e| io_err(e, rel))
            }
            #[cfg(not(unix))]
            {
                std::fs::create_dir(&abs).map_err(|e| io_err(e, rel))
            }
        })
        .await
        .map_err(join_err)?
    }

    /// Renames or moves an entry inside the workspace (protocol 0.6 `fs/rename`). The target's
    /// directory must exist; `AlreadyExists` if the target does (except a case-only rename of the
    /// same file); a directory cannot move into itself.
    pub async fn rename(&self, from: &WorkspacePath, to: &WorkspacePath) -> Result<(), WsError> {
        if from == to {
            return Ok(());
        }
        if to.0.starts_with(&format!("{}/", from.0)) {
            return Err(WsError::InvalidPath(format!(
                "`{}` cannot be moved into itself.",
                display_path(&from.0)
            )));
        }
        let (src, dst) = {
            let (ws, from, to) = (self.clone(), from.clone(), to.clone());
            tokio::task::spawn_blocking(move || {
                Ok::<_, WsError>((ws.resolve_entry(&from)?, ws.resolve_entry(&to)?))
            })
            .await
            .map_err(join_err)??
        };
        if dst.starts_with(&src) {
            return Err(WsError::InvalidPath(format!(
                "`{}` cannot be moved into itself.",
                display_path(&from.0)
            )));
        }
        // Both paths' locks, in a fixed order (no deadlock with a rename the other way).
        let (first, second) = if src <= dst {
            (self.lock_for(&src), self.lock_for(&dst))
        } else {
            (self.lock_for(&dst), self.lock_for(&src))
        };
        let _g1 = first.lock().await;
        let _g2 = second.lock().await;
        let ws = self.clone();
        let (from2, to2) = (from.clone(), to.clone());
        tokio::task::spawn_blocking(move || -> Result<(), WsError> {
            ws.recheck(&from2, &src, true, None)?;
            ws.recheck(&to2, &dst, true, None)?;
            rename_blocking(ws.root(), &src, &dst, &from2.0, &to2.0)
        })
        .await
        .map_err(join_err)??;
        for p in [&from.0, &to.0] {
            self.inner.ignores.invalidate_for_write(p);
        }
        Ok(())
    }

    /// Moves an entry to the person's trash (protocol 0.6 `fs/trash`): the freedesktop.org trash
    /// on Linux and the BSDs, `~/.Trash` on macOS; elsewhere `Unsupported`, never a permanent
    /// delete.
    pub async fn trash(&self, path: &WorkspacePath) -> Result<(), WsError> {
        let abs = {
            let (ws, path) = (self.clone(), path.clone());
            tokio::task::spawn_blocking(move || ws.resolve_entry(&path))
                .await
                .map_err(join_err)??
        };
        let lock = self.lock_for(&abs);
        let _guard = lock.lock().await;
        let ws = self.clone();
        let path2 = path.clone();
        tokio::task::spawn_blocking(move || {
            let rel = path2.0.as_str();
            // Checked right before; the move itself is by path (the trash is outside the
            // workspace), so a swap in the moment between is not caught (review N14).
            ws.recheck(&path2, &abs, true, None)?;
            std::fs::symlink_metadata(&abs).map_err(|e| WsError::from_io(e, rel))?;
            crate::trash::trash(&abs, &crate::trash::Places::from_env()).map_err(|e| match e {
                crate::trash::TrashError::Unsupported(m) => WsError::Unsupported(m),
                crate::trash::TrashError::Io(e) => WsError::from_io(e, rel),
            })
        })
        .await
        .map_err(join_err)??;
        self.inner.ignores.invalidate_for_write(&path.0);
        Ok(())
    }
}

/// [`Workspace::create_file`]'s write: a temp file in the directory, renamed onto the name
/// without replacing. On Unix in the directory opened without following symlinks (review N14).
pub(crate) fn create_file_blocking(
    root: &Path,
    abs: &Path,
    rel: &str,
    bytes: &[u8],
) -> Result<FileVersion, WsError> {
    #[cfg(unix)]
    {
        let (parent, name) = crate::workspace::split(abs, rel)?;
        let dir = crate::beneath::Dir::open_beneath(root, parent, rel)?;
        if dir.kind_of(name, rel)?.is_some() {
            return Err(WsError::AlreadyExists(display_path(rel)));
        }
        dir.temp_with(bytes, None, rel)?
            .rename_to(name, false)
            .map_err(|e| io_err(e, rel))?;
        Ok(version_of(bytes))
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        let parent = abs
            .parent()
            .ok_or_else(|| WsError::InvalidPath("No parent directory.".into()))?;
        match std::fs::metadata(parent) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(WsError::NotADirectory(display_path(rel))),
            Err(e) => return Err(WsError::from_io(e, rel)),
        }
        if std::fs::symlink_metadata(abs).is_ok() {
            return Err(WsError::AlreadyExists(display_path(rel)));
        }
        let mut tmp = tempfile::Builder::new()
            .prefix(".axiom-tmp-")
            .tempfile_in(parent)
            .map_err(|e| WsError::from_io(e, rel))?;
        std::io::Write::write_all(&mut tmp, bytes).map_err(|e| WsError::from_io(e, rel))?;
        tmp.as_file()
            .sync_all()
            .map_err(|e| WsError::from_io(e, rel))?;
        tmp.persist_noclobber(abs)
            .map_err(|e| io_err(e.error, rel))?;
        Ok(version_of(bytes))
    }
}

/// [`Workspace::rename`]'s move. On Unix between the two directories opened without following
/// symlinks (review N14).
pub(crate) fn rename_blocking(
    root: &Path,
    src: &Path,
    dst: &Path,
    rf: &str,
    rt: &str,
) -> Result<(), WsError> {
    #[cfg(unix)]
    {
        use crate::beneath::{rename, rename_noreplace, Dir};
        let (sp, sn) = crate::workspace::split(src, rf)?;
        let (dp, dn) = crate::workspace::split(dst, rt)?;
        let a = Dir::open_beneath(root, sp, rf)?;
        let Some(from) = a.kind_of(sn, rf)? else {
            return Err(WsError::NotFound(display_path(rf)));
        };
        let b = Dir::open_beneath(root, dp, rt)?;
        match rename_noreplace(&a, sn, &b, dn) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                // A case-only rename of the same entry on a case-insensitive volume.
                match b.kind_of(dn, rt)? {
                    Some(to) if to.dev == from.dev && to.ino == from.ino => {
                        rename(&a, sn, &b, dn).map_err(|e| WsError::from_io(e, rf))
                    }
                    _ => Err(io_err(e, rt)),
                }
            }
            Err(e) => Err(io_err(e, rt)),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = root;
        std::fs::symlink_metadata(src).map_err(|e| WsError::from_io(e, rf))?;
        match std::fs::metadata(dst.parent().unwrap_or(dst)) {
            Ok(m) if m.is_dir() => {}
            Ok(_) => return Err(WsError::NotADirectory(display_path(rt))),
            Err(e) => return Err(WsError::from_io(e, rt)),
        }
        // (No case-only rename here: the platform has no way to tell the same entry.)
        rename_noreplace(src, dst).map_err(|e| io_err(e, rt))
    }
}
