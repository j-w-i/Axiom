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
/// Where what bubblewrap is to read is.
pub const FILTER: i32 = MARK + 1;

#[derive(Debug)]
pub struct Tree {
    root: i32,
    /// Process and when it started: numbers are used again.
    known: BTreeMap<i32, u64>,
    /// Our end of the mark. The reaper has none.
    mark: Option<OwnedFd>,
    done: bool,
}

/// What one process uses, as the system says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usage {
    pub pid: u32,
    pub parent: u32,
    /// Its program's name, as the system has it (cut short by it).
    pub name: String,
    /// Processor time it has used, in all, in milliseconds.
    pub cpu_ms: u64,
    /// Memory it holds now, in bytes.
    pub memory: u64,
}

/// What `pid` uses, if it runs.
pub fn usage(pid: u32) -> Option<Usage> {
    sys::usage(i32::try_from(pid).ok()?)
}

/// The TCP ports `pid` listens on, as the system says, that this computer can reach: what it serves
/// (decision 0021).
pub fn listening(pid: u32) -> Vec<u16> {
    let Ok(pid) = i32::try_from(pid) else { return Vec::new() };
    let mut all = sys::listening(pid);
    all.sort_unstable();
    all.dedup();
    all
}

/// Makes what `c` starts a session of its own, with the mark, `filter` if there is one, and no other
/// open file.
#[allow(unsafe_code)]
pub fn prepare(c: &mut std::process::Command, filter: Option<OwnedFd>) -> io::Result<OwnedFd> {
    let (ours, theirs) = io::pipe()?;
    let theirs = OwnedFd::from(theirs);
    let last = if filter.is_some() { FILTER } else { MARK };
    // SAFETY: only calls that are async-signal-safe, on integers; nothing is allocated. `theirs` and
    // `filter` live in the closure, which lives in the command.
    unsafe {
        c.pre_exec(move || {
            if libc::setsid() < 0 || libc::dup2(theirs.as_raw_fd(), MARK) < 0 {
                return Err(io::Error::last_os_error());
            }
            if let Some(f) = &filter
                && libc::dup2(f.as_raw_fd(), FILTER) < 0
            {
                return Err(io::Error::last_os_error());
            }
            let none = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
            libc::setrlimit(libc::RLIMIT_CORE, &raw const none);
            close_on_exec(last);
            Ok(())
        });
    }
    Ok(OwnedFd::from(ours))
}

/// Every file after the first three and besides those from the mark to `last` closes when the program
/// starts. Not at once: the standard library still reports a failed start through one of them.
#[allow(unsafe_code)]
fn close_on_exec(last: i32) {
    // SAFETY: plain system calls.
    unsafe {
        #[cfg(target_os = "linux")]
        {
            const CLOEXEC: libc::c_uint = 4;
            let a = libc::syscall(libc::SYS_close_range, 3, MARK - 1, CLOEXEC);
            let b = libc::syscall(libc::SYS_close_range, last + 1, libc::c_uint::MAX, CLOEXEC);
            if a == 0 && b == 0 {
                return;
            }
        }
        let mut l = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        libc::getrlimit(libc::RLIMIT_NOFILE, &raw mut l);
        let end = i32::try_from(l.rlim_cur).unwrap_or(i32::MAX).min(1 << 16);
        for fd in (3..end).filter(|f| !(MARK..=last).contains(f)) {
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

    /// Those known that still run, the first one first.
    pub fn members(&self) -> Vec<u32> {
        let mut all: Vec<u32> = self
            .known
            .iter()
            .filter(|(p, at)| sys::started(**p) == Some(**at))
            .filter_map(|(p, _)| u32::try_from(*p).ok())
            .collect();
        all.sort_by_key(|p| *p != u32::try_from(self.root).unwrap_or(0));
        all
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

    /// What the system says of an open socket, as `sys/proc_info.h` lays it out: only what is read
    /// here is named; the rest is room.
    #[repr(C)]
    struct SocketFd {
        /// `proc_fileinfo`.
        file: [u32; 6],
        /// `vinfo_stat`.
        stat: [u64; 17],
        so: u64,
        pcb: u64,
        kind_type: i32,
        protocol: i32,
        family: i32,
        shorts: [i16; 8],
        oobmark: u32,
        /// `sockbuf_info` twice.
        bufs: [u32; 12],
        kind: i32,
        rfu: u32,
        /// The union of what each kind of socket has: of TCP, `in_sockinfo` then the state.
        proto: [u8; 640],
    }
    const PROC_PIDLISTFDS: i32 = 1;
    const PROC_PIDFDSOCKETINFO: i32 = 3;
    const PROX_FDTYPE_SOCKET: u32 = 2;
    const SOCKINFO_TCP: i32 = 2;
    const TSI_S_LISTEN: i32 = 1;

    pub fn listening(pid: i32) -> Vec<u16> {
        let mut fds: Vec<libc::proc_fdinfo> = Vec::with_capacity(4096);
        let one = size_of::<libc::proc_fdinfo>();
        let Ok(room) = i32::try_from(4096 * one) else { return Vec::new() };
        // SAFETY: the buffer has room for as many as said; what is written is counted.
        let n =
            unsafe { libc::proc_pidinfo(pid, PROC_PIDLISTFDS, 0, fds.as_mut_ptr().cast(), room) };
        let Ok(n) = usize::try_from(n) else { return Vec::new() };
        // SAFETY: the system wrote n bytes, whole entries.
        unsafe { fds.set_len((n / one).min(4096)) };
        let mut out = Vec::new();
        for f in fds.iter().filter(|f| f.proc_fdtype == PROX_FDTYPE_SOCKET) {
            let Ok(size) = i32::try_from(size_of::<SocketFd>()) else { continue };
            // SAFETY: the buffer is as large as said, and all of it is integers.
            let s = unsafe {
                let mut s: SocketFd = zeroed();
                let got = libc::proc_pidfdinfo(
                    pid,
                    f.proc_fd,
                    PROC_PIDFDSOCKETINFO,
                    (&raw mut s).cast(),
                    size,
                );
                if got <= 0 {
                    continue;
                }
                s
            };
            if s.kind != SOCKINFO_TCP || !matches!(s.family, libc::AF_INET | libc::AF_INET6) {
                continue;
            }
            // in_sockinfo: the far port, the near one (as the network orders it), ...; its size is
            // 80, after which the TCP state follows.
            let lport = i32::from_ne_bytes([s.proto[4], s.proto[5], s.proto[6], s.proto[7]]);
            let state = i32::from_ne_bytes([s.proto[80], s.proto[81], s.proto[82], s.proto[83]]);
            if state == TSI_S_LISTEN {
                out.push(u16::from_be(lport as u16));
            }
        }
        out
    }

    /// How long the machine's own unit of time is, in nanoseconds: `numer / denom`.
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    unsafe extern "C" {
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }

    pub fn usage(pid: i32) -> Option<super::Usage> {
        let size = i32::try_from(size_of::<libc::proc_taskallinfo>()).ok()?;
        // SAFETY: the buffers are as large as said; zeroed, all of them are valid.
        let (a, base) = unsafe {
            let mut a: libc::proc_taskallinfo = zeroed();
            let n =
                libc::proc_pidinfo(pid, libc::PROC_PIDTASKALLINFO, 0, (&raw mut a).cast(), size);
            let mut base = Timebase { numer: 1, denom: 1 };
            mach_timebase_info(&raw mut base);
            if n != size || a.pbsd.pbi_status == libc::SZOMB {
                return None;
            }
            (a, base)
        };
        // Processor time is in the machine's own units.
        let ticks = u128::from(a.ptinfo.pti_total_user + a.ptinfo.pti_total_system);
        let ns = ticks * u128::from(base.numer.max(1)) / u128::from(base.denom.max(1));
        let name: Vec<u8> =
            a.pbsd.pbi_comm.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
        Some(super::Usage {
            pid: u32::try_from(pid).ok()?,
            parent: a.pbsd.pbi_ppid,
            name: String::from_utf8_lossy(&name).into_owned(),
            cpu_ms: u64::try_from(ns / 1_000_000).unwrap_or(u64::MAX),
            memory: a.ptinfo.pti_resident_size,
        })
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

    /// What a process's network namespace lists as listening, by the file of each socket, and
    /// those of them the process holds. Only if that is the host's own network: one a sandbox has
    /// of its own cannot be reached from here, and what cannot be reached is not said.
    pub fn listening(pid: i32) -> Vec<u16> {
        let net = |p: &str| std::fs::read_link(format!("/proc/{p}/ns/net")).ok();
        match (net("self"), net(&pid.to_string())) {
            (Some(a), Some(b)) if a == b => {}
            _ => return Vec::new(),
        }
        let mut by_inode = std::collections::HashMap::new();
        for table in ["tcp", "tcp6"] {
            let Ok(t) = std::fs::read_to_string(format!("/proc/{pid}/net/{table}")) else {
                continue;
            };
            for l in t.lines().skip(1) {
                let f: Vec<&str> = l.split_whitespace().collect();
                // local address, its port after the colon in hex; the state; the inode at 9.
                let (Some(local), Some(st), Some(inode)) = (f.get(1), f.get(3), f.get(9)) else {
                    continue;
                };
                if *st != "0A" {
                    continue;
                }
                let Some(port) =
                    local.rsplit(':').next().and_then(|p| u16::from_str_radix(p, 16).ok())
                else {
                    continue;
                };
                by_inode.insert((*inode).to_owned(), port);
            }
        }
        let fds = std::fs::read_dir(format!("/proc/{pid}/fd")).into_iter().flatten().flatten();
        fds.filter_map(|e| std::fs::read_link(e.path()).ok())
            .filter_map(|l| {
                let l = l.to_string_lossy().into_owned();
                let ino = l.strip_prefix("socket:[")?.strip_suffix(']')?.to_owned();
                by_inode.get(&ino).copied()
            })
            .collect()
    }

    #[allow(unsafe_code)]
    pub fn usage(pid: i32) -> Option<super::Usage> {
        let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let (head, rest) = s.rsplit_once(')')?;
        let name = head.split_once('(')?.1.to_owned();
        let f: Vec<&str> = rest.split_ascii_whitespace().collect();
        if matches!(f.first()?.chars().next()?, 'Z' | 'X') {
            return None;
        }
        // From the state: parent, then user and system time at 11 and 12, the resident pages at 21.
        let parent = f.get(1)?.parse().ok()?;
        let ticks: u64 = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
        let pages: u64 = f.get(21)?.parse().ok()?;
        // SAFETY: plain system calls.
        let (hz, page) =
            unsafe { (libc::sysconf(libc::_SC_CLK_TCK), libc::sysconf(libc::_SC_PAGESIZE)) };
        let hz = u64::try_from(hz).ok().filter(|h| *h > 0)?;
        let page = u64::try_from(page).ok()?;
        Some(super::Usage {
            pid: u32::try_from(pid).ok()?,
            parent,
            name,
            cpu_ms: ticks.saturating_mul(1000) / hz,
            memory: pages.saturating_mul(page),
        })
    }

    /// This user's processes that hold the other end of the mark.
    pub fn holding(mark: &std::os::unix::io::OwnedFd) -> Vec<i32> {
        use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
        // Both ends of a pipe are one file.
        let ours = std::fs::metadata(format!("/proc/self/fd/{}", mark.as_raw_fd()));
        let Ok(want) = ours.map(|m| format!("pipe:[{}]", m.ino())) else { return Vec::new() };
        let me = i32::try_from(std::process::id()).unwrap_or(0);
        let pids = std::fs::read_dir("/proc").into_iter().flatten();
        pids.filter_map(|e| e.ok()?.file_name().to_str()?.parse().ok())
            .filter(|p| *p != me)
            .filter(|p| {
                std::fs::read_link(format!("/proc/{p}/fd/{}", super::MARK))
                    .is_ok_and(|l| l.as_os_str() == want.as_str())
            })
            .collect()
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
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn what_a_process_listens_on_is_said() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let l6 = std::net::TcpListener::bind("[::1]:0").ok();
        let me = std::process::id();
        let ports = listening(me);
        assert!(ports.contains(&port), "{port} in {ports:?}");
        if let Some(l6) = &l6 {
            assert!(ports.contains(&l6.local_addr().unwrap().port()), "{ports:?}");
        }
        // A connection is not a listener.
        let c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let near = c.local_addr().unwrap().port();
        assert!(!listening(me).contains(&near));
        drop(l);
        assert!(!listening(me).contains(&port));
    }

    #[test]
    fn what_a_process_uses_is_said() {
        let mut busy = std::process::Command::new("/bin/sh")
            .args(["-c", "i=0; while [ $i -lt 300000 ]; do i=$((i+1)); done; sleep 63"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = busy.id();
        let mut u = usage(pid).unwrap();
        for _ in 0..200 {
            if u.cpu_ms > 50 {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
            u = usage(pid).unwrap();
        }
        assert_eq!((u.pid, u.parent), (pid, std::process::id()));
        assert!(["sh", "bash", "dash"].contains(&u.name.as_str()), "{u:?}");
        assert!(u.cpu_ms > 50 && u.cpu_ms < 60_000, "{u:?}");
        assert!(u.memory > 64 * 1024 && u.memory < 1 << 30, "{u:?}");
        let t = Tree::new(pid, None);
        assert_eq!(t.members(), [pid]);
        busy.kill().unwrap();
        busy.wait().unwrap();
        assert!(usage(pid).is_none());
        assert!(t.members().is_empty());
    }

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
