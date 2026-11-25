//! What a copy was at points of its agent's conversation (decision 0015): what differed from what was
//! copied, with its content, each content once; and that made again in a copy of its own. Everything
//! is known by its digest and looked at where it was put, so that nothing that changed meanwhile is
//! taken for what it was.

use std::{
    collections::BTreeMap, ffi::OsStr, io, path::Path, sync::atomic::AtomicBool,
    sync::atomic::Ordering,
};

use axiom_policy::Place;
use cap_fs_ext::DirExt;
use cap_std::{ambient_authority, fs::Dir};

use crate::{
    copy::{self, CopyError, Files, From, Is, LARGEST, Most},
    fs,
};

/// What differs from what was copied; absent if it was removed.
pub type Delta = BTreeMap<String, Option<Is>>;

/// What a content is kept as.
pub fn name(is: &Is) -> String {
    let kind = match is {
        Is { link: true, .. } => ".l",
        Is { run: true, .. } => ".x",
        _ => "",
    };
    format!("{}{kind}", is.digest)
}

fn store_of(store: &Path) -> Result<Dir, CopyError> {
    Dir::open_ambient_dir(store, ambient_authority())
        .map_err(|e| CopyError::Io(store.display().to_string(), e))
}

/// Keeps what a copy is now, in a folder of Axiom's own. `None`: there is more than `most`, or
/// something that cannot be kept, or it is being changed.
pub fn keep(
    folder: &Place,
    copy: &Place,
    was: &Files,
    store: &Path,
    most: usize,
    stop: &AtomicBool,
) -> Result<Option<Delta>, CopyError> {
    let (f, c, s) = (copy::open(folder)?, copy::open(copy)?, store_of(store)?);
    let mut delta = Delta::new();
    let whole = copy::differing(&c, copy, was, stop, &mut |rel, is| {
        if delta.len() >= most {
            return Ok(false);
        }
        if let Some(i) = &is
            && !put_by(&c, rel, i, &s)?
        {
            return Ok(false);
        }
        delta.insert(rel.to_owned(), is);
        Ok(true)
    })?
    .whole;
    if !whole {
        return Ok(None);
    }
    for rel in delta.keys() {
        if stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        // What it was when it was copied is wanted for a point before this, and nobody else keeps it
        // once the folder's own has changed as well.
        if let Some(w) = was.get(rel) {
            put_by(&f, rel, &w.is, &s)?;
        }
    }
    Ok(Some(delta))
}

/// Keeps what some files of a folder are, as far as they are still that.
pub fn keep_these<'a>(
    top: &Place,
    which: impl Iterator<Item = (&'a str, &'a Is)>,
    store: &Path,
) -> Result<(), CopyError> {
    let (t, s) = (copy::open(top)?, store_of(store)?);
    for (rel, is) in which {
        put_by(&t, rel, is, &s)?;
    }
    Ok(())
}

/// Keeps one content, if the file has it. Whether it is kept.
fn put_by(top: &Dir, rel: &str, is: &Is, store: &Dir) -> Result<bool, CopyError> {
    let err = |e| CopyError::Io(rel.to_owned(), e);
    if is.digest.is_empty() {
        return Ok(false);
    }
    let name = name(is);
    if store.symlink_metadata(&name).is_ok() {
        return Ok(true);
    }
    let Some((d, n)) = copy::at(top, rel)? else { return Ok(false) };
    let m = d.symlink_metadata(n).map_err(err)?;
    if m.len() > LARGEST {
        return Ok(false);
    }
    let tmp = format!(".axiom-{}.tmp", fs::fresh());
    let one = || -> io::Result<bool> {
        if m.is_symlink() {
            copy::link(store, &d.read_link_contents(n)?, &tmp)?;
        } else {
            copy::content(&d, OsStr::new(n), store, &tmp)?;
            // Nothing of it runs where it is kept, and it can be taken away again.
            plain(store, &tmp, is.run)?;
        }
        let there = copy::is_at(store, OsStr::new(&tmp))?.map(|t| t.0);
        if there.as_ref() != Some(is) {
            return Ok(false);
        }
        store.rename(&tmp, store, &name)?;
        Ok(true)
    };
    let done = one();
    if !matches!(done, Ok(true)) {
        let _ = store.remove_file_or_symlink(&tmp);
    }
    match done {
        // It went away, or became something else, while this went on.
        Err(e) if d.symlink_metadata(n).is_err() || e.kind() == io::ErrorKind::Unsupported => {
            Ok(false)
        }
        r => r.map_err(err),
    }
}

#[cfg(unix)]
fn plain(d: &Dir, name: &str, run: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if run { 0o755 } else { 0o644 };
    d.set_permissions(
        name,
        cap_std::fs::Permissions::from_std(std::fs::Permissions::from_mode(mode)),
    )
}
#[cfg(not(unix))]
fn plain(d: &Dir, name: &str, _: bool) -> io::Result<()> {
    let mut p = d.symlink_metadata(name)?.permissions();
    p.set_readonly(false);
    d.set_permissions(name, p)
}

/// Where what a point was can be found.
pub struct Sources<'a> {
    pub store: &'a Path,
    /// The copy the point is of. Its `.git` is taken as it is.
    pub copy: &'a Place,
    pub folder: &'a Place,
}

#[derive(Debug, thiserror::Error)]
pub enum AgainError {
    /// The first of them, and how many there are.
    #[error("{0} is not to be had any more as it was then{more}", more = and(*.1))]
    Missing(String, usize),
    #[error(transparent)]
    Copy(#[from] CopyError),
}

fn and(n: usize) -> String {
    if n > 1 { format!(", and {} more", n - 1) } else { String::new() }
}

/// Makes files again, in a folder that is there and empty.
pub fn again(
    files: &BTreeMap<String, Is>,
    from: &Sources<'_>,
    to: &Place,
    most: Most,
    stop: &AtomicBool,
) -> Result<Files, AgainError> {
    let target = copy::open(to)?;
    let full = target.entries().map_err(|e| CopyError::Io(to.to_string(), e))?.next().is_some();
    if full {
        return Err(CopyError::There(to.to_string()).into());
    }
    if files.len() > most.files {
        return Err(CopyError::TooMuch(most).into());
    }
    let store = store_of(from.store).ok();
    let (c, f) = (copy::open(from.copy)?, copy::open(from.folder)?);
    let mut all = Files::new();
    let mut missing: Vec<&String> = Vec::new();
    let mut bytes = 0;
    for (rel, is) in files {
        if stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped.into());
        }
        let blob = name(is);
        let mut found = None;
        let kept = store.as_ref().map(|s| (s.try_clone(), blob.as_str()));
        if let Some((Ok(s), n)) = kept
            && s.symlink_metadata(n).is_ok()
        {
            found = take(&s, n, &target, rel, is)?;
        }
        for top in [&c, &f] {
            if found.is_none()
                && let Some((d, n)) = copy::at(top, rel)?
            {
                found = take(&d, n, &target, rel, is)?;
            }
        }
        match found {
            Some(w) => {
                let size = target.symlink_metadata(rel).map_or(0, |m| m.len());
                bytes += size;
                if bytes > most.bytes {
                    return Err(CopyError::TooMuch(most).into());
                }
                all.insert(rel.clone(), w);
            }
            None => missing.push(rel),
        }
    }
    if let Some(first) = missing.first() {
        return Err(AgainError::Missing((*first).clone(), missing.len()));
    }
    if c.symlink_metadata(".git").is_ok_and(|m| m.is_dir()) {
        let may = |_: &Place| true;
        let of = From { folder: from.copy, tracked: &[], may: &may, stop };
        of.git(&c, &mut |d, n, rel| match copy::put(d, n, &target, rel, false) {
            Err(e) if e.kind() == Some(io::ErrorKind::NotFound) => Ok(()),
            r => r.map(drop),
        })?;
    }
    Ok(all)
}

/// Puts a file there, if it is what it is taken for.
fn take(
    from: &Dir,
    name: &str,
    to: &Dir,
    rel: &str,
    is: &Is,
) -> Result<Option<copy::Was>, CopyError> {
    match copy::put(from, OsStr::new(name), to, rel, false) {
        Ok(w) if w.is == *is => Ok(Some(w)),
        Ok(_) => {
            if let Some((d, n)) = copy::at(to, rel)? {
                d.remove_file_or_symlink(n).map_err(|e| CopyError::Io(rel.to_owned(), e))?;
            }
            Ok(None)
        }
        Err(e)
            if from.symlink_metadata(name).is_err()
                || e.kind() == Some(io::ErrorKind::Unsupported) =>
        {
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

#[cfg(all(test, unix))]
#[path = "points_tests.rs"]
mod tests;
