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
