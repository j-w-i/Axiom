//! Moving an entry to the person's trash (protocol 0.6 `fs/trash`, audit AUD-022).
//!
//! - Linux and the other freedesktop.org desktops: the Trash specification 1.0. An entry on the
//!   same file system as `$XDG_DATA_HOME` (default `~/.local/share`) goes to the home trash
//!   `$XDG_DATA_HOME/Trash`; one on another file system to `$topdir/.Trash/$uid` (when
//!   `$topdir/.Trash` is a real directory with the sticky bit) or `$topdir/.Trash-$uid`. The
//!   entry is renamed into `files/` after its `info/<name>.trashinfo` has reserved the name, so
//!   the file managers' "Restore" works. Nothing is ever copied across file systems.
//! - macOS: renamed into `~/.Trash` when on the same volume. This is a plain rename, not the
//!   Finder's own "move to trash" (`NSFileManager.trashItem`, which needs Objective-C and
//!   `unsafe` code this crate does not have): the entry appears in the Trash, but the Finder's
//!   "Put Back" does not know where it came from, and an entry on another volume (an external
//!   disk, a second APFS volume) cannot be trashed at all (its per-volume `.Trashes` is the
//!   Finder's own); that is `Unsupported` and the entry stays where it was.
//! - Anywhere else, or when no trash can hold the entry: `Unsupported`. Never a permanent
//!   delete.
//!
//! `DeletionDate` is written in UTC: the daemon has no time-zone database (the spec asks for
//! local time; a file manager shows the date shifted by the UTC offset, restoring is unaffected).

use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub(crate) enum TrashError {
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Where the trash is, read once per operation from the environment (overridable in tests).
#[derive(Debug, Clone)]
pub(crate) struct Places {
    /// The home trash: `$XDG_DATA_HOME/Trash` (Linux), `~/.Trash` (macOS); `None` without a home.
    #[cfg_attr(not(unix), allow(dead_code))]
    pub home_trash: Option<PathBuf>,
}

impl Places {
    pub fn from_env() -> Self {
        let abs = |v: Option<std::ffi::OsString>| {
            v.map(PathBuf::from)
                .filter(|p| p.is_absolute() && !p.as_os_str().is_empty())
        };
        let home = abs(std::env::var_os("HOME"));
        let home_trash = if cfg!(target_vendor = "apple") {
            home.map(|h| h.join(".Trash"))
        } else {
            abs(std::env::var_os("XDG_DATA_HOME"))
                .or_else(|| home.map(|h| h.join(".local").join("share")))
                .map(|d| d.join("Trash"))
        };
        Places { home_trash }
    }
}

#[cfg(all(unix, not(target_vendor = "apple")))]
/// `Path=` of a `.trashinfo`: the path's bytes, percent-encoded except unreserved characters and
/// `/` (RFC 2396, as GLib writes it).
pub(crate) fn encode_path(p: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let bytes = p.as_os_str().as_bytes();
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~/".contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

#[cfg(all(unix, not(target_vendor = "apple")))]
/// `YYYY-MM-DDThh:mm:ss` of seconds since the epoch (UTC).
pub(crate) fn format_date(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}",
        rem / 3600,
        (rem / 60) % 60,
        rem % 60
    )
}

#[cfg(unix)]
/// Candidate names in the trash for `name`: `name`, `name.2`, `name.3`, … (before the
/// extension: `a.txt`, `a.2.txt`).
fn candidate(name: &str, n: u32) -> String {
    if n == 1 {
        return name.to_string();
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem}.{n}.{ext}"),
        _ => format!("{name}.{n}"),
    }
}

#[cfg(unix)]
fn create_private_dir(p: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    match std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(p)
    {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && p.is_dir() => Ok(()),
        Err(e) => Err(e),
    }
}

/// Moves `abs` into the trash directory `trash` (with `files/` and `info/`), recording
/// `info_path` as where it came from. Returns the path it now has.
#[cfg(all(unix, not(target_vendor = "apple")))]
pub(crate) fn put(abs: &Path, trash: &Path, info_path: &Path) -> Result<PathBuf, TrashError> {
    use std::io::Write;
    let files = trash.join("files");
    let info = trash.join("info");
    create_private_dir(&files)?;
    create_private_dir(&info)?;
    let name = abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| TrashError::Unsupported("The entry has no name.".into()))?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let body = format!(
        "[Trash Info]\nPath={}\nDeletionDate={}\n",
        encode_path(info_path),
        format_date(secs)
    );
    for n in 1..10_000 {
        let cand = candidate(&name, n);
        let info_file = info.join(format!("{cand}.trashinfo"));
        // The `.trashinfo`, created exclusively, reserves the name (spec §"Contents of a trash
        // directory": atomic creation).
        let mut f = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&info_file)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        };
        let target = files.join(&cand);
        if std::fs::symlink_metadata(&target).is_ok() {
            // A leftover without its info file: keep it, take another name.
            drop(f);
            let _ = std::fs::remove_file(&info_file);
            continue;
        }
        let written = f.write_all(body.as_bytes()).and_then(|()| f.sync_all());
        drop(f);
        let moved = written.and_then(|()| crate::fileops::rename_noreplace(abs, &target));
        return match moved {
            Ok(()) => Ok(target),
            Err(e) => {
                let _ = std::fs::remove_file(&info_file);
                if e.raw_os_error() == Some(rustix::io::Errno::XDEV.raw_os_error()) {
                    Err(TrashError::Unsupported(
                        "The trash is on another drive; the entry was not moved.".into(),
                    ))
                } else {
                    Err(e.into())
                }
            }
        };
    }
    Err(TrashError::Unsupported(
        "The trash has too many entries with this name.".into(),
    ))
}

/// The file system device of `p`, or of its nearest existing ancestor.
#[cfg(unix)]
fn device_of(p: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    p.ancestors()
        .find_map(|a| std::fs::metadata(a).ok())
        .map(|m| m.dev())
}

/// The top directory of the file system `abs` is on: its highest ancestor on the same device.
#[cfg(all(unix, not(target_vendor = "apple")))]
fn top_dir(abs: &Path, dev: u64) -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let mut top = abs.parent().unwrap_or(abs).to_path_buf();
    for a in abs.ancestors().skip(1) {
        match std::fs::metadata(a) {
            Ok(m) if m.dev() == dev => top = a.to_path_buf(),
            _ => break,
        }
    }
    top
}

/// The trash directory for a file system's top directory (spec §"Trash directories" (1) and
/// (2)), created if needed; `None` if neither may be used.
#[cfg(all(unix, not(target_vendor = "apple")))]
pub(crate) fn top_trash(top: &Path, uid: u32) -> Option<PathBuf> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let owned_dir = |p: &Path| {
        std::fs::symlink_metadata(p)
            .is_ok_and(|m| m.file_type().is_dir() && m.uid() == uid && m.mode() & 0o022 == 0)
    };
    // (1) `$topdir/.Trash`, set up by an administrator: a real directory with the sticky bit.
    let shared = top.join(".Trash");
    if std::fs::symlink_metadata(&shared)
        .is_ok_and(|m| m.file_type().is_dir() && m.permissions().mode() & 0o1000 != 0)
    {
        let mine = shared.join(uid.to_string());
        if create_private_dir(&mine).is_ok() && owned_dir(&mine) {
            return Some(mine);
        }
    }
    // (2) `$topdir/.Trash-$uid`, created now if missing.
    let own = top.join(format!(".Trash-{uid}"));
    if std::fs::symlink_metadata(&own).is_err() && create_private_dir(&own).is_err() {
        return None;
    }
    owned_dir(&own).then_some(own)
}

/// Moves `abs` (the entry itself, not followed) to the trash.
#[cfg(all(unix, not(target_vendor = "apple")))]
pub(crate) fn trash(abs: &Path, places: &Places) -> Result<PathBuf, TrashError> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::symlink_metadata(abs)?.dev();
    if let Some(home) = &places.home_trash {
        if device_of(home) == Some(dev) {
            return put(abs, home, abs);
        }
    }
    let top = top_dir(abs, dev);
    let uid = rustix::process::getuid().as_raw();
    let Some(t) = top_trash(&top, uid) else {
        return Err(TrashError::Unsupported(
            "This drive has no trash folder Axiom may use; the entry was not moved.".into(),
        ));
    };
    // In a top directory's trash, `Path` is relative to the top directory.
    let rel = abs.strip_prefix(&top).unwrap_or(abs);
    put(abs, &t, rel)
}

/// Moves `abs` (the entry itself, not followed) to `~/.Trash`.
#[cfg(target_vendor = "apple")]
pub(crate) fn trash(abs: &Path, places: &Places) -> Result<PathBuf, TrashError> {
    use std::os::unix::fs::MetadataExt;
    let dev = std::fs::symlink_metadata(abs)?.dev();
    let Some(home) = &places.home_trash else {
        return Err(TrashError::Unsupported("There is no home folder.".into()));
    };
    if device_of(home) != Some(dev) {
        let shown = abs.file_name().map_or_else(
            || "The entry".to_string(),
            |n| format!("`{}`", n.to_string_lossy()),
        );
        return Err(TrashError::Unsupported(format!(
            "{shown} is on another volume than your home folder, and Axiom can only move items \
             to the Trash of your home folder's volume. Nothing was moved; use the Finder's Move \
             to Trash for it"
        )));
    }
    create_private_dir(home)?;
    let name = abs
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| TrashError::Unsupported("The entry has no name.".into()))?;
    for n in 1..10_000 {
        let target = home.join(candidate(&name, n));
        match crate::fileops::rename_noreplace(abs, &target) {
            Ok(()) => return Ok(target),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(TrashError::Unsupported(
        "The trash has too many entries with this name.".into(),
    ))
}

/// No trash on this platform: refuse rather than delete.
#[cfg(not(unix))]
pub(crate) fn trash(_abs: &Path, _places: &Places) -> Result<PathBuf, TrashError> {
    Err(TrashError::Unsupported(
        "Moving to the trash is not supported on this platform yet.".into(),
    ))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn paths_are_percent_encoded_like_glib() {
        assert_eq!(
            encode_path(Path::new("/home/u/My Docs/a+b%.txt")),
            "/home/u/My%20Docs/a%2Bb%25.txt"
        );
        assert_eq!(encode_path(Path::new("/x/é")), "/x/%C3%A9");
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn dates_are_rfc3339_without_a_zone() {
        assert_eq!(format_date(0), "1970-01-01T00:00:00");
        // 2026-11-23T13:45:07Z
        assert_eq!(format_date(1_795_441_507), "2026-11-23T13:45:07");
        assert_eq!(format_date(951_782_400), "2000-02-29T00:00:00");
    }

    /// macOS: an entry on another volume than the home trash is refused with a message that
    /// says so, and stays where it was (review N14).
    #[cfg(target_vendor = "apple")]
    #[test]
    fn another_volume_is_refused_clearly_and_nothing_moves() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.txt");
        std::fs::write(&f, "a").unwrap();
        // `/dev` is never the volume of a temp directory.
        let places = Places {
            home_trash: Some(PathBuf::from("/dev/axiom-no-such-trash")),
        };
        match trash(&f, &places) {
            Err(TrashError::Unsupported(m)) => {
                assert!(m.starts_with("`a.txt` is on another volume"), "{m}");
                assert!(m.contains("Nothing was moved"), "{m}");
            }
            other => panic!("{other:?}"),
        }
        assert!(f.exists());
    }

    #[test]
    fn candidates_keep_the_extension() {
        assert_eq!(candidate("a.txt", 1), "a.txt");
        assert_eq!(candidate("a.txt", 2), "a.2.txt");
        assert_eq!(candidate(".env", 3), ".env.3");
        assert_eq!(candidate("Makefile", 2), "Makefile.2");
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn the_home_trash_gets_the_entry_and_its_info() {
        let d = tempfile::tempdir().unwrap();
        let ws = d.path().join("ws");
        std::fs::create_dir(&ws).unwrap();
        let places = Places {
            home_trash: Some(d.path().join("data").join("Trash")),
        };
        for text in ["one", "two"] {
            std::fs::write(ws.join("a b.txt"), text).unwrap();
            trash(&ws.join("a b.txt"), &places).unwrap();
        }
        let t = d.path().join("data/Trash");
        assert_eq!(
            std::fs::read_to_string(t.join("files/a b.txt")).unwrap(),
            "one"
        );
        assert_eq!(
            std::fs::read_to_string(t.join("files/a b.2.txt")).unwrap(),
            "two"
        );
        let info = std::fs::read_to_string(t.join("info/a b.2.txt.trashinfo")).unwrap();
        let mut lines = info.lines();
        assert_eq!(lines.next(), Some("[Trash Info]"));
        assert_eq!(
            lines.next().unwrap(),
            format!("Path={}", encode_path(&ws.join("a b.txt")))
        );
        let date = lines.next().unwrap().strip_prefix("DeletionDate=").unwrap();
        assert_eq!(date.len(), 19);
        assert!(!ws.join("a b.txt").exists());
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn a_symlink_is_trashed_itself_not_its_target() {
        let d = tempfile::tempdir().unwrap();
        let outside = d.path().join("outside.txt");
        std::fs::write(&outside, "keep").unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let places = Places {
            home_trash: Some(d.path().join("Trash")),
        };
        let at = trash(&link, &places).unwrap();
        assert!(std::fs::symlink_metadata(&at)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "keep");
    }

    #[cfg(all(unix, not(target_vendor = "apple")))]
    #[test]
    fn top_directory_trash_rules() {
        use std::os::unix::fs::PermissionsExt;
        let uid = rustix::process::getuid().as_raw();
        // No `.Trash`: `.Trash-$uid` is created, private.
        let d = tempfile::tempdir().unwrap();
        let t = top_trash(d.path(), uid).unwrap();
        assert_eq!(t, d.path().join(format!(".Trash-{uid}")));
        assert_eq!(
            std::fs::metadata(&t).unwrap().permissions().mode() & 0o777,
            0o700
        );
        // `.Trash` with the sticky bit: `.Trash/$uid`.
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".Trash")).unwrap();
        std::fs::set_permissions(
            d.path().join(".Trash"),
            std::fs::Permissions::from_mode(0o1777),
        )
        .unwrap();
        assert_eq!(
            top_trash(d.path(), uid).unwrap(),
            d.path().join(".Trash").join(uid.to_string())
        );
        // `.Trash` without the sticky bit, or a symlink: not used.
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join(".Trash")).unwrap();
        std::fs::set_permissions(
            d.path().join(".Trash"),
            std::fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        assert_eq!(
            top_trash(d.path(), uid).unwrap(),
            d.path().join(format!(".Trash-{uid}"))
        );
        let d = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::set_permissions(elsewhere.path(), std::fs::Permissions::from_mode(0o1777))
            .unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), d.path().join(".Trash")).unwrap();
        assert_eq!(
            top_trash(d.path(), uid).unwrap(),
            d.path().join(format!(".Trash-{uid}"))
        );
        // A `.Trash-$uid` that is a symlink is refused.
        let d = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), d.path().join(format!(".Trash-{uid}")))
            .unwrap();
        assert!(top_trash(d.path(), uid).is_none());
        // In a top directory's trash, Path is relative.
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/x.rs"), "x").unwrap();
        let t = top_trash(d.path(), uid).unwrap();
        put(&d.path().join("src/x.rs"), &t, Path::new("src/x.rs")).unwrap();
        let info = std::fs::read_to_string(t.join("info/x.rs.trashinfo")).unwrap();
        assert!(info.contains("\nPath=src/x.rs\n"), "{info}");
    }
}
