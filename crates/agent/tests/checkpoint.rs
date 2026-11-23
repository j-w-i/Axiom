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

/// A restore replaces the entry rather than writing through it: a workspace file the agent made
/// a hard link to a file elsewhere (a git hook, a file outside the workspace) gets its own
/// content back, and the other file is left alone (review N14/R5).
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_restore_does_not_write_through_a_hard_link() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir_in(ws.path().parent().unwrap()).unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(outside.path(), "secret", "s\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    std::fs::remove_file(r.join("a.txt")).unwrap();
    std::fs::hard_link(outside.path().join("secret"), r.join("a.txt")).unwrap();
    s.record_post("s").await.unwrap();
    s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(
        read(outside.path(), "secret").unwrap(),
        "s\n",
        "written through the link"
    );
    // No temporary file is left behind.
    let names: Vec<_> = std::fs::read_dir(r)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, vec!["a.txt".to_string()], "{names:?}");
}

/// Review D4: a restore does not write through a directory that became a symlink after the
/// agent last wrote (a conflict the person chose to skip), even one into the workspace's own
/// `.git/hooks`; nothing changes at all.
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_restore_does_not_write_through_a_symlinked_directory() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    std::fs::create_dir_all(r.join(".git/hooks")).unwrap();
    write(r, "a.txt", "a\n");
    write(r, "sub/pre-commit", "#!/bin/sh\necho restored\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    write(r, "a.txt", "agent\n");
    std::fs::remove_dir_all(r.join("sub")).unwrap();
    s.record_post("s").await.unwrap();
    // Since the agent's last write: `sub` is a symlink into the hooks.
    std::os::unix::fs::symlink(".git/hooks", r.join("sub")).unwrap();
    let e = s.restore("s", 1, 1, true).await.unwrap_err();
    assert!(
        matches!(&e, CheckpointError::Io(m) if m.contains("symlink")),
        "{e:?}"
    );
    assert!(
        !r.join(".git/hooks/pre-commit").exists(),
        "a hook was planted"
    );
    assert_eq!(
        read(r, "a.txt").unwrap(),
        "agent\n",
        "a refused restore changed a file"
    );
}

/// A directory the agent replaced with a symlink is put back: the restore removes the symlink
/// before it writes what was under the directory, so the check does not stand in its way, and
/// what the symlink pointed to is left alone.
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_restore_puts_back_a_directory_the_agent_made_a_symlink() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "sub/f.txt", "f\n");
    write(r, "other/f.txt", "other\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    std::fs::remove_dir_all(r.join("sub")).unwrap();
    std::os::unix::fs::symlink("other", r.join("sub")).unwrap();
    s.record_post("s").await.unwrap();
    s.restore("s", 1, 1, false).await.unwrap();
    assert!(!std::fs::symlink_metadata(r.join("sub"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(r, "sub/f.txt").unwrap(), "f\n");
    assert_eq!(read(r, "other/f.txt").unwrap(), "other\n");
}

/// Review D4, decided: a restore never writes into a git directory, a bare repository's
/// (`<name>.git/hooks` run on a push to it) included; hooks there change only through the gate.
/// Review E3: those paths are skipped, and the rest of the restore goes ahead; an ordinary name
/// that ends in `.git` (a file, a folder that is no repository) is restored like any other. The
/// workspace's own configuration (`.axiom/`) is restored like any file: the restore is the
/// person's undo of what the agent left.
#[tokio::test]
async fn checkpoint_restore_skips_git_directories_and_restores_the_rest() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(
        r,
        ".axiom/project.toml",
        "[agent.policy]\ndefault = \"ask\"\n",
    );
    write(r, "a.txt", "a\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    write(
        r,
        ".axiom/project.toml",
        "[agent.policy]\ndefault = \"allow\"\n",
    );
    s.record_post("s").await.unwrap();
    s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(
        read(r, ".axiom/project.toml").unwrap(),
        "[agent.policy]\ndefault = \"ask\"\n"
    );

    // A hooks folder in a `<name>.git` (by its name), a bare repository's own file (found on
    // disk: it has a HEAD), and ordinary names ending in `.git`.
    write(r, "mirror.git/hooks/post-update", "orig\n");
    git(r, &["init", "-q", "--bare", "bare.git"]);
    write(r, "bare.git/description", "orig\n");
    write(r, "deps.git", "orig\n");
    write(r, "vendor/lib.git/notes.txt", "orig\n");
    let n = s.create("s", "with the mirror", 1).await.unwrap().number;
    write(r, "mirror.git/hooks/post-update", "changed\n");
    write(r, "bare.git/description", "changed\n");
    write(r, "deps.git", "changed\n");
    write(r, "vendor/lib.git/notes.txt", "changed\n");
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    let res = s.restore("s", n, 1, false).await.unwrap();
    assert_eq!(
        read(r, "mirror.git/hooks/post-update").unwrap(),
        "changed\n",
        "a restore wrote a hook"
    );
    assert_eq!(read(r, "bare.git/description").unwrap(), "changed\n");
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(read(r, "deps.git").unwrap(), "orig\n");
    assert_eq!(read(r, "vendor/lib.git/notes.txt").unwrap(), "orig\n");
    let mut restored: Vec<&str> = res.restored.iter().map(|p| p.0.as_str()).collect();
    restored.sort();
    assert_eq!(restored, ["a.txt", "deps.git", "vendor/lib.git/notes.txt"]);
}

/// Review D4: the restored file gets the permission bits of the file it replaces, never its
/// setuid, setgid or sticky bit.
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_restore_never_carries_a_setuid_bit() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "run.sh", "#!/bin/sh\necho one\n");
    std::fs::set_permissions(r.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    write(r, "run.sh", "#!/bin/sh\necho two\n");
    s.record_post("s").await.unwrap();
    // Still `100755` to git: the change is not a conflict.
    std::fs::set_permissions(r.join("run.sh"), std::fs::Permissions::from_mode(0o7755)).unwrap();
    let set = std::fs::metadata(r.join("run.sh"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_ne!(
        set & 0o7000,
        0,
        "this file system keeps no setuid bit: {set:o}"
    );
    s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, "run.sh").unwrap(), "#!/bin/sh\necho one\n");
    let mode = std::fs::metadata(r.join("run.sh"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o755, "{mode:o}");
}
