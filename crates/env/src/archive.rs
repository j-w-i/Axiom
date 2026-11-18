//! Safe extraction of verified archives into a staging root.
//!
//! Every entry is placed by our own path logic, never by the archive library's unpacker:
//! - absolute paths, drive prefixes and `..` components are rejected;
//! - an entry may not be written through a symbolic link (no entry's parent may be a link), so a
//!   link inside the archive cannot redirect later entries;
//! - a symbolic link's target must be relative and stay inside the root, resolved lexically from
//!   the link's own directory; hard links must name an earlier regular file inside the root;
//! - devices, FIFOs and other special entries are skipped;
//! - permissions keep only the `rwx` bits (no setuid, setgid or sticky).

use crate::error::EnvError;
use axiom_protocol::provider::ArchiveFormat;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

/// Extracts `archive` into the existing, empty directory `root`, dropping the first
/// `strip_components` path components of every entry. `cancelled` is polled between entries.
/// Returns the number of entries written.
pub fn extract(
    archive: &Path,
    format: ArchiveFormat,
    strip_components: u32,
    root: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<u64, EnvError> {
    let file = File::open(archive).map_err(EnvError::io("Cannot open the downloaded archive"))?;
    match format {
        ArchiveFormat::TarGz => extract_tar(
            flate2::read::GzDecoder::new(file),
            strip_components,
            root,
            cancelled,
        ),
        ArchiveFormat::TarXz => extract_tar(
            xz2::read::XzDecoder::new(file),
            strip_components,
            root,
            cancelled,
        ),
        ArchiveFormat::Zip => extract_zip(file, strip_components, root, cancelled),
    }
}

/// Validates an entry path and applies `strip`. `None`: nothing left after stripping.
pub fn entry_path(raw: &str, strip: u32) -> Result<Option<PathBuf>, EnvError> {
    // Archives made on Windows may use `\`; treat it as a separator everywhere.
    let normalised = raw.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    if normalised.starts_with('/') {
        return Err(EnvError::UnsafeArchive(format!("absolute path `{raw}`")));
    }
    for c in Path::new(&normalised).components() {
        match c {
            Component::Normal(s) => match s.to_str() {
                Some(s) if !s.contains(':') => parts.push(s),
                _ => return Err(EnvError::UnsafeArchive(format!("unusable name `{raw}`"))),
            },
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(EnvError::UnsafeArchive(format!("`..` in `{raw}`")));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(EnvError::UnsafeArchive(format!("absolute path `{raw}`")));
            }
        }
    }
    let rest: PathBuf = parts.iter().skip(strip as usize).collect();
    Ok((!rest.as_os_str().is_empty()).then_some(rest))
}

/// Whether `target`, read from the directory `link_dir` (both relative to the root), stays
/// inside the root. Lexical: no file system access.
pub fn link_stays_inside(link_dir: &Path, target: &str) -> bool {
    let t = target.replace('\\', "/");
    if t.starts_with('/') || t.contains(':') {
        return false;
    }
    let mut depth: Vec<String> = link_dir
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    for c in Path::new(&t).components() {
        match c {
            Component::Normal(s) => depth.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth.pop().is_none() {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// No existing ancestor of `rel` (below `root`) may be a symbolic link.
fn check_parents(root: &Path, rel: &Path) -> Result<(), EnvError> {
    let mut at = root.to_path_buf();
    let comps: Vec<_> = rel.components().collect();
    for c in &comps[..comps.len().saturating_sub(1)] {
        at.push(c);
        if let Ok(md) = std::fs::symlink_metadata(&at) {
            if md.file_type().is_symlink() {
                return Err(EnvError::UnsafeArchive(format!(
                    "`{}` would be written through a symbolic link",
                    rel.display()
                )));
            }
        }
    }
    Ok(())
}

fn create_parent(path: &Path) -> Result<(), EnvError> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(EnvError::io("Cannot create a directory"))?;
    }
    Ok(())
}

fn write_file(path: &Path, mut data: impl Read, mode: Option<u32>) -> Result<(), EnvError> {
    create_parent(path)?;
    // A previous entry of the same name is replaced, never followed.
    if let Ok(md) = std::fs::symlink_metadata(path) {
        if md.is_dir() {
            return Err(EnvError::UnsafeArchive(format!(
                "`{}` is both a directory and a file",
                path.display()
            )));
        }
        std::fs::remove_file(path).map_err(EnvError::io("Cannot replace a file"))?;
    }
    let mut out = File::create(path).map_err(EnvError::io("Cannot create a file"))?;
    io::copy(&mut data, &mut out).map_err(EnvError::io("Cannot extract a file"))?;
    set_mode(path, mode)?;
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: Option<u32>) -> Result<(), EnvError> {
    use std::os::unix::fs::PermissionsExt;
    if let Some(m) = mode {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(m & 0o777))
            .map_err(EnvError::io("Cannot set permissions"))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: Option<u32>) -> Result<(), EnvError> {
    Ok(())
}

fn symlink(root: &Path, rel: &Path, target: &str) -> Result<(), EnvError> {
    let dir = rel.parent().unwrap_or(Path::new(""));
    if !link_stays_inside(dir, target) {
        return Err(EnvError::UnsafeArchive(format!(
            "the link `{}` points outside the install (`{target}`)",
            rel.display()
        )));
    }
    let path = root.join(rel);
    create_parent(&path)?;
    let _ = std::fs::remove_file(&path);
    #[cfg(unix)]
    let r = std::os::unix::fs::symlink(target, &path);
    #[cfg(windows)]
    let r = {
        let resolved = path.parent().unwrap_or(root).join(target);
        if resolved.is_dir() {
            std::os::windows::fs::symlink_dir(target, &path)
        } else {
            std::os::windows::fs::symlink_file(target, &path)
        }
    };
    r.map_err(EnvError::io("Cannot create a symbolic link"))
}

fn extract_tar(
    reader: impl Read,
    strip: u32,
    root: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<u64, EnvError> {
    let mut ar = tar::Archive::new(reader);
    let mut count = 0;
    let bad = |e: io::Error| EnvError::UnsafeArchive(format!("unreadable archive: {e}"));
    for entry in ar.entries().map_err(bad)? {
        if cancelled() {
            return Err(EnvError::Cancelled);
        }
        let mut entry = entry.map_err(bad)?;
        let raw = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        let Some(rel) = entry_path(&raw, strip)? else {
            continue;
        };
        check_parents(root, &rel)?;
        let path = root.join(&rel);
        let kind = entry.header().entry_type();
        let mode = entry.header().mode().ok();
        if kind.is_dir() {
            std::fs::create_dir_all(&path).map_err(EnvError::io("Cannot create a directory"))?;
        } else if kind.is_file() {
            write_file(&path, &mut entry, mode)?;
        } else if kind.is_symlink() {
            let target = entry
                .link_name_bytes()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            symlink(root, &rel, &target)?;
        } else if kind.is_hard_link() {
            let target = entry
                .link_name_bytes()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let Some(trel) = entry_path(&target, strip)? else {
                return Err(EnvError::UnsafeArchive(format!(
                    "hard link `{raw}` has no target"
                )));
            };
            check_parents(root, &trel)?;
            let tpath = root.join(&trel);
            let md = std::fs::symlink_metadata(&tpath).map_err(|_| {
                EnvError::UnsafeArchive(format!("hard link `{raw}` names a missing file"))
            })?;
            if !md.is_file() {
                return Err(EnvError::UnsafeArchive(format!(
                    "hard link `{raw}` does not name a regular file"
                )));
            }
            create_parent(&path)?;
            let _ = std::fs::remove_file(&path);
            std::fs::hard_link(&tpath, &path).map_err(EnvError::io("Cannot create a hard link"))?;
        } else {
            // Devices, FIFOs, PAX/GNU metadata entries: nothing to write.
            continue;
        }
        count += 1;
    }
    Ok(count)
}

fn extract_zip(
    file: File,
    strip: u32,
    root: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<u64, EnvError> {
    let bad =
        |e: zip::result::ZipError| EnvError::UnsafeArchive(format!("unreadable archive: {e}"));
    let mut ar = zip::ZipArchive::new(file).map_err(bad)?;
    let mut count = 0;
    for i in 0..ar.len() {
        if cancelled() {
            return Err(EnvError::Cancelled);
        }
        let mut f = ar.by_index(i).map_err(bad)?;
        let raw = f.name().map_err(bad)?.into_owned();
        let Some(rel) = entry_path(&raw, strip)? else {
            continue;
        };
        check_parents(root, &rel)?;
        let path = root.join(&rel);
        let mode = f.unix_mode();
        let is_link = mode.is_some_and(|m| m & 0o170000 == 0o120000);
        if f.is_dir() {
            std::fs::create_dir_all(&path).map_err(EnvError::io("Cannot create a directory"))?;
        } else if is_link {
            // A link target is a path: never read more than 4 KiB, whatever the size field says.
            let mut target = String::new();
            (&mut f)
                .take(4096)
                .read_to_string(&mut target)
                .map_err(|e| EnvError::UnsafeArchive(format!("unreadable link `{raw}`: {e}")))?;
            symlink(root, &rel, &target)?;
        } else {
            write_file(&path, &mut f, mode)?;
        }
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_paths() {
        assert_eq!(entry_path("a/b/c", 1).unwrap(), Some(PathBuf::from("b/c")));
        assert_eq!(entry_path("./a/", 1).unwrap(), None);
        assert!(entry_path("../x", 0).is_err());
        assert!(entry_path("a/../../x", 0).is_err());
        assert!(entry_path("/etc/passwd", 0).is_err());
        assert!(entry_path("a\\..\\..\\x", 0).is_err());
        assert!(entry_path("C:/x", 0).is_err());
    }

    #[test]
    fn link_targets() {
        assert!(link_stays_inside(Path::new("bin"), "../lib/node"));
        assert!(link_stays_inside(Path::new(""), "lib"));
        assert!(!link_stays_inside(Path::new("bin"), "../../etc"));
        assert!(!link_stays_inside(Path::new(""), "/usr/bin/x"));
        assert!(!link_stays_inside(Path::new("a"), "..\\..\\x"));
    }
}
