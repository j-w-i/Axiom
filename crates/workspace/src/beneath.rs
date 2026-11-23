//! Acting on a path that [`crate::Workspace::resolve`] already checked, without following a
//! symlink swapped in after the check (review N14, R5). On Unix the directory an operation acts
//! in is opened from the workspace root one component at a time with `O_NOFOLLOW`, and the
//! operation is done relative to that directory (`openat`, `renameat`, `unlinkat`): a local
//! process that replaces a directory on the way with a symlink makes the operation fail instead
//! of redirecting it. Elsewhere (Windows) only the re-check right before the operation applies.

#[cfg(unix)]
pub(crate) use unix::*;

#[cfg(unix)]
mod unix {
    use crate::error::{display_path, WsError};
    use rustix::fs::{AtFlags, FileType, Mode, OFlags, CWD};
    use rustix::io::Errno;
    use std::ffi::{OsStr, OsString};
    use std::os::fd::{AsFd, OwnedFd};
    use std::path::{Component, Path};

    /// What a directory on the way became since it was resolved.
    fn swapped(rel: &str) -> WsError {
        WsError::Changed(display_path(rel))
    }

    fn io(e: Errno) -> std::io::Error {
        e.into()
    }

    /// A directory inside the workspace, opened without following symlinks.
    pub(crate) struct Dir {
        fd: OwnedFd,
    }

    impl Dir {
        /// Opens `dir` (a resolved path: canonical, inside `root`) from `root` one component at
        /// a time, never following a symlink. A component that is now a symlink is
        /// [`WsError::Changed`]; one that is missing or not a directory is reported as such for
        /// `rel` (the workspace path the caller acts on).
        pub(crate) fn open_beneath(root: &Path, dir: &Path, rel: &str) -> Result<Dir, WsError> {
            let below = dir.strip_prefix(root).map_err(|_| swapped(rel))?;
            let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
            let mut fd = rustix::fs::openat(CWD, root, flags, Mode::empty())
                .map_err(|e| WsError::from_io(io(e), rel))?;
            for c in below.components() {
                let Component::Normal(name) = c else {
                    return Err(swapped(rel));
                };
                fd = match rustix::fs::openat(&fd, name, flags | OFlags::NOFOLLOW, Mode::empty()) {
                    Ok(next) => next,
                    Err(e) if e == Errno::LOOP || e == Errno::NOTDIR => {
                        // A symlink now (O_NOFOLLOW), or not a directory any more.
                        return Err(
                            match rustix::fs::statat(&fd, name, AtFlags::SYMLINK_NOFOLLOW) {
                                Ok(st)
                                    if FileType::from_raw_mode(st.st_mode) == FileType::Symlink =>
                                {
                                    swapped(rel)
                                }
                                Ok(_) => WsError::NotADirectory(display_path(rel)),
                                Err(e) => WsError::from_io(io(e), rel),
                            },
                        );
                    }
                    Err(e) => return Err(WsError::from_io(io(e), rel)),
                };
            }
            Ok(Dir { fd })
        }

        /// What `name` is in this directory, without following it; `None` if nothing has it.
        // `stat`'s field types differ between platforms (`st_dev` is `i32` on macOS, `u64` on
        // Linux; `st_mode` `u16` and `u32`): the casts are needed on some, not on all.
        #[allow(clippy::unnecessary_cast)]
        pub(crate) fn kind_of(&self, name: &OsStr, rel: &str) -> Result<Option<Kind>, WsError> {
            match rustix::fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
                Ok(st) => Ok(Some(Kind {
                    file_type: FileType::from_raw_mode(st.st_mode),
                    mode: st.st_mode as u32 & 0o7777,
                    dev: st.st_dev as u64,
                    ino: st.st_ino as u64,
                })),
                Err(Errno::NOENT) => Ok(None),
                Err(e) => Err(WsError::from_io(io(e), rel)),
            }
        }

        /// Reads the regular file `name` of at most `limit` bytes, not following a symlink (one
        /// swapped in for it is [`WsError::Changed`]) and never blocking on a FIFO.
        pub(crate) fn read_regular(
            &self,
            name: &OsStr,
            rel: &str,
            limit: u64,
        ) -> Result<Vec<u8>, WsError> {
            use std::io::Read;
            let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
            let fd = match rustix::fs::openat(&self.fd, name, flags, Mode::empty()) {
                Ok(fd) => fd,
                Err(Errno::LOOP) => return Err(swapped(rel)),
                Err(Errno::ISDIR) => return Err(WsError::IsADirectory(display_path(rel))),
                Err(e) => return Err(WsError::from_io(io(e), rel)),
            };
            let f = std::fs::File::from(fd);
            let md = f.metadata().map_err(|e| WsError::from_io(e, rel))?;
            if md.is_dir() {
                return Err(WsError::IsADirectory(display_path(rel)));
            }
            if !md.is_file() {
                return Err(WsError::NotAFile(display_path(rel)));
            }
            if md.len() > limit {
                return Err(WsError::TooLarge { bytes: md.len() });
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

        /// Writes `bytes` to a new temp file in this directory (fsynced) and returns it; it is
        /// removed again if it is dropped without being renamed. Its permissions are `mode`
        /// (those of the file it replaces), else ordinary ones (0644 less the umask) rather
        /// than a temp file's 0600.
        pub(crate) fn temp_with(
            &self,
            bytes: &[u8],
            mode: Option<u32>,
            rel: &str,
        ) -> Result<Temp<'_>, WsError> {
            use std::io::Write;
            let flags =
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
            for attempt in 0u32..100 {
                let name = OsString::from(format!(".axiom-tmp-{}", unique(attempt)));
                let fd =
                    match rustix::fs::openat(&self.fd, &name, flags, Mode::from_raw_mode(0o644)) {
                        Ok(fd) => fd,
                        Err(Errno::EXIST) => continue,
                        Err(e) => return Err(WsError::from_io(io(e), rel)),
                    };
                let temp = Temp {
                    dir: self,
                    name,
                    armed: true,
                };
                if let Some(mode) = mode {
                    rustix::fs::fchmod(&fd, Mode::from_raw_mode(mode as _))
                        .map_err(|e| WsError::from_io(io(e), rel))?;
                }
                let mut f = std::fs::File::from(fd);
                f.write_all(bytes).map_err(|e| WsError::from_io(e, rel))?;
                f.sync_all().map_err(|e| WsError::from_io(e, rel))?;
                return Ok(temp);
            }
            Err(WsError::Io(std::io::Error::from(
                std::io::ErrorKind::AlreadyExists,
            )))
        }

        /// Removes the file (not a directory) `name`.
        pub(crate) fn unlink(&self, name: &OsStr, rel: &str) -> Result<(), WsError> {
            rustix::fs::unlinkat(&self.fd, name, AtFlags::empty())
                .map_err(|e| WsError::from_io(io(e), rel))
        }

        /// Creates the directory `name`.
        pub(crate) fn mkdir(&self, name: &OsStr) -> std::io::Result<()> {
            rustix::fs::mkdirat(&self.fd, name, Mode::from_raw_mode(0o777)).map_err(io)
        }
    }

    /// Renames `from` in `a` to `to` in `b`, replacing what `to` names (it is never followed).
    pub(crate) fn rename(a: &Dir, from: &OsStr, b: &Dir, to: &OsStr) -> std::io::Result<()> {
        rustix::fs::renameat(&a.fd, from, &b.fd, to).map_err(io)
    }

    /// Renames `from` in `a` to `to` in `b`, failing with `AlreadyExists` if `to` exists:
    /// atomically where the platform and the file system can, else checked right before.
    pub(crate) fn rename_noreplace(
        a: &Dir,
        from: &OsStr,
        b: &Dir,
        to: &OsStr,
    ) -> std::io::Result<()> {
        #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
        match rustix::fs::renameat_with(&a.fd, from, &b.fd, to, rustix::fs::RenameFlags::NOREPLACE)
        {
            Ok(()) => return Ok(()),
            // The kernel or the file system has no such flag: check, then rename.
            Err(Errno::NOSYS) | Err(Errno::INVAL) | Err(Errno::NOTSUP) => {}
            Err(e) => return Err(io(e)),
        }
        if rustix::fs::statat(b.fd.as_fd(), to, AtFlags::SYMLINK_NOFOLLOW).is_ok() {
            return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
        }
        rustix::fs::renameat(&a.fd, from, &b.fd, to).map_err(io)
    }

    /// An entry as `lstat` sees it.
    pub(crate) struct Kind {
        pub(crate) file_type: FileType,
        /// The permission bits.
        pub(crate) mode: u32,
        pub(crate) dev: u64,
        pub(crate) ino: u64,
    }

    impl Kind {
        pub(crate) fn is_dir(&self) -> bool {
            self.file_type == FileType::Directory
        }
    }

    /// A temp file in a [`Dir`], removed on drop unless [`Temp::rename_to`] moved it.
    pub(crate) struct Temp<'a> {
        dir: &'a Dir,
        name: OsString,
        armed: bool,
    }

    impl Temp<'_> {
        /// Renames the temp file over `name` in its directory (`replace`), or onto a free
        /// `name` only.
        pub(crate) fn rename_to(mut self, name: &OsStr, replace: bool) -> std::io::Result<()> {
            let r = if replace {
                rename(self.dir, &self.name, self.dir, name)
            } else {
                rename_noreplace(self.dir, &self.name, self.dir, name)
            };
            if r.is_ok() {
                self.armed = false;
            }
            r
        }
    }

    impl Drop for Temp<'_> {
        fn drop(&mut self) {
            if self.armed {
                let _ = rustix::fs::unlinkat(&self.dir.fd, &self.name, AtFlags::empty());
            }
        }
    }

    /// A name no other temp file of this process has (`O_EXCL` settles a clash with anyone
    /// else's).
    fn unique(attempt: u32) -> String {
        use std::hash::{BuildHasher, Hasher};
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(NEXT.fetch_add(1, Ordering::Relaxed));
        h.write_u32(attempt);
        h.write_u32(std::process::id());
        format!("{:016x}", h.finish())
    }
}

/// A directory on the way swapped for a symlink after the path was resolved (review N14, R5):
/// the operation is refused, never redirected. Each operation is held at its per-path lock
/// (where it waits after resolving) while the swap is made, so the order is certain.
#[cfg(all(test, unix))]
mod tests {
    use crate::{Workspace, WsError};
    use axiom_protocol::{WorkspaceId, WorkspacePath};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    fn wp(s: &str) -> WorkspacePath {
        WorkspacePath(s.into())
    }

    /// Refused as leading outside the workspace (the re-check right before), or as changed
    /// (the directory opened without following symlinks).
    fn refused<T>(r: &Result<T, WsError>) -> bool {
        matches!(r, Err(WsError::Escapes(_) | WsError::Changed(_)))
    }

    struct Setup {
        _dirs: (tempfile::TempDir, tempfile::TempDir),
        root: PathBuf,
        outside: PathBuf,
        ws: Workspace,
    }

    fn setup() -> Setup {
        let (r, o) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let root = dunce::canonicalize(r.path()).unwrap();
        let outside = dunce::canonicalize(o.path()).unwrap();
        std::fs::create_dir(root.join("sub")).unwrap();
        let ws = Workspace::new(WorkspaceId("w1".into()), root.clone());
        Setup {
            _dirs: (r, o),
            root,
            outside,
            ws,
        }
    }

    /// Replaces the directory `dir` with a symlink to `to`.
    fn swap(dir: &Path, to: &Path) {
        std::fs::rename(dir, dir.with_extension("old")).unwrap();
        std::os::unix::fs::symlink(to, dir).unwrap();
    }

    /// Holds the lock of `abs`, runs `op`, makes the swap once `op` waits at that lock, then
    /// lets it go on and returns its result.
    async fn swapped_while_waiting<T: Send + 'static>(
        s: &Setup,
        abs: PathBuf,
        swap_to: PathBuf,
        op: impl std::future::Future<Output = Result<T, WsError>> + Send + 'static,
    ) -> Result<T, WsError> {
        let lock = s.ws.lock_for(&abs);
        let guard = lock.lock().await;
        let task = tokio::spawn(op);
        // The map, this test, and the operation once it has resolved its path.
        let end = tokio::time::Instant::now() + Duration::from_secs(10);
        while Arc::strong_count(&lock) < 3 {
            assert!(
                tokio::time::Instant::now() < end,
                "the operation never got to its lock"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        swap(&s.root.join("sub"), &swap_to);
        drop(guard);
        task.await.unwrap()
    }

    #[tokio::test]
    async fn a_write_is_not_redirected_outside_the_workspace() {
        let s = setup();
        let ws = s.ws.clone();
        let r = swapped_while_waiting(
            &s,
            s.root.join("sub/f.txt"),
            s.outside.clone(),
            async move { ws.write_text(&wp("sub/f.txt"), "x", None, false).await },
        )
        .await;
        assert!(refused(&r), "{r:?}");
        assert!(
            !s.outside.join("f.txt").exists(),
            "written outside the workspace"
        );
    }

    #[tokio::test]
    async fn a_created_file_is_not_redirected_outside_the_workspace() {
        let s = setup();
        let ws = s.ws.clone();
        let r = swapped_while_waiting(
            &s,
            s.root.join("sub/f.txt"),
            s.outside.clone(),
            async move { ws.create_file(&wp("sub/f.txt"), "x").await },
        )
        .await;
        assert!(refused(&r), "{r:?}");
        assert!(
            !s.outside.join("f.txt").exists(),
            "created outside the workspace"
        );
    }

    #[tokio::test]
    async fn a_removal_is_not_redirected_outside_the_workspace() {
        let s = setup();
        std::fs::write(s.root.join("sub/f.txt"), "same").unwrap();
        std::fs::write(s.outside.join("f.txt"), "same").unwrap();
        let v = crate::workspace::version_of(b"same");
        let ws = s.ws.clone();
        let r = swapped_while_waiting(
            &s,
            s.root.join("sub/f.txt"),
            s.outside.clone(),
            async move { ws.remove_file(&wp("sub/f.txt"), &v).await },
        )
        .await;
        assert!(refused(&r), "{r:?}");
        assert!(
            s.outside.join("f.txt").exists(),
            "removed outside the workspace"
        );
    }

    #[tokio::test]
    async fn a_rename_is_not_redirected_outside_the_workspace() {
        let s = setup();
        std::fs::write(s.root.join("a.txt"), "a").unwrap();
        let (lo, hi) = (s.root.join("a.txt"), s.root.join("sub/b.txt"));
        // The rename takes the lock of the smaller path first: hold that one.
        let first = if lo <= hi { lo } else { hi };
        let ws = s.ws.clone();
        let r = swapped_while_waiting(&s, first, s.outside.clone(), async move {
            ws.rename(&wp("a.txt"), &wp("sub/b.txt")).await
        })
        .await;
        assert!(refused(&r), "{r:?}");
        assert!(
            !s.outside.join("b.txt").exists(),
            "moved outside the workspace"
        );
        assert!(s.root.join("a.txt").exists());
    }

    /// Review R5: the agent gate judged `sub/post-merge` when the call was prepared; by the
    /// time it is applied `sub` leads into `.git/hooks` (inside the workspace, so the path is
    /// still valid). The checked write is refused instead of writing a hook.
    #[tokio::test]
    async fn a_checked_write_is_refused_when_the_path_now_leads_elsewhere() {
        let s = setup();
        let hooks = s.root.join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        let judged = s.ws.canonical_path(&wp("sub/post-merge")).unwrap();
        assert_eq!(judged, wp("sub/post-merge"));
        swap(&s.root.join("sub"), &hooks);
        let r =
            s.ws.write_text_checked(&wp("sub/post-merge"), &judged, "evil", None, false)
                .await;
        assert!(matches!(r, Err(WsError::Changed(_))), "{r:?}");
        assert!(!hooks.join("post-merge").exists(), "a hook was written");
        let r =
            s.ws.remove_file_checked(
                &wp("sub/x"),
                &wp("sub/x"),
                &crate::workspace::version_of(b""),
            )
            .await;
        assert!(matches!(r, Err(WsError::Changed(_))), "{r:?}");
    }

    /// Review R5: a hard link to a hook inside the workspace is not a way to change the hook. A
    /// write replaces the link's entry (temp file and rename), so the hook keeps its content.
    #[tokio::test]
    async fn writing_a_hard_link_to_a_hook_leaves_the_hook_alone() {
        let s = setup();
        let hooks = s.root.join(".git/hooks");
        std::fs::create_dir_all(&hooks).unwrap();
        std::fs::write(hooks.join("post-merge"), "orig").unwrap();
        std::fs::hard_link(hooks.join("post-merge"), s.root.join("sub/x")).unwrap();
        let v = crate::workspace::version_of(b"orig");
        s.ws.write_text(&wp("sub/x"), "evil", Some(&v), false)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(s.root.join("sub/x")).unwrap(),
            "evil"
        );
        assert_eq!(
            std::fs::read_to_string(hooks.join("post-merge")).unwrap(),
            "orig"
        );
    }

    /// The directory is opened without following a symlink on the way, whatever checked it
    /// before: what makes the moment between the last check and the operation safe.
    #[test]
    fn a_directory_reached_through_a_symlink_is_not_opened() {
        let s = setup();
        std::fs::create_dir(s.root.join("sub/deeper")).unwrap();
        let resolved = s.root.join("sub/deeper");
        swap(&s.root.join("sub"), &s.outside);
        std::fs::create_dir(s.outside.join("deeper")).unwrap();
        let r = super::Dir::open_beneath(&s.root, &resolved, "sub/deeper/f");
        assert!(matches!(r, Err(WsError::Changed(_))), "{:?}", r.err());
        // A plain directory chain opens.
        std::fs::create_dir_all(s.root.join("x/y")).unwrap();
        assert!(super::Dir::open_beneath(&s.root, &s.root.join("x/y"), "x/y/f").is_ok());
    }
}
