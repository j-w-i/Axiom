//! Places in the file system, as policy sees them. A string that could name two places, or that a
//! file system would read differently from us, is not a place.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BadPath {
    #[error("not an absolute path")]
    Relative,
    #[error("the path goes up with ..")]
    Up,
    #[error("the path is not plain text")]
    NotText,
    #[error("the path has a character no file name should have")]
    Character,
    #[error("the path is of a kind that is not used here")]
    Kind,
    #[error("the path is too long")]
    Long,
}

/// An absolute path without `.`, `..` or doubled separators. Lexical: whether it is the real name
/// of what it leads to is for whoever opens it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Place(PathBuf);

const LONGEST: usize = 4096;

impl Place {
    pub fn new(p: impl AsRef<Path>) -> Result<Self, BadPath> {
        let p = p.as_ref();
        let s = p.to_str().ok_or(BadPath::NotText)?;
        if s.len() > LONGEST {
            return Err(BadPath::Long);
        }
        if s.chars().any(char::is_control) {
            return Err(BadPath::Character);
        }
        if !p.is_absolute() {
            return Err(BadPath::Relative);
        }
        let mut out = PathBuf::new();
        for c in p.components() {
            match c {
                Component::ParentDir => return Err(BadPath::Up),
                Component::CurDir => {}
                Component::Normal(n) => {
                    if cfg!(windows) {
                        plain_windows_name(n.to_str().ok_or(BadPath::NotText)?)?;
                    }
                    out.push(n);
                }
                Component::RootDir => out.push(c),
                Component::Prefix(x) => {
                    if !matches!(x.kind(), std::path::Prefix::Disk(_)) {
                        return Err(BadPath::Kind);
                    }
                    out.push(c);
                }
            }
        }
        Ok(Self(out))
    }

    /// `path` as someone standing here means it: absolute paths as they are, others below here.
    /// `..` is worked out on the text and must not leave the root of the file system.
    pub fn join(&self, path: &str) -> Result<Self, BadPath> {
        let p = Path::new(path);
        let mut out = if p.is_absolute() { PathBuf::new() } else { self.0.clone() };
        for c in p.components() {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !out.pop() {
                        return Err(BadPath::Up);
                    }
                }
                c => out.push(c),
            }
        }
        Self::new(out)
    }

    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Always text: checked when it was made.
    pub fn as_str(&self) -> &str {
        self.0.to_str().unwrap_or_default()
    }

    /// Whether `other` is this place or below it, by whole names.
    pub fn covers(&self, other: &Place) -> bool {
        other.0.starts_with(&self.0)
    }

    /// As [`Self::covers`], for things that must be kept away from: a file system that takes `A` for
    /// `a` must not let `.SSH` by.
    pub fn covers_folded(&self, other: &Place) -> bool {
        let (mut a, mut b) = (self.0.components(), other.0.components());
        loop {
            match (a.next(), b.next()) {
                (None, _) => return true,
                (Some(_), None) => return false,
                (Some(x), Some(y)) => {
                    let (x, y) = (x.as_os_str().to_string_lossy(), y.as_os_str().to_string_lossy());
                    if fold(&x) != fold(&y) {
                        return false;
                    }
                }
            }
        }
    }

    pub fn parent(&self) -> Option<Place> {
        self.0.parent().map(|p| Self(p.to_path_buf()))
    }

    pub fn name(&self) -> &str {
        self.0.file_name().and_then(|n| n.to_str()).unwrap_or_default()
    }
}

impl<'de> Deserialize<'de> for Place {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(PathBuf::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Names that Windows reads as something else: streams, devices, names with their end dropped,
/// short names.
fn plain_windows_name(n: &str) -> Result<(), BadPath> {
    let stem = n.split('.').next().unwrap_or(n).trim_end().to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || (stem.chars().count() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.ends_with(|c: char| c.is_ascii_digit() || matches!(c, '¹' | '²' | '³')));
    let short = n.rfind('~').is_some_and(|i| {
        let after = n[i + 1..].split('.').next().unwrap_or_default();
        !after.is_empty() && after.bytes().all(|b| b.is_ascii_digit())
    });
    if device || short || n.ends_with(['.', ' ']) || n.contains([':', '<', '>', '"', '|', '?', '*'])
    {
        return Err(BadPath::Kind);
    }
    Ok(())
}

/// A name as the most forgiving file system would compare it: case folded (with the two letters
/// whose fold is ASCII), and without the code points some file systems ignore.
pub fn fold(name: &str) -> String {
    name.chars()
        .filter(
            |c| !matches!(*c as u32, 0x200C..=0x200F | 0x202A..=0x202E | 0x206A..=0x206F | 0xFEFF),
        )
        .flat_map(|c| match c {
            '\u{17F}' => 's'.to_lowercase(),
            '\u{212A}' => 'k'.to_lowercase(),
            c => c.to_lowercase(),
        })
        .collect()
}

/// Why a name is never written by an agent, wherever it is and whether or not it exists (SI-11):
/// something on this machine would execute it, or take orders from it, outside any sandbox.
pub fn protected_name(name: &str) -> Option<&'static str> {
    let n = fold(name);
    PROTECTED
        .iter()
        .find(|(p, _)| *p == n)
        .map(|(_, why)| *why)
        .or_else(|| PROTECTED_ENDINGS.iter().find(|(p, _)| n.ends_with(p)).map(|(_, why)| *why))
}

/// The first protected name on the way down to `place`.
pub fn protected(place: &Place) -> Option<&'static str> {
    place.as_path().components().find_map(|c| match c {
        Component::Normal(n) => protected_name(n.to_str()?),
        _ => None,
    })
}

const VCS: &str = "version control runs what is written there";
const AGENT: &str = "an agent takes orders from it";
const EDITOR: &str = "an editor runs what is written there";
const SHELL: &str = "a shell runs what is written there";
const LOGIN: &str = "it is for signing in";

/// Folded: lower case.
pub const PROTECTED: &[(&str, &str)] = &[
    (".git", VCS),
    (".gitmodules", VCS),
    (".gitconfig", VCS),
    (".hg", VCS),
    (".hgrc", VCS),
    (".svn", VCS),
    (".jj", VCS),
    (".husky", VCS),
    (".axiom", AGENT),
    (".agents", AGENT),
    (".claude", AGENT),
    (".claude.json", AGENT),
    (".codex", AGENT),
    (".copilot", AGENT),
    (".cursor", AGENT),
    (".cursorrules", AGENT),
    (".gemini", AGENT),
    (".antigravity", AGENT),
    (".goose", AGENT),
    (".opencode", AGENT),
    (".windsurf", AGENT),
    (".mcp.json", AGENT),
    (".vscode", EDITOR),
    (".idea", EDITOR),
    (".zed", EDITOR),
    (".fleet", EDITOR),
    (".nvim.lua", EDITOR),
    (".exrc", EDITOR),
    (".envrc", SHELL),
    (".bashrc", SHELL),
    (".bash_profile", SHELL),
    (".bash_login", SHELL),
    (".bash_logout", SHELL),
    (".profile", SHELL),
    (".zshrc", SHELL),
    (".zshenv", SHELL),
    (".zprofile", SHELL),
    (".zlogin", SHELL),
    (".zlogout", SHELL),
    (".ripgreprc", SHELL),
    (".ssh", LOGIN),
    (".gnupg", LOGIN),
    (".aws", LOGIN),
    (".netrc", LOGIN),
];

pub const PROTECTED_ENDINGS: &[(&str, &str)] = &[(".code-workspace", EDITOR)];

/// What a person should see in a diff before anything outside the sandbox runs it: it executes when
/// someone builds, tests or installs.
pub fn review_name(name: &str) -> bool {
    const NAMES: &[&str] = &[
        "makefile",
        "gnumakefile",
        "justfile",
        "package.json",
        "build.rs",
        "setup.py",
        "conftest.py",
        "pyproject.toml",
        "cargo.toml",
        "config.toml",
        "rakefile",
        "gemfile",
        "dockerfile",
        "cmakelists.txt",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        ".gitattributes",
        ".npmrc",
        ".yarnrc.yml",
        ".pre-commit-config.yaml",
        "podfile",
        "agents.md",
        "claude.md",
        "gemini.md",
    ];
    NAMES.contains(&fold(name).as_str())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;
    #[cfg(unix)]
    use proptest::prelude::*;

    #[cfg(unix)]
    fn p(s: &str) -> Place {
        Place::new(s).unwrap()
    }

    #[test]
    #[cfg(unix)]
    fn only_one_way_to_write_a_place() {
        assert_eq!(p("/a//b/./c/").as_str(), "/a/b/c");
        for (bad, why) in [
            ("a/b", BadPath::Relative),
            ("", BadPath::Relative),
            ("/a/../b", BadPath::Up),
            ("/a/b\0c", BadPath::Character),
            ("/a/b\nc", BadPath::Character),
        ] {
            assert_eq!(Place::new(bad), Err(why), "{bad:?}");
        }
        assert_eq!(Place::new(format!("/{}", "a".repeat(5000))), Err(BadPath::Long));
        assert!(serde_json::from_str::<Place>("\"/a/../b\"").is_err());
        use std::os::unix::ffi::OsStrExt;
        let raw = std::ffi::OsStr::from_bytes(b"/a/\xff");
        assert_eq!(Place::new(raw), Err(BadPath::NotText));
    }

    #[test]
    #[cfg(unix)]
    fn joining_works_out_the_way_up() {
        let w = p("/w/x");
        assert_eq!(w.join("a/../b").unwrap(), p("/w/x/b"));
        assert_eq!(w.join("../y").unwrap(), p("/w/y"));
        assert_eq!(w.join("/etc/x").unwrap(), p("/etc/x"));
        assert_eq!(w.join("../../..").unwrap_err(), BadPath::Up);
        assert_eq!(w.join("").unwrap(), w);
    }

    #[test]
    #[cfg(unix)]
    fn covering_is_by_whole_names() {
        assert!(p("/a/b").covers(&p("/a/b/c")));
        assert!(p("/a/b").covers(&p("/a/b")));
        assert!(!p("/a/b").covers(&p("/a/bc")));
        assert!(!p("/a/b").covers(&p("/a")));
        assert!(!p("/h/.ssh").covers(&p("/h/.SSH/id")));
        assert!(p("/h/.ssh").covers_folded(&p("/h/.SSH/id")));
        assert!(!p("/h/.ssh/x").covers_folded(&p("/h/.SSH")));
    }

    #[test]
    fn protected_however_it_is_spelled() {
        for n in [
            ".git",
            ".GIT",
            ".Git",
            ".v\u{17F}code",
            ".hu\u{17F}\u{212A}y",
            ".g\u{200C}it",
            "x.Code-Workspace",
        ] {
            assert!(protected_name(n).is_some(), "{n:?}");
        }
        for n in ["git", ".gitignore", ".github", "x.git", "src", "code-workspace"] {
            assert_eq!(protected_name(n), None, "{n}");
        }
        for (n, _) in PROTECTED.iter().chain(PROTECTED_ENDINGS) {
            assert_eq!(*n, fold(n), "the list is folded");
        }
    }

    #[test]
    #[cfg(unix)]
    fn protected_anywhere_on_the_way() {
        assert!(protected(&p("/w/sub/.git/hooks/pre-commit")).is_some());
        assert!(protected(&p("/w/.vscode")).is_some());
        assert!(protected(&p("/w/src/main.rs")).is_none());
        assert!(review_name("Makefile") && review_name("package.json") && !review_name("main.rs"));
    }

    #[test]
    fn names_windows_reads_as_something_else() {
        for n in [
            "con",
            "NUL.txt",
            "com1",
            "LPT9.x",
            "a.",
            "a ",
            "a:b",
            "GIT~1",
            "PROGRA~12.X",
            "con .x",
        ] {
            assert_eq!(plain_windows_name(n), Err(BadPath::Kind), "{n:?}");
        }
        for n in ["console", "com", "a~b", "a.b", "com10x", "~", "a~"] {
            assert_eq!(plain_windows_name(n), Ok(()), "{n:?}");
        }
    }

    #[cfg(unix)]
    proptest! {
        #[test]
        fn a_place_is_stable_and_never_goes_up(s in "(/|\\.|\\.\\.|[a-c]| ){0,12}") {
            if let Ok(x) = Place::new(format!("/{s}")) {
                prop_assert_eq!(&Place::new(x.as_path()).unwrap(), &x);
                prop_assert!(!x.as_str().split('/').any(|c| c == ".." || c == "."));
                prop_assert!(p("/").covers(&x));
            }
        }

        #[test]
        fn joining_agrees_with_the_shell(parts in prop::collection::vec("(\\.\\.|\\.|[a-b])", 0..8)) {
            let mut want: Vec<&str> = vec!["r"];
            let mut left = false;
            for c in &parts {
                match c.as_str() {
                    "." => {}
                    ".." => left |= want.pop().is_none(),
                    c => want.push(c),
                }
            }
            let got = p("/r").join(&parts.join("/"));
            if left {
                prop_assert_eq!(got, Err(BadPath::Up));
            } else {
                prop_assert_eq!(got.unwrap().to_string(), format!("/{}", want.join("/")));
            }
        }
    }
}
