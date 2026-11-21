//! Git status and show against real temporary repositories (M4-GIT-001).

use axiom_protocol::git::GitFileState as S;
use axiom_workspace::git;
use std::path::Path;
use std::process::Command;

fn g(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    g(d.path(), &["init", "-q"]);
    std::fs::write(d.path().join("keep.txt"), "keep\n").unwrap();
    std::fs::write(d.path().join("mod.txt"), "one\n").unwrap();
    std::fs::write(d.path().join("del.txt"), "gone\n").unwrap();
    std::fs::write(d.path().join("old.txt"), "moving\n").unwrap();
    std::fs::write(d.path().join("bin.dat"), [0u8, 1, 2, 3]).unwrap();
    g(d.path(), &["add", "."]);
    g(d.path(), &["commit", "-q", "-m", "first"]);
    d
}

fn states(st: &axiom_protocol::git::GitStatus) -> Vec<(String, S, S)> {
    let mut v: Vec<_> = st
        .files
        .iter()
        .map(|f| (f.path.0.clone(), f.index, f.worktree))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn every_kind_of_change() {
    let d = repo();
    let p = d.path();
    std::fs::write(p.join("mod.txt"), "two\n").unwrap();
    std::fs::remove_file(p.join("del.txt")).unwrap();
    g(p, &["mv", "old.txt", "new.txt"]);
    std::fs::write(p.join("staged.txt"), "s\n").unwrap();
    g(p, &["add", "staged.txt"]);
    std::fs::write(p.join("untracked file.txt"), "u\n").unwrap();
    let st = git::status(p).unwrap();
    assert!(st.repo);
    assert_eq!(st.branch.as_deref(), Some("main"));
    assert_eq!(st.head.as_ref().map(String::len), Some(7));
    assert_eq!(
        states(&st),
        vec![
            ("del.txt".into(), S::Unmodified, S::Deleted),
            ("mod.txt".into(), S::Unmodified, S::Modified),
            ("new.txt".into(), S::Renamed, S::Unmodified),
            ("staged.txt".into(), S::Added, S::Unmodified),
            ("untracked file.txt".into(), S::Untracked, S::Untracked),
        ]
    );
    let renamed = st.files.iter().find(|f| f.path.0 == "new.txt").unwrap();
    assert_eq!(
        renamed.orig_path.as_ref().map(|p| p.0.as_str()),
        Some("old.txt")
    );
    // The status run took no lock: no index.lock appeared, and nothing was written to .git/index.
    assert!(!p.join(".git/index.lock").exists());
}

#[test]
fn a_conflict() {
    let d = repo();
    let p = d.path();
    g(p, &["checkout", "-q", "-b", "other"]);
    std::fs::write(p.join("mod.txt"), "theirs\n").unwrap();
    g(p, &["commit", "-qam", "theirs"]);
    g(p, &["checkout", "-q", "main"]);
    std::fs::write(p.join("mod.txt"), "ours\n").unwrap();
    g(p, &["commit", "-qam", "ours"]);
    let _ = Command::new("git")
        .current_dir(p)
        .args(["-c", "core.hooksPath=/dev/null", "merge", "-q", "other"])
        .output()
        .unwrap();
    let st = git::status(p).unwrap();
    assert_eq!(
        states(&st),
        vec![("mod.txt".into(), S::Conflicted, S::Conflicted)]
    );
}

#[test]
fn a_subdirectory_workspace_sees_only_itself_relative_to_its_root() {
    let d = repo();
    let p = d.path();
    std::fs::create_dir(p.join("sub")).unwrap();
    std::fs::write(p.join("sub/a.txt"), "a\n").unwrap();
    g(p, &["add", "sub/a.txt"]);
    g(p, &["commit", "-qm", "sub"]);
    std::fs::write(p.join("sub/a.txt"), "changed\n").unwrap();
    std::fs::write(p.join("mod.txt"), "outside change\n").unwrap();
    let st = git::status(&p.join("sub")).unwrap();
    assert_eq!(
        states(&st),
        vec![("a.txt".into(), S::Unmodified, S::Modified)]
    );
    let (text, binary) = git::show(&p.join("sub"), "a.txt", true).unwrap();
    assert_eq!((text.as_deref(), binary), (Some("a\n"), false));
}

#[test]
fn detached_head_and_ahead_behind() {
    let d = repo();
    let p = d.path();
    // A clone tracking the first repository as its upstream.
    let c = tempfile::tempdir().unwrap();
    g(c.path(), &["clone", "-q", p.to_str().unwrap(), "w"]);
    let w = c.path().join("w");
    std::fs::write(w.join("mine.txt"), "1\n").unwrap();
    g(&w, &["add", "mine.txt"]);
    g(&w, &["commit", "-qm", "ahead"]);
    std::fs::write(p.join("theirs.txt"), "2\n").unwrap();
    g(p, &["add", "theirs.txt"]);
    g(p, &["commit", "-qm", "behind"]);
    g(&w, &["fetch", "-q"]);
    let st = git::status(&w).unwrap();
    assert_eq!(st.upstream.as_deref(), Some("origin/main"));
    assert_eq!((st.ahead, st.behind), (1, 1));
    g(&w, &["checkout", "-q", "--detach"]);
    let st = git::status(&w).unwrap();
    assert!(st.branch.is_none() && st.head.is_some());
}

#[test]
fn show_head_index_binary_and_missing() {
    let d = repo();
    let p = d.path();
    std::fs::write(p.join("mod.txt"), "staged\n").unwrap();
    g(p, &["add", "mod.txt"]);
    std::fs::write(p.join("mod.txt"), "working\n").unwrap();
    assert_eq!(
        git::show(p, "mod.txt", true).unwrap(),
        (Some("one\n".into()), false)
    );
    assert_eq!(
        git::show(p, "mod.txt", false).unwrap(),
        (Some("staged\n".into()), false)
    );
    assert_eq!(git::show(p, "bin.dat", true).unwrap(), (None, true));
    std::fs::write(p.join("new.txt"), "n\n").unwrap();
    assert!(matches!(
        git::show(p, "new.txt", true),
        Err(git::GitError::NotFound(_))
    ));
}

#[test]
fn not_a_repository_is_not_an_error() {
    let d = tempfile::tempdir().unwrap();
    let st = git::status(d.path()).unwrap();
    assert!(!st.repo);
    assert!(st.reason.unwrap().contains("not in a Git repository"));
}

/// The budget (M4-GIT-001): `git/status` on this repository under 300 ms.
#[test]
fn status_of_this_repository_is_fast() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let _ = git::status(&root).unwrap(); // warm the file cache
    let t = std::time::Instant::now();
    let st = git::status(&root).unwrap();
    let ms = t.elapsed().as_millis();
    eprintln!(
        "git status of this repository: {ms} ms, {} files",
        st.files.len()
    );
    assert!(st.repo);
    assert!(ms < 300, "{ms} ms");
}
