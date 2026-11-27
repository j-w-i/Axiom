//! A pseudo-terminal for a person's own shell (decision 0022). The program is the leader of a session
//! of its own whose controlling terminal it is, so that it and all it starts end with it.

use std::{
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::process::CommandExt,
    },
    process::Stdio,
};

use crate::{Contain, Job, Running, SpawnError, reaper, tree};

/// The side of the terminal the host holds: what is read from it is what the program wrote.
#[derive(Debug)]
pub struct Pty {
    master: OwnedFd,
}

#[allow(unsafe_code)]
fn size(fd: i32, cols: u16, rows: u16) -> io::Result<()> {
    let w = libc::winsize { ws_row: rows, ws_col: cols, ws_xpixel: 0, ws_ypixel: 0 };
    // SAFETY: a plain system call on a descriptor this owns, with a value on the stack.
    let r = unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &raw const w) };
    if r < 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

impl Pty {
    /// For reading what the program writes; blocks.
    pub fn reader(&self) -> io::Result<File> {
        Ok(File::from(self.master.try_clone()?))
    }

    /// For typing into it.
    pub fn writer(&self) -> io::Result<File> {
        Ok(File::from(self.master.try_clone()?))
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        size(self.master.as_raw_fd(), cols.max(1), rows.max(1))
    }

    /// The group that has the terminal now: the shell's own while it waits for a command.
    #[allow(unsafe_code)]
    pub fn foreground(&self) -> Option<u32> {
        // SAFETY: a plain system call on a descriptor this owns.
        let g = unsafe { libc::tcgetpgrp(self.master.as_raw_fd()) };
        u32::try_from(g).ok().filter(|g| *g > 0)
    }
}

/// Starts `job` on a terminal of `cols` by `rows`. Only a person's own program: never contained, so
/// it takes the token (SI-7). Its standard output and error are the terminal.
#[allow(unsafe_code)]
pub fn spawn_terminal(job: Job, cols: u16, rows: u16) -> Result<(Running, Pty), SpawnError> {
    if job.args.iter().any(|a| a.contains('\0')) {
        return Err(SpawnError::Argument);
    }
    if !matches!(job.contain, Contain::None(_)) {
        return Err(SpawnError::NoSandbox("a terminal is a person's own".into()));
    }
    let (mut master, mut slave) = (-1, -1);
    // SAFETY: the out-parameters are integers on the stack; no name or settings are passed.
    let r = unsafe {
        libc::openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if r < 0 {
        return Err(SpawnError::Start(io::Error::last_os_error()));
    }
    // SAFETY: openpty gave both, and nothing else owns them.
    let (master, slave) = unsafe { (OwnedFd::from_raw_fd(master), OwnedFd::from_raw_fd(slave)) };
    // SAFETY: plain calls on a descriptor this owns: what the program starts does not inherit it.
    unsafe {
        let flags = libc::fcntl(master.as_raw_fd(), libc::F_GETFD);
        libc::fcntl(master.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC);
    }
    size(master.as_raw_fd(), cols.max(1), rows.max(1)).map_err(SpawnError::Start)?;
    let mut c = std::process::Command::new(job.program.as_path());
    c.args(&job.args)
        .current_dir(job.cwd.as_path())
        .env_clear()
        .envs(job.env.vars.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::from(slave.try_clone().map_err(SpawnError::Start)?))
        .stdout(Stdio::from(slave.try_clone().map_err(SpawnError::Start)?))
        .stderr(Stdio::from(slave));
    let mark = tree::prepare(&mut c, None).map_err(SpawnError::Start)?;
    // SAFETY: async-signal-safe; after `prepare`'s own, which has made it a session's leader.
    unsafe {
        c.pre_exec(|| {
            if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                return Err(io::Error::last_os_error());
            }
            // What the host ignores is not the shell's to ignore: a shell cannot take back a signal
            // that was ignored when it began, and Control-C would do nothing.
            for s in [
                libc::SIGINT,
                libc::SIGQUIT,
                libc::SIGTSTP,
                libc::SIGTTIN,
                libc::SIGTTOU,
                libc::SIGHUP,
            ] {
                libc::signal(s, libc::SIG_DFL);
            }
            Ok(())
        });
    }
    let child =
        tokio::process::Command::from(c).kill_on_drop(true).spawn().map_err(SpawnError::Start)?;
    let tree = tree::Tree::new(child.id().unwrap_or(0), Some(mark));
    reaper::add(child.id());
    let running = Running {
        stdin: None,
        stdout: None,
        stderr: None,
        root: child.id(),
        tree,
        #[cfg(target_os = "linux")]
        before: Vec::new(),
        child,
    };
    Ok((running, Pty { master }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::io::{Read, Write};

    use axiom_policy::{PersonsOwn, Place, Unsandboxed};

    use super::*;
    use crate::Env;

    fn shell(script: &str) -> Job {
        let tmp = Place::new(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        let path = [Place::new("/usr/bin").unwrap(), Place::new("/bin").unwrap()];
        Job {
            program: Place::new("/bin/sh").unwrap(),
            args: vec!["-c".into(), script.into()],
            cwd: tmp.clone(),
            env: Env::new(&path, &tmp, &tmp),
            stdin: true,
            contain: Contain::None(Unsandboxed::persons(PersonsOwn::Terminal)),
        }
    }

    #[tokio::test]
    async fn a_program_has_a_terminal_of_its_size_and_is_typed_into() {
        let (mut p, t) =
            spawn_terminal(shell("stty size; read x; echo \"got $x\""), 91, 17).unwrap();
        let mut r = t.reader().unwrap();
        let mut w = t.writer().unwrap();
        w.write_all(b"hello\n").unwrap();
        let said = tokio::task::spawn_blocking(move || {
            let mut all = Vec::new();
            let mut b = [0; 256];
            // Until the program has ended and the terminal says so.
            while let Ok(n) = r.read(&mut b) {
                if n == 0 {
                    break;
                }
                all.extend_from_slice(&b[..n]);
                if String::from_utf8_lossy(&all).contains("got hello") {
                    break;
                }
            }
            String::from_utf8_lossy(&all).into_owned()
        })
        .await
        .unwrap();
        assert!(said.contains("17 91"), "{said:?}");
        assert!(said.contains("got hello"), "{said:?}");
        assert_eq!(p.wait().await.unwrap().code, Some(0));
    }

    #[tokio::test]
    async fn what_runs_in_front_is_known_and_ends_with_it() {
        let (mut p, t) = spawn_terminal(shell("sleep 64"), 80, 24).unwrap();
        let root = p.id().unwrap();
        let mut front = None;
        for _ in 0..100 {
            front = t.foreground();
            if front.is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(front, Some(root), "the shell leads its own group");
        t.resize(120, 40).unwrap();
        p.kill();
        let _ = p.wait().await;
        assert!(crate::tree::usage(root).is_none());
    }

    #[test]
    fn a_sandbox_is_not_a_terminal() {
        let mut j = shell("true");
        let caps = axiom_policy::Caps { run: true, ..axiom_policy::Caps::default() };
        let keep = axiom_policy::Keep::new(Place::new("/nonexistent").unwrap(), vec![]);
        let tmp = Place::new("/tmp").unwrap();
        let profile =
            axiom_policy::Profile::of(&caps, &keep, &axiom_policy::Reach::none(), &tmp).unwrap();
        j.contain = Contain::Sandbox(profile);
        assert!(matches!(spawn_terminal(j, 80, 24), Err(SpawnError::NoSandbox(_))));
    }
}
