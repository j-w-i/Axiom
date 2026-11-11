use crate::error::{display_path, WsError};
use crate::ignore_rules::IgnoreCache;
use axiom_protocol::fs::{DirEntry, EntryKind, FileVersion, ReadFileResult, MAX_TEXT_FILE_BYTES};
use axiom_protocol::{WorkspaceId, WorkspacePath};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

struct Inner {
    id: WorkspaceId,
    root: PathBuf,
    name: String,
    ignores: IgnoreCache,
    file_locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
}

/// One open workspace. Cheap to clone.
#[derive(Clone)]
pub struct Workspace {
    inner: Arc<Inner>,
}

fn version_of(bytes: &[u8]) -> FileVersion {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    FileVersion(s)
}

fn join_err(e: tokio::task::JoinError) -> WsError {
    WsError::Io(std::io::Error::other(e))
}

impl Workspace {
    pub(crate) fn new(id: WorkspaceId, root: PathBuf) -> Self {
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.to_string_lossy().into_owned());
        Workspace {
            inner: Arc::new(Inner {
                id,
                root,
                name,
                ignores: IgnoreCache::default(),
                file_locks: Mutex::default(),
            }),
        }
    }

    pub fn id(&self) -> &WorkspaceId {
        &self.inner.id
    }

    pub fn root(&self) -> &Path {
        &self.inner.root
    }

    pub fn name(&self) -> &str {
        &self.inner.name
    }

    /// The ONLY function that maps a [`WorkspacePath`] to a host path
    /// (docs/architecture/security.md §Paths).
    pub fn resolve(&self, path: &WorkspacePath) -> Result<PathBuf, WsError> {
        // 1. Lexical rules.
        if !path.is_lexically_valid() {
            return Err(WsError::InvalidPath(format!(
                "`{}` is not a valid workspace path.",
                path.0.escape_debug()
            )));
        }
        // 2. Join onto the canonical root.
        let root = &self.inner.root;
        let joined = if path.0.is_empty() {
            root.clone()
        } else {
            root.join(path.0.split('/').collect::<PathBuf>())
        };
        // 3. Canonicalize the longest existing prefix, re-append the tail.
        let mut existing = joined.clone();
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let canonical = loop {
            match dunce::canonicalize(&existing) {
                Ok(c) => break c,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    // A dangling symlink exists but cannot be resolved; writing
                    // through it could create a file anywhere.
                    if std::fs::symlink_metadata(&existing).is_ok() {
                        return Err(WsError::Escapes(display_path(&path.0)));
                    }
                    match (
                        existing.file_name().map(|n| n.to_os_string()),
                        existing.parent(),
                    ) {
                        (Some(name), Some(parent)) => {
                            tail.push(name);
                            existing = parent.to_path_buf();
                        }
                        _ => return Err(WsError::from_io(e, &path.0)),
                    }
                }
                // A path component is a regular file ("file.txt/child").
                Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => {
                    return Err(WsError::NotADirectory(display_path(&path.0)));
                }
                Err(e) => return Err(WsError::from_io(e, &path.0)),
            }
        };
        let mut result = canonical;
        for name in tail.into_iter().rev() {
            result.push(name);
        }
        // 4. Must stay inside the root.
        if !result.starts_with(root) {
            return Err(WsError::Escapes(display_path(&path.0)));
        }
        debug_assert!(!result
            .components()
            .any(|c| matches!(c, Component::ParentDir)));
        Ok(result)
    }

    pub async fn list(&self, dir: &WorkspacePath) -> Result<Vec<DirEntry>, WsError> {
        let ws = self.clone();
        let dir = dir.clone();
        tokio::task::spawn_blocking(move || ws.list_blocking(&dir))
            .await
            .map_err(join_err)?
    }

    fn list_blocking(&self, dir: &WorkspacePath) -> Result<Vec<DirEntry>, WsError> {
        let abs = self.resolve(dir)?;
        let meta = std::fs::metadata(&abs).map_err(|e| WsError::from_io(e, &dir.0))?;
        if !meta.is_dir() {
            return Err(WsError::NotADirectory(display_path(&dir.0)));
        }
        let mut raw: Vec<(String, EntryKind, Option<u64>)> = Vec::new();
        for entry in std::fs::read_dir(&abs).map_err(|e| WsError::from_io(e, &dir.0))? {
            let entry = entry.map_err(|e| WsError::from_io(e, &dir.0))?;
            let md = match entry.metadata() {
                Ok(m) => m,
                // Vanished between readdir and stat.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(WsError::from_io(e, &dir.0)),
            };
            let ft = md.file_type();
            let (kind, size) = if ft.is_symlink() {
                (EntryKind::Symlink, None)
            } else if ft.is_dir() {
                (EntryKind::Directory, None)
            } else {
                (EntryKind::File, Some(md.len()))
            };
            raw.push((entry.file_name().to_string_lossy().into_owned(), kind, size));
        }
        let probe: Vec<(String, bool)> = raw
            .iter()
            .map(|(n, k, _)| (n.clone(), *k == EntryKind::Directory))
            .collect();
        let ignored = self
            .inner
            .ignores
            .classify(&self.inner.root, &dir.0, &probe);
        let mut out: Vec<DirEntry> = raw
            .into_iter()
            .zip(ignored)
            .map(|((name, kind, size), ignored)| DirEntry {
                name,
                kind,
                size,
                ignored,
            })
            .collect();
        out.sort_by(|a, b| {
            let a_dir = a.kind == EntryKind::Directory;
            let b_dir = b.kind == EntryKind::Directory;
            b_dir
                .cmp(&a_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(out)
    }

    pub async fn read_text(&self, file: &WorkspacePath) -> Result<ReadFileResult, WsError> {
        let ws = self.clone();
        let file = file.clone();
        tokio::task::spawn_blocking(move || {
            let abs = ws.resolve(&file)?;
            let md = std::fs::metadata(&abs).map_err(|e| WsError::from_io(e, &file.0))?;
            if md.is_dir() {
                return Err(WsError::IsADirectory(display_path(&file.0)));
            }
            if md.len() > MAX_TEXT_FILE_BYTES {
                return Err(WsError::TooLarge { bytes: md.len() });
            }
            let bytes = std::fs::read(&abs).map_err(|e| WsError::from_io(e, &file.0))?;
            if bytes.len() as u64 > MAX_TEXT_FILE_BYTES {
                return Err(WsError::TooLarge {
                    bytes: bytes.len() as u64,
                });
            }
            let version = version_of(&bytes);
            let text = String::from_utf8(bytes)
                .map_err(|_| WsError::UnsupportedEncoding(display_path(&file.0)))?;
            Ok(ReadFileResult { text, version })
        })
        .await
        .map_err(join_err)?
    }

    fn lock_for(&self, abs: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self
            .inner
            .file_locks
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // Drop locks nobody holds so the map does not grow without bound.
        map.retain(|_, m| Arc::strong_count(m) > 1);
        map.entry(abs.to_path_buf()).or_default().clone()
    }

    /// Implements the decision table in `docs/protocols/ipc.md` §`fs/writeFile`.
    pub async fn write_text(
        &self,
        file: &WorkspacePath,
        text: &str,
        expected: Option<&FileVersion>,
        overwrite: bool,
    ) -> Result<FileVersion, WsError> {
        if text.len() as u64 > MAX_TEXT_FILE_BYTES {
            return Err(WsError::TooLarge {
                bytes: text.len() as u64,
            });
        }
        let ws = self.clone();
        let file = file.clone();
        let bytes = text.as_bytes().to_vec();
        let expected = expected.cloned();

        let abs = {
            let (ws, file) = (ws.clone(), file.clone());
            tokio::task::spawn_blocking(move || ws.resolve(&file))
                .await
                .map_err(join_err)??
        };
        // The version check and the rename happen under this lock.
        let lock = self.lock_for(&abs);
        let _guard = lock.lock().await;

        let rel = file.0.clone();
        let version = tokio::task::spawn_blocking(move || {
            write_blocking(&abs, &rel, &bytes, expected.as_ref(), overwrite)
        })
        .await
        .map_err(join_err)??;
        ws.inner.ignores.invalidate_for_write(&file.0);
        Ok(version)
    }
}

fn write_blocking(
    abs: &Path,
    rel: &str,
    bytes: &[u8],
    expected: Option<&FileVersion>,
    overwrite: bool,
) -> Result<FileVersion, WsError> {
    let parent = abs
        .parent()
        .ok_or_else(|| WsError::IsADirectory(display_path(rel)))?;
    match std::fs::metadata(parent) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(WsError::NotADirectory(display_path(rel))),
        Err(e) => return Err(WsError::from_io(e, rel)),
    }
    let existing_meta = match std::fs::metadata(abs) {
        Ok(m) if m.is_dir() => return Err(WsError::IsADirectory(display_path(rel))),
        Ok(m) => Some(m),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(WsError::from_io(e, rel)),
    };

    if !overwrite {
        let current = match &existing_meta {
            Some(_) => Some(version_of(
                &std::fs::read(abs).map_err(|e| WsError::from_io(e, rel))?,
            )),
            None => None,
        };
        let allowed = match (&current, expected) {
            (None, None) => true,
            (Some(cur), Some(exp)) => cur == exp,
            _ => false,
        };
        if !allowed {
            return Err(WsError::Conflict { current });
        }
    }

    let mut builder = tempfile::Builder::new();
    builder.prefix(".axiom-tmp-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // New files get ordinary permissions rather than the temp default 0600.
        builder.permissions(std::fs::Permissions::from_mode(0o644));
    }
    let mut tmp = builder
        .tempfile_in(parent)
        .map_err(|e| WsError::from_io(e, rel))?;
    tmp.write_all(bytes).map_err(|e| WsError::from_io(e, rel))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| WsError::from_io(e, rel))?;
    if let Some(m) = &existing_meta {
        std::fs::set_permissions(tmp.path(), m.permissions())
            .map_err(|e| WsError::from_io(e, rel))?;
    }
    tmp.persist(abs)
        .map_err(|e| WsError::from_io(e.error, rel))?;
    Ok(version_of(bytes))
}
