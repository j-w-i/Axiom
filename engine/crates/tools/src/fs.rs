//! Files by handle (SI-9). A path is walked from an open folder, one name at a time, never through a
//! link the system would follow by itself; what it leads to is judged by its real place, and used
//! through the handle that was judged. Nothing that moves in between can be reached instead.

use std::{
    ffi::OsString,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
};

use axiom_policy::Place;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, File, OpenOptions},
};

#[derive(Debug, thiserror::Error)]
pub enum FsError {
    #[error("{0} is not inside a folder this agent may use")]
    Outside(String),
    #[error("{0} does not exist")]
    Missing(String),
    #[error("{0} is not a folder")]
    NotFolder(String),
    #[error("{0} is not a plain file")]
    NotFile(String),
    #[error("{0} goes round through too many links")]
    Links(String),
    #[error("{0} is too large ({1} bytes)")]
    Large(String, u64),
    #[error("{0}: {1}")]
    Io(String, io::Error),
}

/// A name in an open folder, and where that really is.
#[derive(Debug)]
pub struct Spot {
    dir: Dir,
    name: OsString,
    pub place: Place,
    /// Nothing is there, or not a link: links have been walked.
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Missing,
    File,
    Folder,
    Other,
}

const MOST_LINKS: usize = 40;

/// Opens the root that holds `place` and walks to it. `roots` are from the session's record and
/// already real (SI-10). Folders missing on the way are an error; a missing last name is not.
pub fn find(roots: &[Place], place: &Place) -> Result<Spot, FsError> {
    let shown = place.to_string();
    let root =
        roots.iter().find(|r| r.covers(place)).ok_or_else(|| FsError::Outside(shown.clone()))?;
    let top = open_root(root)?;
    let rel = place.as_path().strip_prefix(root.as_path()).unwrap_or(Path::new(""));
    walk(root, top, rel, &shown)
}

fn open_root(root: &Place) -> Result<Dir, FsError> {
    Dir::open_ambient_dir(root.as_path(), ambient_authority()).map_err(|e| of(e, root.as_str()))
}

fn of(e: io::Error, shown: &str) -> FsError {
    match e.kind() {
        io::ErrorKind::NotFound => FsError::Missing(shown.into()),
        io::ErrorKind::NotADirectory => FsError::NotFolder(shown.into()),
        _ => FsError::Io(shown.into(), e),
    }
}

fn walk(root: &Place, top: Dir, rel: &Path, shown: &str) -> Result<Spot, FsError> {
    // What is still to be walked, the next one last.
    let mut todo: Vec<OsString> = Vec::new();
    push(&mut todo, rel);
    // The folders walked so far, below the root, with the name each has in the one before.
    let mut stack: Vec<(OsString, Dir)> = Vec::new();
    let mut links = 0;
    while let Some(name) = todo.pop() {
        if name == ".." {
            // Above the root is outside, whatever it would come back to.
            if stack.pop().is_none() {
                return Err(FsError::Outside(shown.into()));
            }
            continue;
        }
        let here = stack.last().map_or(&top, |s| &s.1);
        let meta = match here.symlink_metadata(&name) {
            Ok(m) => m,
            Err(e) if e.kind() == io::ErrorKind::NotFound && todo.is_empty() => {
                return spot(root, &top, &stack, name, Kind::Missing, shown);
            }
            Err(e) => return Err(of(e, shown)),
        };
        if meta.is_symlink() {
            links += 1;
            if links > MOST_LINKS {
                return Err(FsError::Links(shown.into()));
            }
            let to = here.read_link_contents(&name).map_err(|e| of(e, shown))?;
            if to.is_absolute() {
                // As the system would: from the top. Below the root it is walked like any other.
                let Ok(inside) = to.strip_prefix(root.as_path()) else {
                    return Err(FsError::Outside(shown.into()));
                };
                stack.clear();
                push(&mut todo, inside);
            } else {
                push(&mut todo, &to);
            }
            if todo.len() > 1024 {
                return Err(FsError::Links(shown.into()));
            }
            if todo.is_empty() {
                // A link to the root itself.
                break;
            }
        } else if meta.is_dir() {
            if todo.is_empty() {
                return spot(root, &top, &stack, name, Kind::Folder, shown);
            }
            // If it became a link meanwhile, this is an error.
            let d = here.open_dir_nofollow(&name).map_err(|e| of(e, shown))?;
            stack.push((name, d));
        } else {
            if !todo.is_empty() {
                return Err(FsError::NotFolder(shown.into()));
            }
            let kind = if meta.is_file() { Kind::File } else { Kind::Other };
            return spot(root, &top, &stack, name, kind, shown);
        }
    }
    // Ended on a folder by going up, or on the root itself.
    let place = at(root, &stack, None, shown)?;
    let dir = match stack.pop() {
        Some((_, d)) => d,
        None => top,
    };
    Ok(Spot { dir, name: OsString::from("."), place, kind: Kind::Folder })
}

fn push(todo: &mut Vec<OsString>, p: &Path) {
    let names: Vec<OsString> = p
        .components()
        .filter_map(|c| match c {
            Component::Normal(n) => Some(n.to_owned()),
            Component::ParentDir => Some(OsString::from("..")),
            _ => None,
        })
        .collect();
    todo.extend(names.into_iter().rev());
}

fn at(
    root: &Place,
    stack: &[(OsString, Dir)],
    name: Option<&OsString>,
    shown: &str,
) -> Result<Place, FsError> {
    let mut p: PathBuf = root.as_path().to_path_buf();
    p.extend(stack.iter().map(|s| &s.0));
    p.extend(name);
    Place::new(p).map_err(|_| FsError::Outside(shown.into()))
}

fn spot(
    root: &Place,
    top: &Dir,
    stack: &[(OsString, Dir)],
    name: OsString,
    kind: Kind,
    shown: &str,
) -> Result<Spot, FsError> {
    let place = at(root, stack, Some(&name), shown)?;
    let dir = stack.last().map_or(top, |s| &s.1).try_clone().map_err(|e| of(e, shown))?;
    Ok(Spot { dir, name, place, kind })
}

/// A file as it was read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    pub bytes: Vec<u8>,
    pub digest: String,
}

pub fn digest(b: &[u8]) -> String {
    blake3::hash(b).to_hex().to_string()
}

impl Spot {
    fn shown(&self) -> &str {
        self.place.as_str()
    }

    fn err(&self, e: io::Error) -> FsError {
        of(e, self.shown())
    }

    /// An open plain file. A pipe or a device that took its place is refused, without waiting.
    fn open(&self) -> Result<(File, u64), FsError> {
        let mut o = OpenOptions::new();
        o.read(true).follow(FollowSymlinks::No).nonblock(true);
        let f = self.dir.open_with(&self.name, &o).map_err(|e| self.err(e))?;
        let m = f.metadata().map_err(|e| self.err(e))?;
        if !m.is_file() {
            return Err(FsError::NotFile(self.shown().into()));
        }
        Ok((f, m.len()))
    }

    /// The whole file, if it has at most `most` bytes.
    pub fn read(&self, most: u64) -> Result<Content, FsError> {
        match self.kind {
            Kind::File => {}
            Kind::Missing => return Err(FsError::Missing(self.shown().into())),
            _ => return Err(FsError::NotFile(self.shown().into())),
        }
        let (f, len) = self.open()?;
        if len > most {
            return Err(FsError::Large(self.shown().into(), len));
        }
        let mut bytes = Vec::new();
        f.take(most + 1).read_to_end(&mut bytes).map_err(|e| self.err(e))?;
        if bytes.len() as u64 > most {
            return Err(FsError::Large(self.shown().into(), bytes.len() as u64));
        }
        Ok(Content { digest: digest(&bytes), bytes })
    }

    /// Puts a whole new file in the place of what was there, or nothing at all. What other names the
    /// old file has (hard links) keep the old content. Its permissions are kept.
    pub fn write(&self, bytes: &[u8]) -> Result<(), FsError> {
        let kept = match self.kind {
            Kind::Missing => None,
            Kind::File => Some(self.open()?.0.metadata().map_err(|e| self.err(e))?.permissions()),
            _ => return Err(FsError::NotFile(self.shown().into())),
        };
        let tmp = format!(".axiom-{}.tmp", fresh());
        let mut o = OpenOptions::new();
        o.write(true).create_new(true).follow(FollowSymlinks::No);
        let done = self.dir.open_with(&tmp, &o).and_then(|mut f| {
            f.write_all(bytes)?;
            if let Some(p) = kept {
                f.set_permissions(p)?;
            }
            f.sync_all()?;
            drop(f);
            if self.kind == Kind::Missing {
                // Somebody else's new file is not written over.
                self.dir.hard_link(&tmp, &self.dir, &self.name).or_else(|e| match e.kind() {
                    io::ErrorKind::AlreadyExists => Err(e),
                    // Where there are no hard links.
                    _ => self.dir.rename(&tmp, &self.dir, &self.name),
                })?;
                let _ = self.dir.remove_file(&tmp);
                Ok(())
            } else {
                self.dir.rename(&tmp, &self.dir, &self.name)
            }
        });
        if done.is_err() {
            let _ = self.dir.remove_file(&tmp);
        }
        done.map_err(|e| self.err(e))
    }

    pub fn remove(&self) -> Result<(), FsError> {
        if self.kind != Kind::File {
            return Err(FsError::NotFile(self.shown().into()));
        }
        self.dir.remove_file_or_symlink(&self.name).map_err(|e| self.err(e))
    }

    pub fn create_folder(&self) -> Result<(), FsError> {
        self.dir.create_dir(&self.name).map_err(|e| self.err(e))
    }

    /// The folder itself, to list or walk.
    pub fn folder(&self) -> Result<Dir, FsError> {
        match self.kind {
            Kind::Missing => Err(FsError::Missing(self.shown().into())),
            Kind::Folder if self.name == "." => self.dir.try_clone().map_err(|e| self.err(e)),
            Kind::Folder => self.dir.open_dir_nofollow(&self.name).map_err(|e| self.err(e)),
            _ => Err(FsError::NotFolder(self.shown().into())),
        }
    }
}

fn fresh() -> String {
    let mut b = [0u8; 8];
    // Without randomness the name may be taken: then creating it fails, as it should.
    let _ = axiom_proto::random(&mut b);
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// A file of an open folder: a plain one, by its name there, and not a link.
pub fn read_in(dir: &Dir, name: &std::ffi::OsStr, most: u64) -> Option<Vec<u8>> {
    let mut o = OpenOptions::new();
    o.read(true).follow(FollowSymlinks::No).nonblock(true);
    let f = dir.open_with(name, &o).ok()?;
    let m = f.metadata().ok()?;
    if !m.is_file() || m.len() > most {
        return None;
    }
    let mut v = Vec::new();
    f.take(most).read_to_end(&mut v).ok()?;
    Some(v)
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    struct T {
        _d: tempfile::TempDir,
        root: Place,
        out: PathBuf,
    }
    fn t() -> T {
        let d = tempfile::tempdir().unwrap();
        let p = std::fs::canonicalize(d.path()).unwrap();
        for x in ["w/a/b", "out"] {
            std::fs::create_dir_all(p.join(x)).unwrap();
        }
        std::fs::write(p.join("w/a/f"), "in").unwrap();
        std::fs::write(p.join("out/f"), "out").unwrap();
        T { root: Place::new(p.join("w")).unwrap(), out: p.join("out"), _d: d }
    }
    impl T {
        fn find(&self, s: &str) -> Result<Spot, FsError> {
            find(std::slice::from_ref(&self.root), &self.root.join(s).unwrap())
        }
        fn path(&self, s: &str) -> PathBuf {
            self.root.as_path().join(s)
        }
    }

    #[test]
    fn finds_what_is_there() {
        let t = t();
        let s = t.find("a/f").unwrap();
        assert_eq!((s.kind, s.read(10).unwrap().bytes.as_slice()), (Kind::File, &b"in"[..]));
        assert_eq!(t.find("a/b").unwrap().kind, Kind::Folder);
        assert_eq!(t.find("").unwrap().kind, Kind::Folder);
        assert_eq!(t.find("").unwrap().place, t.root);
        assert_eq!(t.find("a/new").unwrap().kind, Kind::Missing);
        assert!(matches!(t.find("no/new"), Err(FsError::Missing(_))));
        assert!(matches!(t.find("a/f/x"), Err(FsError::NotFolder(_))));
        assert!(matches!(t.find("a/f").unwrap().read(1), Err(FsError::Large(_, 2))));
        assert!(matches!(t.find("a/new").unwrap().read(1), Err(FsError::Missing(_))));
        assert!(matches!(t.find("a/b").unwrap().read(1), Err(FsError::NotFile(_))));
        assert!(t.find("").unwrap().folder().unwrap().exists("a"));
        assert!(t.find("a").unwrap().folder().unwrap().exists("f"));
        assert!(matches!(find(&[], &t.root), Err(FsError::Outside(_))));
    }

    #[test]
    fn links_are_walked_and_judged_by_where_they_lead() {
        let t = t();
        symlink("f", t.path("a/l")).unwrap();
        symlink("../a/l", t.path("a/l2")).unwrap();
        symlink(t.path("a/f"), t.path("abs")).unwrap();
        symlink("a", t.path("d")).unwrap();
        symlink(".", t.path("self")).unwrap();
        symlink(t.root.as_path(), t.path("top")).unwrap();
        symlink("a/none", t.path("dangling")).unwrap();
        for s in ["a/l", "a/l2", "abs", "d/f", "d/l2", "self/self/a/f", "top/a/f", "a/b/../f"] {
            assert_eq!(t.find(s).unwrap().place.as_path(), t.path("a/f"), "{s}");
        }
        assert_eq!(t.find("top").unwrap().place, t.root);
        assert_eq!(t.find("self").unwrap().place, t.root);
        let s = t.find("dangling").unwrap();
        assert_eq!((s.kind, s.place.as_path()), (Kind::Missing, t.path("a/none").as_path()));
        // A protected place reached by another name is known for what it is.
        std::fs::create_dir(t.path(".git")).unwrap();
        symlink(".git", t.path("g")).unwrap();
        let s = t.find("g/config").unwrap();
        assert!(axiom_policy::paths::protected(&s.place).is_some());
    }

    #[test]
    fn nothing_leads_out() {
        let t = t();
        symlink(&t.out, t.path("abs")).unwrap();
        symlink("../out", t.path("rel")).unwrap();
        symlink("../../out/f", t.path("a/f2")).unwrap();
        symlink("../w/a", t.path("round")).unwrap();
        symlink("loop", t.path("loop")).unwrap();
        symlink("/", t.path("slash")).unwrap();
        for s in ["abs", "abs/f", "rel/f", "a/f2", "round/f", "slash/etc/passwd", "rel/new"] {
            assert!(matches!(t.find(s), Err(FsError::Outside(_))), "{s}: {:?}", t.find(s));
        }
        assert!(matches!(t.find("loop"), Err(FsError::Links(_))));
        assert!(matches!(t.find("loop/x"), Err(FsError::Links(_))));
    }

    #[test]
    fn writes_whole_or_not_at_all() {
        let t = t();
        t.find("a/new").unwrap().write(b"x").unwrap();
        assert_eq!(std::fs::read(t.path("a/new")).unwrap(), b"x");
        std::fs::set_permissions(t.path("a/new"), std::fs::Permissions::from_mode(0o751)).unwrap();
        std::fs::hard_link(t.path("a/new"), t.path("a/other")).unwrap();
        t.find("a/new").unwrap().write(b"yy").unwrap();
        assert_eq!(std::fs::metadata(t.path("a/new")).unwrap().permissions().mode() & 0o777, 0o751);
        assert_eq!(
            std::fs::read(t.path("a/other")).unwrap(),
            b"x",
            "its other name is another file now"
        );
        // Made by someone else in between.
        let s = t.find("a/late").unwrap();
        std::fs::write(t.path("a/late"), "theirs").unwrap();
        assert!(s.write(b"mine").is_err());
        assert_eq!(std::fs::read(t.path("a/late")).unwrap(), b"theirs");
        // Became a link in between: the link is replaced, not followed.
        let s = t.find("a/f").unwrap();
        std::fs::remove_file(t.path("a/f")).unwrap();
        symlink(t.out.join("f"), t.path("a/f")).unwrap();
        assert!(s.write(b"z").is_err() || std::fs::read(t.out.join("f")).unwrap() == b"out");
        assert_eq!(std::fs::read(t.out.join("f")).unwrap(), b"out");
        assert!(s.read(10).is_err(), "nor read through");
        let left: Vec<_> =
            std::fs::read_dir(t.path("a")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert!(!left.iter().any(|n| n.to_string_lossy().contains("axiom-")), "{left:?}");
        t.find("a/new").unwrap().remove().unwrap();
        assert!(!t.path("a/new").exists());
        assert!(t.find("a/b").unwrap().remove().is_err());
    }

    /// A folder that was judged and then swapped is still the one that was judged.
    #[test]
    fn a_swapped_folder_is_not_followed() {
        let t = t();
        let s = t.find("a/f").unwrap();
        std::fs::rename(t.path("a"), t.path("moved")).unwrap();
        symlink(&t.out, t.path("a")).unwrap();
        s.write(b"new").unwrap();
        assert_eq!(std::fs::read(t.out.join("f")).unwrap(), b"out");
        assert_eq!(std::fs::read(t.path("moved/f")).unwrap(), b"new");
    }

    /// T13: while something else swaps a folder for a link as fast as it can, nothing outside is
    /// ever read or written.
    #[test]
    fn racing_a_swapper() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let t = t();
        std::fs::create_dir(t.path("x")).unwrap();
        std::fs::write(t.path("x/f"), "in").unwrap();
        symlink(&t.out, t.path("evil")).unwrap();
        let stop = AtomicBool::new(false);
        std::thread::scope(|s| {
            s.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    for (a, b) in [("x", "keep"), ("evil", "x"), ("x", "evil"), ("keep", "x")] {
                        let _ = std::fs::rename(t.path(a), t.path(b));
                    }
                }
            });
            let (mut read, mut wrote, mut no) = (0, 0, 0);
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < end {
                match t.find("x/f").and_then(|s| s.read(100)) {
                    Ok(c) => {
                        assert_ne!(c.bytes, b"out");
                        read += 1;
                    }
                    Err(_) => no += 1,
                }
                match t.find("x/f").and_then(|s| s.write(b"in2")) {
                    Ok(()) => wrote += 1,
                    Err(_) => no += 1,
                }
                let _ = t.find("x/made").and_then(|s| s.write(b"m"));
            }
            stop.store(true, Ordering::Relaxed);
            assert!(read > 0 && wrote > 0 && no > 0, "{read} {wrote} {no}: the race was raced");
        });
        assert_eq!(std::fs::read(t.out.join("f")).unwrap(), b"out");
        assert_eq!(std::fs::read_dir(&t.out).unwrap().count(), 1);
    }

    #[test]
    fn a_pipe_is_not_waited_for() {
        let t = t();
        let st = std::process::Command::new("mkfifo").arg(t.path("pipe")).status().unwrap();
        assert!(st.success());
        assert!(matches!(t.find("pipe").unwrap().read(10), Err(FsError::NotFile(_))));
        // One that took the place of a file.
        let s = t.find("a/f").unwrap();
        std::fs::remove_file(t.path("a/f")).unwrap();
        std::fs::rename(t.path("pipe"), t.path("a/f")).unwrap();
        assert!(matches!(s.read(10), Err(FsError::NotFile(_))));
        assert!(read_in(&t.find("a").unwrap().folder().unwrap(), "f".as_ref(), 10).is_none());
    }
}
