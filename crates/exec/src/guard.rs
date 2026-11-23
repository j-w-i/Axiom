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
//! Without [`install`] (tests that use an `Executor` directly) nothing is sent; [`is_watched`]
//! still answers.
//!
//! Registering never blocks the caller (async code): the lines go through a channel to a thread
//! of their own, which does the writes, in order (review N13). A caller about to hand a new
//! process to a client waits (bounded, off the async thread) with [`flushed`] until its
//! registration is in the pipe, so a daemon killed right after the reply still takes it along.

use std::collections::BTreeMap;
use std::sync::{mpsc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// Lines queued so far (counted under [`WATCHED`]'s lock, with the queueing) and lines the
/// pipe's thread has written (`u64::MAX` once the watchdog is gone: nothing left to wait for).
static QUEUED: Mutex<u64> = Mutex::new(0);
static WRITTEN: (Mutex<u64>, Condvar) = (Mutex::new(0), Condvar::new());

#[cfg(unix)]
fn written_up_to(n: u64) {
    let (m, cv) = &WRITTEN;
    *m.lock().unwrap_or_else(|e| e.into_inner()) = n;
    cv.notify_all();
}

struct Guard {
    lines: Mutex<mpsc::Sender<String>>,
    /// Kept so the watchdog's handle lives as long as the daemon.
    _child: std::process::Child,
}

/// The groups registered and not yet ended, as the watchdog sees them, each with how many
/// owners registered it: a number reused while an earlier owner had not yet unregistered it
/// (theoretical: it is unregistered once reaped) stays watched until both have (review D5).
static WATCHED: Mutex<BTreeMap<u32, usize>> = Mutex::new(BTreeMap::new());

static GUARD: OnceLock<Guard> = OnceLock::new();

/// Starts the watchdog: `argv` runs [`run_watchdog`] (the daemon passes its own executable with
/// a hidden flag). Its stdout and stderr go nowhere: the daemon's stdout is the protocol
/// channel. Called once; later calls do nothing.
#[cfg(unix)]
pub fn install(argv: &[std::ffi::OsString]) -> std::io::Result<()> {
    use std::io::Write;
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
    let mut pipe = child
        .stdin
        .take()
        .ok_or_else(|| std::io::Error::other("no pipe to the watchdog"))?;
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("axiom-watchdog-pipe".into())
        .spawn(move || {
            let mut n = 0u64;
            for line in rx {
                // A watchdog that is gone cannot help; the daemon carries on.
                if pipe.write_all(line.as_bytes()).is_err() {
                    written_up_to(u64::MAX);
                    return;
                }
                n += 1;
                written_up_to(n);
            }
        })?;
    let _ = GUARD.set(Guard {
        lines: Mutex::new(tx),
        _child: child,
    });
    Ok(())
}

/// No watchdog off Unix.
#[cfg(not(unix))]
pub fn install(_argv: &[std::ffi::OsString]) -> std::io::Result<()> {
    Ok(())
}

/// Queues a line for the watchdog: never waits for the pipe. Called with [`WATCHED`] held, so
/// the lines are queued in the order the set changed.
fn send(line: String) {
    if let Some(g) = GUARD.get() {
        let tx = g.lines.lock().unwrap_or_else(|e| e.into_inner());
        if tx.send(line).is_ok() {
            *QUEUED.lock().unwrap_or_else(|e| e.into_inner()) += 1;
        }
    }
}

/// Waits until every line queued so far is in the watchdog's pipe, or `timeout` passes; true if
/// it is (or there is no watchdog). Blocks: from async code, use [`flushed`].
pub fn flush(timeout: Duration) -> bool {
    if GUARD.get().is_none() {
        return true;
    }
    let target = *QUEUED.lock().unwrap_or_else(|e| e.into_inner());
    let (m, cv) = &WRITTEN;
    let written = m.lock().unwrap_or_else(|e| e.into_inner());
    let (written, _) = cv
        .wait_timeout_while(written, timeout, |w| *w < target)
        .unwrap_or_else(|e| e.into_inner());
    *written >= target
}

/// [`flush`] off the async thread: called before a reply hands a client a process just
/// registered (a terminal, a task), so a daemon killed right after it still takes the process
/// along (CI on 1840dc9: a terminal created just before the daemon was killed outlived it).
pub async fn flushed(timeout: Duration) -> bool {
    tokio::task::spawn_blocking(move || flush(timeout))
        .await
        .unwrap_or(false)
}

/// Registers a process group (its leader's pid) to end if the daemon dies. Call it while the
/// leader has not been reaped (its number cannot have been reused yet). Each call needs its own
/// [`unwatch_group`].
pub fn watch_group(pgid: u32) {
    if pgid > 1 {
        let mut w = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
        let owners = w.entry(pgid).or_insert(0);
        *owners += 1;
        if *owners == 1 {
            send(format!("+{pgid}\n"));
        }
    }
}

/// The group ended normally: one owner forgets it, and the watchdog once the last one has.
pub fn unwatch_group(pgid: u32) {
    if pgid > 1 {
        let mut w = WATCHED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(owners) = w.get_mut(&pgid) {
            *owners -= 1;
            if *owners == 0 {
                w.remove(&pgid);
                send(format!("-{pgid}\n"));
            }
        }
    }
}

/// Whether the group is registered (and not unregistered since): what the watchdog would
/// signal if the daemon died now. For tests and diagnostics.
pub fn is_watched(pgid: u32) -> bool {
    WATCHED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains_key(&pgid)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Review D5: two owners of one group number (a number reused before the first owner
    /// unregistered it): one owner's unregister leaves it watched for the other.
    #[test]
    fn a_group_stays_watched_until_every_owner_has_unregistered_it() {
        let g = 3_999_999;
        watch_group(g);
        watch_group(g);
        unwatch_group(g);
        assert!(is_watched(g), "one owner's unregister dropped the other's");
        unwatch_group(g);
        assert!(!is_watched(g));
        // An unregister with no owner left changes nothing.
        unwatch_group(g);
        watch_group(g);
        assert!(is_watched(g));
        unwatch_group(g);
        assert!(!is_watched(g));
    }
}
