#![allow(clippy::unwrap_used, clippy::panic)]
use std::os::unix::fs::{PermissionsExt, symlink};

use super::*;
use crate::copy::{From, changes, make};

struct T {
    _d: tempfile::TempDir,
    folder: Place,
    copy: Place,
    to: Place,
    store: std::path::PathBuf,
}
fn place(p: &Path) -> Place {
    Place::new(std::fs::canonicalize(p).unwrap()).unwrap()
}
fn write(p: &Place, rel: &str, text: &str) {
    let at = p.as_path().join(rel);
    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::fs::write(at, text).unwrap();
}
fn read(p: &Place, rel: &str) -> Option<String> {
    std::fs::read_to_string(p.as_path().join(rel)).ok()
}
fn remove(p: &Place, rel: &str) {
    std::fs::remove_file(p.as_path().join(rel)).unwrap();
}
static GO: AtomicBool = AtomicBool::new(false);

impl T {
    fn new(files: &[(&str, &str)]) -> Self {
        let d = tempfile::tempdir().unwrap();
        for n in ["w", "c", "t", "s"] {
            std::fs::create_dir(d.path().join(n)).unwrap();
        }
        let at = |n| place(&d.path().join(n));
        let t =
            Self { folder: at("w"), copy: at("c"), to: at("t"), store: d.path().join("s"), _d: d };
        for (n, text) in files {
            write(&t.folder, n, text);
        }
        t
    }
    fn made(&self) -> Files {
        let may = |_: &Place| true;
        let from = From { folder: &self.folder, tracked: &[], may: &may, stop: &GO };
        make(&from, &self.copy, Most::default()).unwrap().0
    }
    fn keep(&self, was: &Files) -> Option<Delta> {
        keep(&self.folder, &self.copy, was, &self.store, 100, &GO).unwrap()
    }
    fn again(&self, was: &Files, delta: &Delta) -> Result<Files, AgainError> {
        self.again_within(was, delta, Most::default())
    }
    fn again_within(&self, was: &Files, delta: &Delta, most: Most) -> Result<Files, AgainError> {
        let mut all: BTreeMap<String, Is> =
            was.iter().map(|(p, w)| (p.clone(), w.is.clone())).collect();
        for (p, is) in delta {
            match is {
                Some(is) => all.insert(p.clone(), is.clone()),
                None => all.remove(p),
            };
        }
        let from = Sources { store: &self.store, copy: &self.copy, folder: &self.folder };
        again(&all, &from, &self.to, most, &GO)
    }
    fn kept(&self) -> Vec<String> {
        let names = std::fs::read_dir(&self.store).unwrap();
        names.map(|e| e.unwrap().file_name().into_string().unwrap()).collect()
    }
}

#[test]
fn a_copy_is_made_again_as_it_was_then() {
    let t = T::new(&[("a", "1"), ("b/c", "2"), ("gone", "3"), ("same", "4")]);
    let was = t.made();
    write(&t.copy, "a", "1 then");
    write(&t.copy, "new", "then");
    remove(&t.copy, "gone");
    let then = t.keep(&was).unwrap();
    assert_eq!(then.len(), 3);
    // Everything is something else now, in the copy and in the folder.
    for p in [&t.copy, &t.folder] {
        for f in ["a", "new", "gone", "b/c"] {
            write(p, f, "later");
        }
    }
    // Of `b/c` nobody has what it was: the copy changed it, and so did the folder.
    let e = t.again(&was, &then).unwrap_err();
    assert!(matches!(&e, AgainError::Missing(f, 1) if f == "b/c"), "{e}");
    write(&t.folder, "b/c", "2");
    let t2 = T::new(&[]);
    let t = T { to: t2.to.clone(), ..t };
    let files = t.again(&was, &then).unwrap();
    assert_eq!(read(&t.to, "a").unwrap(), "1 then");
    assert_eq!(read(&t.to, "b/c").unwrap(), "2");
    assert_eq!(read(&t.to, "new").unwrap(), "then");
    assert_eq!(read(&t.to, "same").unwrap(), "4");
    assert_eq!(read(&t.to, "gone"), None);
    assert_eq!(files.len(), 4);
    assert!(files.values().all(|w| w.stamp.is_some()));
    // What the branch has changed, by what counted as copied, is what had been done by then.
    let mut base = was.clone();
    for (p, w) in &files {
        if base.get(p).is_some_and(|b| b.is == w.is) {
            base.insert(p.clone(), w.clone());
        }
    }
    let c = changes(&t.folder, &t.to, &base, 100, &GO).unwrap();
    let names: Vec<_> = c.all.iter().map(|c| (c.path.as_str(), c.clash)).collect();
    assert_eq!(names, [("a", true), ("gone", true), ("new", true)], "{c:?}");
}

#[test]
fn what_it_was_before_is_kept_while_the_folder_has_it() {
    let t = T::new(&[("a", "1")]);
    let was = t.made();
    let first = t.keep(&was).unwrap();
    assert!(first.is_empty() && t.kept().is_empty());
    write(&t.copy, "a", "2");
    let second = t.keep(&was).unwrap();
    assert_eq!(t.kept().len(), 2, "both what it is and what it was");
    write(&t.copy, "a", "3");
    write(&t.folder, "a", "4");
    t.again(&was, &first).unwrap();
    assert_eq!(read(&t.to, "a").unwrap(), "1");
    remove(&t.to, "a");
    t.again(&was, &second).unwrap();
    assert_eq!(read(&t.to, "a").unwrap(), "2");
}

#[test]
fn links_stay_links_and_what_runs_runs() {
    let t = T::new(&[("a", "1"), ("run", "#!")]);
    let was = t.made();
    let can = |p: &Place, f: &str| {
        let m = std::fs::symlink_metadata(p.as_path().join(f)).unwrap();
        m.permissions().mode() & 0o111 != 0
    };
    let x = Permissions::from_mode(0o755);
    std::fs::set_permissions(t.copy.as_path().join("run"), x).unwrap();
    symlink("/etc/hosts", t.copy.as_path().join("l")).unwrap();
    // The same in it as the link has, and another thing.
    write(&t.copy, "f", "/etc/hosts");
    write(&t.copy, "quiet", "#!");
    let then = t.keep(&was).unwrap();
    for f in ["l", "f", "quiet", "run"] {
        remove(&t.copy, f);
    }
    t.again(&was, &then).unwrap();
    let l = std::fs::read_link(t.to.as_path().join("l")).unwrap();
    assert_eq!(l, Path::new("/etc/hosts"));
    assert!(!std::fs::symlink_metadata(t.to.as_path().join("f")).unwrap().is_symlink());
    assert_eq!(read(&t.to, "f").unwrap(), "/etc/hosts");
    assert!(can(&t.to, "run") && !can(&t.to, "quiet") && !can(&t.to, "a"));
}

#[test]
fn what_is_kept_is_not_believed_for_its_name() {
    let t = T::new(&[]);
    let was = t.made();
    write(&t.copy, "a", "then");
    let then = t.keep(&was).unwrap();
    std::fs::write(t.store.join(&t.kept()[0]), "torn").unwrap();
    // The copy has it still.
    t.again(&was, &then).unwrap();
    assert_eq!(read(&t.to, "a").unwrap(), "then");
    remove(&t.to, "a");
    write(&t.copy, "a", "later");
    let e = t.again(&was, &then).unwrap_err();
    assert!(matches!(e, AgainError::Missing(_, 1)), "{e}");
    assert_eq!(read(&t.to, "a"), None);
}

#[test]
fn nothing_is_kept_of_what_cannot_be() {
    let t = T::new(&[("a", "1")]);
    let was = t.made();
    write(&t.copy, "b", "2");
    write(&t.copy, "c", "3");
    let within = |n| keep(&t.folder, &t.copy, &was, &t.store, n, &GO).unwrap();
    assert!(within(1).is_none());
    assert!(within(2).is_some());
    // Not a plain file where one was.
    remove(&t.copy, "a");
    std::fs::create_dir(t.copy.as_path().join("a")).unwrap();
    assert!(within(9).is_none());
    std::fs::remove_dir(t.copy.as_path().join("a")).unwrap();
    let big = std::fs::File::create(t.copy.as_path().join("a")).unwrap();
    big.set_len(LARGEST + 1).unwrap();
    assert!(within(9).is_none());
    assert!(t.kept().iter().all(|n| !n.ends_with(".tmp")));
    let stopped = AtomicBool::new(true);
    let r = keep(&t.folder, &t.copy, &was, &t.store, 9, &stopped);
    assert!(matches!(r, Err(CopyError::Stopped)));
}

#[test]
fn it_is_made_only_in_an_empty_folder_and_not_without_bounds() {
    let t = T::new(&[("a", "1234"), ("b", "1234")]);
    let was = t.made();
    let most = |files, bytes| Most { files, bytes };
    let none = Delta::new();
    for m in [most(1, 100), most(9, 7)] {
        let r = t.again_within(&was, &none, m);
        assert!(matches!(r, Err(AgainError::Copy(CopyError::TooMuch(_)))), "{r:?}");
        for f in ["a", "b"] {
            let _ = std::fs::remove_file(t.to.as_path().join(f));
        }
    }
    t.again_within(&was, &none, most(2, 8)).unwrap();
    let r = t.again(&was, &none);
    assert!(matches!(r, Err(AgainError::Copy(CopyError::There(_)))));
}

#[test]
fn its_repository_goes_with_it_and_not_its_hooks() {
    let t = T::new(&[("a", "1"), (".git/HEAD", "ref: x"), (".git/hooks/pre-commit", "#!")]);
    let was = t.made();
    t.again(&was, &Delta::new()).unwrap();
    assert_eq!(read(&t.to, ".git/HEAD").unwrap(), "ref: x");
    assert_eq!(read(&t.to, ".git/hooks/pre-commit"), None);
}

#[test]
fn nothing_is_followed_out_of_the_copy() {
    let t = T::new(&[("d/a", "1")]);
    let was = t.made();
    let out = tempfile::tempdir().unwrap();
    std::fs::write(out.path().join("a"), "secret").unwrap();
    std::fs::remove_dir_all(t.copy.as_path().join("d")).unwrap();
    symlink(out.path(), t.copy.as_path().join("d")).unwrap();
    let then = t.keep(&was).unwrap();
    assert_eq!(then.get("d/a"), Some(&None), "it is gone");
    assert!(then["d"].as_ref().unwrap().link);
    for f in t.kept() {
        let text = std::fs::read_to_string(t.store.join(&f)).unwrap_or_default();
        assert!(text != "secret" || f.ends_with(".l"), "{f}");
    }
    t.again(&was, &then).unwrap();
    assert!(std::fs::symlink_metadata(t.to.as_path().join("d")).unwrap().is_symlink());
}
