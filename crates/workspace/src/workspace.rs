use crate::error::{display_path, WsError};
use crate::ignore_rules::IgnoreCache;
use axiom_protocol::fs::{DirEntry, EntryKind, FileVersion, ReadFileResult, MAX_TEXT_FILE_BYTES};
use axiom_protocol::{WorkspaceId, WorkspacePath};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

pub(crate) struct Inner {
    id: WorkspaceId,
    pub(crate) root: PathBuf,
    name: String,
    pub(crate) ignores: IgnoreCache,
    file_locks: Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>,
    pub(crate) watch: Mutex<std::sync::Weak<crate::watch::WatchShared>>,
}

/// One open workspace. Cheap to clone.
#[derive(Clone)]
pub struct Workspace {
    pub(crate) inner: Arc<Inner>,
}

pub(crate) fn version_of(bytes: &[u8]) -> FileVersion {
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    FileVersion(s)
}

pub(crate) fn join_err(e: tokio::task::JoinError) -> WsError {
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
                watch: Mutex::new(std::sync::Weak::new()),
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

    /// [`Self::resolve`]d and made relative to the root again: the path a file really has in the
    /// workspace, through any symlink inside it (`link/hooks/x` with `link -> .git` is
    /// `.git/hooks/x`). What the agent gate judges (AUD-104, review B4).
    pub fn canonical_path(&self, path: &WorkspacePath) -> Result<WorkspacePath, WsError> {
        let abs = self.resolve(path)?;
        let rel = abs
            .strip_prefix(&self.inner.root)
            .map_err(|_| WsError::Escapes(display_path(&path.0)))?;
        let mut parts = Vec::new();
        for c in rel.components() {
            match c {
                Component::Normal(s) => parts.push(s.to_string_lossy().into_owned()),
                _ => return Err(WsError::Escapes(display_path(&path.0))),
            }
        }
        Ok(WorkspacePath(parts.join("/")))
    }

    /// Right before an operation, under its lock: `path` must still resolve to `abs`, the host
    /// path it had when the operation started (`entry`: resolved as an entry, its own name not
    /// followed), and, with `judged`, be the workspace path the agent gate judged when the call
    /// was prepared ([`Self::canonical_path`]). A directory on the way that was replaced since
    /// (by a symlink, say) is [`WsError::Changed`] (review N14, R5).
    pub(crate) fn recheck(
        &self,
        path: &WorkspacePath,
        abs: &Path,
        entry: bool,
        judged: Option<&WorkspacePath>,
    ) -> Result<(), WsError> {
        let now = if entry {
            self.resolve_entry(path)?
        } else {
            self.resolve(path)?
        };
        if now != abs {
            return Err(WsError::Changed(display_path(&path.0)));
        }
        if let Some(judged) = judged {
            let rel = abs
                .strip_prefix(&self.inner.root)
                .map_err(|_| WsError::Escapes(display_path(&path.0)))?;
            let rel: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            if rel.join("/") != judged.0 {
                return Err(WsError::Changed(display_path(&path.0)));
            }
        }
        Ok(())
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
            let bytes = ws.read_resolved(&abs, &file.0, MAX_TEXT_FILE_BYTES)?;
            let version = version_of(&bytes);
            let text = String::from_utf8(bytes)
                .map_err(|_| WsError::UnsupportedEncoding(display_path(&file.0)))?;
            Ok(ReadFileResult { text, version })
        })
        .await
        .map_err(join_err)?
    }

    /// Reads the regular file at `abs` (resolved by [`Self::resolve`]). On Unix through its
    /// directory opened without following symlinks, so a directory swapped for a symlink since
    /// the path was resolved cannot redirect the read (review N14).
    pub(crate) fn read_resolved(
        &self,
        abs: &Path,
        rel: &str,
        limit: u64,
    ) -> Result<Vec<u8>, WsError> {
        #[cfg(unix)]
        {
            let (parent, name) = split(abs, rel)?;
            crate::beneath::Dir::open_beneath(&self.inner.root, parent, rel)?
                .read_regular(name, rel, limit)
        }
        #[cfg(not(unix))]
        {
            read_regular(abs, rel, limit)
        }
    }

    pub(crate) fn lock_for(&self, abs: &Path) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self
            .inner
            .file_locks
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        // Drop locks nobody holds so the map does not grow without bound.
        map.retain(|_, m| Arc::strong_count(m) > 1);
        map.entry(abs.to_path_buf()).or_default().clone()
    }

    /// Removes a file (not a directory), only if it still has `expected` (M3: `fs.applyPatch`
    /// with `delete`). Same lock as `write_text`.
    pub async fn remove_file(
        &self,
        file: &WorkspacePath,
        expected: &FileVersion,
    ) -> Result<(), WsError> {
        self.remove_file_inner(file, None, expected).await
    }

    /// [`Self::remove_file`], refused ([`WsError::Changed`]) unless `file` is still `judged`,
    /// the path the agent gate judged when the call was prepared (review R5).
    pub async fn remove_file_checked(
        &self,
        file: &WorkspacePath,
        judged: &WorkspacePath,
        expected: &FileVersion,
    ) -> Result<(), WsError> {
        self.remove_file_inner(file, Some(judged), expected).await
    }

    async fn remove_file_inner(
        &self,
        file: &WorkspacePath,
        judged: Option<&WorkspacePath>,
        expected: &FileVersion,
    ) -> Result<(), WsError> {
        let abs = {
            let (ws, file) = (self.clone(), file.clone());
            tokio::task::spawn_blocking(move || ws.resolve(&file))
                .await
                .map_err(join_err)??
        };
        let lock = self.lock_for(&abs);
        let _guard = lock.lock().await;
        let ws = self.clone();
        let (file2, judged) = (file.clone(), judged.cloned());
        let expected = expected.clone();
        tokio::task::spawn_blocking(move || {
            ws.recheck(&file2, &abs, false, judged.as_ref())?;
            remove_blocking(ws.root(), &abs, &file2.0, &expected)
        })
        .await
        .map_err(join_err)??;
        self.inner.ignores.invalidate_for_write(&file.0);
        Ok(())
    }

    /// Implements the decision table in `docs/protocols/ipc.md` §`fs/writeFile`.
    pub async fn write_text(
        &self,
        file: &WorkspacePath,
        text: &str,
        expected: Option<&FileVersion>,
        overwrite: bool,
    ) -> Result<FileVersion, WsError> {
        self.write_text_inner(file, None, text, expected, overwrite)
            .await
    }

    /// [`Self::write_text`], refused ([`WsError::Changed`]) unless `file` is still `judged`,
    /// the path the agent gate judged when the call was prepared (review R5).
    pub async fn write_text_checked(
        &self,
        file: &WorkspacePath,
        judged: &WorkspacePath,
        text: &str,
        expected: Option<&FileVersion>,
        overwrite: bool,
    ) -> Result<FileVersion, WsError> {
        self.write_text_inner(file, Some(judged), text, expected, overwrite)
            .await
    }

    async fn write_text_inner(
        &self,
        file: &WorkspacePath,
        judged: Option<&WorkspacePath>,
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

        let judged = judged.cloned();
        let ws2 = ws.clone();
        let file2 = file.clone();
        let version = tokio::task::spawn_blocking(move || {
            ws2.recheck(&file2, &abs, false, judged.as_ref())?;
            write_blocking(
                ws2.root(),
                &abs,
                &file2.0,
                &bytes,
                expected.as_ref(),
                overwrite,
            )
        })
        .await
        .map_err(join_err)??;
        ws.inner.ignores.invalidate_for_write(&file.0);
        Ok(version)
    }
}

/// Reads a regular file of at most `limit` bytes (`TooLarge` beyond). Anything else (a FIFO, a
/// device, a socket) is refused without blocking: on Unix the file is opened non-blocking and
/// checked after opening, so a FIFO swapped in after a check cannot hang a thread either; the
/// read itself stops after `limit + 1` bytes whatever the size claimed (AUD-097).
pub(crate) fn read_regular(abs: &Path, rel: &str, limit: u64) -> Result<Vec<u8>, WsError> {
    use std::io::Read;
    let md = std::fs::metadata(abs).map_err(|e| WsError::from_io(e, rel))?;
    if md.is_dir() {
        return Err(WsError::IsADirectory(display_path(rel)));
    }
    if !md.is_file() {
        return Err(WsError::NotAFile(display_path(rel)));
    }
    if md.len() > limit {
        return Err(WsError::TooLarge { bytes: md.len() });
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
    }
    let f = opts.open(abs).map_err(|e| WsError::from_io(e, rel))?;
    let md = f.metadata().map_err(|e| WsError::from_io(e, rel))?;
    if !md.is_file() {
        return Err(WsError::NotAFile(display_path(rel)));
    }
    let mut bytes = Vec::with_capacity(md.len().min(limit) as usize);
    f.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| WsError::from_io(e, rel))?;
    if bytes.len() as u64 > limit {
        return Err(WsError::TooLarge {
            bytes: bytes.len() as u64,
        });
    }
    Ok(bytes)
}

/// [`read_regular`] for a host path outside any workspace (a language server's location, read
/// only through `fs/readFile`'s allow list); `shown` names it in errors.
pub fn read_regular_file(abs: &Path, shown: &str, limit: u64) -> Result<Vec<u8>, WsError> {
    read_regular(abs, shown, limit)
}

/// Renames the temp file over the target. On Windows a replace can fail with
/// access-denied while another process (an indexer, a virus scanner, or a reader
/// that has not closed its handle yet) still has the destination open; retrying
/// briefly is what every Windows editor does. On Unix `rename` either succeeds or
/// fails for a reason that retrying cannot fix.
#[cfg(not(unix))]
fn persist_with_retry(tmp: tempfile::NamedTempFile, abs: &Path, rel: &str) -> Result<(), WsError> {
    #[cfg(windows)]
    {
        let mut tmp = tmp;
        const ATTEMPTS: u32 = 20;
        for attempt in 0..ATTEMPTS {
            match tmp.persist(abs) {
                Ok(_) => return Ok(()),
                Err(e)
                    if e.error.kind() == std::io::ErrorKind::PermissionDenied
                        && attempt + 1 < ATTEMPTS =>
                {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    tmp = e.file;
                }
                Err(e) => return Err(WsError::from_io(e.error, rel)),
            }
        }
        unreachable!("the loop either persists or returns")
    }
    #[cfg(not(windows))]
    {
        tmp.persist(abs)
            .map_err(|e| WsError::from_io(e.error, rel))?;
        Ok(())
    }
}

/// The directory and the name of a resolved path (never the root: it has no parent). Used by
/// the operations that act in a directory opened without following symlinks (Unix only).
#[cfg(unix)]
pub(crate) fn split<'a>(
    abs: &'a Path,
    rel: &str,
) -> Result<(&'a Path, &'a std::ffi::OsStr), WsError> {
    match (abs.parent(), abs.file_name()) {
        (Some(p), Some(n)) => Ok((p, n)),
        _ => Err(WsError::IsADirectory(display_path(rel))),
    }
}

/// The `fs/writeFile` decision table and the write itself, in the file's directory opened
/// without following symlinks (review N14): the temp file, the version check and the rename
/// all act in that directory, so a directory on the way that is swapped for a symlink cannot
/// redirect them. The rename replaces the entry, so a hard link to the file elsewhere keeps its
/// old content (review R5).
#[cfg(unix)]
pub(crate) fn write_blocking(
    root: &Path,
    abs: &Path,
    rel: &str,
    bytes: &[u8],
    expected: Option<&FileVersion>,
    overwrite: bool,
) -> Result<FileVersion, WsError> {
    use rustix::fs::FileType;
    let (parent, name) = split(abs, rel)?;
    let dir = crate::beneath::Dir::open_beneath(root, parent, rel)?;
    let existing = dir.kind_of(name, rel)?;
    match &existing {
        Some(k) if k.is_dir() => return Err(WsError::IsADirectory(display_path(rel))),
        // `resolve` followed it, so it was not a symlink then.
        Some(k) if k.file_type == FileType::Symlink => {
            return Err(WsError::Changed(display_path(rel)))
        }
        _ => {}
    }
    if !overwrite {
        let current = match &existing {
            Some(_) => Some(version_of(&dir.read_regular(name, rel, u64::MAX)?)),
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
    let tmp = dir.temp_with(bytes, existing.as_ref().map(|k| k.mode), rel)?;
    tmp.rename_to(name, true)
        .map_err(|e| WsError::from_io(e, rel))?;
    Ok(version_of(bytes))
}

/// [`Workspace::remove_file`]'s check and removal, in the file's directory opened without
/// following symlinks (review N14).
#[cfg(unix)]
pub(crate) fn remove_blocking(
    root: &Path,
    abs: &Path,
    rel: &str,
    expected: &FileVersion,
) -> Result<(), WsError> {
    let (parent, name) = split(abs, rel)?;
    let dir = crate::beneath::Dir::open_beneath(root, parent, rel)?;
    match dir.kind_of(name, rel)? {
        None => return Err(WsError::NotFound(display_path(rel))),
        Some(k) if k.is_dir() => return Err(WsError::IsADirectory(display_path(rel))),
        Some(_) => {}
    }
    // A FIFO or a device is refused rather than read (it could block).
    let current = version_of(&dir.read_regular(name, rel, u64::MAX)?);
    if current != *expected {
        return Err(WsError::Conflict {
            current: Some(current),
        });
    }
    dir.unlink(name, rel)
}

#[cfg(not(unix))]
pub(crate) fn remove_blocking(
    _root: &Path,
    abs: &Path,
    rel: &str,
    expected: &FileVersion,
) -> Result<(), WsError> {
    let md = std::fs::symlink_metadata(abs).map_err(|e| WsError::from_io(e, rel))?;
    if md.is_dir() {
        return Err(WsError::IsADirectory(display_path(rel)));
    }
    // A FIFO or a device is refused rather than read (it could block).
    let current = version_of(&read_regular(abs, rel, u64::MAX)?);
    if current != *expected {
        return Err(WsError::Conflict {
            current: Some(current),
        });
    }
    std::fs::remove_file(abs).map_err(|e| WsError::from_io(e, rel))
}

#[cfg(not(unix))]
pub(crate) fn write_blocking(
    _root: &Path,
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
            Some(_) => Some(version_of(&read_regular(abs, rel, u64::MAX)?)),
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
    std::io::Write::write_all(&mut tmp, bytes).map_err(|e| WsError::from_io(e, rel))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| WsError::from_io(e, rel))?;
    if let Some(m) = &existing_meta {
        std::fs::set_permissions(tmp.path(), m.permissions())
            .map_err(|e| WsError::from_io(e, rel))?;
    }
    persist_with_retry(tmp, abs, rel)?;
    Ok(version_of(bytes))
}
