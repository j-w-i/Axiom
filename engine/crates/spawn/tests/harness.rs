//! The environment of a program that signs in by itself.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use axiom_spawn::{Contain, Env, Job, SpawnError, spawn};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn it_has_what_it_needs_and_nothing_else() {
    let f = fixture();
    let path = ["/usr/bin", "/bin"].map(p);
    let env = Env::harness(&path, &f.at("home"), "me", &f.at("tmp")).unwrap();
    let job = Job {
        program: p("/usr/bin/env"),
        args: vec![],
        cwd: f.at("home/w"),
        env,
        stdin: false,
        contain: Contain::None(unsandboxed()),
    };
    let o = run(job).await.unwrap();
    let mut all: Vec<&str> = o.out.lines().collect();
    all.sort_unstable();
    let want = [
        format!("HOME={}", f.at("home")),
        "LANG=en_US.UTF-8".into(),
        "PATH=/usr/bin:/bin".into(),
        format!("TMPDIR={}", f.at("tmp")),
        "USER=me".into(),
    ];
    assert_eq!(all, want);
}

#[test]
fn a_name_is_a_name_and_a_folder_one_folder() {
    let f = fixture();
    let make = |user: &str| Env::harness(&[], &f.at("home"), user, &f.at("tmp"));
    for bad in ["", "a\nb", "a\0", "../x"] {
        assert!(matches!(make(bad), Err(SpawnError::Env(_))), "{bad:?}");
    }
    assert!(make("José Müller").is_ok());
    let e = Env::harness(&[p("/a:/b"), p("/c")], &f.at("home"), "me", &f.at("tmp")).unwrap();
    assert!(format!("{e:?}").contains("(\"PATH\", \"/c\")"), "{e:?}");
}

/// Whatever the case, and whatever it is for: nothing that makes something run.
#[test]
fn what_is_set_makes_nothing_run_and_a_secret_is_not_shown() {
    let f = fixture();
    let mut e = Env::harness(&[], &f.at("home"), "me", &f.at("tmp")).unwrap();
    for bad in [
        "",
        "1A",
        "A-B",
        "A=B",
        "LD_PRELOAD",
        "ld_preload",
        "DYLD_X",
        "NODE_OPTIONS",
        "PYTHONPATH",
        "GIT_SSH",
        "SSH_ASKPASS",
        "RUBYOPT",
        "PERL5LIB",
        "SHELL",
        "EDITOR",
        "BASH_ENV",
        "ENV",
    ] {
        assert!(matches!(e.set_said(bad, "x", false), Err(SpawnError::Env(_))), "{bad:?}");
        assert!(e.set_said(bad, "x", true).is_err(), "{bad:?}");
    }
    assert!(
        e.set_said("A", "a\0b", false).is_err()
            && e.set_said(&"A".repeat(129), "x", false).is_err()
    );
    e.set_said("SHOWN", "value-one", false).unwrap();
    e.set_said("HELD", "value-two", true).unwrap();
    // Axiom's own are another matter.
    e.set("GIT_CONFIG_NOSYSTEM", "1").unwrap();
    assert!(e.set("LD_PRELOAD", "x").is_err());
    let text = format!("{e:?}");
    assert!(text.contains("value-one") && text.contains("HELD") && !text.contains("value-two"));
    // What it was is not what it is.
    e.set_said("HELD", "value-three", false).unwrap();
    assert!(format!("{e:?}").contains("value-three"));
}

#[tokio::test]
async fn its_input_stays_open_while_it_is_held() {
    let f = fixture();
    let job = Job {
        program: p("/bin/cat"),
        args: vec![],
        cwd: f.at("home/w"),
        env: Env::harness(&[], &f.at("home"), "me", &f.at("tmp")).unwrap(),
        stdin: true,
        contain: Contain::None(unsandboxed()),
    };
    let mut r = spawn(job).unwrap();
    let (mut to, mut from) = (r.stdin.take().unwrap(), r.stdout.take().unwrap());
    let mut b = [0; 2];
    for _ in 0..3 {
        to.write_all(b"a\n").await.unwrap();
        from.read_exact(&mut b).await.unwrap();
        assert_eq!(&b, b"a\n");
    }
    drop(to);
    assert_eq!(r.wait().await.unwrap().code, Some(0));
}
