//! Where a person's own program is. Looked for in folders that were named as trusted, never in a
//! workspace or anything an agent may write to (SI-22), and started by the name it really has: what
//! was found is what is started.

use axiom_policy::{Place, Reach};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotFound {
    #[error("{0:?} is not the name of a program")]
    Name(String),
    #[error("{0} is not in any of the folders that are looked in")]
    Missing(String),
    #[error("{0} is inside what an agent may write to")]
    Inside(Place),
    #[error("{0} may be changed by anyone on this computer")]
    Open(Place),
    #[error("{0} cannot be used: {1}")]
    Cannot(Place, String),
}

/// A program that may be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program(Place);

impl Program {
    #[cfg(test)]
    pub(crate) fn for_test(p: Place) -> Self {
        Self(p)
    }

    pub fn place(&self) -> &Place {
        &self.0
    }
}

/// The program `name` in the first of `trusted` that has it. `written`: workspaces and everything
/// else agents may write to. One found there is an error and not passed over: it was put there.
pub fn find(name: &str, trusted: &[Place], written: &Reach) -> Result<Program, NotFound> {
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !plain {
        return Err(NotFound::Name(name.to_owned()));
    }
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    for dir in trusted {
        let Ok(at) = dir.join(&file) else { continue };
        // A link that leads nowhere is not there.
        if std::fs::metadata(at.as_path()).is_ok_and(|m| m.is_file()) {
            return take(&at, written);
        }
    }
    Err(NotFound::Missing(name.to_owned()))
}

/// A program a person named with its whole path.
pub fn take(at: &Place, written: &Reach) -> Result<Program, NotFound> {
    let cannot = |why: String| NotFound::Cannot(at.clone(), why);
    let real = std::fs::canonicalize(at.as_path()).map_err(|e| cannot(e.to_string()))?;
    let real = Place::new(real).map_err(|e| cannot(e.to_string()))?;
    for p in [at, &real] {
        if inside(p, written) {
            return Err(NotFound::Inside(p.clone()));
        }
    }
    let m = std::fs::metadata(real.as_path()).map_err(|e| cannot(e.to_string()))?;
    if !m.is_file() {
        return Err(cannot("it is not a file".into()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = m.permissions().mode();
        if mode & 0o111 == 0 {
            return Err(cannot("it is not a program".into()));
        }
        if mode & 0o002 != 0 {
            return Err(NotFound::Open(real));
        }
    }
    Ok(Program(real))
}

/// Also by the names that links in between lead to, and whatever the case.
pub(crate) fn inside(p: &Place, written: &Reach) -> bool {
    written.roots().iter().any(|w| {
        let real = std::fs::canonicalize(w.as_path()).ok().and_then(|r| Place::new(r).ok());
        w.covers_folded(p) || real.is_some_and(|r| r.covers_folded(p))
    })
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    struct Dir(tempfile::TempDir);
    impl Dir {
        fn at(&self, s: &str) -> Place {
            Place::new(std::fs::canonicalize(self.0.path()).unwrap()).unwrap().join(s).unwrap()
        }
        fn program(&self, s: &str, mode: u32) -> Place {
            let at = self.at(s);
            std::fs::create_dir_all(at.as_path().parent().unwrap()).unwrap();
            std::fs::write(at.as_path(), "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(at.as_path(), PermissionsExt::from_mode(mode)).unwrap();
            at
        }
    }
    fn dir() -> Dir {
        let d = Dir(tempfile::tempdir().unwrap());
        for x in ["a", "b", "w"] {
            std::fs::create_dir(d.at(x).as_path()).unwrap();
        }
        d
    }

    #[test]
    fn the_first_folder_that_has_it() {
        let d = dir();
        let b = d.program("b/agent", 0o755);
        let dirs = [d.at("a"), d.at("b")];
        assert_eq!(find("agent", &dirs, &Reach::none()).unwrap().place(), &b);
        let a = d.program("a/agent", 0o755);
        assert_eq!(find("agent", &dirs, &Reach::none()).unwrap().place(), &a);
        let missing = |n: &str| find(n, &dirs, &Reach::none()).unwrap_err();
        assert_eq!(missing("other"), NotFound::Missing("other".into()));
        std::fs::create_dir(d.at("a/folder").as_path()).unwrap();
        symlink(d.at("nowhere").as_path(), d.at("a/dead").as_path()).unwrap();
        for n in ["folder", "dead"] {
            assert_eq!(missing(n), NotFound::Missing(n.into()));
        }
        // No folder is looked in by itself: not the working directory, not PATH.
        assert!(find("sh", &[], &Reach::none()).is_err());
    }

    #[test]
    fn a_name_is_a_name() {
        let d = dir();
        d.program("a/agent", 0o755);
        for n in
            ["", "../a/agent", "/bin/sh", "a/agent", ".", "..", ".agent", "a b", "agent\0", "ä"]
        {
            let e = find(n, &[d.at("a"), d.at("b")], &Reach::none()).unwrap_err();
            assert_eq!(e, NotFound::Name(n.into()), "{n:?}");
        }
    }

    #[test]
    fn nothing_an_agent_could_have_put_there() {
        let d = dir();
        let w = Reach::of([d.at("w")]);
        let planted = d.program("w/bin/agent", 0o755);
        d.program("b/agent", 0o755);
        // In a trusted folder inside the workspace: an error, though another has it too.
        let dirs = [d.at("w/bin"), d.at("b")];
        assert_eq!(find("agent", &dirs, &w).unwrap_err(), NotFound::Inside(planted.clone()));
        // By a link out of a trusted folder.
        symlink(planted.as_path(), d.at("a/agent").as_path()).unwrap();
        assert_eq!(find("agent", &[d.at("a")], &w).unwrap_err(), NotFound::Inside(planted.clone()));
        // The other way: a link inside it that leads out.
        let out = d.program("a/real", 0o755);
        symlink(out.as_path(), d.at("w/link").as_path()).unwrap();
        assert_eq!(take(&d.at("w/link"), &w).unwrap_err(), NotFound::Inside(d.at("w/link")));
        // The workspace itself known by a link, or in another case.
        symlink(d.at("w").as_path(), d.at("way").as_path()).unwrap();
        assert!(take(&planted, &Reach::of([d.at("way")])).is_err());
        assert!(take(&planted, &Reach::of([d.at("W")])).is_err());
        assert!(take(&out, &w).is_ok());
    }

    #[test]
    fn what_is_found_is_what_is_started() {
        let d = dir();
        let real = d.program("b/versions/2/agent", 0o755);
        symlink(real.as_path(), d.at("a/agent").as_path()).unwrap();
        assert_eq!(find("agent", &[d.at("a")], &Reach::none()).unwrap().place(), &real);
    }

    #[test]
    fn a_program_nobody_else_changes() {
        let d = dir();
        let p = d.program("a/plain", 0o644);
        assert!(matches!(take(&p, &Reach::none()), Err(NotFound::Cannot(..))));
        let p = d.program("a/open", 0o757);
        assert_eq!(take(&p, &Reach::none()).unwrap_err(), NotFound::Open(p));
        assert!(matches!(take(&d.at("a"), &Reach::none()), Err(NotFound::Cannot(..))));
        assert!(matches!(take(&d.at("a/none"), &Reach::none()), Err(NotFound::Cannot(..))));
        assert!(take(&d.program("a/group", 0o775), &Reach::none()).is_ok());
    }
}
