//! A folder's files for one agent of its own (decision 0014): made, compared with the folder, and
//! brought back into it, all by handle (SI-9). Links are copied as links and never followed. Nothing
//! is merged: what has changed in both is said to be a clash.

use std::{
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

fn open(p: &Place) -> Result<Dir, CopyError> {
    Dir::open_ambient_dir(p.as_path(), ambient_authority())
        .map_err(|e| CopyError::Io(p.to_string(), e))
}

type Each<'a> = &'a mut dyn FnMut(&Dir, &OsStr, &str) -> Result<(), CopyError>;

struct Walk<'a> {
    base: &'a Place,
    /// What git ignores is left out.
    ignoring: bool,
    may: May<'a>,
    stop: &'a AtomicBool,
}

impl Walk<'_> {
    fn each(&self, f: Each<'_>) -> Result<(), CopyError> {
        self.folder(&open(self.base)?, "", 0, &mut Vec::new(), f)
    }

    fn folder(
        &self,
        dir: &Dir,
        rel: &str,
        depth: usize,
        rules: &mut Vec<Gitignore>,
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
            // What has no name that can be written down is left where it is.
            let Some(n) = name.to_str() else { continue };
            if n.eq_ignore_ascii_case(".git") && self.ignoring {
                continue;
            }
            let path = here.join(n);
            if ignored(rules, &path, kind.is_dir()) {
                continue;
            }
            let Ok(place) = Place::new(&path) else { continue };
            if !(self.may)(&place) {
                continue;
            }
            let below = if rel.is_empty() { n.to_owned() } else { format!("{rel}/{n}") };
            if kind.is_dir() {
                // If it became a link meanwhile, it is none of the folder's.
                if depth < DEEPEST
                    && let Ok(d) = dir.open_dir_nofollow(&name)
                {
                    self.folder(&d, &below, depth + 1, rules, f)?;
                }
            } else if kind.is_file() || kind.is_symlink() {
                f(dir, &name, &below)?;
            }
        }
        rules.truncate(had);
        Ok(())
    }
}

/// The file that a name of an open folder is, without following a link.
fn is_at(dir: &Dir, name: &OsStr) -> io::Result<Option<(Is, Option<String>)>> {
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
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        match f.read(&mut buf)? {
            0 => break,
            n => h.update(&buf[..n]),
        };
    }
    // Of what was read, and after: what is written meanwhile has another.
    let m = f.metadata()?;
    let digest = h.finalize().to_hex().to_string();
    Ok(Some((Is { digest, link: false, run: runs(&m) }, stamp(&m))))
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

fn parts(rel: &str) -> Option<(Vec<&str>, &str)> {
    let mut names: Vec<&str> = rel.split('/').collect();
    let last = names.pop()?;
    let plain = |n: &str| !n.is_empty() && n != "." && n != ".." && !n.contains(['\\', '\0']);
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

/// Puts a file of one folder into another, where there is none, as the system can do it best.
fn put(from: &Dir, name: &OsStr, to: &Dir, rel: &str) -> Result<Was, CopyError> {
    let err = |e| CopyError::Io(rel.to_owned(), e);
    let Some((names, last)) = parts(rel) else {
        return Err(err(io::ErrorKind::InvalidInput.into()));
    };
    let Some(d) = parent(to, &names, true).map_err(err)? else {
        return Err(err(io::ErrorKind::NotADirectory.into()));
    };
    let m = from.symlink_metadata(name).map_err(err)?;
    if m.is_symlink() {
        let contents = from.read_link_contents(name).map_err(err)?;
        link(&d, &contents, last).map_err(err)?;
    } else {
        from.copy(name, &d, last).map_err(err)?;
    }
    let Some((is, stamp)) = is_at(&d, OsStr::new(last)).map_err(err)? else {
        return Err(err(io::ErrorKind::NotFound.into()));
    };
    Ok(Was { is, stamp })
}

#[cfg(unix)]
fn link(d: &Dir, contents: &Path, name: &str) -> io::Result<()> {
    d.symlink_contents(contents, name)
}
#[cfg(not(unix))]
fn link(_: &Dir, _: &Path, _: &str) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "links are not copied on this system"))
}

/// Makes the copy, in a folder that is there and empty. What would be too much is refused before
/// anything is made.
pub fn make(from: &From<'_>, to: &Place, most: Most) -> Result<Files, CopyError> {
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

    let mut all = Files::new();
    let mut done = |d: &Dir, n: &OsStr, rel: &str| {
        if from.stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        match put(d, n, &target, rel) {
            Ok(w) => all.insert(rel.to_owned(), w),
            // It went away while this went on.
            Err(CopyError::Io(_, e)) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) if cfg!(not(unix)) && unsupported(&e) => None,
            Err(e) => return Err(e),
        };
        Ok(())
    };
    walk.each(&mut done)?;
    for rel in more {
        if let Some((d, n)) = at(&source, rel)? {
            done(&d, OsStr::new(n), rel)?;
        }
    }
    if git {
        from.git(&source, &mut |d, n, rel| match put(d, n, &target, rel) {
            Err(CopyError::Io(_, e)) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r.map(drop),
        })?;
    }
    Ok(all)
}

fn unsupported(e: &CopyError) -> bool {
    matches!(e, CopyError::Io(_, e) if e.kind() == io::ErrorKind::Unsupported)
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
fn at<'r>(top: &Dir, rel: &'r str) -> Result<Option<(Dir, &'r str)>, CopyError> {
    let Some((names, last)) = parts(rel) else { return Ok(None) };
    let d = parent(top, &names, false).map_err(|e| CopyError::Io(rel.to_owned(), e))?;
    Ok(d.filter(|d| d.symlink_metadata(last).is_ok_and(|m| m.is_file() || m.is_symlink()))
        .map(|d| (d, last)))
}

/// Of a repository's own folder, what only concerns it where it is.
const NOT_GIT: &[&str] = &["hooks", "worktrees", "modules"];

impl From<'_> {
    fn walk(&self) -> Walk<'_> {
        Walk { base: self.folder, ignoring: true, may: self.may, stop: self.stop }
    }

    fn allowed(&self, rel: &str) -> bool {
        parts(rel).is_some_and(|(names, last)| {
            !names.iter().chain([&last]).any(|n| n.eq_ignore_ascii_case(".git"))
        }) && self.folder.join(rel).is_ok_and(|p| (self.may)(&p))
    }

    /// What lets git say what there was: read only, as all of it is for agents (SI-11).
    fn git(&self, source: &Dir, f: Each<'_>) -> Result<(), CopyError> {
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
        let w = Walk { base: &place, ignoring: false, may: &may, stop: self.stop };
        w.folder(&dir, "", 0, &mut Vec::new(), &mut |d, n, rel| {
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
    /// The folder's own has changed as well since it was copied.
    pub clash: bool,
    /// Why it cannot be brought in, whatever a person says.
    pub never: Option<&'static str>,
    /// It runs when someone builds, tests or installs.
    pub runs: bool,
}

/// Those that differ from what was copied, or from what the folder has had brought in since.
pub fn changes(
    folder: &Place,
    copy: &Place,
    was: &Files,
    most: usize,
    stop: &AtomicBool,
) -> Result<(Vec<Change>, bool), CopyError> {
    let (f, c) = (open(folder)?, open(copy)?);
    let mut all: BTreeMap<String, Option<Is>> = BTreeMap::new();
    let mut whole = true;
    for (rel, w) in was {
        if stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        let is = look(&c, rel, Some(w))?.map(|n| n.is);
        if is.as_ref() != Some(&w.is) {
            if all.len() >= most {
                whole = false;
                break;
            }
            all.insert(rel.clone(), is);
        }
    }
    let may = |_: &Place| true;
    let walk = Walk { base: copy, ignoring: true, may: &may, stop };
    walk.each(&mut |d, n, rel| {
        if was.contains_key(rel) {
            return Ok(());
        }
        if all.len() >= most {
            whole = false;
            return Ok(());
        }
        let is = is_at(d, n).map_err(|e| CopyError::Io(rel.to_owned(), e))?;
        all.insert(rel.to_owned(), is.map(|i| i.0));
        Ok(())
    })?;
    let mut out = Vec::new();
    for (path, is) in all {
        let was = was.get(&path).map(|w| &w.is);
        let there = now(&f, &path)?;
        if has(there.as_ref(), is.as_ref()) {
            // Nothing to bring in.
            continue;
        }
        let what = match (was, &is) {
            (_, None) => What::Removed,
            (None, _) => What::New,
            _ => What::Changed,
        };
        let never = never(&path, is.as_ref(), there.as_ref());
        let runs = path.rsplit('/').next().is_some_and(paths::review_name)
            || is.as_ref().is_some_and(|i| i.run && !was.is_some_and(|w| w.run));
        out.push(Change { clash: there.as_ref() != was, path, what, is, never, runs });
    }
    Ok((out, whole))
}

/// The folder has what the copy has. That it can be run there too is no change.
fn has(there: Option<&Is>, is: Option<&Is>) -> bool {
    match (there, is) {
        (None, None) => true,
        (Some(t), Some(i)) => t.digest == i.digest && t.link == i.link && (t.run || !i.run),
        _ => false,
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

/// What was there before a file was brought in, so that it can be put back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Before {
    pub path: String,
    /// Absent: there was none.
    pub bytes: Option<Vec<u8>>,
    /// What the folder has now; absent: none.
    pub after: Option<Is>,
    pub run: bool,
}

/// Why a file was not brought in or put back.
pub type Why = String;

fn spot(folder: &Place, rel: &str, make: bool) -> Result<fs::Spot, Why> {
    if parts(rel).is_none() {
        return Err("it is not a name of a file".into());
    }
    let place = folder.join(rel).map_err(|e| e.to_string())?;
    let roots = std::slice::from_ref(folder);
    if make {
        let (names, _) = parts(rel).ok_or("it is not a name of a file")?;
        let top = open(folder).map_err(|e| e.to_string())?;
        if parent(&top, &names, true).map_err(|e| e.to_string())?.is_none() {
            return Err("there is something else where its folder would be".into());
        }
    }
    let s = match fs::find(roots, &place) {
        Err(FsError::Missing(_)) => return Err("its folder is not there".into()),
        s => s.map_err(|e| e.to_string())?,
    };
    if s.place != place {
        return Err("a link leads to it".into());
    }
    Ok(s)
}

/// The bytes of a folder's file, to be kept; `None`: there is none.
pub fn peek(folder: &Place, path: &str) -> Result<Option<Vec<u8>>, Why> {
    let f = open(folder).map_err(|e| e.to_string())?;
    match now(&f, path).map_err(|e| e.to_string())? {
        None => Ok(None),
        Some(_) => spot(folder, path, false)?
            .read(LARGEST)
            .map(|c| Some(c.bytes))
            .map_err(|e| e.to_string()),
    }
}

/// Brings one change in, as a person has seen it: `seen` is what the copy had then, absent if it
/// had removed it. The folder's own is only touched as long as it is what was copied.
pub fn bring(
    folder: &Place,
    copy: &Place,
    path: &str,
    was: Option<&Was>,
    seen: Option<&str>,
) -> Result<Before, Why> {
    if parts(path).is_none() {
        return Err("it is not a name of a file".into());
    }
    let words = |e: CopyError| e.to_string();
    let (f, c) = (open(folder).map_err(words)?, open(copy).map_err(words)?);
    let (is, there) = (now(&c, path).map_err(words)?, now(&f, path).map_err(words)?);
    if is.as_ref().map(|i| i.digest.as_str()) != seen {
        return Err("it is not what was shown any more".into());
    }
    if let Some(why) = never(path, is.as_ref(), there.as_ref()) {
        return Err(why.into());
    }
    if has(there.as_ref(), is.as_ref()) {
        return Err("the folder has it already".into());
    }
    let was = was.map(|w| &w.is);
    if there.as_ref().map(|t| &t.digest) != was.map(|w| &w.digest) {
        return Err("the folder's own has changed since it was copied".into());
    }
    let before = match &there {
        None => None,
        Some(_) => Some(spot(folder, path, false)?.read(LARGEST).map_err(|e| e.to_string())?.bytes),
    };
    let run = there.as_ref().is_some_and(|t| t.run);
    match &is {
        None => spot(folder, path, false)?.remove().map_err(|e| e.to_string())?,
        Some(i) => {
            let from = spot(copy, path, false)?.read(LARGEST).map_err(|e| e.to_string())?;
            if from.digest != i.digest {
                return Err("it is not what was shown any more".into());
            }
            spot(folder, path, true)?.write_as(&from.bytes, i.run).map_err(|e| e.to_string())?;
        }
    }
    Ok(Before { path: path.to_owned(), bytes: before, after: is, run })
}

/// Puts back what was there before, if nothing else has changed it since.
pub fn back(folder: &Place, b: &Before) -> Result<(), Why> {
    let f = open(folder).map_err(|e| e.to_string())?;
    let there = now(&f, &b.path).map_err(|e| e.to_string())?;
    if there.as_ref().map(|t| &t.digest) != b.after.as_ref().map(|t| &t.digest) {
        return Err("it has changed since".into());
    }
    match &b.bytes {
        Some(bytes) => {
            spot(folder, &b.path, true)?.write_as(bytes, b.run).map_err(|e| e.to_string())
        }
        None if there.is_none() => Ok(()),
        None => spot(folder, &b.path, false)?.remove().map_err(|e| e.to_string()),
    }
}

/// What bringing a copy up to date came to.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Renewed {
    /// Taken over from the folder.
    pub taken: Vec<String>,
    /// Changed in both: the folder's is beside the copy now, for the agent to look at.
    pub clashes: Vec<String>,
}

/// Takes over what has changed in the folder and not in the copy. Where both have, the copy's stays
/// and the folder's is put into `beside`; from then on it counts as what was copied, and bringing
/// the file in shows what that would undo.
pub fn renew(
    from: &From<'_>,
    copy: &Place,
    was: &mut Files,
    beside: &Place,
    most: Most,
) -> Result<Renewed, CopyError> {
    let (f, c, b) = (open(from.folder)?, open(copy)?, open(beside)?);
    let mut names: BTreeSet<String> = was.keys().cloned().collect();
    from.walk().each(&mut |_, _, rel| {
        names.insert(rel.to_owned());
        if names.len() > most.files { Err(CopyError::TooMuch(most)) } else { Ok(()) }
    })?;
    names.extend(from.tracked.iter().filter(|t| from.allowed(t)).cloned());
    let mut out = Renewed::default();
    for rel in names {
        if from.stop.load(Ordering::Relaxed) {
            return Err(CopyError::Stopped);
        }
        let known = was.get(&rel).cloned();
        let there = if from.allowed(&rel) { now(&f, &rel)? } else { None };
        if there.as_ref() == known.as_ref().map(|w| &w.is) {
            continue;
        }
        let here = look(&c, &rel, known.as_ref())?;
        let err = |e| CopyError::Io(rel.clone(), e);
        let set = |was: &mut Files, w: Option<Was>| match w {
            Some(w) => was.insert(rel.clone(), w),
            None => was.remove(&rel),
        };
        if here.as_ref().map(|w| &w.is) == there.as_ref() {
            set(was, here);
            continue;
        }
        let untouched = here.as_ref().map(|w| &w.is) == known.as_ref().map(|w| &w.is);
        // Not what it may not have, which counts as gone.
        let at_folder = if there.is_some() { at(&f, &rel)? } else { None };
        if untouched {
            if here.is_some()
                && let Some((d, n)) = at(&c, &rel)?
            {
                d.remove_file_or_symlink(n).map_err(err)?;
            }
            let w = match at_folder {
                Some((d, n)) => Some(put(&d, OsStr::new(n), &c, &rel)?),
                None => None,
            };
            set(was, w);
            out.taken.push(rel);
        } else {
            if let Some((d, n)) = at(&b, &rel)? {
                d.remove_file_or_symlink(n).map_err(err)?;
            }
            if let Some((d, n)) = at_folder {
                put(&d, OsStr::new(n), &b, &rel)?;
            }
            // No stamp: the copy's own is another file.
            set(was, there.map(|is| Was { is, stamp: None }));
            out.clashes.push(rel);
        }
    }
    Ok(out)
}

/// Opens a folder, for [`now`].
pub fn folder(p: &Place) -> Result<Dir, CopyError> {
    open(p)
}

#[cfg(all(test, unix))]
#[path = "copy_tests.rs"]
mod tests;
