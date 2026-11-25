//! Ending a process ends everything it started (SI-24).
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use common::*;

use std::time::{Duration, Instant};

use axiom_spawn::spawn;
use tokio::io::{AsyncBufReadExt, BufReader};

async fn lines(r: &mut axiom_spawn::Running, n: usize) -> Vec<String> {
    let mut l = BufReader::new(r.stdout.take().unwrap()).lines();
    let mut v = Vec::new();
    for _ in 0..n {
        let x =
            tokio::time::timeout(Duration::from_secs(20), l.next_line()).await.unwrap().unwrap();
        v.push(x.unwrap());
    }
    v
}

/// Those whose arguments hold `mark`: numbers inside a sandbox may be others than outside.
fn running(mark: &str) -> usize {
    let o = std::process::Command::new("ps").args(["-axo", "stat=,args="]).output().unwrap();
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter(|l| l.contains(mark) && !l.trim_start().starts_with('Z') && l.contains("sleep "))
        .count()
}

fn mark(n: u32) -> String {
    format!("4{:05}{n}", std::process::id() % 100_000)
}

async fn gone(mark: &str) {
    let t = Instant::now();
    while running(mark) > 0 {
        assert!(t.elapsed() < Duration::from_secs(5), "{} still run", running(mark));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn killing_ends_all_of_it_at_once() {
    let f = fixture();
    let m = mark(1);
    let script =
        format!("sleep {m} & (sleep {m} & sleep {m}) & trap '' TERM HUP INT; echo up; sleep {m}");
    let mut r = spawn(f.job(&script, f.profile())).unwrap();
    lines(&mut r, 1).await;
    while running(&m) < 4 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let t = Instant::now();
    r.kill();
    let e = r.wait().await.unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    assert!(e.signal.is_some() || e.code != Some(0), "{e:?}");
    gone(&m).await;
}

#[tokio::test]
async fn what_leaves_the_group_is_found() {
    let f = fixture();
    let m = mark(2);
    // A session of its own, its parent gone, its output elsewhere.
    let script = format!(
        "perl -MPOSIX -e 'if (!fork) {{ setsid(); if (!fork) {{ open STDOUT, \">/dev/null\"; open STDERR, \">/dev/null\"; exec \"sleep {m}\" }} exit }} wait'; echo up; sleep {m}"
    );
    let mut r = spawn(f.job(&script, f.profile())).unwrap();
    lines(&mut r, 1).await;
    while running(&m) < 2 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    r.kill();
    r.wait().await.unwrap();
    gone(&m).await;
}

#[tokio::test]
async fn what_is_left_behind_ends_with_it() {
    let f = fixture();
    let m = mark(3);
    let o = f.sh(&format!("sleep {m} >/dev/null 2>&1 & echo done")).await;
    assert_eq!((o.out.as_str(), o.ended.code), ("done\n", Some(0)), "{o:?}");
    gone(&m).await;
}

#[tokio::test]
async fn dropping_ends_it() {
    let f = fixture();
    let m = mark(4);
    let mut r = spawn(f.job(&format!("sleep {m} & echo up; sleep {m}"), f.profile())).unwrap();
    lines(&mut r, 1).await;
    drop(r);
    gone(&m).await;
}

#[tokio::test]
async fn it_is_asked_first_if_there_is_time() {
    let f = fixture();
    let m = mark(5);
    let script = format!("trap 'echo bye > src/bye; exit 3' TERM; echo up; sleep {m} & wait");
    let mut r = spawn(f.job(&script, f.profile())).unwrap();
    lines(&mut r, 1).await;
    let e = r.stop(Duration::from_secs(10)).await.unwrap();
    if cfg!(target_os = "macos") {
        assert_eq!(e.code, Some(3), "{e:?}");
        assert_eq!(f.read("home/w/src/bye").as_deref(), Some("bye\n"));
    }
    gone(&m).await;
    let m = mark(6);
    let mut r =
        spawn(f.job(&format!("trap '' TERM; echo up; sleep {m}; sleep {m}"), f.profile())).unwrap();
    lines(&mut r, 1).await;
    let t = Instant::now();
    r.stop(Duration::from_millis(200)).await.unwrap();
    assert!(t.elapsed() < Duration::from_secs(5));
    gone(&m).await;
}

#[tokio::test]
async fn others_with_the_same_name_live() {
    let f = fixture();
    let m = mark(7);
    let mut other = std::process::Command::new("sleep").arg(&m).spawn().unwrap();
    let mut r = spawn(f.job("echo up; sleep 60", f.profile())).unwrap();
    lines(&mut r, 1).await;
    r.kill();
    r.wait().await.unwrap();
    assert!(other.try_wait().unwrap().is_none());
    other.kill().unwrap();
    other.wait().unwrap();
}
