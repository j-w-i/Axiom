//! A folder as its person looks at it: a file read, what a folder holds, files found by name, lines
//! found in them. Read by handle below the folder (SI-9), never what is kept from everybody (SI-12),
//! written never (decision 0020). Off the runtime's threads, each with limits.

use std::sync::atomic::AtomicBool;

use axiom_policy::Place;
use axiom_proto::{FileEntry, FileList, FileText, Found, Hit, WireError};
use axiom_tools::{
    Site,
    fs::{self, FsError, Kind},
    walk::{self, Look},
};

use crate::agents::bad;

/// Of a file at all: larger ones are said to be there, and nothing else.
const READ: u64 = 16 * 1024 * 1024;
/// Of text, sent.
const TEXT: usize = 2 * 1024 * 1024;
/// Of a picture, sent.
const IMAGE: usize = 4 * 1024 * 1024;
/// Names looked through for one that fits.
const NAMES: usize = 200_000;
/// Names that fit, sent.
const FITS: usize = 50;
/// Lines found, sent.
const HITS: usize = 2000;
/// Entries of one folder.
const ENTRIES: usize = 5000;

/// Where a person looks, and as whom: the folder, or an agent's copy of it.
pub struct Here {
    pub site: Site,
}

/// A path below the folder, as a person or a link in an answer gives it: `/` or `\` between names,
/// none of them `..`, nothing that leads above or elsewhere. `""` is the folder itself.
pub fn below(root: &Place, path: &str) -> Result<Place, WireError> {
    let path = path.trim_start_matches("./");
    if path.len() > 4096 || path.contains('\0') {
        return Err(bad("that is no path of the folder"));
    }
    let mut at = root.as_path().to_path_buf();
    for n in path.split(['/', '\\']).filter(|n| !n.is_empty() && *n != ".") {
        if n == ".." || n.contains(':') && cfg!(windows) {
            return Err(bad("only what is in the folder can be looked at here"));
        }
        at.push(n);
    }
    if path.starts_with(['/', '\\']) {
        // Whole, it must be inside.
        let whole = Place::new(path).map_err(|e| bad(e.to_string()))?;
        return if root.covers(&whole) {
            Ok(whole)
        } else {
            Err(bad("only what is in the folder can be looked at here"))
        };
    }
    Place::new(at).map_err(|e| bad(e.to_string()))
}

fn said(e: FsError) -> WireError {
    bad(e.to_string())
}

/// The path below the root, with `/`.
fn shown(root: &Place, p: &Place) -> String {
    let rel = p.as_path().strip_prefix(root.as_path()).unwrap_or(p.as_path());
    let names: Vec<_> = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect();
    names.join("/")
}

impl Here {
    fn root(&self) -> &Place {
        &self.site.workspace
    }

    /// What nobody reads: Axiom's own, and what signs a person in.
    fn may(&self, p: &Place) -> bool {
        !self.site.keep.hides(p)
    }

    fn spot(&self, path: &str) -> Result<fs::Spot, WireError> {
        let at = below(self.root(), path)?;
        let spot = fs::find(std::slice::from_ref(self.root()), &at).map_err(said)?;
        // Where it really is, after links inside the folder, counts.
        if !self.may(&spot.place) {
            return Err(bad("that is kept from being read"));
        }
        Ok(spot)
    }

    /// A file there, for another program to open: real, plain, below the folder.
    pub fn file(&self, path: &str) -> Result<Place, WireError> {
        let spot = self.spot(path)?;
        match spot.kind {
            Kind::File => Ok(spot.place),
            Kind::Missing => Err(said(FsError::Missing(path.to_owned()))),
            _ => Err(bad(format!("{path} is not a file"))),
        }
    }

    pub fn read(&self, path: &str) -> Result<FileText, WireError> {
        let spot = self.spot(path)?;
        let path = shown(self.root(), &spot.place);
        match spot.kind {
            Kind::File => {}
            Kind::Missing => return Err(said(FsError::Missing(path))),
            _ => return Err(bad(format!("{path} is not a file"))),
        }
        let c = match spot.read(READ) {
            Ok(c) => c,
            Err(FsError::Large(_, size)) => {
                return Ok(FileText { path, size, kind: "other".into(), ..FileText::default() });
            }
            Err(e) => return Err(said(e)),
        };
        let size = c.bytes.len() as u64;
        let mut t = FileText { path, size, digest: c.digest, whole: true, ..FileText::default() };
        if let Some(kind) = picture(&c.bytes) {
            t.kind = "image".into();
            if c.bytes.len() <= IMAGE {
                t.image = Some(kind.into());
                t.bytes = Some(base64(&c.bytes));
            } else {
                t.whole = false;
            }
        } else if c.bytes[..c.bytes.len().min(8192)].contains(&0) {
            t.kind = "other".into();
        } else {
            t.kind = "text".into();
            let mut end = c.bytes.len().min(TEXT);
            // Not in the middle of a character.
            while end < c.bytes.len() && end > 0 && (c.bytes[end] & 0xC0) == 0x80 {
                end -= 1;
            }
            t.whole = end == c.bytes.len();
            t.text = Some(String::from_utf8_lossy(&c.bytes[..end]).into_owned());
        }
        Ok(t)
    }

    pub fn list(&self, path: &str) -> Result<FileList, WireError> {
        let spot = self.spot(path)?;
        let path = shown(self.root(), &spot.place);
        let dir = spot.folder().map_err(said)?;
        let (mut entries, mut hidden) = (Vec::new(), 0);
        let mut whole = true;
        for e in dir.entries().map_err(|e| bad(e.to_string()))?.flatten() {
            let name = e.file_name();
            let Some(name) = name.to_str() else { continue };
            if name == ".git" {
                continue;
            }
            let Ok(p) = spot.place.join(name) else { continue };
            if !self.may(&p) {
                hidden += 1;
                continue;
            }
            if entries.len() >= ENTRIES {
                whole = false;
                break;
            }
            let folder = e.file_type().is_ok_and(|t| t.is_dir());
            entries.push(FileEntry { name: name.to_owned(), folder });
        }
        // Folders first, then by name as a person reads it.
        entries.sort_by(|a, b| {
            b.folder.cmp(&a.folder).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(FileList { path, entries, hidden, whole })
    }

    fn look<T>(&self, f: impl FnOnce(&Look<'_>) -> T) -> Result<T, WireError> {
        let spot = self.spot("")?;
        let dir = spot.folder().map_err(said)?;
        let stop = AtomicBool::new(false);
        let may = |p: &Place| self.may(p);
        Ok(f(&Look { dir: &dir, place: self.root(), may: &may, stop: &stop }))
    }

    /// Paths that fit what is typed, the best first.
    pub fn find(&self, words: &str) -> Result<Vec<String>, WireError> {
        let words = words.trim();
        if words.len() > 300 {
            return Err(bad("that is too long to look for"));
        }
        let (names, _) = self.look(|l| l.names(NAMES))?;
        Ok(best(&names, words, FITS))
    }

    pub fn search(
        &self,
        words: &str,
        case: bool,
        word: bool,
        expression: bool,
    ) -> Result<Found, WireError> {
        let re = walk::pattern(words, case, word, expression).map_err(bad)?;
        let (hits, files, skipped) = self.look(|l| l.hits(&re, HITS))?;
        let hits = hits.into_iter().map(|h| Hit { path: h.path, line: h.line, text: h.text });
        Ok(Found {
            hits: hits.collect(),
            files,
            unread: skipped.unread,
            whole: skipped.ended.is_none(),
            ended: skipped.ended.map(str::to_owned),
        })
    }
}

/// What kind of picture it is, by its first bytes; none that is drawn by a program (SVG).
pub fn picture(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpeg")
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        Some("gif")
    } else if b.len() >= 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some("webp")
    } else if b.starts_with(b"BM") && b.len() > 26 {
        Some("bmp")
    } else {
        None
    }
}

fn base64(b: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= c.len() {
                out.push(char::from(A[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// How well a path fits words typed for it: each word, in its letters in order, somewhere in it;
/// better in the file's own name, at the beginning of a name, and in letters that follow each other.
/// None if it does not fit.
fn fit(path: &str, words: &str) -> Option<i64> {
    let lower = path.to_lowercase();
    let name_at = lower.rfind('/').map_or(0, |i| i + 1);
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    let mut score = 0i64;
    for w in words.to_lowercase().split_whitespace() {
        // The best of: from the file's own name, or from anywhere.
        let mut best = None;
        for from in [name_at, 0] {
            let mut s = 0i64;
            let mut last: Option<usize> = None;
            let mut k = chars.iter().position(|(b, _)| *b >= from)?;
            let mut ok = true;
            for wc in w.chars() {
                let Some(j) = chars[k..].iter().position(|(_, c)| *c == wc) else {
                    ok = false;
                    break;
                };
                let j = k + j;
                let at = chars[j].0;
                let begins = j == 0 || matches!(chars[j - 1].1, '/' | '_' | '-' | '.' | ' ');
                s += 1;
                if begins {
                    s += 8;
                }
                if last.is_some_and(|l| l + 1 == j) {
                    s += 5;
                }
                if at >= name_at {
                    s += 3;
                }
                last = Some(j);
                k = j + 1;
            }
            if ok {
                best = best.max(Some(s));
            }
        }
        score += best?;
    }
    // Of two that fit as well, the shorter.
    Some(score * 1000 - path.len() as i64)
}

fn best(names: &[String], words: &str, most: usize) -> Vec<String> {
    if words.is_empty() {
        return names.iter().take(most).cloned().collect();
    }
    let mut fits: Vec<(i64, &String)> =
        names.iter().filter_map(|n| fit(n, words).map(|s| (s, n))).collect();
    fits.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    fits.into_iter().take(most).map(|(_, n)| n.clone()).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn names_that_fit_come_best_first() {
        let names: Vec<String> = [
            "src/server.rs",
            "docs/serving.md",
            "src/files.rs",
            "engine/crates/host/src/server.rs",
            "app/lib/shell/find.dart",
        ]
        .map(String::from)
        .to_vec();
        assert_eq!(
            best(&names, "server", 9)[..2],
            ["src/server.rs", "engine/crates/host/src/server.rs"]
        );
        assert_eq!(best(&names, "srv", 9).first().unwrap(), "src/server.rs");
        assert_eq!(best(&names, "shell find", 9), ["app/lib/shell/find.dart"]);
        assert_eq!(best(&names, "fid", 9), ["app/lib/shell/find.dart"]);
        assert_eq!(best(&names, "fls", 9), ["src/files.rs"]);
        assert!(best(&names, "zz", 9).is_empty());
        assert_eq!(best(&names, "", 2).len(), 2);
        assert_eq!(best(&names, "FILES", 9), ["src/files.rs"]);
    }

    #[test]
    fn pictures_by_what_they_begin_with() {
        assert_eq!(picture(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(picture(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpeg"));
        assert_eq!(picture(b"GIF89a.."), Some("gif"));
        assert_eq!(picture(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(picture(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"), None);
        assert_eq!(picture(b"BM"), None);
        assert_eq!(picture(b""), None);
    }

    #[test]
    fn base64_as_everybody_has_it() {
        for (i, o) in
            [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg==")]
        {
            assert_eq!(base64(i.as_bytes()), o);
        }
        assert_eq!(base64(&[0xFF, 0xFE, 0x00]), "//4A");
    }

    #[cfg(unix)]
    #[test]
    fn nothing_above_the_folder() {
        let root = Place::new("/w/p").unwrap();
        assert_eq!(below(&root, "a/b.rs").unwrap().as_str(), "/w/p/a/b.rs");
        assert_eq!(below(&root, "./a//b.rs").unwrap().as_str(), "/w/p/a/b.rs");
        assert_eq!(below(&root, "").unwrap().as_str(), "/w/p");
        assert_eq!(below(&root, "/w/p/x").unwrap().as_str(), "/w/p/x");
        for out in ["../x", "a/../../x", "/w/q/x", "/etc/passwd", "a\0b"] {
            assert!(below(&root, out).is_err(), "{out}");
        }
    }
}
