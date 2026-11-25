//! What has been changed in a session's folder, as git has it.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::Path, process::Command};

use axiom_host::{Config, client::Client};
use axiom_proto::*;

mod common;
use common::*;

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

async fn start(f: &Fixture) -> (Client, SessionId) {
    let w = f.dir.path().join("w");
    for d in ["w", "home"] {
        std::fs::create_dir(f.dir.path().join(d)).unwrap();
    }
    let mut c = Config::new(f.paths.clone());
    c.home = Some(f.dir.path().join("home"));
    f.serve_with(c);
    let mut c = f.client().await;
    let w = Some(w.to_str().unwrap().to_owned());
    let v = c.call(Call::SessionCreate { title: "S".into(), workspace: w }).await.unwrap();
    (c, SessionId(v["session"].as_str().unwrap().into()))
}
async fn changes(c: &mut Client, s: &SessionId) -> Changes {
    serde_json::from_value(c.call(Call::Changes { session: s.clone() }).await.unwrap()).unwrap()
}

#[tokio::test]
async fn what_has_changed_is_told() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    assert_eq!(changes(&mut c, &s).await, Changes::default(), "no repository");
    git(&w, &["init", "-q", "-b", "trunk"]);
    std::fs::write(w.join("a.txt"), "one\ntwo\n").unwrap();
    let all = changes(&mut c, &s).await;
    assert_eq!((all.repository, all.branch.as_deref()), (true, Some("trunk")));
    assert!(all.diff.contains("+one") && all.whole, "{all:?}");
    std::fs::write(w.join("gone.txt"), "x\n").unwrap();
    std::fs::write(w.join(".gitignore"), "ignored\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    assert_eq!(changes(&mut c, &s).await.files, []);
    std::fs::write(w.join("a.txt"), "one\n2\n").unwrap();
    std::fs::remove_file(w.join("gone.txt")).unwrap();
    std::fs::write(w.join("ignored"), "x").unwrap();
    std::fs::create_dir(w.join("new dir")).unwrap();
    std::fs::write(w.join("new dir/é.txt"), "fresh\n").unwrap();
    std::fs::write(w.join("staged.txt"), "s\n").unwrap();
    git(&w, &["add", "staged.txt"]);
    std::fs::write(w.join("bin"), b"a\0b").unwrap();
    // What a link leads to is not shown.
    std::fs::write(f.dir.path().join("secret"), "SECRET").unwrap();
    std::os::unix::fs::symlink(f.dir.path().join("secret"), w.join("link")).unwrap();
    let all = changes(&mut c, &s).await;
    let mut files: Vec<(&str, &str)> =
        all.files.iter().map(|f| (f.path.as_str(), f.what.as_str())).collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("a.txt", "changed"),
            ("bin", "new"),
            ("gone.txt", "removed"),
            ("link", "new"),
            ("new dir/é.txt", "new"),
            ("staged.txt", "new"),
        ]
    );
    for part in ["-two", "+2", "-x", "+fresh", "+s", "--- a.txt", "+++ a.txt", "+++ new dir/é.txt"]
    {
        assert!(all.diff.contains(part), "{part}: {}", all.diff);
    }
    assert!(!all.diff.contains("SECRET") && all.whole);
    // Nothing of the repository was touched by looking.
    std::fs::write(w.join("big"), "y\n".repeat(200_000)).unwrap();
    let all = changes(&mut c, &s).await;
    assert!(!all.whole && all.files.iter().any(|f| f.path == "big"));
}

/// SI-6: a repository's own configuration can name programs. None of it has an effect.
#[tokio::test]
async fn nothing_of_a_repository_takes_effect() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    git(&w, &["init", "-q"]);
    std::fs::write(w.join("a.txt"), "one\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    let mark = f.dir.path().join("ran");
    let inside = w.join("ran");
    let evil = w.join("evil.sh");
    let script =
        format!("#!/bin/sh\ntouch '{}' '{}'\ncat \"$@\"\n", mark.display(), inside.display());
    std::fs::write(&evil, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&evil, std::fs::Permissions::from_mode(0o755)).unwrap();
    let e = evil.to_str().unwrap();
    for (k, v) in [
        ("core.fsmonitor", e),
        ("diff.external", e),
        ("core.pager", e),
        ("pager.diff", e),
        ("diff.evil.textconv", e),
        ("diff.evil.command", e),
        ("filter.evil.clean", e),
        ("filter.evil.process", e),
    ] {
        git(&w, &["config", k, v]);
    }
    std::fs::write(w.join(".git/hooks/post-index-change"), format!("#!/bin/sh\nexec {e}\n"))
        .unwrap();
    std::fs::write(w.join(".gitattributes"), "*.txt diff=evil filter=evil\n").unwrap();
    std::fs::write(w.join("a.txt"), "two\n").unwrap();
    let all = changes(&mut c, &s).await;
    assert!(all.repository && all.files.iter().any(|f| f.path == "a.txt"), "{all:?}");
    assert!(!mark.exists() && !inside.exists(), "something of it was run with an effect");
}
