//! The exit watchdog's registrations never block the caller (review N13): with a watchdog that
//! is not reading its pipe, registering far more groups than the pipe holds returns at once.
//! Its own test binary: `guard::install` is once per process.
#![cfg(unix)]

use axiom_exec::guard;
use std::time::{Duration, Instant};

#[test]
fn registering_groups_does_not_wait_for_the_watchdog() {
    // A stand-in watchdog that reads nothing for three seconds, then drains its pipe (and ends
    // when this process does).
    guard::install(&[
        "sh".into(),
        "-c".into(),
        "sleep 3; exec cat >/dev/null".into(),
    ])
    .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let start = Instant::now();
        // About 800 KB of registrations: far more than a pipe's buffer.
        for g in 1_000_000..1_050_000u32 {
            guard::watch_group(g);
            guard::unwatch_group(g);
        }
        guard::watch_group(4_000_000);
        let _ = tx.send(start.elapsed());
    });
    let took = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("registering finished");
    assert!(
        took < Duration::from_secs(1),
        "registering waited for the watchdog to read: {took:?}"
    );
    assert!(guard::is_watched(4_000_000));
    assert!(!guard::is_watched(1_000_000));
    guard::unwatch_group(4_000_000);
    assert!(!guard::is_watched(4_000_000));
}
