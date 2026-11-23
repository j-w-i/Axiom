//! A registration is in the watchdog's pipe before the caller hands the process out
//! (`guard::flush`): a daemon killed right after `terminal/create` answered still takes the
//! terminal along (CI on 1840dc9). Its own test binary: `guard::install` is once per process.
#![cfg(unix)]

use axiom_exec::guard;
use std::time::{Duration, Instant};

#[test]
fn flush_waits_until_the_registration_is_in_the_pipe() {
    let seen = std::env::temp_dir().join(format!("axiom-guard-flush-{}", std::process::id()));
    let _ = std::fs::remove_file(&seen);
    // A stand-in watchdog that reads nothing for a second, then copies what it gets to a file.
    guard::install(&[
        "sh".into(),
        "-c".into(),
        format!("sleep 1; exec cat > '{}'", seen.display()).into(),
    ])
    .unwrap();
    // Fill the pipe, so the line below waits behind the rest in the queue.
    for g in 2_000_000..2_050_000u32 {
        guard::watch_group(g);
        guard::unwatch_group(g);
    }
    guard::watch_group(5_000_000);
    let start = Instant::now();
    assert!(
        guard::flush(Duration::from_secs(10)),
        "the registration never reached the pipe"
    );
    // It could only be written once the watchdog started reading.
    assert!(
        start.elapsed() >= Duration::from_millis(500),
        "flush returned before the line was in the pipe: {:?}",
        start.elapsed()
    );
    // And the watchdog does get it.
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(&seen).unwrap_or_default();
        if text.contains("+5000000\n") {
            break;
        }
        assert!(
            Instant::now() < end,
            "the watchdog never read the registration"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    guard::unwatch_group(5_000_000);
    let _ = std::fs::remove_file(&seen);
}
