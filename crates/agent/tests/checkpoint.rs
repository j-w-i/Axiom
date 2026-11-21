//! Checkpoints against the real `git` (M3-CKPT-001).

use axiom_agent::checkpoint::{CheckpointError, Shadow};
use axiom_protocol::agent::ConflictReason;
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, text).unwrap();
}

fn read(root: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(root.join(rel)).ok()
}

/// Everything about the user's repository a checkpoint must not change.
fn repo_state(root: &Path) -> Vec<String> {
    let index = std::fs::read(root.join(".git/index")).unwrap();
    let mut files: Vec<String> = walk(&root.join(".git"))
        .into_iter()
        .filter(|p| !p.ends_with("index"))
        .collect();
    files.sort();
    vec![
        format!("{:x?}", sha(&index)),
        git(
            root,
            &[
                "--no-optional-locks",
                "status",
                "--porcelain=v2",
                "--branch",
            ],
        ),
        git(root, &["rev-parse", "HEAD"]),
        git(root, &["for-each-ref"]),
        git(root, &["stash", "list"]),
        files.join("\n"),
    ]
}

fn walk(dir: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            v.extend(walk(&p));
        } else {
            let bytes = std::fs::read(&p).unwrap();
            v.push(format!("{} {:x?}", p.display(), sha(&bytes)));
        }
    }
    v
}

fn sha(b: &[u8]) -> [u8; 8] {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish().to_le_bytes()
}

#[tokio::test]
async fn checkpoint_never_changes_the_users_repository() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    git(r, &["init", "-q", "-b", "main"]);
    write(r, ".gitignore", "target/\n");
    write(r, "src/a.rs", "fn a() {}\n");
    write(r, "src/b.rs", "fn b() {}\n");
    git(r, &["add", "-A"]);
    git(r, &["commit", "-qm", "init"]);
    // A stash, a staged change and an unstaged change.
    write(r, "src/a.rs", "fn a() { 1 }\n");
    git(r, &["stash", "-q"]);
    write(r, "src/b.rs", "fn b() { 2 }\n");
    git(r, &["add", "src/b.rs"]);
    write(r, "src/a.rs", "fn a() { 3 }\n");
    write(r, "untracked.txt", "u\n");
    let before = repo_state(r);

    let s = Shadow::new(data.path(), r);
    let c1 = s.create("s1", "before fs.applyPatch", 1).await.unwrap();
    assert_eq!(c1.number, 1);
    write(r, "src/a.rs", "fn a() { agent }\n");
    write(r, "src/new.rs", "new\n");
    std::fs::remove_file(r.join("untracked.txt")).unwrap();
    s.record_post("s1").await.unwrap();
    let res = s.restore("s1", 1, 2, false).await.unwrap();
    let mut restored: Vec<_> = res.restored.iter().map(|p| p.0.as_str()).collect();
    restored.sort();
    assert_eq!(restored, ["src/a.rs", "src/new.rs", "untracked.txt"]);
    assert!(res.conflicts.is_empty());
    assert_eq!(read(r, "src/a.rs").unwrap(), "fn a() { 3 }\n");
    assert_eq!(read(r, "src/new.rs"), None);
    assert_eq!(read(r, "untracked.txt").unwrap(), "u\n");

    assert_eq!(repo_state(r), before, "the user's repository changed");
    let list = s.list("s1").await.unwrap();
    assert_eq!(
        list.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(),
        ["before fs.applyPatch", "before restore to 1"]
    );
    assert_eq!(res.undo_checkpoint, 2);
}

#[tokio::test]
async fn checkpoint_non_git_workspace_round_trip_and_undo() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(r, "dir/b.txt", "b\n");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        write(r, "run.sh", "#!/bin/sh\n");
        std::fs::set_permissions(r.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    write(r, "a.txt", "changed\n");
    std::fs::remove_file(r.join("dir/b.txt")).unwrap();
    write(r, "c.txt", "added\n");
    #[cfg(unix)]
    std::fs::remove_file(r.join("run.sh")).unwrap();
    s.record_post("s").await.unwrap();
    s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(read(r, "dir/b.txt").unwrap(), "b\n");
    assert_eq!(read(r, "c.txt"), None);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(r.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111, "the executable bit is restored");
    }
    // Undo the restore: back to the agent's state.
    s.restore("s", 2, 1, false).await.unwrap();
    assert_eq!(read(r, "a.txt").unwrap(), "changed\n");
    assert_eq!(read(r, "c.txt").unwrap(), "added\n");
}

#[tokio::test]
async fn checkpoint_conflicts_leave_the_users_changes_alone() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(r, "b.txt", "b\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    // The agent changes both and creates one…
    write(r, "a.txt", "agent a\n");
    write(r, "b.txt", "agent b\n");
    s.record_post("s").await.unwrap();
    // …then the user edits a, and creates d.
    write(r, "a.txt", "user a\n");
    write(r, "d.txt", "user d\n");
    let err = s.restore("s", 1, 1, false).await.unwrap_err();
    let CheckpointError::Conflicts(c) = err else {
        panic!("{err:?}")
    };
    let got: Vec<_> = c.iter().map(|c| (c.path.0.as_str(), c.reason)).collect();
    assert_eq!(
        got,
        [
            ("a.txt", ConflictReason::ModifiedSinceAgent),
            ("d.txt", ConflictReason::CreatedSinceAgent)
        ]
    );
    // Nothing changed, and no undo checkpoint was taken.
    assert_eq!(read(r, "b.txt").unwrap(), "agent b\n");
    assert_eq!(s.list("s").await.unwrap().len(), 1);
    // Skipping conflicts restores the rest.
    let res = s.restore("s", 1, 1, true).await.unwrap();
    assert_eq!(
        res.restored
            .iter()
            .map(|p| p.0.as_str())
            .collect::<Vec<_>>(),
        ["b.txt"]
    );
    assert_eq!(read(r, "a.txt").unwrap(), "user a\n");
    assert_eq!(read(r, "b.txt").unwrap(), "b\n");
    assert_eq!(read(r, "d.txt").unwrap(), "user d\n");
}

#[tokio::test]
async fn checkpoint_ignores_ignored_and_large_files() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, ".gitignore", "target/\n");
    write(r, "target/out.o", "old\n");
    let big = vec![b'x'; (axiom_agent::checkpoint::MAX_FILE_BYTES + 1) as usize];
    std::fs::write(r.join("big.bin"), &big).unwrap();
    let s = Shadow::new(data.path(), r);
    let c = s.create("s", "start", 1).await.unwrap();
    assert_eq!(
        c.skipped.iter().map(|p| p.0.as_str()).collect::<Vec<_>>(),
        ["big.bin"]
    );
    write(r, "target/out.o", "new\n");
    s.record_post("s").await.unwrap();
    let res = s.restore("s", 1, 1, false).await.unwrap();
    assert!(res.restored.is_empty(), "{:?}", res.restored);
    assert_eq!(read(r, "target/out.o").unwrap(), "new\n");
    assert!(r.join("big.bin").exists());
}

#[tokio::test]
async fn checkpoint_without_git_is_prerequisite_missing() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let s = Shadow::with_git(data.path(), ws.path(), "/nonexistent/git".into());
    assert_eq!(
        s.create("s", "x", 1).await.unwrap_err(),
        CheckpointError::GitMissing
    );
    assert!(matches!(
        Shadow::new(data.path(), ws.path())
            .restore("s", 9, 1, false)
            .await,
        Err(CheckpointError::NotFound(9) | CheckpointError::Git(_))
    ));
}

#[tokio::test]
async fn checkpoint_timing_on_this_repository() {
    // Snapshot cost on a real, medium-sized tree (this repository), for the handoff.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let data = tempfile::tempdir().unwrap();
    let s = Shadow::new(data.path(), &root.canonicalize().unwrap());
    let t = std::time::Instant::now();
    s.create("timing", "first", 1).await.unwrap();
    let first = t.elapsed();
    let t = std::time::Instant::now();
    s.create("timing", "second", 1).await.unwrap();
    let second = t.elapsed();
    eprintln!("checkpoint of this repository: first {first:?}, second {second:?}");
}
