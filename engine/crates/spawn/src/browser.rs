//! The browser of decision 0021: a Chromium the person has installed, headless, spoken to over the
//! DevTools pipe (what it reads is descriptor 3, what it writes 4), never over a port. It is found
//! only where the system keeps programs, so the token it is started with starts nothing else.

use std::{
    fs::File,
    io,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    path::Path,
    process::Stdio,
};

use axiom_policy::{Place, Unsandboxed};

use crate::{Env, Running, SpawnError, reaper, tree};

/// A Chromium found where the system keeps programs; made only by [`chromium`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chromium(Place);

impl Chromium {
    pub fn path(&self) -> &Place {
        &self.0
    }
}

#[cfg(target_os = "macos")]
const PLACES: &[&str] = &[
    "Google Chrome.app/Contents/MacOS/Google Chrome",
    "Chromium.app/Contents/MacOS/Chromium",
    "Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "Brave Browser.app/Contents/MacOS/Brave Browser",
];

#[cfg(not(target_os = "macos"))]
const PLACES: &[&str] = &[
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/usr/bin/microsoft-edge",
    "/usr/bin/brave-browser",
    "/opt/google/chrome/chrome",
];

/// The first Chromium there is: on macOS in the system's applications or `home`'s.
pub fn chromium(home: Option<&Place>) -> Option<Chromium> {
    #[cfg(target_os = "macos")]
    let all: Vec<std::path::PathBuf> = {
        let mut roots = vec![std::path::PathBuf::from("/Applications")];
        roots.extend(home.map(|h| h.as_path().join("Applications")));
        roots.iter().flat_map(|r| PLACES.iter().map(move |p| r.join(p))).collect()
    };
    #[cfg(not(target_os = "macos"))]
    let all: Vec<std::path::PathBuf> = {
        let _ = home;
        PLACES.iter().map(std::path::PathBuf::from).collect()
    };
    all.into_iter().filter(|p| Path::new(p).is_file()).find_map(|p| {
        // What it is, not a link to something an agent could have written.
        let real = std::fs::canonicalize(&p).ok()?;
        Place::new(real).ok().map(Chromium)
    })
}

/// A started browser: what it is told goes to `to`, what it says comes from `from`, each message
/// ended by a zero byte.
#[derive(Debug)]
pub struct Piped {
    pub running: Running,
    pub to: File,
    pub from: File,
}

/// Starts `c` with `args` in `cwd`. What it says, it says over the pipe; its errors are `stderr`.
#[allow(unsafe_code)]
pub fn spawn_browser(
    c: &Chromium,
    args: &[String],
    cwd: &Place,
    env: &Env,
) -> Result<Piped, SpawnError> {
    if args.iter().any(|a| a.contains('\0')) {
        return Err(SpawnError::Argument);
    }
    // Ours: write commands into `to`, read from `from`. Theirs become 3 and 4.
    let (their_in, to) = io::pipe().map_err(SpawnError::Start)?;
    let (from, their_out) = io::pipe().map_err(SpawnError::Start)?;
    let (their_in, their_out) = (OwnedFd::from(their_in), OwnedFd::from(their_out));
    let _token = Unsandboxed::browser();
    let mut cmd = std::process::Command::new(c.0.as_path());
    cmd.args(args)
        .current_dir(cwd.as_path())
        .env_clear()
        .envs(env.vars.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // What it says when it cannot start; read by whoever started it, a little at most.
        .stderr(Stdio::piped());
    let mark = tree::prepare(&mut cmd, None).map_err(SpawnError::Start)?;
    // SAFETY: async-signal-safe calls on integers, after `prepare`'s own. Both are first put out of
    // the way, for either may be 3 or 4 already; `dup2` leaves what it makes open at the start.
    unsafe {
        cmd.pre_exec(move || {
            let high = |fd: i32| {
                let n = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 300);
                if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n) }
            };
            let (i, o) = (high(their_in.as_raw_fd())?, high(their_out.as_raw_fd())?);
            if libc::dup2(i, 3) < 0 || libc::dup2(o, 4) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child =
        tokio::process::Command::from(cmd).kill_on_drop(true).spawn().map_err(SpawnError::Start)?;
    let tree = tree::Tree::new(child.id().unwrap_or(0), Some(mark));
    reaper::add(child.id());
    let running = Running {
        stdin: None,
        stdout: None,
        stderr: child.stderr.take(),
        root: child.id(),
        tree,
        #[cfg(target_os = "linux")]
        before: Vec::new(),
        child,
    };
    Ok(Piped { running, to: File::from(OwnedFd::from(to)), from: File::from(OwnedFd::from(from)) })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::io::{Read, Write};

    use super::*;

    /// What is started reads 3 and writes 4, as Chromium does; a program of the test's stands in.
    #[tokio::test]
    async fn what_it_is_told_on_3_it_answers_on_4() {
        let tmp = Place::new(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let sh = Chromium(Place::new("/bin/sh").unwrap());
        let path = [Place::new("/usr/bin").unwrap(), Place::new("/bin").unwrap()];
        let args = ["-c".to_owned(), "read x <&3; printf \"got %s\\0\" \"$x\" >&4".to_owned()];
        let mut p = spawn_browser(&sh, &args, &tmp, &Env::new(&path, &tmp, &tmp)).unwrap();
        p.to.write_all(b"hello\n").unwrap();
        let mut said = Vec::new();
        let mut from = p.from;
        let said = tokio::task::spawn_blocking(move || {
            from.read_to_end(&mut said).unwrap();
            said
        })
        .await
        .unwrap();
        assert_eq!(said, b"got hello\0");
        assert_eq!(p.running.wait().await.unwrap().code, Some(0));
    }
}
