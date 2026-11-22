//! Creating, renaming and trashing workspace entries (protocol 0.6: `fs/createFile`,
//! `fs/createDirectory`, `fs/rename`, `fs/trash`; audit AUD-022). Every path goes through
//! [`Workspace::resolve`]; the entry itself is never followed when it is a symlink (renaming or
//! trashing a link moves the link, not what it points to), nothing ever replaces an existing
//! entry, and the change notifications come from the workspace watcher as for any other change.

use crate::error::{display_path, WsError};
use crate::workspace::{join_err, version_of, Workspace};
use axiom_protocol::fs::{FileVersion, MAX_TEXT_FILE_BYTES};
use axiom_protocol::WorkspacePath;
use std::io::Write;
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

/// Whether two paths name the same file (a case-only rename on a case-insensitive volume).
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (std::fs::symlink_metadata(a), std::fs::symlink_metadata(b)) {
            (Ok(x), Ok(y)) => x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (a, b);
        false
    }
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
        let rel = file.0.clone();
        let bytes = text.as_bytes().to_vec();
        let version = tokio::task::spawn_blocking(move || -> Result<FileVersion, WsError> {
            let parent = abs
                .parent()
                .ok_or_else(|| WsError::InvalidPath("No parent directory.".into()))?;
            match std::fs::metadata(parent) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(WsError::NotADirectory(display_path(&rel))),
                Err(e) => return Err(WsError::from_io(e, &rel)),
            }
            if std::fs::symlink_metadata(&abs).is_ok() {
                return Err(WsError::AlreadyExists(display_path(&rel)));
            }
            let mut builder = tempfile::Builder::new();
            builder.prefix(".axiom-tmp-");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                builder.permissions(std::fs::Permissions::from_mode(0o644));
            }
            let mut tmp = builder
                .tempfile_in(parent)
                .map_err(|e| WsError::from_io(e, &rel))?;
            tmp.write_all(&bytes)
                .map_err(|e| WsError::from_io(e, &rel))?;
            tmp.as_file()
                .sync_all()
                .map_err(|e| WsError::from_io(e, &rel))?;
            tmp.persist_noclobber(&abs)
                .map_err(|e| io_err(e.error, &rel))?;
            Ok(version_of(&bytes))
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
            std::fs::create_dir(&abs).map_err(|e| io_err(e, rel))
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
        let (rf, rt) = (from.0.clone(), to.0.clone());
        tokio::task::spawn_blocking(move || -> Result<(), WsError> {
            std::fs::symlink_metadata(&src).map_err(|e| WsError::from_io(e, &rf))?;
            match std::fs::metadata(dst.parent().unwrap_or(&dst)) {
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(WsError::NotADirectory(display_path(&rt))),
                Err(e) => return Err(WsError::from_io(e, &rt)),
            }
            match rename_noreplace(&src, &dst) {
                Ok(()) => Ok(()),
                Err(e)
                    if e.kind() == std::io::ErrorKind::AlreadyExists && same_file(&src, &dst) =>
                {
                    std::fs::rename(&src, &dst).map_err(|e| WsError::from_io(e, &rf))
                }
                Err(e) => Err(io_err(e, &rt)),
            }
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
        let rel = path.0.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::symlink_metadata(&abs).map_err(|e| WsError::from_io(e, &rel))?;
            crate::trash::trash(&abs, &crate::trash::Places::from_env()).map_err(|e| match e {
                crate::trash::TrashError::Unsupported(m) => WsError::Unsupported(m),
                crate::trash::TrashError::Io(e) => WsError::from_io(e, &rel),
            })
        })
        .await
        .map_err(join_err)??;
        self.inner.ignores.invalidate_for_write(&path.0);
        Ok(())
    }
}
