//! What is particular to Linux.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

/// A mount covers what is there. What was not there is told afterwards.
#[tokio::test]
async fn a_protected_name_that_appears_is_told() {
    let f = fixture();
    let o = f.sh("mkdir -p sub/.vscode src/.git && echo x > .envrc && echo x > plain").await;
    let mut made: Vec<String> = o
        .ended
        .made
        .iter()
        .map(|p| p.strip_prefix(f.at("home/w").as_path()).unwrap().display().to_string())
        .collect();
    made.sort();
    assert_eq!(made, [".envrc", "src/.git", "sub/.vscode"], "{o:?}");
    assert!(f.sh("echo y > plain").await.ended.made.is_empty());
}

#[tokio::test]
async fn it_is_alone() {
    let f = fixture();
    let o = f.sh("ls /proc | grep -c '^[0-9]'; ls /sys /run /home /root /var 2>&1 | head -3; cat /proc/1/cmdline | tr '\\0' ' ' | head -c 20").await;
    let n: usize = o.out.lines().next().unwrap().parse().unwrap();
    assert!(n < 12, "{o:?}");
    let o = f.sh("unshare -U true && echo nested; mount -t tmpfs x /tmp && echo mounted; chroot / true && echo rooted; ls /sys/class/net").await;
    for w in ["nested", "mounted", "rooted", "eth0"] {
        assert!(!o.out.contains(w), "{o:?}");
    }
}

/// What the filter refuses, tried for real. Those marked are also done outside, where they are not
/// refused so: that it is the filter is seen on them.
#[tokio::test]
async fn calls_that_lead_out_are_refused() {
    let f = fixture();
    // What each does is one call by its number; its error is printed.
    let call = |nr: i64, args: &str| {
        format!("perl -e '$! = 0; $r = syscall({nr}, {args}); print $r < 0 ? $!+0 : \"done\"'")
    };
    let (eperm, enosys, family) = ("1", "38", "97");
    for (what, tells, script, want) in [
        ("io_uring", false, call(libc::SYS_io_uring_setup, "1, 0"), enosys),
        ("clone3", true, call(libc::SYS_clone3, "0, 0"), enosys),
        ("openat2", true, call(libc::SYS_openat2, "-100, 0, 0, 0"), enosys),
        ("a user's room", false, call(libc::SYS_unshare, "0x10000000"), eperm),
        ("ptrace", true, call(libc::SYS_ptrace, "0, 0, 0, 0"), eperm),
        ("keyctl", false, call(libc::SYS_keyctl, "0, -3, 0"), eperm),
        ("bpf", false, call(libc::SYS_bpf, "0, 0, 0"), eperm),
        ("userfaultfd", false, call(libc::SYS_userfaultfd, "1"), eperm),
        ("perf", false, call(libc::SYS_perf_event_open, "0, 0, -1, -1, 0"), eperm),
        ("pidfd_getfd", true, call(libc::SYS_pidfd_getfd, "-1, 0, 0"), eperm),
        ("process_vm_readv", false, call(libc::SYS_process_vm_readv, "1, 0, 0, 0, 0, 0"), eperm),
        ("mount", false, call(libc::SYS_mount, "0, 0, 0, 0, 0"), eperm),
        ("fsopen", false, call(libc::SYS_fsopen, "0, 0"), eperm),
        ("a socket of this machine", true, call(libc::SYS_socket, "1, 1, 0"), family),
        ("AF_ALG", false, call(libc::SYS_socket, "38, 5, 0"), family),
        ("AF_VSOCK", false, call(libc::SYS_socket, "40, 1, 0"), family),
        ("a packet socket", false, call(libc::SYS_socket, "17, 3, 0"), family),
        ("audit's netlink", false, call(libc::SYS_socket, "16, 3, 9"), family),
        ("typing for a person", false, call(libc::SYS_ioctl, "0, 0x5412, 0"), eperm),
    ] {
        let o = f.sh(&script).await;
        assert_eq!(o.out, want, "{what}: {o:?}");
        if tells {
            let mut j = f.job(&script, f.profile());
            j.contain = axiom_spawn::Contain::None(unsandboxed());
            let outside = run(j).await.unwrap();
            assert_ne!(outside.out, want, "{what} is refused outside too");
        }
    }
}

/// What people's work needs of the same calls.
#[tokio::test]
async fn work_goes_on_with_the_filter() {
    let f = fixture();
    for (what, script) in [
        ("a process", "(true) && sh -c 'echo ok'"),
        ("pipes", "echo ok | cat | cat"),
        (
            "a pair of sockets",
            "perl -MSocket -e 'socketpair(A, B, AF_UNIX, SOCK_STREAM, 0) and print \"ok\\n\"'",
        ),
        (
            "a server of its own",
            "perl -MIO::Socket::INET -e '$s = IO::Socket::INET->new(Listen=>1, LocalAddr=>\"127.0.0.1\") \
             and IO::Socket::INET->new(PeerAddr=>\"127.0.0.1\", PeerPort=>$s->sockport) and print \"ok\\n\"'",
        ),
        (
            "modes",
            "echo x > m && chmod 755 m && mkdir -p s && chmod 2755 s && cp -a m n && echo ok",
        ),
        ("a temporary file", "t=$(mktemp) && echo x > $t && rm $t && echo ok"),
        (
            "a thread",
            "command -v python3 >/dev/null || { echo ok; exit; }; python3 -c 'import threading as t; x = t.Thread(target=lambda: print(\"ok\")); x.start(); x.join()'",
        ),
    ] {
        let o = f.sh(script).await;
        assert_eq!(o.out, "ok\n", "{what}: {o:?}");
    }
}

/// The filter's file is bubblewrap's and not what runs in it.
#[tokio::test]
async fn the_filter_itself_is_not_passed_on() {
    let f = fixture();
    let o = f.sh("ls -l /proc/self/fd | grep -c ' 201 '; grep Seccomp: /proc/self/status").await;
    assert!(o.out.starts_with("0\n") && o.out.contains("2"), "{o:?}");
}
