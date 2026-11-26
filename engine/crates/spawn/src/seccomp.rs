//! Linux: the calls that are refused to what is contained, as a program for the kernel's own filter.
//! The rooms are bubblewrap's (files, network, processes); this is for what leads out of such rooms, or
//! is an attack on the kernel with no use for the work done here. Anything not named is allowed: what a
//! build needs is not known beforehand.
//!
//! - Other processes' memory and files, also inside.
//! - Rooms inside the room, mounts, modules, keys, and what needs a right that is not there anyway.
//! - `io_uring`, which does calls past this filter. `clone3` and `openat2`, whose arguments are behind a
//!   pointer and cannot be looked at: not there, and whoever uses them does without.
//! - Sockets of this machine: a name that leads into the room (the folder itself) would be a way to a
//!   process outside of it. Only pairs. And of what has an address, only the Internet's, of which there
//!   is none but the room's own; the kernel may be asked what its network is.
//! - Characters put into a terminal's input, and files that run as whoever owns them.
//!
//! Written for the two processors there are machines for; elsewhere nothing is started (SI-8). No
//! library: a page of numbers, checked against the system's own where that is (`tests`).

/// One instruction of the kernel's filter, as it takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ins {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

// Of `linux/filter.h`, `linux/bpf_common.h`.
const LOAD: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const EQUAL: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const NOT_LESS: u16 = 0x35; // BPF_JMP | BPF_JGE | BPF_K
const ANY_OF: u16 = 0x45; // BPF_JMP | BPF_JSET | BPF_K
const RETURN: u16 = 0x06; // BPF_RET | BPF_K

// Of `linux/seccomp.h`.
const ALLOW: u32 = 0x7fff_0000;
const KILL: u32 = 0x8000_0000;
const ERRNO: u32 = 0x0005_0000;

const EPERM: u32 = 1;
const ENOSYS: u32 = 38;
const EAFNOSUPPORT: u32 = 97;

/// Where `struct seccomp_data` has them. Arguments have 64 bits; their low half comes first on both.
const NUMBER: u32 = 0;
const ARCH: u32 = 4;
const fn low(arg: u32) -> u32 {
    16 + 8 * arg
}

/// The processor, and what its calls are numbered. `None`: it has no such call.
pub struct Calls {
    arch: u32,
    /// Numbers from here are another set of the same calls (x32).
    other: Option<u32>,
    refused: &'static [(&'static str, u32)],
    absent: &'static [(&'static str, u32)],
    clone: u32,
    unshare: u32,
    ioctl: u32,
    socket: u32,
    socketpair: u32,
    /// With which argument the mode is.
    mode: &'static [(&'static str, u32, u32)],
    /// With which arguments the flags and the mode are.
    open: &'static [(&'static str, u32, u32, u32)],
}

macro_rules! both {
    ($($mine:expr),* $(,)?) => {
        &[
            $($mine,)*
            ("open_tree", 428), ("move_mount", 429), ("fsopen", 430), ("fsconfig", 431),
            ("fsmount", 432), ("fspick", 433), ("pidfd_getfd", 438), ("mount_setattr", 442),
            ("quotactl_fd", 443), ("open_tree_attr", 467),
        ]
    };
}
/// Numbered alike on both.
const ABSENT: &[(&str, u32)] = &[
    ("io_uring_setup", 425),
    ("io_uring_enter", 426),
    ("io_uring_register", 427),
    ("clone3", 435),
    ("openat2", 437),
];

pub const X86_64: Calls = Calls {
    arch: 0xc000_003e,
    other: Some(0x4000_0000),
    refused: both![
        ("ptrace", 101),
        ("syslog", 103),
        ("uselib", 134),
        ("vhangup", 153),
        ("pivot_root", 155),
        ("adjtimex", 159),
        ("chroot", 161),
        ("acct", 163),
        ("settimeofday", 164),
        ("mount", 165),
        ("umount2", 166),
        ("swapon", 167),
        ("swapoff", 168),
        ("reboot", 169),
        ("iopl", 172),
        ("ioperm", 173),
        ("init_module", 175),
        ("delete_module", 176),
        ("quotactl", 179),
        ("clock_settime", 227),
        ("kexec_load", 246),
        ("add_key", 248),
        ("request_key", 249),
        ("keyctl", 250),
        ("perf_event_open", 298),
        ("fanotify_init", 300),
        ("open_by_handle_at", 304),
        ("clock_adjtime", 305),
        ("setns", 308),
        ("process_vm_readv", 310),
        ("process_vm_writev", 311),
        ("kcmp", 312),
        ("finit_module", 313),
        ("kexec_file_load", 320),
        ("bpf", 321),
        ("userfaultfd", 323),
    ],
    absent: ABSENT,
    clone: 56,
    unshare: 272,
    ioctl: 16,
    socket: 41,
    socketpair: 53,
    mode: &[
        ("chmod", 90, 1),
        ("fchmod", 91, 1),
        ("fchmodat", 268, 2),
        ("fchmodat2", 452, 2),
        ("creat", 85, 1),
        ("mknod", 133, 1),
        ("mknodat", 259, 2),
    ],
    open: &[("open", 2, 1, 2), ("openat", 257, 2, 3)],
};

pub const AARCH64: Calls = Calls {
    arch: 0xc000_00b7,
    other: None,
    refused: both![
        ("umount2", 39),
        ("mount", 40),
        ("pivot_root", 41),
        ("chroot", 51),
        ("vhangup", 58),
        ("quotactl", 60),
        ("acct", 89),
        ("kexec_load", 104),
        ("init_module", 105),
        ("delete_module", 106),
        ("clock_settime", 112),
        ("syslog", 116),
        ("ptrace", 117),
        ("reboot", 142),
        ("settimeofday", 170),
        ("adjtimex", 171),
        ("add_key", 217),
        ("request_key", 218),
        ("keyctl", 219),
        ("swapon", 224),
        ("swapoff", 225),
        ("perf_event_open", 241),
        ("fanotify_init", 262),
        ("open_by_handle_at", 265),
        ("clock_adjtime", 266),
        ("setns", 268),
        ("process_vm_readv", 270),
        ("process_vm_writev", 271),
        ("kcmp", 272),
        ("finit_module", 273),
        ("bpf", 280),
        ("userfaultfd", 282),
        ("kexec_file_load", 294),
    ],
    absent: ABSENT,
    clone: 220,
    unshare: 97,
    ioctl: 29,
    socket: 198,
    socketpair: 199,
    mode: &[("fchmod", 52, 1), ("fchmodat", 53, 2), ("fchmodat2", 452, 2), ("mknodat", 33, 2)],
    open: &[("openat", 56, 2, 3)],
};

/// Rooms of one's own: mount, cgroup, uts, ipc, user, pid, net. In `unshare` time as well, which in
/// `clone` is a part of the signal's number.
const ROOMS: u32 = 0x7e02_0000;
const TIME: u32 = 0x80;
/// Puts characters into a terminal's input (`TIOCSTI`, `TIOCLINUX`).
const INTO_TERMINAL: [u32; 2] = [0x5412, 0x541c];
const UNIX: u32 = 1;
const INET: u32 = 2;
const INET6: u32 = 10;
const NETLINK: u32 = 16;
/// Of what may be asked of the kernel: what its network is.
const ROUTE: u32 = 0;
/// `S_ISUID`: it runs as whoever owns it.
const AS_OWNER: u32 = 0o4000;
/// `O_CREAT`; what makes a file without a name (`O_TMPFILE`) has a bit of its own.
const MAKES: u32 = 0o100 | 0o20_000_000;

/// What runs this can be contained on.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn here() -> Option<&'static Calls> {
    if cfg!(target_arch = "x86_64") {
        Some(&X86_64)
    } else if cfg!(target_arch = "aarch64") {
        Some(&AARCH64)
    } else {
        None
    }
}

/// The filter, as bubblewrap reads it.
pub fn program(c: &Calls) -> Vec<u8> {
    let mut out = Vec::new();
    for i in instructions(c) {
        out.extend(i.code.to_ne_bytes());
        out.extend([i.jt, i.jf]);
        out.extend(i.k.to_ne_bytes());
    }
    out
}

fn instructions(c: &Calls) -> Vec<Ins> {
    let load = |k| Ins { code: LOAD, jt: 0, jf: 0, k };
    let end = |k| Ins { code: RETURN, jt: 0, jf: 0, k };
    let no = |e: u32| end(ERRNO | e);
    // Goes on with the next if so, and otherwise past `past` of them.
    let when = |code, k, past: usize| Ins { code, jt: 0, jf: u8::try_from(past).unwrap_or(0), k };
    let unless = |code, k, past: usize| Ins { code, jt: u8::try_from(past).unwrap_or(0), jf: 0, k };
    let mut p = vec![load(ARCH), unless(EQUAL, c.arch, 1), end(KILL), load(NUMBER)];
    if let Some(n) = c.other {
        p.extend([when(NOT_LESS, n, 1), no(ENOSYS)]);
    }
    for (_, n) in c.refused {
        p.extend([when(EQUAL, *n, 1), no(EPERM)]);
    }
    for (_, n) in c.absent {
        p.extend([when(EQUAL, *n, 1), no(ENOSYS)]);
    }
    // Each ends by itself: what comes after one of them has the call's number still.
    let mut rule = |n: u32, body: &[Ins]| {
        p.push(when(EQUAL, n, body.len()));
        p.extend(body);
    };
    for (n, rooms) in [(c.clone, ROOMS), (c.unshare, ROOMS | TIME)] {
        rule(n, &[load(low(0)), when(ANY_OF, rooms, 1), no(EPERM), end(ALLOW)]);
    }
    let [a, b] = INTO_TERMINAL;
    rule(c.ioctl, &[load(low(1)), unless(EQUAL, a, 1), when(EQUAL, b, 1), no(EPERM), end(ALLOW)]);
    rule(
        c.socket,
        &[
            load(low(0)),
            unless(EQUAL, INET, 5),
            unless(EQUAL, INET6, 4),
            when(EQUAL, NETLINK, 2),
            load(low(2)),
            unless(EQUAL, ROUTE, 1),
            no(EAFNOSUPPORT),
            end(ALLOW),
        ],
    );
    rule(c.socketpair, &[load(low(0)), unless(EQUAL, UNIX, 1), no(EAFNOSUPPORT), end(ALLOW)]);
    for (_, n, mode) in c.mode {
        rule(*n, &[load(low(*mode)), when(ANY_OF, AS_OWNER, 1), no(EPERM), end(ALLOW)]);
    }
    for (_, n, flags, mode) in c.open {
        rule(
            *n,
            &[
                load(low(*flags)),
                when(ANY_OF, MAKES, 3),
                load(low(*mode)),
                when(ANY_OF, AS_OWNER, 1),
                no(EPERM),
                end(ALLOW),
            ],
        );
    }
    p.push(end(ALLOW));
    p
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// What the kernel does with a call: as `kernel/seccomp.c` runs a filter.
    fn run(c: &Calls, arch: u32, nr: u32, args: [u64; 6]) -> u32 {
        let p = instructions(c);
        let word = |at: u32| match at {
            0 => nr,
            4 => arch,
            16..64 => {
                let a = args[(at as usize - 16) / 8];
                if at.is_multiple_of(8) { a as u32 } else { (a >> 32) as u32 }
            }
            _ => panic!("reads at {at}"),
        };
        let (mut at, mut a) = (0, 0);
        loop {
            let i = p[at];
            at += 1;
            let jump = |yes: bool| usize::from(if yes { i.jt } else { i.jf });
            match i.code {
                LOAD => a = word(i.k),
                EQUAL => at += jump(a == i.k),
                NOT_LESS => at += jump(a >= i.k),
                ANY_OF => at += jump(a & i.k != 0),
                RETURN => return i.k,
                x => panic!("an instruction {x:#x}"),
            }
        }
    }
    fn call(c: &Calls, nr: u32, args: &[u64]) -> u32 {
        let mut all = [0; 6];
        all[..args.len()].copy_from_slice(args);
        run(c, c.arch, nr, all)
    }
    const NO: u32 = ERRNO | EPERM;
    const NONE: u32 = ERRNO | ENOSYS;
    const FAMILY: u32 = ERRNO | EAFNOSUPPORT;
    const ALL: [&Calls; 2] = [&X86_64, &AARCH64];

    #[test]
    fn what_is_named_is_refused_and_nothing_else() {
        for c in ALL {
            for (n, nr) in c.refused {
                assert_eq!(call(c, *nr, &[0; 6]), NO, "{n}");
                assert_eq!(call(c, *nr, &[u64::MAX; 6]), NO, "{n}");
            }
            for (n, nr) in c.absent {
                assert_eq!(call(c, *nr, &[0; 6]), NONE, "{n}");
            }
            let mut named: Vec<u32> = c.refused.iter().chain(c.absent).map(|x| x.1).collect();
            named.extend([c.clone, c.unshare, c.ioctl, c.socket, c.socketpair]);
            named.extend(c.mode.iter().map(|x| x.1).chain(c.open.iter().map(|x| x.1)));
            let count = named.len();
            named.sort_unstable();
            named.dedup();
            assert_eq!(named.len(), count, "each once");
            for nr in (0..600).filter(|n| !named.contains(n)) {
                for a in [0, u64::MAX] {
                    assert_eq!(call(c, nr, &[a; 6]), ALLOW, "{nr}");
                }
            }
        }
    }

    #[test]
    fn both_refuse_the_same() {
        let names = |c: &Calls| -> Vec<&str> {
            let mut all: Vec<_> = c.refused.iter().chain(c.absent).map(|x| x.0).collect();
            all.sort_unstable();
            all
        };
        let (x, a) = (names(&X86_64), names(&AARCH64));
        let more: Vec<_> = x.iter().filter(|n| !a.contains(n)).collect();
        assert_eq!(more, [&"ioperm", &"iopl", &"uselib"], "what the other has not");
        assert!(a.iter().all(|n| x.contains(n)));
    }

    #[test]
    fn another_processors_calls_are_the_end_of_it() {
        for (c, o) in [(&X86_64, &AARCH64), (&AARCH64, &X86_64)] {
            // i386, whose numbers are others, and what nobody has.
            for arch in [o.arch, 0x4000_0003, 0x4000_0028, 0] {
                for nr in [0, c.clone, 400] {
                    assert_eq!(run(c, arch, nr, [0; 6]), KILL, "{arch:#x}");
                }
            }
        }
        // The same calls by other numbers are not there.
        for nr in [0, 1, X86_64.socket, 101, 425] {
            assert_eq!(call(&X86_64, 0x4000_0000 + nr, &[1]), NONE);
        }
    }

    #[test]
    fn no_rooms_inside() {
        for c in ALL {
            for f in [0x2_0000, 0x200_0000, 0x400_0000, 0x800_0000] {
                for f in [f, 0x1000_0000, 0x2000_0000, 0x4000_0000] {
                    assert_eq!(call(c, c.clone, &[f | 17]), NO, "{f:#x}");
                    assert_eq!(call(c, c.unshare, &[f]), NO);
                }
            }
            assert_eq!(call(c, c.unshare, &[0x80]), NO);
            // A process, a thread, files of its own; a signal's number is no room.
            for f in [17, 0x3d_0f00, 0x0100_0000 | 17, 0x80] {
                assert_eq!(call(c, c.clone, &[f]), ALLOW, "{f:#x}");
            }
            assert_eq!(call(c, c.unshare, &[0x400]), ALLOW);
        }
    }

    #[test]
    fn only_the_internets_sockets_and_pairs() {
        for c in ALL {
            for family in 0..50 {
                let want = if [2, 10, 16].contains(&family) { ALLOW } else { FAMILY };
                assert_eq!(call(c, c.socket, &[family, 1, 0]), want, "{family}");
                let want = if family == 1 { ALLOW } else { FAMILY };
                assert_eq!(call(c, c.socketpair, &[family, 1, 0]), want, "{family}");
            }
            for protocol in 1..25 {
                assert_eq!(call(c, c.socket, &[16, 3, protocol]), FAMILY, "{protocol}");
                assert_eq!(call(c, c.socket, &[2, 1, protocol]), ALLOW, "{protocol}");
            }
            // The kernel looks at 32 bits; what is above them hides nothing.
            assert_eq!(call(c, c.socket, &[0xf_0000_0001, 1, 0]), FAMILY);
        }
    }

    #[test]
    fn nothing_is_typed_for_a_person() {
        for c in ALL {
            for (cmd, want) in [(0x5412, NO), (0x541c, NO), (0x5413, ALLOW), (0x5401, ALLOW)] {
                assert_eq!(call(c, c.ioctl, &[0, cmd, 0]), want, "{cmd:#x}");
                assert_eq!(call(c, c.ioctl, &[0, cmd | 1 << 32, 0]), want, "{cmd:#x}");
            }
        }
    }

    #[test]
    fn nothing_is_made_to_run_as_its_owner() {
        for c in ALL {
            for (n, nr, at) in c.mode {
                let with = |mode: u64| {
                    let mut a = [0; 6];
                    a[*at as usize] = mode;
                    call(c, *nr, &a)
                };
                assert_eq!((with(0o4755), with(0o104_755)), (NO, NO), "{n}");
                assert_eq!((with(0o755), with(0o2755), with(0o1777)), (ALLOW, ALLOW, ALLOW), "{n}");
            }
            for (n, nr, flags, mode) in c.open {
                let with = |f: u64, m: u64| {
                    let mut a = [0; 6];
                    a[*flags as usize] = f;
                    a[*mode as usize] = m;
                    call(c, *nr, &a)
                };
                assert_eq!((with(0o100, 0o4755), with(0o20_200_000, 0o4755)), (NO, NO), "{n}");
                assert_eq!(with(0o1101, 0o644), ALLOW, "{n}");
                // Without making one the mode is whatever was there.
                assert_eq!(with(0o2, u64::MAX), ALLOW, "{n}");
            }
        }
    }

    #[test]
    fn it_is_what_the_kernel_takes() {
        for c in ALL {
            let p = program(c);
            assert!(p.len().is_multiple_of(8) && p.len() / 8 <= 4096, "{}", p.len());
            let i = instructions(c);
            assert_eq!(i.last().unwrap(), &Ins { code: RETURN, jt: 0, jf: 0, k: ALLOW });
            for (at, x) in i.iter().enumerate() {
                assert!(at + 1 + usize::from(x.jt.max(x.jf)) < i.len() || x.code == RETURN);
            }
        }
    }

    /// The numbers, as the system has them.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_numbers_are_the_systems() {
        macro_rules! check {
            ($($(#[$m:meta])* $name:literal $const:ident),* $(,)?) => {{
                let c = here().unwrap();
                let all = c.refused.iter().chain(c.absent).copied()
                    .chain(c.mode.iter().map(|x| (x.0, x.1)))
                    .chain(c.open.iter().map(|x| (x.0, x.1)))
                    .chain([("clone", c.clone), ("unshare", c.unshare), ("ioctl", c.ioctl),
                        ("socket", c.socket), ("socketpair", c.socketpair)]);
                let mut all: std::collections::BTreeMap<&str, u32> = all.collect();
                $($(#[$m])* {
                    let n = all.remove($name).unwrap_or_else(|| panic!("{} is not named", $name));
                    assert_eq!(i64::from(n), libc::$const as i64, $name);
                })*
                all
            }};
        }
        let left = check!(
            "ptrace" SYS_ptrace, "syslog" SYS_syslog, "vhangup" SYS_vhangup,
            "pivot_root" SYS_pivot_root, "adjtimex" SYS_adjtimex, "chroot" SYS_chroot,
            "acct" SYS_acct, "settimeofday" SYS_settimeofday, "mount" SYS_mount,
            "umount2" SYS_umount2, "swapon" SYS_swapon, "swapoff" SYS_swapoff,
            "reboot" SYS_reboot, "init_module" SYS_init_module,
            "delete_module" SYS_delete_module, "quotactl" SYS_quotactl,
            "clock_settime" SYS_clock_settime, "kexec_load" SYS_kexec_load,
            "add_key" SYS_add_key, "request_key" SYS_request_key, "keyctl" SYS_keyctl,
            "perf_event_open" SYS_perf_event_open, "fanotify_init" SYS_fanotify_init,
            "open_by_handle_at" SYS_open_by_handle_at, "clock_adjtime" SYS_clock_adjtime,
            "setns" SYS_setns, "process_vm_readv" SYS_process_vm_readv,
            "process_vm_writev" SYS_process_vm_writev, "kcmp" SYS_kcmp,
            "finit_module" SYS_finit_module, "kexec_file_load" SYS_kexec_file_load,
            "bpf" SYS_bpf, "userfaultfd" SYS_userfaultfd, "open_tree" SYS_open_tree,
            "move_mount" SYS_move_mount, "fsopen" SYS_fsopen, "fsconfig" SYS_fsconfig,
            "fsmount" SYS_fsmount, "fspick" SYS_fspick, "pidfd_getfd" SYS_pidfd_getfd,
            "mount_setattr" SYS_mount_setattr, "quotactl_fd" SYS_quotactl_fd,
            "io_uring_setup" SYS_io_uring_setup, "io_uring_enter" SYS_io_uring_enter,
            "io_uring_register" SYS_io_uring_register, "clone3" SYS_clone3,
            "openat2" SYS_openat2, "clone" SYS_clone, "unshare" SYS_unshare,
            "ioctl" SYS_ioctl, "socket" SYS_socket, "socketpair" SYS_socketpair,
            "fchmod" SYS_fchmod, "fchmodat" SYS_fchmodat,
            "mknodat" SYS_mknodat, "openat" SYS_openat,
            #[cfg(target_arch = "x86_64")] "uselib" SYS_uselib,
            #[cfg(target_arch = "x86_64")] "iopl" SYS_iopl,
            #[cfg(target_arch = "x86_64")] "ioperm" SYS_ioperm,
            #[cfg(target_arch = "x86_64")] "chmod" SYS_chmod,
            #[cfg(target_arch = "x86_64")] "creat" SYS_creat,
            #[cfg(target_arch = "x86_64")] "mknod" SYS_mknod,
            #[cfg(target_arch = "x86_64")] "open" SYS_open,
        );
        // Newer than the library that knows the others.
        let newer = [("fchmodat2", 452), ("open_tree_attr", 467)];
        assert_eq!(left.into_iter().collect::<Vec<_>>(), newer);
    }
}
