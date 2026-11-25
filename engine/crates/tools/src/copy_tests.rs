#![allow(clippy::unwrap_used, clippy::panic)]
use std::os::unix::fs::{PermissionsExt, symlink};

use super::*;

struct T {
    _d: tempfile::TempDir,
    folder: Place,
    copy: Place,
    beside: Place,
}
fn place(p: &Path) -> Place {
    Place::new(std::fs::canonicalize(p).unwrap()).unwrap()
}
impl T {
    fn new(files: &[(&str, &str)]) -> Self {
        let d = tempfile::tempdir().unwrap();
        for n in ["w", "c", "b"] {
            std::fs::create_dir(d.path().join(n)).unwrap();
        }
        let at = |n| place(&d.path().join(n));
        let (folder, copy, beside) = (at("w"), at("c"), at("b"));
        let t = Self { folder, copy, beside, _d: d };
        for (n, text) in files {
            write(&t.folder, n, text);
        }
        t
    }
    fn make(&self, tracked: &[&str]) -> Result<Files, CopyError> {
        self.make_within(tracked, Most::default())
    }
    fn make_within(&self, tracked: &[&str], most: Most) -> Result<Files, CopyError> {
        let tracked: Vec<String> = tracked.iter().map(|s| (*s).to_owned()).collect();
        let stop = AtomicBool::new(false);
        let may = |p: &Place| p.name() != "kept";
        let from = From { folder: &self.folder, tracked: &tracked, may: &may, stop: &stop };
        make(&from, &self.copy, most).map(|m| m.0)
    }
    fn news(&self, was: &mut Files) -> Renewal {
        let stop = AtomicBool::new(false);
        let may = |p: &Place| p.name() != "kept";
        let from = From { folder: &self.folder, tracked: &[], may: &may, stop: &stop };
        news(&from, &self.copy, was, Most::default()).unwrap()
    }
    /// All that can be taken over is, and the folder's side of what cannot is put beside.
    fn renew(&self, was: &mut Files) -> (Vec<String>, Vec<String>) {
        let r = self.news(was);
        assert_eq!(r.left, []);
        for (p, is) in &r.taking {
            match take(&self.folder, &self.copy, p, is.as_ref()).unwrap() {
                Some(w) => was.insert(p.clone(), w),
                None => was.remove(p),
            };
        }
        for (p, is) in &r.clashes {
            beside(&self.folder, &self.beside, p).unwrap();
            match is {
                Some(is) => was.insert(p.clone(), Was { is: is.clone(), stamp: None }),
                None => was.remove(p),
            };
        }
        (r.taking.into_keys().collect(), r.clashes.into_keys().collect())
    }
    fn look(&self, was: &Files) -> Changes {
        changes(&self.folder, &self.copy, was, 1000, &AtomicBool::new(false)).unwrap()
    }
    fn changes(&self, was: &Files) -> Vec<Change> {
        let c = self.look(was);
        assert!(c.whole);
        c.all
    }
    fn plan(&self, was: &Files, path: &str) -> Result<Plan, Why> {
        let at = |p: &Place| now(&open(p).unwrap(), path).unwrap();
        let (is, there) = (at(&self.copy), at(&self.folder));
        let shown = seen(is.as_ref(), there.as_ref());
        plan(&self.folder, &self.copy, path, was.get(path), shown)
    }
    fn bring(&self, was: &Files, path: &str) -> Result<Plan, Why> {
        let p = self.plan(was, path)?;
        bring(&self.folder, &self.copy, path, p.is.as_ref(), p.there.as_ref())?;
        Ok(p)
    }
    fn back(&self, path: &str, p: &Plan) -> Result<(), Why> {
        let before = p.bytes.as_deref().map(|b| (b, p.mode));
        back(&self.folder, path, before, p.is.as_ref())
    }
}
fn seen<'a>(is: Option<&'a Is>, there: Option<&'a Is>) -> Seen<'a> {
    Seen {
        digest: is.map(|i| i.digest.as_str()),
        run: is.is_some_and(|i| i.run),
        there: there.map(|i| i.digest.as_str()),
    }
}
fn mode(p: &Place, n: &str) -> u32 {
    std::fs::metadata(p.as_path().join(n)).unwrap().permissions().mode() & 0o777
}
fn chmod(p: &Place, n: &str, m: u32) {
    std::fs::set_permissions(p.as_path().join(n), PermissionsExt::from_mode(m)).unwrap();
}
fn write(p: &Place, n: &str, text: &str) {
    let f = p.as_path().join(n);
    std::fs::create_dir_all(f.parent().unwrap()).unwrap();
    std::fs::write(f, text).unwrap();
}
fn read(p: &Place, n: &str) -> Option<String> {
    std::fs::read_to_string(p.as_path().join(n)).ok()
}
fn names(f: &Files) -> Vec<&str> {
    f.keys().map(String::as_str).collect()
}
fn shown(c: &[Change]) -> Vec<(&str, What, bool)> {
    c.iter().map(|c| (c.path.as_str(), c.what, c.clash)).collect()
}

#[test]
fn what_is_copied() {
    let t = T::new(&[
        (".gitignore", "/target\n*.log\n"),
        ("a.txt", "a"),
        ("sub/b.sh", "b"),
        ("sub/kept", "secret"),
        ("sub/x.log", "ignored"),
        ("sub/seen.log", "tracked although it is ignored"),
        ("target/big", "ignored"),
        (".git/HEAD", "ref: refs/heads/main"),
        (".git/objects/ab/cd", "o"),
        (".git/hooks/pre-commit", "x"),
        (".git/index.lock", "x"),
        (".git/worktrees/other/HEAD", "x"),
        ("sub/inner/.git", "gitdir: elsewhere"),
        (".axiom/policy.toml", "version = 1"),
    ]);
    let b = t.folder.as_path().join("sub/b.sh");
    std::fs::set_permissions(&b, PermissionsExt::from_mode(0o755)).unwrap();
    symlink("/etc/passwd", t.folder.as_path().join("link")).unwrap();
    symlink("/etc", t.folder.as_path().join(".git/outside")).unwrap();
    let outside = t._d.path().join("outside");
    std::fs::write(&outside, "x").unwrap();
    symlink(t._d.path(), t.folder.as_path().join("folder")).unwrap();
    let tracked = [
        "a.txt",
        "sub/seen.log",
        "sub/kept",
        "../outside",
        "folder/outside",
        "gone",
        ".git/HEAD",
        "",
    ];
    let was = t.make(&tracked).unwrap();
    let all =
        [".axiom/policy.toml", ".gitignore", "a.txt", "folder", "link", "sub/b.sh", "sub/seen.log"];
    assert_eq!(names(&was), all);
    for n in [".git/HEAD", ".git/objects/ab/cd"] {
        assert_eq!(read(&t.copy, n), read(&t.folder, n), "{n}");
    }
    for n in [
        ".git/hooks",
        ".git/index.lock",
        ".git/worktrees",
        ".git/outside",
        "target",
        "sub/kept",
        "sub/x.log",
        "sub/inner",
    ] {
        assert!(std::fs::symlink_metadata(t.copy.as_path().join(n)).is_err(), "{n}");
    }
    // As a link, and what it leads to is not here.
    let link = std::fs::read_link(t.copy.as_path().join("link")).unwrap();
    assert_eq!(link, Path::new("/etc/passwd"));
    assert!(was["link"].is.link && was["folder"].is.link && !was["a.txt"].is.link);
    assert!(was["sub/b.sh"].is.run && !was["a.txt"].is.run);
    let mode = std::fs::metadata(t.copy.as_path().join("sub/b.sh")).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert_eq!(t.changes(&was), []);
    // Not another time into the same folder.
    assert!(matches!(t.make(&[]), Err(CopyError::There(_))));
}

#[test]
fn too_much_is_refused_before_anything_is_made() {
    let files: Vec<(String, String)> =
        (0..20).map(|i| (format!("f{i}"), "0123456789".into())).collect();
    let files: Vec<(&str, &str)> = files.iter().map(|f| (f.0.as_str(), f.1.as_str())).collect();
    let t = T::new(&files);
    for most in [Most { files: 19, bytes: 1 << 30 }, Most { files: 100, bytes: 199 }] {
        assert!(matches!(t.make_within(&[], most), Err(CopyError::TooMuch(_))));
        assert_eq!(std::fs::read_dir(t.copy.as_path()).unwrap().count(), 0);
    }
    write(&t.folder, ".git/objects/x", "y");
    let most = Most { files: 20, bytes: 1 << 30 };
    assert!(matches!(t.make_within(&[], most), Err(CopyError::TooMuch(_))));
    let stop = AtomicBool::new(true);
    let may = |_: &Place| true;
    let from = From { folder: &t.folder, tracked: &[], may: &may, stop: &stop };
    assert!(matches!(make(&from, &t.copy, Most::default()), Err(CopyError::Stopped)));
    assert!(t.make_within(&[], Most { files: 21, bytes: 201 }).is_ok());
}

#[test]
fn what_was_changed_is_brought_in_and_put_back() {
    let t = T::new(&[("a", "1\n"), ("b", "2\n"), ("c", "3\n"), ("Makefile", "x\n"), ("same", "s")]);
    let mut was = t.make(&[]).unwrap();
    write(&t.copy, "a", "one\n");
    write(&t.copy, "new/deep/d.sh", "4\n");
    std::fs::set_permissions(
        t.copy.as_path().join("new/deep/d.sh"),
        PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    std::fs::remove_file(t.copy.as_path().join("b")).unwrap();
    write(&t.copy, "Makefile", "y\n");
    // Touched, and the same.
    write(&t.copy, "same", "s");
    write(&t.copy, ".gitignore", "ignored\n");
    write(&t.copy, "ignored", "x");
    let c = t.changes(&was);
    assert_eq!(
        shown(&c),
        [
            (".gitignore", What::New, false),
            ("Makefile", What::Changed, false),
            ("a", What::Changed, false),
            ("b", What::Removed, false),
            ("new/deep/d.sh", What::New, false),
        ]
    );
    let runs: Vec<&str> = c.iter().filter(|c| c.runs).map(|c| c.path.as_str()).collect();
    assert_eq!(runs, ["Makefile", "new/deep/d.sh"]);
    assert_eq!(diff(&t.folder, &t.copy, &c[2]).unwrap(), "--- a\n+++ a\n@@ -1 +1 @@\n-1\n+one\n");
    assert!(diff(&t.folder, &t.copy, &c[3]).unwrap().contains("+++ (removed)"));
    let mut undo = Vec::new();
    for p in ["a", "b", "new/deep/d.sh"] {
        let b = t.bring(&was, p).unwrap();
        match &b.is {
            Some(i) => was.insert(p.into(), Was { is: i.clone(), stamp: None }),
            None => was.remove(p),
        };
        undo.push(b);
    }
    assert_eq!(read(&t.folder, "a").as_deref(), Some("one\n"));
    assert_eq!(read(&t.folder, "new/deep/d.sh").as_deref(), Some("4\n"));
    assert_eq!(read(&t.folder, "b"), None);
    let m = std::fs::metadata(t.folder.as_path().join("new/deep/d.sh")).unwrap();
    assert_eq!(m.permissions().mode() & 0o100, 0o100);
    assert_eq!(
        shown(&t.changes(&was)).iter().map(|c| c.0).collect::<Vec<_>>(),
        [".gitignore", "Makefile"]
    );
    // Changed since: not put back over it.
    write(&t.folder, "a", "mine\n");
    assert_eq!(t.back("a", &undo[0]).unwrap_err(), "it has changed since");
    t.back("b", &undo[1]).unwrap();
    t.back("new/deep/d.sh", &undo[2]).unwrap();
    assert_eq!(read(&t.folder, "b").as_deref(), Some("2\n"));
    assert_eq!(read(&t.folder, "new/deep/d.sh"), None);
    assert_eq!(read(&t.folder, "a").as_deref(), Some("mine\n"));
}

#[test]
fn the_folders_own_is_not_written_over() {
    let t = T::new(&[("a", "1"), ("b", "2"), ("gone", "3")]);
    let was = t.make(&[]).unwrap();
    for f in ["a", "b", "gone", "n"] {
        write(&t.copy, f, "agent");
    }
    write(&t.folder, "a", "mine");
    std::fs::remove_file(t.folder.as_path().join("gone")).unwrap();
    write(&t.folder, "n", "mine too");
    let c = t.changes(&was);
    let clash: Vec<&str> = c.iter().filter(|c| c.clash).map(|c| c.path.as_str()).collect();
    assert_eq!(clash, ["a", "gone", "n"]);
    for f in ["a", "gone", "n"] {
        let e = t.bring(&was, f).unwrap_err();
        assert!(e.contains("has changed since"), "{f}: {e}");
    }
    assert_eq!(read(&t.folder, "a").as_deref(), Some("mine"));
    assert_eq!(read(&t.folder, "n").as_deref(), Some("mine too"));
    // Only what was seen, of both.
    let at = |p: &Place| now(&open(p).unwrap(), "b").unwrap();
    let (is, there) = (at(&t.copy), at(&t.folder));
    let right = seen(is.as_ref(), there.as_ref());
    for wrong in [
        Seen { digest: Some("other"), ..right },
        Seen { digest: None, ..right },
        Seen { run: true, ..right },
        Seen { there: Some("other"), ..right },
        Seen { there: None, ..right },
    ] {
        let e = plan(&t.folder, &t.copy, "b", was.get("b"), wrong).unwrap_err();
        assert!(e.contains("not what was shown"), "{e}");
    }
    // And only while both are still what they were.
    let p = t.plan(&was, "b").unwrap();
    write(&t.folder, "b", "saved meanwhile");
    let e = bring(&t.folder, &t.copy, "b", p.is.as_ref(), p.there.as_ref()).unwrap_err();
    assert!(e.contains("being changed"), "{e}");
    write(&t.folder, "b", "2");
    write(&t.copy, "b", "agent again");
    let e = bring(&t.folder, &t.copy, "b", p.is.as_ref(), p.there.as_ref()).unwrap_err();
    assert!(e.contains("not what was shown"), "{e}");
    write(&t.copy, "b", "agent");
    assert_eq!(read(&t.folder, "b").as_deref(), Some("2"));
    t.bring(&was, "b").unwrap();
    // What the folder has already is no change.
    write(&t.folder, "a", "agent");
    assert!(!t.changes(&was).iter().any(|c| c.path == "a"));
}

/// Not through a link either that leads to a folder of the same.
#[test]
fn a_link_within_is_not_followed() {
    let t = T::new(&[("d/x", "1")]);
    symlink("d", t.folder.as_path().join("alias")).unwrap();
    let mut was = t.make(&["alias/x"]).unwrap();
    assert_eq!(names(&was), ["alias", "d/x"]);
    assert_eq!(now(&open(&t.folder).unwrap(), "alias/x").unwrap(), None);
    // In the copy it is a folder now, and in the folder it is a link still.
    std::fs::remove_file(t.copy.as_path().join("alias")).unwrap();
    write(&t.copy, "alias/x", "agent");
    write(&t.copy, "alias/new", "agent");
    for p in ["alias/x", "alias/new"] {
        assert!(t.bring(&was, p).is_err(), "{p}");
    }
    assert_eq!((read(&t.folder, "d/x").as_deref(), read(&t.folder, "d/new")), (Some("1"), None));
    write(&t.folder, "d/x", "2");
    t.renew(&mut was);
    assert_eq!(read(&t.copy, "alias/x").as_deref(), Some("agent"));
}

/// What may not be read any more is not left in a copy.
#[test]
fn a_rule_that_came_later_holds_for_a_copy() {
    let t = T::new(&[("a", "1"), ("b", "2")]);
    let mut was = t.make(&[]).unwrap();
    let stop = AtomicBool::new(false);
    let may = |p: &Place| p.name() != "a";
    let from = From { folder: &t.folder, tracked: &[], may: &may, stop: &stop };
    let r = news(&from, &t.copy, &mut was, Most::default()).unwrap();
    assert_eq!(r.taking, Taking::from([("a".to_owned(), None)]));
    take(&t.folder, &t.copy, "a", None).unwrap();
    was.remove("a");
    assert_eq!(read(&t.copy, "a"), None);
    assert_eq!((names(&was), t.changes(&was)), (vec!["b"], vec![]));
    assert_eq!(read(&t.folder, "a").as_deref(), Some("1"));
}

#[test]
fn that_it_can_be_run_is_a_change_too() {
    let t = T::new(&[("a", "1"), ("b", "2")]);
    chmod(&t.folder, "b", 0o755);
    let was = t.make(&[]).unwrap();
    chmod(&t.copy, "a", 0o755);
    chmod(&t.copy, "b", 0o644);
    let c = t.changes(&was);
    assert_eq!(shown(&c), [("a", What::Changed, false)]);
    assert!(c[0].runs);
    t.bring(&was, "a").unwrap();
    let m = std::fs::metadata(t.folder.as_path().join("a")).unwrap().permissions().mode();
    assert_eq!(m & 0o111, 0o111);
    assert_eq!(t.changes(&was), []);
}

#[test]
fn what_is_never_brought_in() {
    let t = T::new(&[("a", "1"), ("d/x", "2"), ("l", "3")]);
    let was = t.make(&[]).unwrap();
    write(&t.copy, ".envrc", "x");
    write(&t.copy, "d/.vscode/tasks.json", "x");
    write(&t.copy, "sub/.HUSKY/x", "x");
    std::fs::remove_file(t.copy.as_path().join("l")).unwrap();
    symlink("/etc/passwd", t.copy.as_path().join("l")).unwrap();
    symlink("a", t.copy.as_path().join("l2")).unwrap();
    let never: Vec<String> =
        t.changes(&was).into_iter().filter(|c| c.never.is_some()).map(|c| c.path).collect();
    assert_eq!(never, [".envrc", "d/.vscode/tasks.json", "l", "l2", "sub/.HUSKY/x"]);
    for p in &never {
        assert!(t.bring(&was, p).is_err(), "{p}");
    }
    for n in [".envrc", "d/.vscode", "l2", "sub"] {
        assert!(std::fs::symlink_metadata(t.folder.as_path().join(n)).is_err(), "{n}");
    }
    // A folder that became a link, in the folder or in the copy, is nobody's way out.
    let out = t._d.path().join("out");
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("x"), "2").unwrap();
    write(&t.copy, "d/x", "agent");
    std::fs::remove_dir_all(t.folder.as_path().join("d")).unwrap();
    symlink(&out, t.folder.as_path().join("d")).unwrap();
    assert!(t.bring(&was, "d/x").is_err());
    write(&t.copy, "d/new", "agent");
    assert!(t.bring(&was, "d/new").is_err());
    assert_eq!(std::fs::read_dir(&out).unwrap().count(), 1);
    assert_eq!(std::fs::read_to_string(out.join("x")).unwrap(), "2");
    std::fs::remove_dir_all(t.copy.as_path().join("d")).unwrap();
    symlink(&out, t.copy.as_path().join("d")).unwrap();
    let c = t.changes(&was);
    assert!(c.iter().all(|c| c.path != "d/new"), "{c:?}");
    for bad in ["../out/x", "/etc/x", "a/../b", "./a", "a//b", "a/", "", "nothing"] {
        assert!(plan(&t.folder, &t.copy, bad, None, seen(None, None)).is_err(), "{bad}");
    }
}

#[test]
fn a_copy_is_brought_up_to_date() {
    let t = T::new(&[("a", "1"), ("b", "2"), ("c", "3"), ("d", "4"), ("e", "5"), ("both", "6")]);
    let mut was = t.make(&[]).unwrap();
    // In the folder: changed, removed, new, kept from agents.
    write(&t.folder, "a", "folder");
    std::fs::remove_file(t.folder.as_path().join("b")).unwrap();
    write(&t.folder, "x/new", "folder");
    write(&t.folder, "kept", "secret");
    // In both: differently, the same, and one removed in the folder.
    for (n, f, c) in [("c", "folder", "agent"), ("both", "same", "same")] {
        write(&t.folder, n, f);
        write(&t.copy, n, c);
    }
    std::fs::remove_file(t.folder.as_path().join("d")).unwrap();
    write(&t.copy, "d", "agent");
    // In the copy only.
    write(&t.copy, "e", "agent");
    let (taken, clashes) = t.renew(&mut was);
    assert_eq!(taken, ["a", "b", "x/new"]);
    assert_eq!(clashes, ["c", "d"]);
    assert_eq!(read(&t.copy, "a").as_deref(), Some("folder"));
    assert_eq!(read(&t.copy, "x/new").as_deref(), Some("folder"));
    assert_eq!((read(&t.copy, "b"), read(&t.copy, "kept")), (None, None));
    assert_eq!(read(&t.copy, "c").as_deref(), Some("agent"));
    assert_eq!(read(&t.beside, "c").as_deref(), Some("folder"));
    assert_eq!(read(&t.beside, "d"), None);
    // Now it is the agent's to settle, and what that would undo is shown.
    let c = t.changes(&was);
    assert_eq!(
        shown(&c),
        [("c", What::Changed, false), ("d", What::New, false), ("e", What::Changed, false)]
    );
    assert!(diff(&t.folder, &t.copy, &c[0]).unwrap().contains("-folder"));
    assert_eq!(t.news(&mut was), Renewal::default());
    t.bring(&was, "c").unwrap();
    assert_eq!(read(&t.folder, "c").as_deref(), Some("agent"));
}

/// What goes by another rule is at least counted.
#[test]
fn what_is_not_listed_is_counted() {
    use std::os::unix::ffi::OsStrExt;
    let t = T::new(&[("a", "1"), (".gitignore", "out/\n"), ("out/tracked", "t")]);
    let was = t.make(&["out/tracked"]).unwrap();
    assert_eq!(names(&was), [".gitignore", "a", "out/tracked"]);
    let c = t.look(&was);
    assert_eq!((c.all, c.ignored, c.odd), (vec![], 0, 0));
    // The agent moves a file to where git does not look, and says itself what git ignores.
    std::fs::remove_file(t.copy.as_path().join("a")).unwrap();
    write(&t.copy, "out/a", "1");
    write(&t.copy, "gen/a", "1");
    write(&t.copy, ".gitignore", "out/\ngen/\n");
    write(&t.copy, "out/tracked", "agent");
    let c = t.look(&was);
    let all: Vec<&str> = c.all.iter().map(|c| c.path.as_str()).collect();
    assert_eq!((all, c.ignored), (vec![".gitignore", "a", "out/tracked"], 2));
    let mut deep = t.copy.as_path().to_path_buf();
    for _ in 0..70 {
        deep.push("d");
    }
    std::fs::create_dir_all(&deep).unwrap();
    // Where the file system takes names that are no text.
    let odd = t.copy.as_path().join(OsStr::from_bytes(b"\xff"));
    let n = 1 + usize::from(std::fs::write(odd, "x").is_ok());
    assert_eq!(t.look(&was).odd, n);
    // No more than there is room for, of what the folder does not have already.
    let t = T::new(&[]);
    let was = t.make(&[]).unwrap();
    for n in ["a", "b", "c"] {
        write(&t.copy, n, "x");
        write(&t.folder, n, "x");
    }
    write(&t.copy, "d", "x");
    let few = |n| changes(&t.folder, &t.copy, &was, n, &AtomicBool::new(false)).unwrap();
    assert_eq!((few(1).all.len(), few(1).whole), (1, true));
    assert_eq!((few(0).all.len(), few(0).whole), (0, false));
}

/// A folder that became a file is not brought in as the removal of all that was in it.
#[test]
fn what_took_the_place_of_a_folder() {
    let t = T::new(&[("src/a", "1"), ("src/sub/b", "2"), ("gone/c", "3"), ("file", "4")]);
    let was = t.make(&[]).unwrap();
    std::fs::remove_dir_all(t.copy.as_path().join("src")).unwrap();
    write(&t.copy, "src", "now a file");
    std::fs::remove_dir_all(t.copy.as_path().join("gone")).unwrap();
    std::fs::remove_file(t.copy.as_path().join("file")).unwrap();
    write(&t.copy, "file/x", "now a folder");
    for c in t.changes(&was) {
        if c.path == "file/x" {
            // Nothing to be said of the file itself, only of where it would be.
            assert!(t.bring(&was, "file/x").is_err());
            continue;
        }
        let can = c.path == "gone/c";
        assert_eq!(c.never.is_none() && t.plan(&was, &c.path).is_ok(), can, "{c:?}");
    }
    assert!(t.bring(&was, "src/a").is_err());
    assert!(t.bring(&was, "src").is_err());
    assert_eq!(read(&t.folder, "src/a").as_deref(), Some("1"));
    assert_eq!(read(&t.folder, "file").as_deref(), Some("4"));
    t.bring(&was, "gone/c").unwrap();
}

#[test]
fn permissions_are_put_back_as_they_were() {
    let t = T::new(&[("secret", "1"), ("a", "2"), ("run", "3"), ("off", "4")]);
    chmod(&t.folder, "secret", 0o600);
    chmod(&t.folder, "run", 0o755);
    chmod(&t.folder, "off", 0o755);
    let mut was = t.make(&[]).unwrap();
    std::fs::remove_file(t.copy.as_path().join("secret")).unwrap();
    write(&t.copy, "a", "agent");
    chmod(&t.copy, "a", 0o755);
    write(&t.copy, "off", "agent");
    chmod(&t.copy, "off", 0o644);
    let mut all = Vec::new();
    for n in ["secret", "a", "off"] {
        let p = t.bring(&was, n).unwrap();
        let there = now(&open(&t.folder).unwrap(), n).unwrap();
        match there {
            Some(is) => was.insert(n.into(), Was { is, stamp: None }),
            None => was.remove(n),
        };
        all.push((n, p));
    }
    assert_eq!((mode(&t.folder, "a"), mode(&t.folder, "off")), (0o755, 0o755));
    assert_eq!(t.changes(&was), [], "that it cannot be run any more is no change");
    for (n, p) in &all {
        t.back(n, p).unwrap();
    }
    assert_eq!(mode(&t.folder, "secret"), 0o600);
    assert_eq!((mode(&t.folder, "a"), read(&t.folder, "a").as_deref()), (0o644, Some("2")));
    assert_eq!(mode(&t.folder, "off"), 0o755);
    // Only that it can be run differs from what was copied: no clash.
    let was = T::new(&[]).make(&[]).unwrap_or_default();
    let mut was = was;
    was.insert("a".into(), Was { is: all[1].1.there.clone().unwrap(), stamp: None });
    write(&t.copy, "a", "agent");
    chmod(&t.folder, "a", 0o755);
    assert!(t.changes(&was).iter().all(|c| c.path != "a" || !c.clash));
}

/// Two names that are one to the file system of the copy.
#[test]
fn one_name_is_not_written_over_by_another() {
    let t = T::new(&[("a", "1")]);
    let f = open(&t.folder).unwrap();
    let c = open(&t.copy).unwrap();
    put(&f, OsStr::new("a"), &c, "a", false).unwrap();
    write(&t.folder, "b", "2");
    let e = put(&f, OsStr::new("b"), &c, "a", false).unwrap_err();
    assert_eq!(e.kind(), Some(io::ErrorKind::AlreadyExists));
    assert_eq!(read(&t.copy, "a").as_deref(), Some("1"));
    // In its place, whole or not at all, with nothing left over.
    put(&f, OsStr::new("b"), &c, "a", true).unwrap();
    assert_eq!(read(&t.copy, "a").as_deref(), Some("2"));
    write(&t.copy, "dir/x", "x");
    assert!(put(&f, OsStr::new("b"), &c, "dir", true).is_err());
    assert_eq!(std::fs::read_dir(t.copy.as_path()).unwrap().count(), 2);
}

/// A thing that cannot be looked at does not keep the rest from being taken over.
#[test]
fn one_that_is_in_the_way_is_said() {
    let t = T::new(&[("a", "1"), ("b", "2")]);
    let mut was = t.make(&[]).unwrap();
    write(&t.folder, "a", "folder");
    chmod(&t.folder, "a", 0o000);
    write(&t.folder, "b", "folder");
    let r = t.news(&mut was);
    chmod(&t.folder, "a", 0o644);
    // (To whoever may read everything it is not in the way.)
    if !r.left.is_empty() {
        assert_eq!(r.left[0].0, "a");
        assert_eq!(r.taking.keys().collect::<Vec<_>>(), ["b"]);
    }
    write(&t.copy, "out", "agent");
    write(&t.folder, "out/x", "folder");
    assert!(
        take(
            &t.folder,
            &t.copy,
            "out/x",
            now(&open(&t.folder).unwrap(), "out/x").unwrap().as_ref()
        )
        .is_err()
    );
    assert_eq!(read(&t.copy, "out").as_deref(), Some("agent"));
}

#[test]
fn what_was_not_touched_is_not_read() {
    let t = T::new(&[("a", "1")]);
    let was = t.make(&[]).unwrap();
    // The stamp is the system's: setting the time of the content back does not set it back.
    let f = t.copy.as_path().join("a");
    let m = std::fs::metadata(&f).unwrap();
    std::fs::write(&f, "2").unwrap();
    let h = std::fs::File::options().write(true).open(&f).unwrap();
    h.set_modified(m.modified().unwrap()).unwrap();
    drop(h);
    assert_eq!(shown(&t.changes(&was)), [("a", What::Changed, false)]);
}
