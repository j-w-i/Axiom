//! Ends what a host that was killed had started. The host starts it as a process of its own (its own
//! program with [`ARG`]) and tells it what it starts; when the pipe between them ends, whatever is
//! still running is ended. On Linux the sandbox's own processes also end with their parent.

use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    os::unix::process::CommandExt,
    path::Path,
    process::{ChildStdin, Command, Stdio},
    sync::{Mutex, PoisonError},
};

use crate::tree::Tree;

pub const ARG: &str = "--reap";

/// The host's end of the pipe.
static REAPER: Mutex<Option<ChildStdin>> = Mutex::new(None);

/// Starts it. Once for a host.
pub fn start(program: &Path) -> io::Result<()> {
    let mut child = Command::new(program)
        .arg(ARG)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // What is sent to the host's group is not meant for it.
        .process_group(0)
        .spawn()?;
    *REAPER.lock().unwrap_or_else(PoisonError::into_inner) = child.stdin.take();
    // It ends when this process does; nobody waits for it before.
    Ok(())
}

fn tell(what: char, pid: Option<u32>) {
    let mut r = REAPER.lock().unwrap_or_else(PoisonError::into_inner);
    if let (Some(w), Some(pid)) = (r.as_mut(), pid)
        && writeln!(w, "{what}{pid}").is_err()
    {
        *r = None;
    }
}

pub(crate) fn add(pid: Option<u32>) {
    tell('+', pid);
}
pub(crate) fn remove(pid: Option<u32>) {
    tell('-', pid);
}

/// The reaper itself: what its process does until the host is gone.
pub fn run(from: impl BufRead) {
    let mut all: BTreeMap<u32, Tree> = BTreeMap::new();
    for l in from.lines() {
        // Half a line is the host's last word.
        let Ok(l) = l else { break };
        let Some((what, pid)) = l.split_at_checked(1) else { continue };
        let Ok(pid) = pid.parse::<u32>() else { continue };
        match what {
            "+" => drop(all.insert(pid, Tree::new(pid, None))),
            _ => drop(all.remove(&pid)),
        }
        // What they have started meanwhile is looked for as often as anything is said.
        for t in all.values_mut() {
            t.look();
        }
    }
    for t in all.values_mut() {
        t.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_still_there_is_ended_and_nothing_else() {
        let sleep = || Command::new("sleep").arg("60").process_group(0).spawn().unwrap();
        let (mut a, mut b, mut c) = (sleep(), sleep(), sleep());
        let words = format!("+{}\n+{}\nnonsense\n+x\n\n-{}\n+{}", a.id(), b.id(), b.id(), c.id());
        run(words.as_bytes());
        // Waited for, or it would be a zombie to look at.
        assert!(!a.wait().unwrap().success());
        assert!(b.try_wait().unwrap().is_none(), "it had ended, as far as was known");
        assert!(!c.wait().unwrap().success());
        b.kill().unwrap();
        b.wait().unwrap();
    }
}
