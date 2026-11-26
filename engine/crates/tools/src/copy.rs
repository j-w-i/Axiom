//! A folder's files for one agent of its own (decision 0014): made, compared with the folder, and
//! brought back into it, all by handle (SI-9). Links are copied as links and never followed. Nothing
//! is merged: what has changed in both is said to be a clash.

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    io::{self, Read},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use axiom_policy::{Place, paths};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use ignore::gitignore::Gitignore;
use serde::{Deserialize, Serialize};

use crate::{
    fs::{self, FsError, Kind},
    text,
    walk::{May, ignore_rules, ignored},
};

const DEEPEST: usize = 64;
/// Of one file, what may be brought in or put back.
pub const LARGEST: u64 = 64 * 1024 * 1024;
/// Of one file, what is compared line by line.
const SHOWN: u64 = 1024 * 1024;

/// What a copy may come to.
#[derive(Debug, Clone, Copy)]
pub struct Most {
    pub files: usize,
    pub bytes: u64,
}
impl Default for Most {
    fn default() -> Self {
        Self { files: 100_000, bytes: 16 * 1024 * 1024 * 1024 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error(
        "the folder has more than {} files or {} GiB that would be copied (what git ignores is not counted)",
        .0.files, .0.bytes >> 30
    )]
    TooMuch(Most),
    #[error("it was stopped")]
    Stopped,
    #[error("{0} is there already")]
    There(String),
    #[error(transparent)]
    Fs(#[from] FsError),
    #[error("{0}: {1}")]
    Io(String, io::Error),
}

impl CopyError {
    pub(crate) fn kind(&self) -> Option<io::ErrorKind> {
        match self {
            CopyError::Io(_, e) => Some(e.kind()),
            _ => None,
        }
    }
}

/// A file as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Is {
    pub digest: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub link: bool,
    /// It can be run.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub run: bool,
}

impl Is {
    /// With the same in it, whether or not it can be run.
    fn like(&self, o: &Is) -> bool {
        self.digest == o.digest && self.link == o.link
    }
}

/// Whether two have the same in them, or are both not there.
pub fn alike(a: Option<&Is>, b: Option<&Is>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.like(b),
        _ => false,
    }
}

/// A file of the copy as it was put there: that it has not been touched since is seen without
/// reading it, where the system says so in a way that is not anybody's to set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Was {
    #[serde(flatten)]
    pub is: Is,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stamp: Option<String>,
}

/// By their names below the folder, with `/`.
pub type Files = BTreeMap<String, Was>;

/// What is copied, and what of it may not be.
pub struct From<'a> {
    pub folder: &'a Place,
    /// Tracked by git, whatever it ignores now.
    pub tracked: &'a [String],
    pub may: May<'a>,
    pub stop: &'a AtomicBool,
}

pub(crate) fn open(p: &Place) -> Result<Dir, CopyError> {
    Dir::open_ambient_dir(p.as_path(), ambient_authority())
        .map_err(|e| CopyError::Io(p.to_string(), e))
}

pub(crate) type Each<'a> = &'a mut dyn FnMut(&Dir, &OsStr, &str) -> Result<(), CopyError>;

struct Walk<'a> {
    base: &'a Place,
    /// What git ignores is left out.
    ignoring: bool,
    /// These are met although git ignores them.
    known: Option<&'a Files>,
    may: May<'a>,
    stop: &'a AtomicBool,
    /// What was not met: because git ignores it, and because it cannot be named or is too deep.
    ignored: Cell<usize>,
    odd: Cell<usize>,
}

fn more(c: &Cell<usize>) {
    c.set(c.get() + 1);
}

impl<'a> Walk<'a> {
    fn new(base: &'a Place, ignoring: bool, may: May<'a>, stop: &'a AtomicBool) -> Self {
        let (ignored, odd) = (Cell::new(0), Cell::new(0));
        Self { base, ignoring, known: None, may, stop, ignored, odd }
    }

    fn each(&self, f: Each<'_>) -> Result<(), CopyError> {
        self.folder(&open(self.base)?, "", 0, &mut Vec::new(), false, f)
    }

    /// Whether something known is below a folder.
    fn holds(&self, below: &str) -> bool {
        let at = format!("{below}/");
        self.known
            .is_some_and(|k| k.range(at.clone()..).next().is_some_and(|(n, _)| n.starts_with(&at)))
    }

    fn folder(
        &self,
        dir: &Dir,
        rel: &str,
        depth: usize,
        rules: &mut Vec<Gitignore>,
        out: bool,
        f: Each<'_>,
    ) -> Result<(), CopyError> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        let here = self.base.as_path().join(rel);
        let had = rules.len();
        if self.ignoring {
            rules.extend(ignore_rules(dir, &here, &[".gitignore"]));
        }
        let err = |e| CopyError::Io(here.display().to_string(), e);
        let mut names = Vec::new();
        for e in dir.entries().map_err(err)? {
            let e = e.map_err(err)?;
            names.push((e.file_name(), e.file_type().map_err(err)?));
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (name, kind) in names {
            if self.ignoring && name.eq_ignore_ascii_case(".git") {
                continue;
            }
            // What has no name that can be written down is left where it is.
            let Some(n) = name.to_str().filter(|n| parts(n).is_some()) else {
                more(&self.odd);
                continue;
            };
            let path = here.join(n);
            let below = if rel.is_empty() { n.to_owned() } else { format!("{rel}/{n}") };
            let out = out || ignored(rules, &path, kind.is_dir());
            if out {
                let met = match kind.is_dir() {
                    true => self.holds(&below),
                    false => self.known.is_some_and(|k| k.contains_key(&below)),
                };
                if !met {
                    more(&self.ignored);
                    continue;
                }
            }
            let Ok(place) = Place::new(&path) else {
                more(&self.odd);
                continue;
            };
            if !(self.may)(&place) {
                continue;
            }
            if kind.is_dir() {
                if depth >= DEEPEST {
                    more(&self.odd);
                    continue;
                }
                match dir.open_dir_nofollow(&name) {
                    Ok(d) => self.folder(&d, &below, depth + 1, rules, out, f)?,
                    // It is gone, or became a link: then it is none of the folder's.
                    Err(_) if !dir.symlink_metadata(&name).is_ok_and(|m| m.is_dir()) => {}
                    Err(e) => return Err(CopyError::Io(path.display().to_string(), e)),
                }
            } else if kind.is_file() || kind.is_symlink() {
                f(dir, &name, &below)?;
            } else {
                more(&self.odd);
            }
        }
        rules.truncate(had);
        Ok(())
    }
}

/// The file that a name of an open folder is, without following a link.
pub(crate) fn is_at(dir: &Dir, name: &OsStr) -> io::Result<Option<(Is, Option<String>)>> {
    let m = match dir.symlink_metadata(name) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if m.is_symlink() {
        let to = dir.read_link_contents(name)?;
        let digest = fs::digest(to.as_os_str().as_encoded_bytes());
        return Ok(Some((Is { digest, link: true, run: false }, stamp(&m))));
    }
    if !m.is_file() {
        // A pipe, a device, a folder in its place: something, and not a file.
        return Ok(Some((Is { digest: String::new(), link: false, run: false }, None)));
    }
    let mut o = OpenOptions::new();
    o.read(true).follow(FollowSymlinks::No).nonblock(true);
    let mut f = dir.open_with(name, &o)?;
    let before = stamp(&f.metadata()?);
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        match f.read(&mut buf)? {
            0 => break,
            n => h.update(&buf[..n]),
        };
    }
    // What was written to meanwhile has none.
    let m = f.metadata()?;
    let digest = h.finalize().to_hex().to_string();
    let stamp = stamp(&m).filter(|s| before.as_ref() == Some(s));
    Ok(Some((Is { digest, link: false, run: runs(&m) }, stamp)))
}

#[cfg(unix)]
fn runs(m: &cap_std::fs::Metadata) -> bool {
    use cap_std::fs::MetadataExt;
    m.mode() & 0o111 != 0
}
#[cfg(not(unix))]
fn runs(_: &cap_std::fs::Metadata) -> bool {
    false
}

/// The time of its last change is the system's own: no program sets it.
#[cfg(unix)]
fn stamp(m: &cap_std::fs::Metadata) -> Option<String> {
    use cap_std::fs::MetadataExt;
    Some(format!("{}:{}:{}:{}", m.ino(), m.size(), m.ctime(), m.ctime_nsec()))
}
#[cfg(not(unix))]
fn stamp(_: &cap_std::fs::Metadata) -> Option<String> {
    None
}

pub(crate) fn parts(rel: &str) -> Option<(Vec<&str>, &str)> {
    let mut names: Vec<&str> = rel.split('/').collect();
    let last = names.pop()?;
    let plain = |n: &str| {
        !n.is_empty() && n != "." && n != ".." && !n.contains(['\0', '\n'])
        // On Windows it parts names.
        && !(cfg!(windows) && n.contains(['\\', ':']))
    };
    (names.iter().all(|n| plain(n)) && plain(last)).then_some((names, last))
}

/// The folder that holds a file of a folder, never through a link. `None`: there is none.
fn parent(top: &Dir, names: &[&str], make: bool) -> io::Result<Option<Dir>> {
    let mut d = top.try_clone()?;
    for n in names {
        d = match d.open_dir_nofollow(n) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound && make => {
                match d.create_dir(n) {
                    Err(e) if e.kind() != io::ErrorKind::AlreadyExists => return Err(e),
                    _ => {}
                }
                d.open_dir_nofollow(n)?
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            // A file or a link where a folder was.
            Err(_) if d.symlink_metadata(n).is_ok_and(|m| !m.is_dir()) => return Ok(None),
            Err(e) => return Err(e),
        };
    }
    Ok(Some(d))
}

/// A file of a folder as it is now.
pub fn now(folder: &Dir, rel: &str) -> Result<Option<Is>, CopyError> {
    Ok(look(folder, rel, None)?.map(|w| w.is))
}

fn look(folder: &Dir, rel: &str, was: Option<&Was>) -> Result<Option<Was>, CopyError> {
    let err = |e| CopyError::Io(rel.to_owned(), e);
    let Some((names, last)) = parts(rel) else { return Ok(None) };
    let Some(d) = parent(folder, &names, false).map_err(err)? else { return Ok(None) };
    if let Some(w) = was
        && w.stamp.is_some()
        && d.symlink_metadata(last).ok().and_then(|m| stamp(&m)) == w.stamp
    {
        return Ok(Some(w.clone()));
    }
    Ok(is_at(&d, OsStr::new(last)).map_err(err)?.map(|(is, stamp)| Was { is, stamp }))
}

/// Puts a file of one folder into another, as the system can do it best. `over`: in the place of one
/// that is there, whole or not at all; otherwise there must be none, by whatever name the system
/// takes for the same.
pub(crate) fn put(
    from: &Dir,
    name: &OsStr,
    to: &Dir,
    rel: &str,
    over: bool,
) -> Result<Was, CopyError> {
    let err = |e| CopyError::Io(rel.to_owned(), e);
    let Some((names, last)) = parts(rel) else {
        return Err(err(io::ErrorKind::InvalidInput.into()));
    };
    let Some(d) = parent(to, &names, true).map_err(err)? else {
        return Err(err(io::ErrorKind::NotADirectory.into()));
    };
    let m = from.symlink_metadata(name).map_err(err)?;
    let tmp = format!(".axiom-{}.tmp", fs::fresh());
    let new = if over { tmp.as_str() } else { last };
    if d.symlink_metadata(new).is_ok() {
        return Err(err(io::ErrorKind::AlreadyExists.into()));
    }
    let done = if m.is_symlink() {
        from.read_link_contents(name).and_then(|c| link(&d, &c, new))
    } else {
        content(from, name, &d, new)
    };
    let done = done.and_then(|()| if over { d.rename(new, &d, last) } else { Ok(()) });
    if let Err(e) = done {
        let _ = d.remove_file_or_symlink(new);
        return Err(err(e));
    }
    let Some((is, stamp)) = is_at(&d, OsStr::new(last)).map_err(err)? else {
        return Err(err(io::ErrorKind::NotFound.into()));
    };
    Ok(Was { is, stamp })
}

/// On Linux by the standard library, which ends when a file becomes shorter meanwhile.
#[cfg(target_os = "linux")]
pub(crate) fn content(from: &Dir, name: &OsStr, to: &Dir, new: &str) -> io::Result<()> {
    let mut o = OpenOptions::new();
    o.read(true).follow(FollowSymlinks::No).nonblock(true);
    let mut r = from.open_with(name, &o)?.into_std();
    let m = r.metadata()?;
    if !m.is_file() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut o = OpenOptions::new();
    o.write(true).create_new(true).follow(FollowSymlinks::No);
    let mut w = to.open_with(new, &o)?.into_std();
    io::copy(&mut r, &mut w)?;
    w.set_permissions(m.permissions())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn content(from: &Dir, name: &OsStr, to: &Dir, new: &str) -> io::Result<()> {
    from.copy(name, to, new).map(drop)
}

#[cfg(unix)]
pub(crate) fn link(d: &Dir, contents: &Path, name: &str) -> io::Result<()> {
    d.symlink_contents(contents, name)
}
#[cfg(not(unix))]
pub(crate) fn link(_: &Dir, _: &Path, _: &str) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "links are not copied on this system"))
}

/// Makes the copy, in a folder that is there and empty. What would be too much is refused before
/// anything is made. With how many files there are that cannot be copied.
pub fn make(from: &From<'_>, to: &Place, most: Most) -> Result<(Files, usize), CopyError> {
    let target = open(to)?;
    if target.entries().map_err(|e| CopyError::Io(to.to_string(), e))?.next().is_some() {
        return Err(CopyError::There(to.to_string()));
    }
    let source = open(from.folder)?;
    let git = source.symlink_metadata(".git").is_ok_and(|m| m.is_dir());
    let mut count = Count { most, files: 0, bytes: 0 };
    let walk = from.walk();
    let mut seen = BTreeSet::new();
    walk.each(&mut |d, n, rel| {
        seen.insert(rel.to_owned());
        count.add(d, n)
    })?;
    let more: Vec<&String> =
        from.tracked.iter().filter(|t| !seen.contains(*t) && from.allowed(t)).collect();
    for rel in &more {
        if let Some((d, n)) = at(&source, rel)? {
            count.add(&d, OsStr::new(n))?;
        }
    }
    if git {
        from.git(&source, &mut |d, n, _| count.add(d, n))?;
    }
    drop(seen);

    let walk = from.walk();
    let mut all = Files::new();
    let mut odd = 0;
    let mut done = |d: &Dir, n: &OsStr, rel: &str, again: bool| {
        if from.stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        match put(d, n, &target, rel, false) {
            Ok(w) => {
                all.insert(rel.to_owned(), w);
            }
            // It went away while this went on.
            Err(e)
                if e.kind() == Some(io::ErrorKind::NotFound) && d.symlink_metadata(n).is_err() => {}
            Err(e) if e.kind() == Some(io::ErrorKind::Unsupported) => odd += 1,
            // Copied already, by the name the system has for it.
            Err(e) if again && e.kind() == Some(io::ErrorKind::AlreadyExists) => {}
            Err(e) => return Err(e),
        }
        Ok(())
    };
    walk.each(&mut |d, n, rel| done(d, n, rel, false))?;
    for rel in more {
        if let Some((d, n)) = at(&source, rel)? {
            done(&d, OsStr::new(n), rel, true)?;
        }
    }
    if git {
        from.git(&source, &mut |d, n, rel| match put(d, n, &target, rel, false) {
            Err(e) if e.kind() == Some(io::ErrorKind::NotFound) => Ok(()),
            r => r.map(drop),
        })?;
    }
    Ok((all, odd + walk.odd.get()))
}

struct Count {
    most: Most,
    files: usize,
    bytes: u64,
}
impl Count {
    fn add(&mut self, d: &Dir, n: &OsStr) -> Result<(), CopyError> {
        self.files += 1;
        self.bytes += d.symlink_metadata(n).map_or(0, |m| m.len());
        if self.files > self.most.files || self.bytes > self.most.bytes {
            return Err(CopyError::TooMuch(self.most));
        }
        Ok(())
    }
}

/// A file by its name, in the folder that holds it.
pub(crate) fn at<'r>(top: &Dir, rel: &'r str) -> Result<Option<(Dir, &'r str)>, CopyError> {
    let Some((names, last)) = parts(rel) else { return Ok(None) };
    let d = parent(top, &names, false).map_err(|e| CopyError::Io(rel.to_owned(), e))?;
    Ok(d.filter(|d| d.symlink_metadata(last).is_ok_and(|m| m.is_file() || m.is_symlink()))
        .map(|d| (d, last)))
}

/// Of a repository's own folder, what only concerns it where it is.
const NOT_GIT: &[&str] = &["hooks", "worktrees", "modules"];

impl From<'_> {
    fn walk(&self) -> Walk<'_> {
        Walk::new(self.folder, true, self.may, self.stop)
    }

    fn allowed(&self, rel: &str) -> bool {
        parts(rel).is_some_and(|(names, last)| {
            !names.iter().chain([&last]).any(|n| n.eq_ignore_ascii_case(".git"))
        }) && self.folder.join(rel).is_ok_and(|p| (self.may)(&p))
    }

    /// What lets git say what there was: read only, as all of it is for agents (SI-11).
    pub(crate) fn git(&self, source: &Dir, f: Each<'_>) -> Result<(), CopyError> {
        let err = |e| CopyError::Io(".git".into(), e);
        let place =
            self.folder.join(".git").map_err(|_| err(io::ErrorKind::InvalidInput.into()))?;
        let dir = source.open_dir_nofollow(".git").map_err(err)?;
        let may = |p: &Place| {
            let first =
                p.as_path().strip_prefix(place.as_path()).ok().and_then(|r| r.iter().next());
            let first = first.and_then(OsStr::to_str).unwrap_or_default();
            !NOT_GIT.contains(&first) && !p.name().ends_with(".lock")
        };
        let w = Walk::new(&place, false, &may, self.stop);
        w.folder(&dir, "", 0, &mut Vec::new(), false, &mut |d, n, rel| {
            let m = d.symlink_metadata(n).map_err(err)?;
            // A link in there would lead who knows where.
            if m.is_symlink() { Ok(()) } else { f(d, n, &format!(".git/{rel}")) }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum What {
    New,
    Changed,
    Removed,
}

/// What an agent has changed in its copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub path: String,
    pub what: What,
    /// In the copy; absent if it is not there any more.
    pub is: Option<Is>,
    /// In the folder.
    pub there: Option<Is>,
    /// The folder's own has changed as well since it was copied.
    pub clash: bool,
    /// Why it cannot be brought in, whatever a person says.
    pub never: Option<&'static str>,
    /// It runs when someone builds, tests or installs.
    pub runs: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    pub all: Vec<Change>,
    /// These are all.
    pub whole: bool,
    /// What git ignores and only the copy has: files, or folders of them.
    pub ignored: usize,
    /// What has no name that can be written down, or is too deep.
    pub odd: usize,
}

/// What differs from what was copied, or from what the folder has had brought in since, and the
/// folder does not have.
pub fn changes(
    folder: &Place,
    copy: &Place,
    was: &Files,
    most: usize,
    stop: &AtomicBool,
) -> Result<Changes, CopyError> {
    let (f, c) = (open(folder)?, open(copy)?);
    let mut all: BTreeMap<String, (Option<Is>, Option<Is>)> = BTreeMap::new();
    let d = differing(&c, copy, was, stop, &mut |rel, is| {
        let there = now(&f, rel)?;
        if has(there.as_ref(), is.as_ref()) {
            return Ok(true);
        }
        if all.len() >= most {
            return Ok(false);
        }
        all.insert(rel.to_owned(), (is, there));
        Ok(true)
    })?;
    let mut out = Vec::new();
    for (path, (is, there)) in all {
        let was = was.get(&path).map(|w| &w.is);
        let what = match (was, &is) {
            (_, None) => What::Removed,
            (None, _) => What::New,
            _ => What::Changed,
        };
        let mut never = never(&path, is.as_ref(), there.as_ref());
        if is.is_none() && never.is_none() && !in_folders(&c, &path) {
            never = Some("a folder it was in is something else in the copy");
        }
        let runs = path.rsplit('/').next().is_some_and(paths::review_name)
            || is.as_ref().is_some_and(|i| i.run && !was.is_some_and(|w| w.run));
        let clash = !alike(there.as_ref(), was);
        out.push(Change { clash, path, what, is, there, never, runs });
    }
    Ok(Changes { all: out, whole: d.whole, ignored: d.ignored, odd: d.odd })
}

/// What came of looking through a copy.
pub(crate) struct Differing {
    pub whole: bool,
    pub ignored: usize,
    pub odd: usize,
}

/// Takes a file that is not what it was, or has enough.
pub(crate) type Found<'a> = &'a mut dyn FnMut(&str, Option<Is>) -> Result<bool, CopyError>;

/// Each file of a copy that is not what was copied, as it is now, until `found` has enough.
pub(crate) fn differing(
    c: &Dir,
    copy: &Place,
    was: &Files,
    stop: &AtomicBool,
    found: Found<'_>,
) -> Result<Differing, CopyError> {
    let mut whole = true;
    for (rel, w) in was {
        if stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        let is = look(c, rel, Some(w))?.map(|n| n.is);
        if is.as_ref() != Some(&w.is) && !found(rel, is)? {
            whole = false;
            break;
        }
    }
    let may = |_: &Place| true;
    let walk = Walk { known: Some(was), ..Walk::new(copy, true, &may, stop) };
    walk.each(&mut |d, n, rel| {
        if was.contains_key(rel) || !whole {
            return Ok(());
        }
        let is = is_at(d, n).map_err(|e| CopyError::Io(rel.to_owned(), e))?;
        whole = found(rel, is.map(|i| i.0))?;
        Ok(())
    })?;
    Ok(Differing { whole, ignored: walk.ignored.get(), odd: walk.odd.get() })
}

/// Nothing on the way to a file is anything but a folder, or not there.
fn in_folders(top: &Dir, rel: &str) -> bool {
    let Some((names, _)) = parts(rel) else { return false };
    let mut at = String::new();
    for n in names {
        if !at.is_empty() {
            at.push('/');
        }
        at.push_str(n);
        match top.symlink_metadata(&at) {
            Ok(m) if m.is_dir() => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => return true,
            _ => return false,
        }
    }
    true
}

/// The folder has what the copy has. That it can be run there too is no change.
fn has(there: Option<&Is>, is: Option<&Is>) -> bool {
    alike(there, is)
        && match (there, is) {
            (Some(t), Some(i)) => t.run || !i.run,
            _ => true,
        }
}

/// What is never brought in, and why.
fn never(path: &str, is: Option<&Is>, there: Option<&Is>) -> Option<&'static str> {
    if let Some(why) = path.split('/').find_map(paths::protected_name) {
        return Some(why);
    }
    if is.is_some_and(|i| i.link) || there.is_some_and(|i| i.link) {
        return Some("it is a link");
    }
    if is.is_some_and(|i| i.digest.is_empty()) || there.is_some_and(|i| i.digest.is_empty()) {
        return Some("it is not a plain file");
    }
    None
}

/// What bringing a change in would do to the folder as it is now, if it can be read line by line.
pub fn diff(folder: &Place, copy: &Place, c: &Change) -> Option<String> {
    let read = |p: &Place| -> Option<Option<String>> {
        let place = p.join(&c.path).ok()?;
        match fs::find(std::slice::from_ref(p), &place) {
            // Not what a link leads to: that is another file.
            Ok(s) if s.place == place && s.kind == Kind::File => {
                let t = String::from_utf8(s.read(SHOWN).ok()?.bytes).ok()?;
                (!t.contains('\0')).then_some(Some(t))
            }
            Ok(s) if s.kind == Kind::Missing => Some(None),
            Err(FsError::Missing(_)) => Some(None),
            _ => None,
        }
    };
    let (before, after) = (read(folder)?, read(copy)?);
    Some(text::diff(&c.path, before.as_deref(), after.as_deref()))
}

/// Why a file was not brought in or put back.
pub type Why = String;

fn spot(folder: &Place, rel: &str, make: bool) -> Result<fs::Spot, Why> {
    let Some((names, _)) = parts(rel) else {
        return Err("it is not a name of a file".into());
    };
    let place = folder.join(rel).map_err(|e| e.to_string())?;
    if make {
        let top = open(folder).map_err(|e| e.to_string())?;
        if parent(&top, &names, true).map_err(|e| e.to_string())?.is_none() {
            return Err("there is something else where its folder would be".into());
        }
    }
    let s = match fs::find(std::slice::from_ref(folder), &place) {
        Err(FsError::Missing(_)) => return Err("its folder is not there".into()),
        s => s.map_err(|e| e.to_string())?,
    };
    if s.place != place {
        return Err("a link leads to it".into());
    }
    Ok(s)
}

/// A change as a person was shown it.
#[derive(Debug, Clone, Copy)]
pub struct Seen<'a> {
    /// In the copy; absent if it was removed there.
    pub digest: Option<&'a str>,
    pub run: bool,
    /// In the folder; absent if it had none.
    pub there: Option<&'a str>,
}

/// What a change would do, and what is there before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub is: Option<Is>,
    pub there: Option<Is>,
    pub bytes: Option<Vec<u8>>,
    /// The permissions of what is there, where there are such.
    pub mode: Option<u32>,
}

/// Whether a change may be brought in, as it was seen: only as long as the folder's own is what was
/// copied, and both are what was shown. Nothing is touched.
pub fn plan(
    folder: &Place,
    copy: &Place,
    path: &str,
    was: Option<&Was>,
    seen: Seen<'_>,
) -> Result<Plan, Why> {
    if parts(path).is_none() {
        return Err("it is not a name of a file".into());
    }
    let words = |e: CopyError| e.to_string();
    let (f, c) = (open(folder).map_err(words)?, open(copy).map_err(words)?);
    let (is, there) = (now(&c, path).map_err(words)?, now(&f, path).map_err(words)?);
    let digest = |i: &Option<Is>| i.as_ref().map(|i| i.digest.clone());
    if digest(&is).as_deref() != seen.digest || is.as_ref().is_some_and(|i| i.run != seen.run) {
        return Err("it is not what was shown any more".into());
    }
    if let Some(why) = never(path, is.as_ref(), there.as_ref()) {
        return Err(why.into());
    }
    if has(there.as_ref(), is.as_ref()) {
        return Err("the folder has it already".into());
    }
    if !alike(there.as_ref(), was.map(|w| &w.is)) {
        return Err("the folder's own has changed since it was copied".into());
    }
    if digest(&there).as_deref() != seen.there {
        return Err("the folder's own is not what was shown any more".into());
    }
    if is.is_none() && !in_folders(&c, path) {
        return Err("a folder it was in is something else in the copy".into());
    }
    let (mut bytes, mut mode) = (None, None);
    if let Some(t) = &there {
        let s = spot(folder, path, false)?;
        mode = s.mode().map_err(|e| e.to_string())?;
        let content = s.read(LARGEST).map_err(|e| e.to_string())?;
        if content.digest != t.digest {
            return Err("the folder's own is being changed".into());
        }
        bytes = Some(content.bytes);
    }
    Ok(Plan { is, there, bytes, mode })
}

/// Does what was planned, if both are still what they were.
pub fn bring(
    folder: &Place,
    copy: &Place,
    path: &str,
    is: Option<&Is>,
    there: Option<&Is>,
) -> Result<(), Why> {
    let words = |e: CopyError| e.to_string();
    let from = match is {
        None => None,
        Some(i) => {
            let c = spot(copy, path, false)?.read(LARGEST).map_err(|e| e.to_string())?;
            if c.digest != i.digest {
                return Err("it is not what was shown any more".into());
            }
            Some((c.bytes, i.run))
        }
    };
    let f = open(folder).map_err(words)?;
    if !alike(now(&f, path).map_err(words)?.as_ref(), there) {
        return Err("the folder's own is being changed".into());
    }
    match from {
        None => spot(folder, path, false)?.remove().map_err(|e| e.to_string()),
        Some((bytes, run)) => {
            let mode = if run { fs::Mode::Run } else { fs::Mode::Keep };
            spot(folder, path, true)?.write_as(&bytes, mode).map_err(|e| e.to_string())
        }
    }
}

/// Puts back what was there before, if nothing has changed it since. It may have been put back.
pub fn back(
    folder: &Place,
    path: &str,
    before: Option<(&[u8], Option<u32>)>,
    after: Option<&Is>,
) -> Result<(), Why> {
    let f = open(folder).map_err(|e| e.to_string())?;
    let there = now(&f, path).map_err(|e| e.to_string())?;
    if !alike(there.as_ref(), after) {
        return Err("it has changed since".into());
    }
    match before {
        Some((bytes, mode)) => {
            let mode = mode.map_or(fs::Mode::Keep, fs::Mode::Exactly);
            spot(folder, path, true)?.write_as(bytes, mode).map_err(|e| e.to_string())
        }
        None if there.is_none() => Ok(()),
        None => spot(folder, path, false)?.remove().map_err(|e| e.to_string()),
    }
}

/// What is to be taken over into a copy: each as the folder has it, absent if it has none any more.
pub type Taking = BTreeMap<String, Option<Is>>;

/// What bringing a copy up to date is to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Renewal {
    pub taking: Taking,
    /// Changed in both, with what the folder has.
    pub clashes: Taking,
    /// What cannot be looked at, with why.
    pub left: Vec<(String, Why)>,
}

/// What has changed in the folder since it was copied. Where the copy has it too, it counts as
/// copied from here on.
pub fn news(
    from: &From<'_>,
    copy: &Place,
    was: &mut Files,
    most: Most,
) -> Result<Renewal, CopyError> {
    let (f, c) = (open(from.folder)?, open(copy)?);
    let mut names: BTreeSet<String> = was.keys().cloned().collect();
    from.walk().each(&mut |_, _, rel| {
        names.insert(rel.to_owned());
        if names.len() > most.files { Err(CopyError::TooMuch(most)) } else { Ok(()) }
    })?;
    names.extend(from.tracked.iter().filter(|t| from.allowed(t)).cloned());
    let mut out = Renewal::default();
    for rel in names {
        if from.stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        let known = was.get(&rel).cloned();
        let both = || -> Result<_, CopyError> {
            // What it may not have counts as gone.
            let there = if from.allowed(&rel) { now(&f, &rel)? } else { None };
            Ok((there, look(&c, &rel, known.as_ref())?))
        };
        let (there, here) = match both() {
            Ok(b) => b,
            Err(e) => {
                out.left.push((rel, e.to_string()));
                continue;
            }
        };
        let is = |w: &Option<Was>| w.as_ref().map(|w| w.is.clone());
        if there == is(&known) {
            continue;
        }
        if is(&here) == there {
            match here {
                Some(w) => was.insert(rel, w),
                None => was.remove(&rel),
            };
        } else if is(&here) == is(&known) {
            out.taking.insert(rel, there);
        } else {
            out.clashes.insert(rel, there);
        }
    }
    Ok(out)
}

/// Takes one over, if the folder still has it so.
pub fn take(
    folder: &Place,
    copy: &Place,
    rel: &str,
    is: Option<&Is>,
) -> Result<Option<Was>, CopyError> {
    let (f, c) = (open(folder)?, open(copy)?);
    if is.is_none() {
        if let Some((d, n)) = at(&c, rel)? {
            d.remove_file_or_symlink(n).map_err(|e| CopyError::Io(rel.to_owned(), e))?;
        }
        return Ok(None);
    }
    match at(&f, rel)? {
        Some((d, n)) => put(&d, OsStr::new(n), &c, rel, true).map(Some),
        None => Err(CopyError::Io(rel.to_owned(), io::ErrorKind::NotFound.into())),
    }
}

/// Puts the folder's side of a clash beside the copy, for the agent to look at.
pub fn beside(folder: &Place, beside: &Place, rel: &str) -> Result<(), CopyError> {
    match at(&open(folder)?, rel)? {
        Some((d, n)) => put(&d, OsStr::new(n), &open(beside)?, rel, true).map(drop),
        None => Ok(()),
    }
}

/// Opens a folder, for [`now`].
pub fn folder(p: &Place) -> Result<Dir, CopyError> {
    open(p)
}

#[cfg(all(test, unix))]
#[path = "copy_tests.rs"]
mod tests;
