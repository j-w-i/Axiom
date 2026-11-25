//! What a contained process cannot do (SI-7, SI-8, SI-11, SI-12, SI-13; threat-model T13–T20, T41,
//! T45). Each is tried for real, and what it would have done is looked for outside.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use axiom_policy::{Caps, Host, Keep, Net, Profile, Reach};
use axiom_spawn::{Contain, SpawnError, spawn};

#[tokio::test]
async fn does_its_work() {
    let f = fixture();
    let o = f.sh("cat src/a.txt; echo new > src/b.txt; mkdir d && mv src/b.txt d/ && cat d/b.txt ../lib/l; pwd; echo $HOME").await;
    assert!(o.ok(), "{o:?}");
    assert_eq!(o.out, format!("textnew\nlib{}\n{}\n", f.at("home/w"), f.at("home")));
    assert_eq!(f.read("home/w/d/b.txt").as_deref(), Some("new\n"));
    let o = f.sh("echo $TMPDIR; echo x > $TMPDIR/t && cat $TMPDIR/t; exit 7").await;
    assert_eq!(o.ended.code, Some(7), "{o:?}");
    assert!(o.out.ends_with("x\n"));
    let o = f.sh("ls / >/dev/null; sort src/a.txt | grep -c text; awk 'BEGIN{print 1+1}'; date >/dev/null && echo ok").await;
    assert_eq!(o.out, "1\n2\nok\n", "{o:?}");
}

#[tokio::test]
async fn writes_nowhere_else() {
    let f = fixture();
    for s in [
        "echo x > ../../out/new",
        "echo x > ../../out/f",
        "echo x > ../x",
        "rm ../../out/f",
        "rm ../lib/l",
        "mv ../lib/l l",
        "chmod 777 ../lib/l",
        "touch ../lib/l",
        "ln -s ../../out/new l; echo x > l",
        "ln -s ../../out d; echo x > d/new",
        "ln ../lib/l hard && echo x > hard",
        "mkdir ../../out/d",
        "mv ../w ../w2",
        "echo x > ../.bashrc",
        "echo x > /tmp/axiom-escape-test",
        "echo x > /etc/axiom-escape-test",
        "echo x > ~/new",
    ] {
        let o = f.sh(s).await;
        // On Linux the temporary folder, where this test has its own, is another one inside: what is
        // written there goes nowhere.
        let private =
            cfg!(target_os = "linux") && !["lib/l", "hard", "etc"].iter().any(|w| s.contains(w));
        assert!(private || !o.ok(), "{s}: {o:?}");
    }
    assert_eq!(f.read("out/f").as_deref(), Some("outside"));
    assert_eq!(f.read("home/lib/l").as_deref(), Some("lib"));
    for gone in ["out/new", "out/d", "home/x", "home/w2", "home/.bashrc", "home/new"] {
        assert!(!f.dir.path().join(gone).exists(), "{gone}");
    }
    for x in ["/tmp/axiom-escape-test", "/etc/axiom-escape-test"] {
        assert!(!std::path::Path::new(x).exists());
    }
}

#[tokio::test]
async fn reads_no_secret_and_nothing_of_axiom() {
    let f = fixture();
    for s in [
        "cat ../.ssh/id",
        "cat ../.SSH/ID",
        "ls ../.ssh",
        "cat ../../own/token",
        "cat ../../out/f",
        "ln -s ../.ssh/id l; cat l",
        "ln ../.ssh/id h; cat h",
        "cp -R ../.ssh ssh; cat ssh/id",
        "cat /etc/master.passwd /etc/shadow",
        "cat < ../../own/token",
    ] {
        let o = f.sh(s).await;
        assert!(!o.ok() && !o.out.contains("SECRET") && !o.out.contains("outside"), "{s}: {o:?}");
    }
    // Also when all around it may be read.
    let caps = Caps { read: Reach::of([f.at("home")]), ..f.caps() };
    let wide = Profile::of(&caps, &f.keep(), &system(), &f.at("tmp")).unwrap();
    let o = run(f.job("cat ../lib/l; cat ../.ssh/id; cat ../.SSH/id; ls -a ../.ssh; find .. -name id -exec cat {} +", wide)).await.unwrap();
    assert!(o.out.starts_with("lib") && !o.out.contains("SECRET"), "{o:?}");
}

#[tokio::test]
async fn what_is_protected_is_not_written() {
    let f = fixture();
    for s in [
        "echo x > .git/config",
        "echo x > .git/hooks/pre-commit",
        "rm .git/config",
        "rm -r .git",
        "mv .git g",
        "chmod 777 .git/config",
        "ln .git/config c && echo x > c",
        "ln -s .git/config c2; echo x > c2",
        "cd .git && echo x > config",
        "echo x | tee .git/config",
        "mkdir .git/x",
        "truncate -s 0 .git/config || : > .git/config",
    ] {
        let o = f.sh(s).await;
        assert!(!o.ok(), "{s}: {o:?}");
        assert_eq!(f.read("home/w/.git/config").as_deref(), Some("orig"), "{s}");
    }
    assert!(!f.dir.path().join("home/w/.git/hooks/pre-commit").exists());
    let o = f.sh("cat .git/config").await;
    assert_eq!(o.out, "orig", "it may be read");
}

#[tokio::test]
async fn no_network_and_no_names() {
    let f = fixture();
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    l.set_nonblocking(true).unwrap();
    let o = f.sh(&format!("exec 3<>/dev/tcp/127.0.0.1/{port} && echo open")).await;
    assert!(!o.out.contains("open"), "{o:?}");
    let o = f
        .sh(&format!(
            "/bin/bash -c 'exec 3<>/dev/tcp/127.0.0.1/{port} && echo open'; curl -sm 2 http://127.0.0.1:{port}/ && echo open; nc -w 1 127.0.0.1 {port} </dev/null && echo open"
        ))
        .await;
    assert!(!o.out.contains("open"), "{o:?}");
    assert!(l.accept().is_err(), "nothing arrived");
    let o = f.sh("perl -e 'print \"found\" if gethostbyname(\"example.com\")'; perl -e 'print \"found\" if gethostbyname(\"localhost.example.com\")'").await;
    assert!(!o.out.contains("found"), "{o:?}");
    // A socket of this machine, as the host's own is.
    let path = f.at("home/w/s");
    let u = std::os::unix::net::UnixListener::bind(path.as_path()).unwrap();
    u.set_nonblocking(true).unwrap();
    let o = f.sh("perl -MIO::Socket::UNIX -e 'print \"open\" if IO::Socket::UNIX->new(Peer=>\"s\")'; nc -U s </dev/null && echo open").await;
    if cfg!(target_os = "macos") {
        assert!(!o.out.contains("open"), "{o:?}");
        assert!(u.accept().is_err());
    }
    let out = f.at("out/s");
    let u = std::os::unix::net::UnixListener::bind(out.as_path()).unwrap();
    u.set_nonblocking(true).unwrap();
    let o = f.sh("perl -MIO::Socket::UNIX -e 'print \"open\" if IO::Socket::UNIX->new(Peer=>\"../../out/s\")'").await;
    assert!(!o.out.contains("open") && u.accept().is_err(), "{o:?}");
}

#[tokio::test]
async fn nothing_is_inherited() {
    let f = fixture();
    // The test's own environment has something to leak.
    assert!(std::env::var_os("CARGO_MANIFEST_DIR").is_some() || std::env::var_os("USER").is_some());
    let o = f.sh("env | sed 's/=.*//' | sort | tr '\\n' ' '").await;
    let names: Vec<&str> = o.out.split_whitespace().collect();
    for n in &names {
        assert!(
            [
                "PATH",
                "HOME",
                "TMPDIR",
                "LANG",
                "TERM",
                "NO_COLOR",
                "PAGER",
                "GIT_PAGER",
                "GIT_TERMINAL_PROMPT",
                "GIT_CONFIG_GLOBAL",
                "CI",
                "PWD",
                "OLDPWD",
                "SHLVL",
                "_",
                "__CF_USER_TEXT_ENCODING"
            ]
            .contains(n),
            "{n} in {names:?}"
        );
    }
    // Open files: one that would be passed on by mistake.
    use std::os::fd::AsRawFd;
    let secret = std::fs::File::open(f.at("own/token").as_path()).unwrap();
    let n = keep_open(secret.as_raw_fd());
    let o = f.sh(&format!("cat <&{n} 2>&1; ls /dev/fd")).await;
    assert!(!o.out.contains("SECRET"), "{o:?}");
    for fd in o.out.split_whitespace().filter_map(|x| x.parse::<i32>().ok()) {
        assert!(fd < 8 || fd == 200, "{o:?}");
    }
}

/// A copy of the file that would stay open in a child.
fn keep_open(fd: i32) -> i32 {
    // Without unsafe: a shell-free way to clear close-on-exec is what try_clone does not do, so the
    // copy is made by the standard library for a child's input and leaked there.
    use std::os::fd::{AsFd, AsRawFd, IntoRawFd};
    let f = std::fs::File::open(format!("/dev/fd/{fd}")).unwrap();
    let n = f.as_fd().try_clone_to_owned().unwrap();
    let raw = n.as_raw_fd();
    let _ = n.into_raw_fd();
    raw
}

#[tokio::test]
async fn others_are_out_of_reach() {
    let f = fixture();
    let mut victim = std::process::Command::new("sleep").arg("60").spawn().unwrap();
    let o = f.sh(&format!("kill -9 {} && echo killed; kill -0 1 && echo seen", victim.id())).await;
    assert!(!o.out.contains("killed"), "{o:?}");
    // On Linux the first process is the sandbox's own.
    assert!(cfg!(target_os = "linux") || !o.out.contains("seen"), "{o:?}");
    assert!(victim.try_wait().unwrap().is_none());
    victim.kill().unwrap();
    victim.wait().unwrap();
}

#[tokio::test]
async fn a_set_user_bit_is_not_made() {
    let f = fixture();
    let o = f.sh("cp /bin/ls own-ls && chmod u+s own-ls && test -u own-ls && echo set").await;
    if cfg!(target_os = "macos") {
        assert!(!o.out.contains("set"), "{o:?}");
    }
}

/// SI-8.
#[tokio::test]
async fn what_cannot_be_contained_is_not_started() {
    let f = fixture();
    let marker = f.dir.path().join("home/w/ran");
    let bad = |caps: Caps, keep: Keep| Profile::of(&caps, &keep, &system(), &f.at("tmp")).unwrap();
    let net = Caps { net: Net::of([Host::parse("a.io").unwrap()]), ..f.caps() };
    let e = spawn(f.job("touch ran", bad(net, f.keep()))).unwrap_err();
    assert!(matches!(e, SpawnError::Network), "{e}");
    let gone = Caps { write: Reach::of([f.at("home/w"), f.at("home/nothing")]), ..f.caps() };
    let e = spawn(f.job("touch ran", bad(gone, f.keep()))).unwrap_err();
    assert!(matches!(e, SpawnError::Root(..)), "{e}");
    // A writable folder that turns out to lead into what is hidden.
    std::os::unix::fs::symlink(f.at("own").as_path(), f.dir.path().join("home/l")).unwrap();
    let link = Caps { write: Reach::of([f.at("home/w"), f.at("home/l")]), ..f.caps() };
    let e = spawn(f.job("touch ran", bad(link, f.keep()))).unwrap_err();
    assert!(matches!(e, SpawnError::Hidden(_)), "{e}");
    // Hidden by a name that leads into the workspace: hidden there too.
    let keep = Keep::new(f.at("home"), vec![f.at("home/alias")]);
    std::os::unix::fs::symlink(f.at("home/w").as_path(), f.dir.path().join("home/alias")).unwrap();
    let e = spawn(f.job("touch ran", bad(f.caps(), keep))).unwrap_err();
    assert!(matches!(e, SpawnError::Hidden(_)), "{e}");
    for cwd in ["out", "home", "own", "home/.ssh"] {
        let mut j = f.job("touch ran", f.profile());
        j.cwd = f.at(cwd);
        let e = spawn(j).unwrap_err();
        assert!(matches!(e, SpawnError::Cwd(_)), "{cwd}: {e}");
    }
    let mut j = f.job("touch ran", f.profile());
    j.args.push("a\0b".into());
    assert!(matches!(spawn(j).unwrap_err(), SpawnError::Argument));
    assert!(!marker.exists());
    // A program that may not be read does not run.
    std::fs::copy("/bin/ls", f.dir.path().join("out/prog")).unwrap();
    let mut j = f.job("", f.profile());
    j.program = f.at("out/prog");
    j.args.clear();
    let o = run(j).await.unwrap();
    assert!(!o.ok(), "{o:?}");
}

#[tokio::test]
async fn words_are_never_the_sandboxs() {
    let f = fixture();
    // Arguments that a wrapper would take for its own.
    for a in ["-p", "(version 1)(allow default)", "--bind", "/", "-D", "--", "--dev-bind"] {
        let mut j = f.job("", f.profile());
        j.program = p("/bin/echo");
        j.args = vec![a.into(), "x".into()];
        let o = run(j).await.unwrap();
        assert!(o.ok() && o.out.trim_end().ends_with("x"), "{a}: {o:?}");
    }
    // A folder whose name looks like a rule.
    let odd = "home/w/a\") (allow default) (\"b";
    std::fs::create_dir_all(f.dir.path().join(odd)).unwrap();
    let caps = Caps { write: Reach::of([f.at(odd)]), ..f.caps() };
    let prof = Profile::of(&caps, &f.keep(), &system(), &f.at("tmp")).unwrap();
    let mut j = f.job("echo x > here; echo x > ../no; cat ../../../out/f", prof);
    j.cwd = f.at(odd);
    let o = run(j).await.unwrap();
    assert!(f.dir.path().join(odd).join("here").exists(), "{o:?}");
    assert!(!f.dir.path().join("home/w/no").exists() && !o.out.contains("outside"), "{o:?}");
}

#[tokio::test]
async fn an_environment_takes_no_orders() {
    let f = fixture();
    let mut e = f.env();
    for n in [
        "DYLD_INSERT_LIBRARIES",
        "LD_PRELOAD",
        "BASH_ENV",
        "ENV",
        "IFS",
        "ZDOTDIR",
        "",
        "A=B",
        "A B",
        "PROMPT_COMMAND",
        "SHELLOPTS",
    ] {
        assert!(e.set(n, "x").is_err(), "{n}");
    }
    assert!(e.set("A", "x\0").is_err());
    e.set("RUST_BACKTRACE", "1").unwrap();
    e.set("TERM", "xterm").unwrap();
    let mut j = f.job("echo $RUST_BACKTRACE $TERM", f.profile());
    j.env = e;
    assert_eq!(run(j).await.unwrap().out, "1 xterm\n");
}

#[tokio::test]
async fn outside_every_sandbox_only_with_the_token() {
    let f = fixture();
    let mut j = f.job("cat ../../out/f; env | wc -l", f.profile());
    j.contain = Contain::None(unsandboxed());
    let o = run(j).await.unwrap();
    assert!(o.out.starts_with("outside"), "{o:?}");
}
