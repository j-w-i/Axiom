//! Folders as a person has named and set them up (W-36): a name, a pin, what a new session there
//! begins with, and commands. Kept in Axiom's own folder, said only by a client with the token, and
//! never read from the folder itself (SI-5). A folder is known by its real path and by what the
//! system says it is (device and file number): another folder at the same path is not taken for it
//! (T2), and until its person says so again nothing kept of it is used.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
};

use axiom_proto::{Access, ErrorCode, Execution, FolderCommand, FolderInfo, WireError, now_ms};
use serde::{Deserialize, Serialize};

use crate::{
    connections::{bad, internal},
    paths::private_file,
};

pub const MOST: usize = 500;
pub const COMMANDS: usize = 20;
const NAME: usize = 100;
const COMMAND: usize = 4096;
const ICONS: &[&str] = &["play", "terminal", "check", "list", "web", "branch", "file", "refresh"];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Kept {
    #[serde(flatten)]
    info: FolderInfo,
    /// What the system said the folder is when it was set up.
    #[serde(default)]
    dev: u64,
    #[serde(default)]
    ino: u64,
}

pub struct Folders {
    file: PathBuf,
    all: Mutex<Vec<Kept>>,
}

/// What a person says of a folder.
#[derive(Debug, Default)]
pub struct Said {
    pub path: String,
    pub name: Option<String>,
    pub pinned: bool,
    pub execution: Option<Execution>,
    pub access: Option<Access>,
    pub commands: Vec<FolderCommand>,
}

/// What the system says a folder is.
fn identity(p: &Path) -> io::Result<(u64, u64)> {
    let m = std::fs::metadata(p)?;
    if !m.is_dir() {
        return Err(io::Error::other("it is not a folder"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok((m.dev(), m.ino()))
    }
    #[cfg(not(unix))]
    Ok((0, 0))
}

fn plain(s: &str, most: usize, what: &str) -> Result<(), WireError> {
    if s.trim().is_empty() || s.len() > most || s.chars().any(char::is_control) {
        return Err(bad(format!("{what} must be a line of at most {most} bytes")));
    }
    Ok(())
}

impl Folders {
    /// As they were kept. A file that cannot be read is an error: nothing is written over it.
    pub fn open(file: &Path) -> io::Result<Self> {
        let all = match std::fs::read_to_string(file) {
            Ok(t) => serde_json::from_str(&t)
                .map_err(|e| io::Error::other(format!("{}: {e}", file.display())))?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(Self { file: file.into(), all: Mutex::new(all) })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Kept>> {
        self.all.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self, all: &[Kept]) -> Result<(), WireError> {
        let text = serde_json::to_string_pretty(all).map_err(internal)?;
        private_file(&self.file, &text).map_err(internal)
    }

    fn shown(k: &Kept) -> FolderInfo {
        let now = identity(Path::new(&k.info.path)).ok();
        FolderInfo { moved: now.is_some_and(|n| n != (k.dev, k.ino)), ..k.info.clone() }
    }

    pub fn list(&self) -> Vec<FolderInfo> {
        self.lock().iter().map(Self::shown).collect()
    }

    /// All of it said anew. `path` is whole, as sessions name the folder.
    pub fn set(&self, s: Said) -> Result<FolderInfo, WireError> {
        let (dev, ino) =
            identity(Path::new(&s.path)).map_err(|e| bad(format!("{}: {e}", s.path)))?;
        if let Some(n) = &s.name {
            plain(n, NAME, "a name")?;
        }
        if s.commands.len() > COMMANDS {
            return Err(bad(format!("at most {COMMANDS} commands")));
        }
        let mut keys = Vec::new();
        for c in &s.commands {
            plain(&c.name, NAME, "a command's name")?;
            if c.command.trim().is_empty() || c.command.len() > COMMAND || c.command.contains('\0')
            {
                return Err(bad(format!("a command must be at most {COMMAND} bytes")));
            }
            if let Some(i) = &c.icon
                && !ICONS.contains(&i.as_str())
            {
                return Err(bad(format!("no such icon: {i}")));
            }
            if let Some(k) = c.key {
                if !(1..=9).contains(&k) || keys.contains(&k) {
                    return Err(bad("a command's key is 1 to 9, each once"));
                }
                keys.push(k);
            }
        }
        let mut all = self.lock();
        let at = all.iter().position(|k| k.info.path == s.path);
        if at.is_none() && all.len() >= MOST {
            return Err(bad(format!("at most {MOST} folders are kept")));
        }
        let id = at.map_or_else(
            || format!("f_{:011x}{:08x}", now_ms(), ino as u32),
            |i| all[i].info.id.clone(),
        );
        let k = Kept {
            info: FolderInfo {
                id,
                path: s.path,
                name: s.name.map(|n| n.trim().to_owned()),
                pinned: s.pinned,
                execution: s.execution,
                access: s.access,
                commands: s.commands,
                moved: false,
            },
            dev,
            ino,
        };
        let shown = k.info.clone();
        let mut next = all.clone();
        match at {
            Some(i) => next[i] = k,
            None => next.push(k),
        }
        self.write(&next)?;
        *all = next;
        Ok(shown)
    }

    pub fn remove(&self, id: &str) -> Result<(), WireError> {
        let mut all = self.lock();
        let next: Vec<Kept> = all.iter().filter(|k| k.info.id != id).cloned().collect();
        if next.len() == all.len() {
            return Err(WireError::new(ErrorCode::NotFound, "no such folder"));
        }
        self.write(&next)?;
        *all = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn said(path: &Path) -> Said {
        Said { path: path.to_str().unwrap().into(), ..Said::default() }
    }

    #[test]
    fn a_folder_is_kept_by_what_it_is() {
        let d = tempfile::tempdir().unwrap();
        let w = std::fs::canonicalize(d.path()).unwrap().join("w");
        std::fs::create_dir(&w).unwrap();
        let file = d.path().join("folders.json");
        let f = Folders::open(&file).unwrap();
        let cmd = |name: &str, key| FolderCommand {
            name: name.into(),
            command: "npm run dev".into(),
            icon: Some("play".into()),
            key,
        };
        let one = f
            .set(Said {
                name: Some(" Web ".into()),
                pinned: true,
                access: Some(Access::Ask),
                commands: vec![cmd("Dev", Some(1)), cmd("Test", None)],
                ..said(&w)
            })
            .unwrap();
        assert_eq!((one.name.as_deref(), one.pinned, one.commands.len()), (Some("Web"), true, 2));
        // Kept, and the same one when it is said again.
        let again = Folders::open(&file).unwrap();
        assert_eq!(again.list(), std::slice::from_ref(&one));
        let two = again.set(said(&w)).unwrap();
        assert_eq!((two.id == one.id, two.name, two.pinned), (true, None, false));
        // Another folder at the same path is not it.
        std::fs::remove_dir(&w).unwrap();
        std::fs::create_dir(d.path().join("other")).unwrap();
        std::fs::rename(d.path().join("other"), &w).unwrap();
        assert!(again.list()[0].moved);
        assert!(!again.set(said(&w)).unwrap().moved, "said again, it is this one");
        again.remove(&one.id).unwrap();
        assert!(again.list().is_empty());
        assert_eq!(again.remove(&one.id).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn what_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let w = std::fs::canonicalize(d.path()).unwrap();
        let f = Folders::open(&d.path().join("folders.json")).unwrap();
        let c = |name: &str, command: &str, icon: Option<&str>, key| FolderCommand {
            name: name.into(),
            command: command.into(),
            icon: icon.map(String::from),
            key,
        };
        for bad in [
            Said { name: Some("a\nb".into()), ..said(&w) },
            Said { name: Some(" ".into()), ..said(&w) },
            Said { commands: vec![c("", "x", None, None)], ..said(&w) },
            Said { commands: vec![c("A", " ", None, None)], ..said(&w) },
            Said { commands: vec![c("A", "x", Some("rocket"), None)], ..said(&w) },
            Said { commands: vec![c("A", "x", None, Some(0))], ..said(&w) },
            Said {
                commands: vec![c("A", "x", None, Some(2)), c("B", "y", None, Some(2))],
                ..said(&w)
            },
            Said { commands: vec![c("A", "x", None, None); COMMANDS + 1], ..said(&w) },
            said(&w.join("missing")),
        ] {
            assert!(f.set(bad).is_err());
        }
        assert!(f.list().is_empty(), "nothing of it was kept");
        std::fs::write(d.path().join("broken.json"), "{").unwrap();
        assert!(Folders::open(&d.path().join("broken.json")).is_err());
    }
}
