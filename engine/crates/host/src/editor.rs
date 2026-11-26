//! The person's own editor, and the system's file manager, for a file of a folder (decision 0019).
//! Started on the one spawn path, outside the sandbox as the person's own act, for their press only.

use std::path::PathBuf;

use axiom_harness::find;
use axiom_policy::{PersonsOwn, Place, Reach, Unsandboxed};
use axiom_proto::{EditorInfo, WireError};
use axiom_spawn::{Contain, Env, Job, spawn};

use crate::agents::bad;

/// How an editor is told a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    /// `-g FILE:LINE`.
    Goto,
    /// `FILE:LINE`.
    Colon,
    /// `--line LINE FILE`.
    Flag,
}

struct Known {
    id: &'static str,
    name: &'static str,
    /// On the path.
    program: &'static str,
    /// Inside an application on macOS, below `/Applications` or `~/Applications`.
    app: Option<&'static str>,
    line: Line,
}

/// From each one's own documentation of its command line (read 2026-11-26).
const KNOWN: &[Known] = &[
    Known {
        id: "vscode",
        name: "Visual Studio Code",
        program: "code",
        app: Some("Visual Studio Code.app/Contents/Resources/app/bin/code"),
        line: Line::Goto,
    },
    Known {
        id: "cursor",
        name: "Cursor",
        program: "cursor",
        app: Some("Cursor.app/Contents/Resources/app/bin/cursor"),
        line: Line::Goto,
    },
    Known {
        id: "windsurf",
        name: "Windsurf",
        program: "windsurf",
        app: Some("Windsurf.app/Contents/Resources/app/bin/windsurf"),
        line: Line::Goto,
    },
    Known {
        id: "vscodium",
        name: "VSCodium",
        program: "codium",
        app: Some("VSCodium.app/Contents/Resources/app/bin/codium"),
        line: Line::Goto,
    },
    Known {
        id: "zed",
        name: "Zed",
        program: "zed",
        app: Some("Zed.app/Contents/MacOS/cli"),
        line: Line::Colon,
    },
    Known {
        id: "sublime",
        name: "Sublime Text",
        program: "subl",
        app: Some("Sublime Text.app/Contents/SharedSupport/bin/subl"),
        line: Line::Colon,
    },
    Known { id: "idea", name: "IntelliJ IDEA", program: "idea", app: None, line: Line::Flag },
    Known { id: "pycharm", name: "PyCharm", program: "pycharm", app: None, line: Line::Flag },
    Known { id: "webstorm", name: "WebStorm", program: "webstorm", app: None, line: Line::Flag },
    Known { id: "goland", name: "GoLand", program: "goland", app: None, line: Line::Flag },
    Known { id: "clion", name: "CLion", program: "clion", app: None, line: Line::Flag },
    Known { id: "rustrover", name: "RustRover", program: "rustrover", app: None, line: Line::Flag },
    Known { id: "phpstorm", name: "PhpStorm", program: "phpstorm", app: None, line: Line::Flag },
    Known { id: "rubymine", name: "RubyMine", program: "rubymine", app: None, line: Line::Flag },
];

/// Where programs are looked for, and what agents may write to: neither may hold one (SI-22).
pub struct Where {
    pub path: Vec<Place>,
    pub home: Place,
    pub written: Reach,
    pub tmp: Place,
    /// Where applications are, on macOS.
    pub apps: Vec<PathBuf>,
}

impl Where {
    fn found(&self, k: &Known) -> Option<Place> {
        if let Ok(p) = find::find(k.program, &self.path, &self.written) {
            return Some(p.place().clone());
        }
        let app = k.app.filter(|_| cfg!(target_os = "macos"))?;
        for root in &self.apps {
            let Ok(at) = Place::new(root.join(app)) else { continue };
            if std::fs::metadata(at.as_path()).is_ok_and(|m| m.is_file())
                && let Ok(p) = find::take(&at, &self.written)
            {
                return Some(p.place().clone());
            }
        }
        None
    }

    /// The editors of the person's that are there, in the order of the list.
    pub fn editors(&self) -> Vec<EditorInfo> {
        KNOWN
            .iter()
            .filter_map(|k| {
                let p = self.found(k)?;
                Some(EditorInfo { id: k.id.into(), name: k.name.into(), path: p.to_string() })
            })
            .collect()
    }

    fn env(&self) -> Result<Env, WireError> {
        let user = ["USER", "LOGNAME", "USERNAME"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .find(|v| !v.is_empty())
            .ok_or_else(|| bad("this computer does not say who uses it"))?;
        Env::harness(&self.path, &self.home, &user, &self.tmp).map_err(|e| bad(e.to_string()))
    }

    /// Opens `file`, a real file the host has found below a folder, in the editor chosen, or the
    /// first there is.
    pub fn open(
        &self,
        file: &Place,
        line: Option<u32>,
        editor: Option<&str>,
    ) -> Result<(), WireError> {
        let k = match editor {
            Some(id) => KNOWN.iter().find(|k| k.id == id).ok_or_else(|| bad("no such editor"))?,
            None => KNOWN
                .iter()
                .find(|k| self.found(k).is_some())
                .ok_or_else(|| bad("none of the editors Axiom knows was found"))?,
        };
        let program = self.found(k).ok_or_else(|| bad(format!("{} was not found", k.name)))?;
        let args = arguments(k.line, file, line);
        start(program, args, self.env()?, &self.home, PersonsOwn::Editor)
    }

    /// Shows `file` in the system's file manager. Nothing is opened (T47).
    pub fn reveal(&self, file: &Place) -> Result<(), WireError> {
        let (program, args) = if cfg!(target_os = "macos") {
            ("/usr/bin/open", vec!["-R".to_owned(), file.to_string()])
        } else if cfg!(windows) {
            ("C:\\Windows\\explorer.exe", vec![format!("/select,{file}")])
        } else {
            let folder = file.parent().ok_or_else(|| bad("it has no folder"))?;
            ("/usr/bin/xdg-open", vec![folder.to_string()])
        };
        let program = Place::new(program).map_err(|e| bad(e.to_string()))?;
        if !program.as_path().is_file() {
            return Err(bad(format!("{program} is not there")));
        }
        start(program, args, self.env()?, &self.home, PersonsOwn::Reveal)
    }
}

/// The file is one argument, made by the host; the line a number.
fn arguments(how: Line, file: &Place, line: Option<u32>) -> Vec<String> {
    let f = file.to_string();
    match (how, line) {
        (Line::Goto, Some(n)) => vec!["-g".into(), format!("{f}:{n}")],
        (Line::Colon, Some(n)) => vec![format!("{f}:{n}")],
        (Line::Flag, Some(n)) => vec!["--line".into(), n.to_string(), f],
        (_, None) => vec![f],
    }
}

fn start(
    program: Place,
    args: Vec<String>,
    env: Env,
    cwd: &Place,
    act: PersonsOwn,
) -> Result<(), WireError> {
    let job = Job {
        program,
        args,
        cwd: cwd.clone(),
        env,
        stdin: false,
        contain: Contain::None(Unsandboxed::persons(act)),
    };
    let mut p = spawn(job).map_err(|e| bad(format!("it could not be started: {e}")))?;
    // What it says is nobody's; it is not waited for, but its end is collected.
    drop(p.stdout.take());
    drop(p.stderr.take());
    tokio::spawn(async move {
        let _ = tokio::time::timeout(std::time::Duration::from_secs(600), p.wait()).await;
    });
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_line_as_each_takes_it() {
        let f = Place::new("/w/a b/-x.rs").unwrap();
        assert_eq!(arguments(Line::Goto, &f, Some(4)), ["-g", "/w/a b/-x.rs:4"]);
        assert_eq!(arguments(Line::Colon, &f, Some(4)), ["/w/a b/-x.rs:4"]);
        assert_eq!(arguments(Line::Flag, &f, Some(4)), ["--line", "4", "/w/a b/-x.rs"]);
        assert_eq!(arguments(Line::Flag, &f, None), ["/w/a b/-x.rs"]);
    }

    #[test]
    fn every_known_one_has_its_own_name() {
        let mut ids: Vec<_> = KNOWN.iter().map(|k| k.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), KNOWN.len());
    }

    #[test]
    fn none_is_found_where_agents_write() {
        let d = tempfile::tempdir().unwrap();
        let bin = std::fs::canonicalize(d.path()).unwrap();
        let code = bin.join("code");
        std::fs::write(&code, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&code, std::fs::Permissions::from_mode(0o755)).unwrap();
        let place = Place::new(&bin).unwrap();
        let w = |written| Where {
            path: vec![place.clone()],
            home: Place::new("/nonexistent-home").unwrap(),
            written,
            tmp: place.clone(),
            apps: vec![],
        };
        let found = w(Reach::none()).editors();
        assert_eq!(found.first().map(|e| e.id.as_str()), Some("vscode"));
        assert!(w(Reach::of([place.clone()])).editors().is_empty(), "it was put there");
    }
}
