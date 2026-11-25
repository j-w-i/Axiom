//! Looking through folders by handle: what is there, what is named so, what holds these words.
//! Links are not followed. What `.gitignore` leaves out and what is kept from agents is not shown.

use std::{
    ffi::OsStr,
    fmt::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use axiom_policy::Place;
use cap_std::fs::Dir;
use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::fs::read_in;

/// Entries looked at before giving up.
const MOST_SEEN: usize = 500_000;
/// Deeper folders are not entered.
const DEEPEST: usize = 64;
/// Longer files are not searched.
const LARGEST: u64 = 4 * 1024 * 1024;
const LONGEST: Duration = Duration::from_secs(20);
/// Of a line that is found, characters.
const SHOWN: usize = 300;

/// What all of it may come to, bytes.
const TEXT: usize = 64 * 1024;

/// What may be shown: judged for each file by its real place.
pub type May<'a> = &'a dyn Fn(&Place) -> bool;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Not allowed.
    pub kept: usize,
    /// Too large or not text.
    pub unread: usize,
    /// Why it ended early.
    pub ended: Option<&'static str>,
}

impl Skipped {
    fn say(&self, out: &mut String) {
        if self.kept > 0 {
            let _ = writeln!(out, "({} not looked at: not allowed)", self.kept);
        }
        if self.unread > 0 {
            let _ = writeln!(out, "({} not searched: too large or not text)", self.unread);
        }
        if let Some(e) = self.ended {
            let _ = writeln!(out, "(ended early: {e})");
        }
    }
}

struct Walk<'a> {
    base: PathBuf,
    may: May<'a>,
    stop: &'a AtomicBool,
    began: Instant,
    seen: usize,
    skipped: Skipped,
}

impl Walk<'_> {
    /// Every file, in order of names; `f` says whether to go on. The second path is below the
    /// beginning, with `/` between names.
    fn files(&mut self, dir: &Dir, f: &mut dyn FnMut(&Dir, &OsStr, &Place, &str) -> bool) {
        let mut rules = Vec::new();
        self.folder(dir, &PathBuf::new(), 0, &mut rules, f);
    }

    fn folder(
        &mut self,
        dir: &Dir,
        rel: &std::path::Path,
        depth: usize,
        rules: &mut Vec<Gitignore>,
        f: &mut dyn FnMut(&Dir, &OsStr, &Place, &str) -> bool,
    ) -> bool {
        use cap_fs_ext::DirExt;
        let here = self.base.join(rel);
        let mine = ignore_rules(dir, &here, &[".gitignore", ".ignore"]);
        let had = rules.len();
        rules.extend(mine);
        let mut entries: Vec<_> = match dir.entries() {
            Ok(e) => {
                e.flatten().take(MOST_SEEN).map(|e| (e.file_name(), e.file_type().ok())).collect()
            }
            Err(_) => Vec::new(),
        };
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut on = true;
        for (name, kind) in entries {
            self.seen += 1;
            if self.seen > MOST_SEEN {
                self.skipped.ended = Some("too many files");
                on = false;
            } else if self.stop.load(Ordering::Relaxed) {
                self.skipped.ended = Some("stopped");
                on = false;
            } else if self.began.elapsed() > LONGEST {
                self.skipped.ended = Some("it took too long");
                on = false;
            }
            if !on {
                break;
            }
            let Some(kind) = kind else { continue };
            let (is_dir, is_file) = (kind.is_dir(), kind.is_file());
            if !is_dir && !is_file {
                continue;
            }
            let path = here.join(&name);
            if name == ".git" || ignored(rules, &path, is_dir) {
                continue;
            }
            let Ok(place) = Place::new(&path) else { continue };
            if !(self.may)(&place) {
                self.skipped.kept += 1;
                continue;
            }
            let rel = rel.join(&name);
            if is_dir {
                if depth + 1 < DEEPEST
                    && let Ok(d) = dir.open_dir_nofollow(&name)
                {
                    on = self.folder(&d, &rel, depth + 1, rules, f);
                }
            } else {
                let shown: Vec<_> =
                    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect();
                on = f(dir, &name, &place, &shown.join("/"));
            }
            if !on {
                break;
            }
        }
        rules.truncate(had);
        on
    }
}

/// The folder's own rules, read through its handle.
pub(crate) fn ignore_rules(dir: &Dir, here: &std::path::Path, files: &[&str]) -> Option<Gitignore> {
    let mut b = GitignoreBuilder::new(here);
    let mut any = false;
    for name in files {
        let Some(text) = read_in(dir, OsStr::new(name), 256 * 1024) else { continue };
        for l in String::from_utf8_lossy(&text).lines().take(10_000) {
            // A line that is no rule is left out, as git does.
            any |= b.add_line(None, l).is_ok();
        }
    }
    any.then(|| b.build().ok()).flatten()
}

pub(crate) fn ignored(rules: &[Gitignore], path: &std::path::Path, dir: bool) -> bool {
    // The nearest rule that speaks decides.
    for r in rules.iter().rev() {
        let m = r.matched(path, dir);
        if m.is_ignore() {
            return true;
        }
        if m.is_whitelist() {
            return false;
        }
    }
    false
}

pub struct Look<'a> {
    /// The folder, and where it is.
    pub dir: &'a Dir,
    pub place: &'a Place,
    pub may: May<'a>,
    pub stop: &'a AtomicBool,
}

impl Look<'_> {
    fn walk(&self) -> Walk<'_> {
        Walk {
            base: self.place.as_path().to_path_buf(),
            may: self.may,
            stop: self.stop,
            began: Instant::now(),
            seen: 0,
            skipped: Skipped::default(),
        }
    }

    /// The folder's own entries; folders end in `/`, links in `@`.
    pub fn list(&self, most: usize) -> String {
        let mut names: Vec<String> = Vec::new();
        let mut kept = 0;
        for e in self.dir.entries().into_iter().flatten().flatten().take(MOST_SEEN) {
            let name = e.file_name();
            let Ok(place) = Place::new(self.place.as_path().join(&name)) else { continue };
            if !(self.may)(&place) {
                kept += 1;
                continue;
            }
            let mark = match e.file_type() {
                Ok(t) if t.is_dir() => "/",
                Ok(t) if t.is_symlink() => "@",
                _ => "",
            };
            names.push(format!("{}{mark}", name.to_string_lossy()));
        }
        if names.is_empty() && kept == 0 {
            return "(the folder is empty)".into();
        }
        names.sort();
        let all = names.len();
        let mut out = String::new();
        let mut n = 0;
        for name in names.iter().take(most) {
            if out.len() + name.len() > TEXT {
                break;
            }
            let _ = writeln!(out, "{name}");
            n += 1;
        }
        let more = all - n;
        if more > 0 {
            let _ = writeln!(out, "({more} more)");
        }
        Skipped { kept, ..Skipped::default() }.say(&mut out);
        out
    }

    /// Files whose path below the folder fits `pattern` (`*`, `**`, `?`, `[…]`, `{…}`).
    pub fn glob(&self, pattern: &str, most: usize) -> Result<String, String> {
        let set = glob(pattern)?;
        let (mut out, mut n, mut more) = (String::new(), 0, 0);
        let mut w = self.walk();
        w.files(self.dir, &mut |_, _, _, rel| {
            if set.is_match(rel) {
                if n < most && out.len() + rel.len() < TEXT {
                    let _ = writeln!(out, "{rel}");
                    n += 1;
                } else {
                    more += 1;
                }
            }
            true
        });
        if n == 0 {
            out.push_str("(no file fits)\n");
        }
        if more > 0 {
            let _ = writeln!(out, "({more} more)");
        }
        w.skipped.say(&mut out);
        Ok(out)
    }

    /// Lines that fit the regular expression `pattern`, in files that fit `only`.
    pub fn search(
        &self,
        pattern: &str,
        only: Option<&str>,
        around: usize,
        most: usize,
    ) -> Result<String, String> {
        let re = regex::bytes::RegexBuilder::new(pattern)
            .multi_line(true)
            .size_limit(4 * 1024 * 1024)
            .build()
            .map_err(|e| format!("the pattern cannot be used: {e}"))?;
        let only = only.map(glob).transpose()?;
        let around = around.min(20);
        let (mut out, mut n, mut files) = (String::new(), 0, 0);
        let mut unread = 0;
        let mut w = self.walk();
        w.files(self.dir, &mut |dir, name, _, rel| {
            if only.as_ref().is_some_and(|g| !g.is_match(rel)) {
                return true;
            }
            let Some(text) = read_in(dir, name, LARGEST) else {
                unread += 1;
                return true;
            };
            if text[..text.len().min(8192)].contains(&0) {
                unread += 1;
                return true;
            }
            let lines: Vec<&[u8]> = text.split(|b| *b == b'\n').collect();
            let last = if text.ends_with(b"\n") { lines.len() - 1 } else { lines.len() };
            let mut shown = 0;
            let mut any = false;
            for i in 0..last {
                if n >= most || out.len() > TEXT {
                    break;
                }
                if !re.is_match(lines[i]) {
                    continue;
                }
                if !any {
                    files += 1;
                    any = true;
                }
                n += 1;
                let from = i.saturating_sub(around).max(shown);
                let to = (i + around + 1).min(last);
                if around > 0 && from > shown && shown > 0 {
                    out.push_str("--\n");
                }
                for (j, l) in lines.iter().enumerate().take(to).skip(from) {
                    let mark = if re.is_match(l) { ':' } else { '-' };
                    let l = String::from_utf8_lossy(l);
                    let l = l.trim_end_matches('\r');
                    let cut = l.char_indices().nth(SHOWN).map_or(l.len(), |c| c.0);
                    let dots = if cut < l.len() { "…" } else { "" };
                    let _ = writeln!(out, "{rel}{mark}{}{mark}{}{dots}", j + 1, &l[..cut]);
                }
                shown = to;
            }
            n < most && out.len() <= TEXT
        });
        let long = out.len() > TEXT;
        w.skipped.unread = unread;
        if n == 0 {
            out.push_str("(nothing found)\n");
        } else {
            let _ = writeln!(out, "({n} lines in {files} files)");
        }
        if (n >= most || long) && w.skipped.ended.is_none() {
            let _ = writeln!(out, "(there may be more: only the first {n} are shown)");
        }
        w.skipped.say(&mut out);
        Ok(out)
    }
}

/// A pattern without a `/` fits a name in any folder.
fn glob(pattern: &str) -> Result<ignore::overrides::Override, String> {
    if pattern.len() > 1000 {
        return Err("the pattern is too long".into());
    }
    let mut b = ignore::overrides::OverrideBuilder::new("/");
    b.add(pattern.trim_start_matches(['!', '/']))
        .map_err(|e| format!("the pattern cannot be used: {e}"))?;
    b.build().map_err(|e| format!("the pattern cannot be used: {e}"))
}

trait Fits {
    fn is_match(&self, rel: &str) -> bool;
}
impl Fits for ignore::overrides::Override {
    fn is_match(&self, rel: &str) -> bool {
        self.matched(rel, false).is_whitelist()
    }
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use cap_std::ambient_authority;

    struct T {
        d: tempfile::TempDir,
        dir: Dir,
        place: Place,
        stop: AtomicBool,
    }
    fn t(files: &[(&str, &str)]) -> T {
        let d = tempfile::tempdir().unwrap();
        for (p, c) in files {
            let p = d.path().join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, c).unwrap();
        }
        let real = std::fs::canonicalize(d.path()).unwrap();
        T {
            dir: Dir::open_ambient_dir(&real, ambient_authority()).unwrap(),
            place: Place::new(real).unwrap(),
            d,
            stop: AtomicBool::new(false),
        }
    }
    impl T {
        fn look<'a>(&'a self, may: May<'a>) -> Look<'a> {
            Look { dir: &self.dir, place: &self.place, may, stop: &self.stop }
        }
    }
    const ALL: May<'static> = &|_| true;

    #[test]
    fn lists_what_is_there() {
        let t = t(&[("b", ""), ("a/x", ""), (".h", "")]);
        std::os::unix::fs::symlink("b", t.d.path().join("l")).unwrap();
        assert_eq!(t.look(ALL).list(10), ".h\na/\nb\nl@\n");
        assert_eq!(t.look(ALL).list(2), ".h\na/\n(2 more)\n");
        assert_eq!(
            t.look(&|p| p.name() != "b").list(10),
            ".h\na/\nl@\n(1 not looked at: not allowed)\n"
        );
        let e = self::t(&[]);
        assert_eq!(e.look(ALL).list(10), "(the folder is empty)");
    }

    #[test]
    fn finds_names() {
        let t = t(&[
            ("a.rs", ""),
            ("src/b.rs", ""),
            ("src/deep/c.rs", ""),
            ("src/d.txt", ""),
            ("target/e.rs", ""),
            (".git/f.rs", ""),
            (".hid/g.rs", ""),
            (".gitignore", "/target\n*.log\n"),
            ("src/.gitignore", "!x.log\n"),
            ("src/x.log", ""),
            ("y.log", ""),
        ]);
        let g = |p| t.look(ALL).glob(p, 100).unwrap();
        assert_eq!(g("*.rs"), ".hid/g.rs\na.rs\nsrc/b.rs\nsrc/deep/c.rs\n");
        assert_eq!(g("src/*.rs"), "src/b.rs\n");
        assert_eq!(g("src/**/*.rs"), "src/b.rs\nsrc/deep/c.rs\n");
        assert_eq!(g("**/*.{txt,log}"), "src/d.txt\nsrc/x.log\n");
        assert_eq!(g("nothing"), "(no file fits)\n");
        assert_eq!(t.look(ALL).glob("*.rs", 1).unwrap(), ".hid/g.rs\n(3 more)\n");
        assert!(t.look(ALL).glob("a[", 1).is_err());
        assert!(t.look(ALL).glob(&"a".repeat(2000), 1).is_err());
    }

    #[test]
    fn finds_words() {
        let t = t(&[
            ("a.rs", "one\ntwo needle\nthree\nfour\nfive needle\n"),
            ("b.txt", "needle\r\n"),
            ("bin", "needle\0"),
            ("n/.env", "needle secret"),
        ]);
        let may: May<'_> = &|p| p.name() != ".env";
        let s = |p, o, a| t.look(may).search(p, o, a, 100).unwrap();
        assert_eq!(
            s("needle", None, 0),
            "a.rs:2:two needle\na.rs:5:five needle\nb.txt:1:needle\n(3 lines in 2 files)\n(1 not looked at: not allowed)\n(1 not searched: too large or not text)\n"
        );
        assert!(
            s("^needle$", None, 0).starts_with("(nothing found)"),
            "a carriage return is part of the line"
        );
        assert!(s("^one$", Some("*.rs"), 1).starts_with("a.rs:1:one\na.rs-2-two needle\n(1 lines"));
        assert!(s("needle", Some("*.rs"), 1).starts_with(
            "a.rs-1-one\na.rs:2:two needle\na.rs-3-three\na.rs-4-four\na.rs:5:five needle\n(2 lines in 1 files)"
        ));
        assert!(
            t.look(may)
                .search("needle", None, 0, 1)
                .unwrap()
                .contains("only the first 1 are shown")
        );
        assert!(t.look(may).search("(", None, 0, 1).is_err());
        assert!(t.look(may).search("((a{1000}){1000}){1000}", None, 0, 1).is_err());
    }

    #[test]
    fn follows_no_link_and_stops_when_told() {
        let t = t(&[("a/f", "needle")]);
        let out = self::t(&[("g", "needle")]);
        std::os::unix::fs::symlink(out.d.path(), t.d.path().join("l")).unwrap();
        std::os::unix::fs::symlink(out.d.path().join("g"), t.d.path().join("m")).unwrap();
        std::os::unix::fs::symlink("..", t.d.path().join("a/up")).unwrap();
        assert_eq!(
            t.look(ALL).search("needle", None, 0, 9).unwrap(),
            "a/f:1:needle\n(1 lines in 1 files)\n"
        );
        t.stop.store(true, Ordering::Relaxed);
        assert!(t.look(ALL).search("needle", None, 0, 9).unwrap().contains("ended early: stopped"));
    }

    #[test]
    fn long_lines_are_cut() {
        let t = t(&[("a", &format!("{}needle", "é".repeat(400)))]);
        let s = t.look(ALL).search("needle", None, 0, 9).unwrap();
        assert!(s.lines().next().unwrap().ends_with("é…"), "{s}");
    }
}
