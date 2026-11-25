//! SI-7: no other crate starts a process. Read from the source, so that a new way shows up here.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        if e.file_type().unwrap().is_dir() {
            sources(&e.path(), out);
        } else if e.path().extension().is_some_and(|x| x == "rs") {
            out.push(e.path());
        }
    }
}

#[test]
fn nobody_else_starts_a_process() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut seen = 0;
    for c in std::fs::read_dir(crates).unwrap().flatten() {
        let src = c.path().join("src");
        if c.file_name() == "spawn" || !src.is_dir() {
            continue;
        }
        let mut all = Vec::new();
        sources(&src, &mut all);
        for f in all {
            seen += 1;
            let text = std::fs::read_to_string(&f).unwrap();
            // What only tests do comes after this, at the end of a file.
            let text = text.split("#[cfg(test)]\nmod tests").next().unwrap();
            let text = text.split("#[cfg(all(test, unix))]\nmod tests").next().unwrap();
            for way in [
                "process::Command",
                "Command::new",
                "posix_spawn",
                "libc::fork",
                "libc::exec",
                "CreateProcess",
                "std::process::{Command",
            ] {
                assert!(!text.contains(way), "{}: {way}", f.display());
            }
        }
    }
    assert!(seen > 40, "{seen}");
}
