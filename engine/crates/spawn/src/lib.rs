//! The only way to start a process (SI-7). What is started is contained by a [`Profile`], or comes
//! with a person's [`Unsandboxed`] token; a sandbox that cannot be made is an error (SI-8).
//!
//! Where: macOS by Seatbelt. Linux by bubblewrap. Windows has no verified backend: everything
//! contained is refused there.

use std::{io, process::Stdio, time::Duration};

use axiom_policy::{BadPath, Place, Profile, Unsandboxed};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout};

#[cfg(target_os = "linux")]
mod bwrap;
#[cfg(unix)]
pub mod reaper;
#[cfg(target_os = "macos")]
mod seatbelt;
#[cfg(any(target_os = "linux", test))]
mod seccomp;
#[cfg(unix)]
mod tree;

/// What is to be started. Everything in it comes from the host; of a model's words only `args` and
/// what is written to `stdin` may hold any.
#[derive(Debug)]
pub struct Job {
    /// Absolute: nothing is looked up.
    pub program: Place,
    pub args: Vec<String>,
    /// Below something the profile reads.
    pub cwd: Place,
    /// Where programs are found, and the rest of the environment. Nothing is inherited (SI-3).
    pub env: Env,
    pub stdin: bool,
    pub contain: Contain,
}

#[derive(Debug)]
pub enum Contain {
    Sandbox(Profile),
    /// The token is spent by being here.
    None(Unsandboxed),
}

/// A whole environment.
#[derive(Debug, Clone)]
pub struct Env {
    vars: Vec<(String, String)>,
}

impl Env {
    /// `path` is where programs are found; `home` and `tmp` as they are.
    pub fn new(path: &[Place], home: &Place, tmp: &Place) -> Self {
        let path: Vec<&str> = path.iter().map(Place::as_str).filter(|p| !p.contains(':')).collect();
        let vars = [
            ("PATH", path.join(":")),
            ("HOME", home.to_string()),
            ("TMPDIR", tmp.to_string()),
            ("LANG", "en_US.UTF-8".into()),
            ("TERM", "dumb".into()),
            ("NO_COLOR", "1".into()),
            ("PAGER", "cat".into()),
            ("GIT_PAGER", "cat".into()),
            ("GIT_TERMINAL_PROMPT", "0".into()),
            // A person's own names programs to run, and may hold what signs them in.
            ("GIT_CONFIG_GLOBAL", "/dev/null".into()),
            ("CI", "1".into()),
        ];
        Self { vars: vars.into_iter().map(|(k, v)| (k.to_string(), v)).collect() }
    }

    /// For a program of a person's own that signs in by itself (an agent harness): what it needs to
    /// find its files and other programs, and nothing that is not here. No key is passed on (SI-3).
    pub fn harness(
        path: &[Place],
        home: &Place,
        user: &str,
        tmp: &Place,
    ) -> Result<Self, SpawnError> {
        if user.is_empty() || user.chars().any(|c| c.is_control() || c == '/') {
            return Err(SpawnError::Env("USER".into()));
        }
        let path: Vec<&str> = path.iter().map(Place::as_str).filter(|p| !p.contains(':')).collect();
        let vars = [
            ("PATH", path.join(":")),
            ("HOME", home.to_string()),
            ("USER", user.to_owned()),
            ("LANG", "en_US.UTF-8".into()),
            ("TMPDIR", tmp.to_string()),
        ];
        Ok(Self { vars: vars.into_iter().map(|(k, v)| (k.to_string(), v)).collect() })
    }

    /// Sets one more. Names that make a loader or a shell run something are refused.
    pub fn set(&mut self, name: &str, value: &str) -> Result<(), SpawnError> {
        let plain =
            !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        let loaded = ["LD_", "DYLD_", "BASH_", "MALLOC_", "__"].iter().any(|p| name.starts_with(p))
            || matches!(name, "ENV" | "IFS" | "SHELLOPTS" | "PS4" | "PROMPT_COMMAND" | "ZDOTDIR");
        if !plain || loaded || value.contains('\0') {
            return Err(SpawnError::Env(name.to_owned()));
        }
        self.vars.retain(|(k, _)| k != name);
        self.vars.push((name.to_owned(), value.to_owned()));
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SpawnError {
    #[error("there is no sandbox on this system: {0}")]
    NoSandbox(String),
    #[error("a network is not available inside the sandbox yet")]
    Network,
    #[error("{0} cannot be opened: {1}")]
    Root(Place, io::Error),
    #[error("{0} leads to a path that is not used here: {1}")]
    Leads(Place, BadPath),
    #[error("{0} is where Axiom, or what signs you in, is kept")]
    Hidden(Place),
    #[error("{0} is outside what the sandbox reads")]
    Cwd(Place),
    #[error("the environment may not hold {0}")]
    Env(String),
    #[error("an argument holds a zero byte")]
    Argument,
    #[error("the process could not be started: {0}")]
    Start(io::Error),
}

/// How a process ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ended {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    /// Processes it had left behind, which were ended with it.
    pub left: usize,
    /// Protected names that appeared while it ran, where the system cannot prevent it (Linux): to be
    /// shown to the person before anything else reads them.
    pub made: Vec<std::path::PathBuf>,
}

/// A running process and all it starts. Dropped, all of it ends.
#[derive(Debug)]
pub struct Running {
    child: Child,
    #[cfg(unix)]
    root: Option<u32>,
    #[cfg(unix)]
    tree: tree::Tree,
    /// Writable folders, with the protected names that were there.
    #[cfg(target_os = "linux")]
    before: Vec<(Place, Vec<std::path::PathBuf>)>,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

/// Starts `job`, or says why not. Nothing is started on an error.
pub fn spawn(job: Job) -> Result<Running, SpawnError> {
    if job.args.iter().any(|a| a.contains('\0')) {
        return Err(SpawnError::Argument);
    }
    #[cfg(target_os = "linux")]
    let before = match &job.contain {
        Contain::Sandbox(p) => bwrap::before(p)?,
        Contain::None(_) => Vec::new(),
    };
    let (program, args) = match &job.contain {
        Contain::Sandbox(p) => contained(&job, p)?,
        Contain::None(_) => (
            std::ffi::OsString::from(job.program.as_path()),
            job.args.iter().map(Into::into).collect::<Vec<_>>(),
        ),
    };
    let mut c = std::process::Command::new(program);
    c.args(args)
        .current_dir(job.cwd.as_path())
        .env_clear()
        .envs(job.env.vars.iter().map(|(k, v)| (k, v)))
        .stdin(if job.stdin { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(target_os = "linux")]
    let filter = match &job.contain {
        Contain::Sandbox(_) => Some(bwrap::filter()?),
        Contain::None(_) => None,
    };
    #[cfg(all(unix, not(target_os = "linux")))]
    let filter = None;
    #[cfg(unix)]
    let mark = tree::prepare(&mut c, filter).map_err(SpawnError::Start)?;
    let mut child =
        tokio::process::Command::from(c).kill_on_drop(true).spawn().map_err(SpawnError::Start)?;
    #[cfg(unix)]
    let tree = tree::Tree::new(child.id().unwrap_or(0), Some(mark));
    #[cfg(unix)]
    reaper::add(child.id());
    Ok(Running {
        stdin: child.stdin.take(),
        stdout: child.stdout.take(),
        stderr: child.stderr.take(),
        #[cfg(unix)]
        root: child.id(),
        #[cfg(unix)]
        tree,
        #[cfg(target_os = "linux")]
        before,
        child,
    })
}

/// The command that runs `job` contained.
fn contained(
    job: &Job,
    p: &Profile,
) -> Result<(std::ffi::OsString, Vec<std::ffi::OsString>), SpawnError> {
    if !p.net().is_empty() {
        return Err(SpawnError::Network);
    }
    let real = Real::of(p)?;
    let cwd = real_place(&job.cwd)?;
    if !real.read.iter().any(|r| r.covers(&cwd))
        || real.hidden.iter().any(|h| h.covers_folded(&cwd))
    {
        return Err(SpawnError::Cwd(job.cwd.clone()));
    }
    #[cfg(target_os = "macos")]
    return Ok(seatbelt::wrap(job, &real));
    #[cfg(target_os = "linux")]
    return bwrap::wrap(job, &real);
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    Err(SpawnError::NoSandbox("none has been verified for it".into()))
}

/// A profile's places as the system names them: what links lead to.
#[cfg_attr(not(any(target_os = "macos", target_os = "linux")), allow(dead_code))]
struct Real {
    read: Vec<Place>,
    write: Vec<Place>,
    hidden: Vec<Place>,
}

fn real_place(p: &Place) -> Result<Place, SpawnError> {
    let c = std::fs::canonicalize(p.as_path()).map_err(|e| SpawnError::Root(p.clone(), e))?;
    Place::new(c).map_err(|e| SpawnError::Leads(p.clone(), e))
}

impl Real {
    fn of(p: &Profile) -> Result<Self, SpawnError> {
        // A name that leads elsewhere is kept beside where it leads; so is one that leads nowhere.
        let mut hidden = p.hidden().to_vec();
        for h in p.hidden() {
            match std::fs::canonicalize(h.as_path()) {
                Ok(c) => {
                    let c = Place::new(c).map_err(|e| SpawnError::Leads(h.clone(), e))?;
                    if !hidden.contains(&c) {
                        hidden.push(c);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(SpawnError::Root(h.clone(), e)),
            }
        }
        let all = |roots: &[Place]| -> Result<Vec<Place>, SpawnError> {
            roots.iter().map(real_place).collect()
        };
        let read = all(p.read().roots())?;
        let write = all(p.write().roots())?;
        for w in &write {
            // What a link leads to is looked at again: the profile saw only its name.
            if w.parent().is_none()
                || hidden.iter().any(|h| h.covers_folded(w) || w.covers_folded(h))
            {
                return Err(SpawnError::Hidden(w.clone()));
            }
        }
        Ok(Self { read, write, hidden })
    }
}

impl Running {
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Waits for the process, then ends whatever it left running.
    pub async fn wait(&mut self) -> io::Result<Ended> {
        drop(self.stdin.take());
        let st = self.child.wait().await?;
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            let left = self.tree.kill();
            #[cfg(target_os = "linux")]
            let made = bwrap::made(&self.before);
            #[cfg(not(target_os = "linux"))]
            let made = Vec::new();
            Ok(Ended { code: st.code(), signal: st.signal(), left, made })
        }
        #[cfg(not(unix))]
        Ok(Ended { code: st.code(), ..Ended::default() })
    }

    /// Asks all of it to end, and after `grace` ends it.
    pub async fn stop(&mut self, grace: Duration) -> io::Result<Ended> {
        #[cfg(unix)]
        self.tree.ask();
        if tokio::time::timeout(grace, self.child.wait()).await.is_err() {
            self.kill();
        }
        self.wait().await
    }

    /// Ends all of it now.
    pub fn kill(&mut self) {
        #[cfg(unix)]
        self.tree.kill();
        // Reaped by `wait`, or by the runtime.
        let _ = self.child.start_kill();
    }

    /// Looks at what it has started; call now and then while it runs, so that what leaves its
    /// process group is known when it has to end.
    pub fn look(&mut self) {
        #[cfg(unix)]
        self.tree.look();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.kill();
        #[cfg(unix)]
        reaper::remove(self.root);
    }
}
