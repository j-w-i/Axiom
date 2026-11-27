//! What is particular to macOS.
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use axiom_spawn::spawn;

/// SI-11: whatever is protected is so before it exists, anywhere, however it is spelled.
#[tokio::test]
async fn protected_names_cannot_be_made() {
    let f = fixture();
    for s in [
        "mkdir .vscode",
        "mkdir .VSCode",
        "mkdir .v\u{17F}code",
        "mkdir .hu\u{17F}\u{212A}y",
        "mkdir .hus\u{212A}y",
        "mkdir sub/.git",
        "mkdir sub/.Git",
        "mv sub .git2 && mkdir .git2/.git",
        "mv src sub/.git",
        "cp -R src sub/.git",
        "echo x > .envrc",
        "echo x > .mcp.json",
        "echo x > a.code-workspace",
        "echo x > A.Code-Workspace",
        "ln -s src .claude",
        "git init sub2",
    ] {
        let o = f.sh(s).await;
        assert!(!o.ok(), "{s:?}: {o:?}");
    }
    let o = f.sh("mkdir .github .gitx git && echo x > .gitignore && echo ok").await;
    assert_eq!(o.out, "ok\n", "{o:?}");
    for e in std::fs::read_dir(f.dir.path().join("home/w")).unwrap() {
        let n = e.unwrap().file_name().into_string().unwrap();
        assert!(n == ".git" || axiom_policy::paths::protected_name(&n).is_none(), "{n}");
    }
}

/// T18, T45: the services of the desktop. None of these opens a window even if it is let through.
#[tokio::test]
async fn no_service_of_the_desktop_answers() {
    let f = fixture();
    for (s, sign) in [
        ("security find-generic-password -l nothing 2>&1; security list-keychains", "keychain-db"),
        ("launchctl setenv AXIOM_TEST x && echo made", "made"),
        ("defaults read com.apple.dock; defaults read NSGlobalDomain", " = "),
        ("scutil --dns | head -3", "resolver"),
        ("sandbox-exec -p '(version 1)(allow default)' /bin/cat ../../out/f", "outside"),
        ("pbpaste; echo x | pbcopy && echo copied", "copied"),
        ("mdfind -count kMDItemFSName=zzz", "0"),
        ("log show --last 1s 2>/dev/null | head -1", "Timestamp"),
        ("sysctl -n kern.procargs2 kern.proc.all | head -1", "bytes"),
        ("ps -axo pid | head -2", "PID"),
        ("lsof -p 1 | head -1", "COMMAND"),
        ("dscl . -read /Users/$(id -un) | head -1", "dsAttr"),
    ] {
        let o = f.sh(s).await;
        assert!(!o.out.contains(sign), "{s}: {o:?}");
    }
}

/// What others start, they start outside the sandbox: launchd and cron are not made to.
#[tokio::test]
async fn nobody_else_is_made_to_start_anything() {
    let f = fixture();
    let out = f.at("out/escaped");
    let label = format!("axiom.test.{}", std::process::id());
    let o = f.sh(&format!("launchctl submit -l {label} -- /usr/bin/touch {out}; sleep 1")).await;
    let made = out.as_path().exists();
    let known = std::process::Command::new("launchctl").args(["list", &label]).output().unwrap();
    std::process::Command::new("launchctl").args(["remove", &label]).output().unwrap();
    assert!(!made && !known.status.success(), "{o:?}");
    for s in [
        "echo '* * * * * true' | crontab -",
        "echo true | at now",
        "sudo -n true",
        "su root -c true",
    ] {
        let o = f.sh(s).await;
        assert!(!o.ok(), "{s}: {o:?}");
    }
}

/// Decision 0021: what an agent runs may listen on this computer's loopback address, and be
/// reached there from outside its sandbox; it reaches nothing itself, not even that.
#[tokio::test]
async fn a_server_listens_on_loopback_and_reaches_nothing() {
    let f = fixture();
    let port = 40000 + (std::process::id() % 20000) as u16;
    let job = f.job(&format!("nc -l 127.0.0.1 {port}"), f.profile());
    let mut p = spawn(job).unwrap();
    let mut out = p.stdout.take().unwrap();
    let mut sent = false;
    for _ in 0..100 {
        if let Ok(mut s) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            std::io::Write::write_all(&mut s, b"hello\n").unwrap();
            sent = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(sent, "it listens");
    let mut got = vec![0; 6];
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::io::AsyncReadExt::read_exact(&mut out, &mut got),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(got, b"hello\n");
    p.kill();
    // Out of it, also to loopback: another's server, or its own.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let theirs = listener.local_addr().unwrap().port();
    let o = f.sh(&format!("nc -z -w 2 127.0.0.1 {theirs} && echo reached")).await;
    assert!(!o.out.contains("reached"), "{o:?}");
}
