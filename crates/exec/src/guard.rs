//! The daemon-exit watchdog (AUD-027): a small process that outlives a daemon killed with
//! SIGKILL, or one that crashed, and ends the process groups the daemon was running (tasks,
//! terminals), so they do not linger as orphans holding ports and memory.
//!
//! The daemon starts the watchdog once ([`install`]) as its own child in its own process group,
//! with a pipe as the watchdog's stdin. The daemon writes `+<pgid>` when a group starts and
//! `-<pgid>` when it ends. When the pipe reaches end of file the daemon is gone (however it
//! ended: the kernel closes the pipe): the watchdog sends SIGTERM and SIGHUP to every group
//! still registered, SIGKILL to those still there a second later, and exits. Unix only; on
//! other platforms [`install`] does nothing (Windows tasks are in kill-on-close job objects).
//!
//! Without [`install`] (tests that use an `Executor` directly) registration is a no-op.

use std::io::Write;
use std::sync::{Mutex, OnceLock};

struct Guard {
    pipe: Mutex<std::process::ChildStdin>,
    /// Kept so the watchdog's handle lives as long as the daemon.
    _child: std::process::Child,
}

static GUARD: OnceLock<Guard> = OnceLock::new();

/// Starts the watchdog: `argv` runs [`run_watchdog`] (the daemon passes its own executable with
/// a hidden flag). Its stdout and stderr go nowhere: the daemon's stdout is the protocol
/// channel. Called once; later calls do nothing.
#[cfg(unix)]
pub fn install(argv: &[std::ffi::OsString]) -> std::io::Result<()> {
    use std::os::unix::process::CommandExt;
    if GUARD.get().is_some() {
        return Ok(());
    }
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::other("empty watchdog command"))?;
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        // Its own group: a Ctrl-C to the daemon's foreground group must not take it too.
        .process_group(0)
        .spawn()?;
    let pipe = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no pipe to the watchdog"))?;
    let _ = GUARD.set(Guard {
        pipe: Mutex::new(pipe),
        _child: child,
    });
    Ok(())
}

/// No watchdog off Unix.
#[cfg(not(unix))]
pub fn install(_argv: &[std::ffi::OsString]) -> std::io::Result<()> {
    Ok(())
}

fn send(line: String) {
    if let Some(g) = GUARD.get() {
        let mut p = g.pipe.lock().unwrap_or_else(|e| e.into_inner());
        // A watchdog that is gone cannot help; the daemon carries on.
        let _ = p.write_all(line.as_bytes());
    }
}

/// Registers a process group (its leader's pid) to end if the daemon dies.
pub fn watch_group(pgid: u32) {
    if pgid > 1 {
        send(format!("+{pgid}\n"));
    }
}

/// The group ended normally: forget it.
pub fn unwatch_group(pgid: u32) {
    if pgid > 1 {
        send(format!("-{pgid}\n"));
    }
}

/// The watchdog's main: reads registrations from stdin until end of file, then ends every group
/// still registered. Returns when done (the caller exits).
#[cfg(unix)]
pub fn run_watchdog() {
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    use std::io::BufRead;
    let mut groups = std::collections::BTreeSet::new();
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let (op, n) = line.split_at(line.len().min(1));
        let Ok(pgid) = n.trim().parse::<i32>() else {
            continue;
        };
        match op {
            "+" => {
                groups.insert(pgid);
            }
            "-" => {
                groups.remove(&pgid);
            }
            _ => {}
        }
    }
    if groups.is_empty() {
        return;
    }
    let alive = |g: i32| killpg(Pid::from_raw(g), None).is_ok();
    for &g in &groups {
        let _ = killpg(Pid::from_raw(g), Signal::SIGHUP);
        let _ = killpg(Pid::from_raw(g), Signal::SIGTERM);
    }
    let end = std::time::Instant::now() + std::time::Duration::from_secs(1);
    while std::time::Instant::now() < end && groups.iter().any(|g| alive(*g)) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    for &g in &groups {
        let _ = killpg(Pid::from_raw(g), Signal::SIGKILL);
    }
}

/// No watchdog off Unix.
#[cfg(not(unix))]
pub fn run_watchdog() {}
