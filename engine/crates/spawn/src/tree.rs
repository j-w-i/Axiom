//! A process and everything it starts. It is the leader of a session and a group of its own; what
//! leaves the group is known from its parents as long as it is looked for, and at the end from a
//! mark that is passed on: an open pipe. What closes every file it got and loses its parent before
//! anyone looked is missed on macOS; it is contained all the same.

use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    os::unix::{
        io::{AsRawFd, OwnedFd},
        process::CommandExt,
    },
    time::{Duration, Instant},
};

/// What is ended is waited for this long, and after that this long elsewhere.
const SOON: Duration = Duration::from_secs(1);
const LATE: Duration = Duration::from_secs(30);

/// Where the mark is in what is started: high enough for no shell to use it.
const MARK: i32 = 200;

#[derive(Debug)]
pub struct Tree {
    root: i32,
    /// Process and when it started: numbers are used again.
    known: BTreeMap<i32, u64>,
    /// Our end of the mark. The reaper has none.
    mark: Option<OwnedFd>,
    done: bool,
}

/// Makes what `c` starts a session of its own, with the mark and no other open file.
#[allow(unsafe_code)]
pub fn prepare(c: &mut std::process::Command) -> io::Result<OwnedFd> {
    let (ours, theirs) = io::pipe()?;
    let theirs = OwnedFd::from(theirs);
    // SAFETY: only calls that are async-signal-safe, on integers; nothing is allocated. `theirs`
    // lives in the closure, which lives in the command.
    unsafe {
        c.pre_exec(move || {
            if libc::setsid() < 0 || libc::dup2(theirs.as_raw_fd(), MARK) < 0 {
                return Err(io::Error::last_os_error());
            }
            let none = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            libc::setrlimit(libc::RLIMIT_CORE, &raw const none);
            close_on_exec();
            Ok(())
        });
    }
    Ok(OwnedFd::from(ours))
}

/// Every file after the first three and besides the mark closes when the program starts. Not at
/// once: the standard library still reports a failed start through one of them.
#[allow(unsafe_code)]
fn close_on_exec() {
    // SAFETY: plain system calls.
    unsafe {
        #[cfg(target_os = "linux")]
        {
            const CLOEXEC: libc::c_uint = 4;
            let a = libc::syscall(libc::SYS_close_range, 3, MARK - 1, CLOEXEC);
            let b = libc::syscall(libc::SYS_close_range, MARK + 1, libc::c_uint::MAX, CLOEXEC);
            if a == 0 && b == 0 {
                return;
            }
        }
        let mut l = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut l);
        let end = i32::try_from(l.rlim_cur).unwrap_or(i32::MAX).min(1 << 16);
        for fd in (3..end).filter(|f| *f != MARK) {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
}

#[allow(unsafe_code)]
fn signal(pid: i32, sig: i32) -> bool {
    // SAFETY: a plain system call.
    unsafe { libc::kill(pid, sig) == 0 }
}

#[allow(unsafe_code)]
fn group(pid: i32) -> i32 {
    // SAFETY: a plain system call.
    unsafe { libc::getpgid(pid) }
}

impl Tree {
    pub fn new(pid: u32, mark: Option<OwnedFd>) -> Self {
        let root = i32::try_from(pid).unwrap_or(0);
        let mut t = Self { root, known: BTreeMap::new(), mark, done: root <= 1 };
        if !t.done {
            t.known.insert(root, sys::started(root).unwrap_or(0));
        }
        t
    }

    /// Adds what those known have started since, and drops those that have ended: their numbers may
    /// be somebody else's by now, with children that are none of this.
    pub fn look(&mut self) {
        let all = sys::Table::new();
        let mut todo: Vec<(i32, u64)> = self.known.iter().map(|(p, at)| (*p, *at)).collect();
        while let Some((p, at)) = todo.pop() {
            if sys::started(p) != Some(at) {
                // The root's own start is what tells whether its group is still its.
                if p != self.root {
                    self.known.remove(&p);
                }
                continue;
            }
            let children = all.children(p);
            // Ended meanwhile is no matter. Begun anew meanwhile: whose they are is not known.
            if sys::started(p).is_some_and(|x| x != at) {
                continue;
            }
            for c in children {
                let Some(at) = sys::started(c) else { continue };
                if self.known.insert(c, at) != Some(at) {
                    todo.push((c, at));
                }
            }
        }
    }

    /// Asks all of it to end.
    pub fn ask(&mut self) {
        if self.done {
            return;
        }
        self.look();
        signal(-self.root, libc::SIGTERM);
        for p in self.others() {
            signal(p, libc::SIGTERM);
        }
    }

    /// Known, still the same process, and not the root (which the runtime reaps).
    fn others(&self) -> Vec<i32> {
        self.known
            .iter()
            .filter(|(p, at)| **p != self.root && sys::started(**p) == Some(**at))
            .map(|(p, _)| *p)
            .collect()
    }

    /// Ends all of it. How many others there were. What is not gone soon (it may be waiting for a
    /// drive) is not waited for here.
    pub fn kill(&mut self) -> usize {
        if self.done {
            return 0;
        }
        self.done = true;
        let mut seen = BTreeSet::new();
        if !self.kill_within(SOON, &mut seen) {
            let mut rest = Self {
                root: self.root,
                known: std::mem::take(&mut self.known),
                mark: self.mark.take(),
                done: true,
            };
            std::thread::spawn(move || rest.kill_within(LATE, &mut BTreeSet::new()));
        }
        seen.len()
    }

    /// Whether all of it is gone.
    fn kill_within(&mut self, most: Duration, seen: &mut BTreeSet<i32>) -> bool {
        let began = Instant::now();
        // A process may start another while it is being ended, so that a signal to the group misses
        // it. Nobody holding the mark any more is the proof that none did.
        while began.elapsed() < most {
            self.look();
            #[cfg(target_os = "macos")]
            for p in self.mark.iter().flat_map(sys::holding) {
                if let Some(at) = sys::started(p) {
                    self.known.insert(p, at);
                }
            }
            let others = self.others();
            let root = self.known.get(&self.root).copied();
            let rooted = root.is_some() && sys::started(self.root) == root;
            if others.is_empty() && !rooted && self.released(0) {
                return true;
            }
            // Its group's number is nobody else's as long as one of the group lives.
            if rooted || others.iter().any(|p| group(*p) == self.root) {
                signal(-self.root, libc::SIGKILL);
            }
            for p in others {
                signal(p, libc::SIGKILL);
                seen.insert(p);
            }
            self.released(1);
        }
        false
    }

    /// Whether nobody holds the mark, within `ms`.
    fn released(&self, ms: i32) -> bool {
        match &self.mark {
            Some(m) => released(m, ms),
            None => {
                std::thread::sleep(std::time::Duration::from_millis(ms.unsigned_abs().into()));
                true
            }
        }
    }
}

/// Whether nobody holds the other end, within `ms`.
#[allow(unsafe_code)]
fn released(mark: &OwnedFd, ms: i32) -> bool {
    let mut p = libc::pollfd { fd: mark.as_raw_fd(), events: libc::POLLIN, revents: 0 };
    // SAFETY: one valid pollfd.
    let n = unsafe { libc::poll(&raw mut p, 1, ms) };
    n == 1 && p.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod sys {
    use std::{
        mem::{size_of, zeroed},
        os::{fd::AsRawFd, unix::io::OwnedFd},
    };

    fn info(pid: i32) -> Option<libc::proc_bsdinfo> {
        let size = i32::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
        // SAFETY: the buffer is as large as said; a zeroed proc_bsdinfo is valid.
        unsafe {
            let mut i: libc::proc_bsdinfo = zeroed();
            let n = libc::proc_pidinfo(pid, libc::PROC_PIDTBSDINFO, 0, (&raw mut i).cast(), size);
            // A dead process that nobody has waited for starts nothing and cannot be ended.
            (n == size && i.pbi_status != libc::SZOMB).then_some(i)
        }
    }

    /// When it started, if it runs.
    pub fn started(pid: i32) -> Option<u64> {
        info(pid).map(|i| i.pbi_start_tvsec * 1_000_000 + i.pbi_start_tvusec)
    }

    fn list(call: impl Fn(*mut libc::c_void, i32) -> i32) -> Vec<i32> {
        let mut v = vec![0i32; 4096];
        let bytes = i32::try_from(v.len() * size_of::<i32>()).unwrap_or(0);
        let n = usize::try_from(call(v.as_mut_ptr().cast(), bytes)).unwrap_or(0);
        v.truncate(n.min(4096));
        v.retain(|p| *p > 1);
        v
    }

    /// Who has started whom: asked of the system each time.
    pub struct Table;
    impl Table {
        pub fn new() -> Self {
            Self
        }
        pub fn children(&self, pid: i32) -> Vec<i32> {
            // SAFETY: the buffer is as large as said.
            list(|b, n| unsafe { libc::proc_listchildpids(pid, b, n) })
        }
    }

    /// This user's processes that hold the other end of the mark.
    pub fn holding(mark: &OwnedFd) -> Vec<i32> {
        // SAFETY: as above.
        let me = unsafe { libc::getpid() };
        let Some(want) = pipe(me, mark.as_raw_fd()).map(|p| p.1) else { return Vec::new() };
        // SAFETY: the buffer is as large as said.
        let all = list(|b, n| unsafe { libc::proc_listallpids(b, n) });
        all.into_iter()
            .filter(|p| *p != me && pipe(*p, super::MARK).is_some_and(|x| x.0 == want))
            .collect()
    }

    /// The handles of a pipe's end and of its other end.
    fn pipe(pid: i32, fd: i32) -> Option<(u64, u64)> {
        const PROC_PIDFDPIPEINFO: i32 = 6;
        #[repr(C)]
        struct PipeFd {
            file: [u64; 3],
            stat: [u64; 17],
            handle: u64,
            peer: u64,
            status: i32,
            pad: i32,
        }
        let size = i32::try_from(size_of::<PipeFd>()).ok()?;
        // SAFETY: the buffer is as large as said; all of it are integers.
        unsafe {
            let mut p: PipeFd = zeroed();
            let n = libc::proc_pidfdinfo(pid, fd, PROC_PIDFDPIPEINFO, (&raw mut p).cast(), size);
            (n == size).then_some((p.handle, p.peer))
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod sys {
    fn stat(pid: i32) -> Option<(char, i32, u64)> {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // The name may hold anything, also brackets: what follows it starts after the last.
        let mut f = s.rsplit_once(')')?.1.split_ascii_whitespace();
        let state = f.next()?.chars().next()?;
        let parent = f.next()?.parse().ok()?;
        let at = f.nth(17)?.parse().ok()?;
        Some((state, parent, at))
    }

    pub fn started(pid: i32) -> Option<u64> {
        stat(pid).filter(|s| !matches!(s.0, 'Z' | 'X')).map(|s| s.2)
    }

    /// Who has started whom, read once.
    pub struct Table(std::collections::BTreeMap<i32, Vec<i32>>);
    impl Table {
        pub fn new() -> Self {
            let mut all: std::collections::BTreeMap<i32, Vec<i32>> = Default::default();
            let pids = std::fs::read_dir("/proc").into_iter().flatten();
            for p in pids.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok()) {
                if let Some((_, parent, _)) = stat(p) {
                    all.entry(parent).or_default().push(p);
                }
            }
            Self(all)
        }
        pub fn children(&self, pid: i32) -> Vec<i32> {
            self.0.get(&pid).cloned().unwrap_or_default()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// A number that is somebody else's by now: neither it nor its children are ended.
    #[test]
    fn who_has_a_number_after_one_of_ours_lives() {
        let mut other = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 61 & echo up; wait"])
            .stdout(std::process::Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        let mut up = [0; 3];
        io::Read::read_exact(other.stdout.as_mut().unwrap(), &mut up).unwrap();
        let pid = i32::try_from(other.id()).unwrap();
        let at = sys::started(pid).unwrap();
        let children = sys::Table::new().children(pid);
        assert_eq!(children.len(), 1);
        let mut ours =
            std::process::Command::new("sleep").arg("62").process_group(0).spawn().unwrap();
        for root in [pid, i32::try_from(ours.id()).unwrap()] {
            let mut t = Tree { root, known: BTreeMap::new(), mark: None, done: false };
            t.known.insert(root, sys::started(root).unwrap());
            t.known.insert(pid, at - 1);
            t.look();
            assert!(!t.known.contains_key(&children[0]));
            t.kill();
        }
        assert!(!ours.wait().unwrap().success());
        assert!(other.try_wait().unwrap().is_none());
        assert!(sys::started(children[0]).is_some());
        // As it was when it was ours: all of it ends.
        let mut t = Tree { root: pid, known: [(pid, at)].into(), mark: None, done: false };
        assert_eq!(t.kill(), 1);
        other.wait().unwrap();
        assert!(sys::started(children[0]).is_none());
    }
}
