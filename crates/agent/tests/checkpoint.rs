//! Checkpoints against the real `git` (M3-CKPT-001).

use axiom_agent::checkpoint::{CheckpointError, NotRestored, Shadow};
use axiom_protocol::agent::ConflictReason;
use axiom_protocol::WorkspacePath;
use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::time::{Duration, Instant};

fn wp(s: &str) -> WorkspacePath {
    WorkspacePath(s.into())
}

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
    let res = s.restore("s1", 1, 2, false).await.unwrap().result;
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
    let res = s.restore("s", 1, 1, true).await.unwrap().result;
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
    let res = s.restore("s", 1, 1, false).await.unwrap().result;
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
    let out = s.restore("s", n, 1, false).await.unwrap();
    let res = &out.result;
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
    // The skipped paths are reported, so the person can be told (review E round).
    assert_eq!(
        out.not_restored,
        [
            (wp("bare.git/description"), NotRestored::GitDirectory),
            (
                wp("mirror.git/hooks/post-update"),
                NotRestored::GitDirectory
            ),
        ]
    );
    let notice = out.notice().expect("a notice");
    assert!(
        notice.contains("left 2 paths")
            && notice.contains("bare.git/description, mirror.git/hooks/post-update"),
        "{notice}"
    );
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

/// Review E round (shadow git): a nested repository with no commit yet has no HEAD for `git add`
/// to record as a git link, which made every checkpoint (and so every write) fail. The
/// checkpoint is taken without it, like a nested repository's content, and a restore leaves it
/// alone.
#[tokio::test]
async fn checkpoint_of_a_workspace_with_a_nested_repository_without_a_commit() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    git(r, &["init", "-q", "sub"]);
    write(r, "sub/f.txt", "nested\n");
    let s = Shadow::new(data.path(), r);
    let c = s.create("s", "start", 1).await.unwrap();
    assert!(c.skipped.is_empty(), "{:?}", c.skipped);
    write(r, "a.txt", "agent\n");
    write(r, "sub/f.txt", "nested, changed\n");
    s.record_post("s").await.unwrap();
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(
        out.result
            .restored
            .iter()
            .map(|p| p.0.as_str())
            .collect::<Vec<_>>(),
        ["a.txt"]
    );
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(read(r, "sub/f.txt").unwrap(), "nested, changed\n");
    assert!(out.not_restored.is_empty(), "{:?}", out.not_restored);
}

/// An unreadable file is left out of the checkpoint, which is still taken (`--ignore-errors`).
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_of_a_workspace_with_an_unreadable_file() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(r, "secret.txt", "s\n");
    let secret = r.join("secret.txt");
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&secret).is_ok() {
        eprintln!("running as a user who reads any file: nothing to check");
        return;
    }
    let s = Shadow::new(data.path(), r);
    let made = s.create("s", "start", 1).await;
    std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o644)).unwrap();
    made.unwrap();
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    // The checkpoint never saw it: a restore does not remove it as "created since".
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(read(r, "secret.txt").unwrap(), "s\n");
    assert_eq!(
        out.not_restored,
        [(wp("secret.txt"), NotRestored::NotCaptured)]
    );
}

/// A file over 16 MiB the checkpoint skipped, which the agent then shrank, is not removed by a
/// restore as if it were new: the checkpoint never had its content.
#[tokio::test]
async fn checkpoint_restore_never_removes_a_file_it_skipped() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    let big = vec![b'x'; (axiom_agent::checkpoint::MAX_FILE_BYTES + 1) as usize];
    std::fs::write(r.join("big.bin"), &big).unwrap();
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    write(r, "big.bin", "small\n");
    s.record_post("s").await.unwrap();
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, "big.bin").unwrap(), "small\n");
    assert!(out.result.restored.is_empty(), "{:?}", out.result.restored);
    assert_eq!(
        out.not_restored,
        [(wp("big.bin"), NotRestored::NotCaptured)]
    );
    assert!(out.notice().unwrap().contains("big.bin"));
}

/// A nested repository is never removed or replaced by a restore, even one made after the
/// checkpoint (removing its directory would lose it, and failed half-way through the restore);
/// it is reported instead, and the rest is restored.
#[tokio::test]
async fn checkpoint_restore_leaves_nested_repositories_and_reports_them() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    // The agent clones something, and edits a file.
    let x = r.join("vendor/x");
    std::fs::create_dir_all(&x).unwrap();
    git(&x, &["init", "-q"]);
    write(&x, "lib.rs", "lib\n");
    git(&x, &["add", "."]);
    git(&x, &["commit", "-q", "-m", "x"]);
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(read(&x, "lib.rs").unwrap(), "lib\n");
    assert!(x.join(".git/HEAD").exists());
    assert_eq!(
        out.result
            .restored
            .iter()
            .map(|p| p.0.as_str())
            .collect::<Vec<_>>(),
        ["a.txt"]
    );
    assert_eq!(
        out.not_restored,
        [(wp("vendor/x"), NotRestored::NestedRepository)]
    );
    let notice = out.notice().unwrap();
    assert!(
        notice.contains("left 1 path")
            && notice.contains("Nested repositories")
            && notice.contains("vendor/x"),
        "{notice}"
    );
}

/// Review E round (shadow git): every `git` the store runs is bounded. A `git` that hangs (here a
/// stand-in whose `add` never finishes) is killed at its bound, the checkpoint fails with
/// `TimedOut`, and the store is usable again.
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_git_that_hangs_is_stopped() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    write(ws.path(), "a.txt", "a\n");
    let pid_file = bin.path().join("pid");
    let stand_in = bin.path().join("git");
    std::fs::write(
        &stand_in,
        format!(
            "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = add ]; then echo $$ > '{}'; exec sleep 300; fi\ndone\nexec git \"$@\"\n",
            pid_file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
    let bound = Duration::from_secs(2);
    let s = Shadow::with_git(data.path(), ws.path(), stand_in)
        .with_timeouts(bound, Duration::from_secs(60));
    let t = Instant::now();
    let e = tokio::time::timeout(Duration::from_secs(60), s.create("s", "start", 1))
        .await
        .expect("the hanging git was not stopped")
        .unwrap_err();
    assert_eq!(
        e,
        CheckpointError::TimedOut {
            command: "add".into(),
            after: bound
        }
    );
    assert!(t.elapsed() < Duration::from_secs(30), "{:?}", t.elapsed());
    assert!(e.to_string().contains("did not finish within 2 s"), "{e}");
    // The stand-in was killed (and reaped), not left running.
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let alive = Command::new("kill")
        .args(["-0", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(!alive, "git {} still runs", pid.trim());
    // The store's lock was released.
    assert!(s.list("s").await.unwrap().is_empty());
}

/// Review E round (G1, for the shadow git): a filter driver, fsmonitor or hook the workspace's
/// repositories configure never runs when a checkpoint is taken, even with a `.gitattributes`
/// that routes every file through the filter: the shadow git reads only its own configuration.
/// A nested repository's own configuration is not read either.
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_runs_no_program_the_workspace_configures() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    let marker = data.path().join("ran");
    let script = format!(
        "#!/bin/sh\necho ran >> '{}'\ntr a-z A-Z\n",
        marker.display()
    );
    let program = |dir: &Path| {
        write(dir, "tools/upper.sh", &script);
        let p = dir.join("tools/upper.sh");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p.display().to_string()
    };
    git(r, &["init", "-q"]);
    let up = program(r);
    git(r, &["config", "filter.up.clean", &up]);
    git(r, &["config", "filter.up.process", &up]);
    git(r, &["config", "filter.up.required", "true"]);
    git(r, &["config", "core.fsmonitor", &up]);
    write(r, ".gitattributes", "* filter=up\n");
    write(r, "a.txt", "plain\n");
    // A nested repository with a commit, configured the same way.
    let x = r.join("vendor/x");
    std::fs::create_dir_all(&x).unwrap();
    git(&x, &["init", "-q"]);
    write(&x, "lib.txt", "lib\n");
    git(&x, &["add", "lib.txt"]);
    git(&x, &["commit", "-q", "-m", "x"]);
    let up_x = program(&x);
    git(&x, &["config", "filter.up.clean", &up_x]);
    git(&x, &["config", "core.fsmonitor", &up_x]);
    write(&x, ".gitattributes", "* filter=up\n");

    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    // Changed files, so a second snapshot hashes them again.
    write(r, "a.txt", "agent\n");
    write(&x, "lib.txt", "lib, changed\n");
    s.record_post("s").await.unwrap();
    s.restore("s", 1, 1, false).await.unwrap();
    assert!(
        !marker.exists(),
        "a configured program ran: {:?}",
        std::fs::read_to_string(&marker)
    );
    // The checkpoint holds the bytes on disk, not a filter's output.
    assert_eq!(read(r, "a.txt").unwrap(), "plain\n");
}

/// Review H1: a file the person wrote where the snapshot now has no entry, because it is ignored
/// now, is not written over by a restore: no checkpoint has its content. It is reported as not
/// captured, and the rest is restored.
#[tokio::test]
async fn checkpoint_restore_never_writes_over_a_file_ignored_now() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, ".env", "OLD=1\n");
    write(r, "a.txt", "a\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    // The agent deletes `.env` and edits a file…
    std::fs::remove_file(r.join(".env")).unwrap();
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    // …then the person ignores `.env` and writes a new one.
    write(r, ".gitignore", ".env\n");
    write(r, ".env", "SECRET=new\n");
    let out = s.restore("s", 1, 1, true).await.unwrap();
    assert_eq!(read(r, ".env").unwrap(), "SECRET=new\n", "written over");
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(read(r, ".gitignore").unwrap(), ".env\n");
    let restored: Vec<&str> = out.result.restored.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(restored, ["a.txt"]);
    let conflicts: Vec<_> = out
        .result
        .conflicts
        .iter()
        .map(|c| (c.path.0.as_str(), c.reason))
        .collect();
    assert_eq!(
        conflicts,
        [(".gitignore", ConflictReason::CreatedSinceAgent)]
    );
    assert_eq!(out.not_restored, [(wp(".env"), NotRestored::NotCaptured)]);
    assert!(out.notice().unwrap().contains("ignored"));
}

/// Review H1, the agent's variant and the undo: the agent ignores `.env` and the person writes
/// one. The restore leaves it (it removes the ignore rule), and its undo checkpoint, which does
/// not capture `.env` either (ignored when it was taken), leaves it too: undoing the restore does
/// not delete a file the restore never touched.
#[tokio::test]
async fn checkpoint_restore_and_its_undo_leave_a_file_ignored_when_they_ran() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, ".env", "OLD=1\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    std::fs::remove_file(r.join(".env")).unwrap();
    write(r, ".gitignore", ".env\n");
    s.record_post("s").await.unwrap();
    write(r, ".env", "SECRET=new\n");
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(r, ".env").unwrap(), "SECRET=new\n", "written over");
    assert_eq!(read(r, ".gitignore"), None);
    assert_eq!(
        out.result
            .restored
            .iter()
            .map(|p| p.0.as_str())
            .collect::<Vec<_>>(),
        [".gitignore"]
    );
    assert_eq!(out.not_restored, [(wp(".env"), NotRestored::NotCaptured)]);
    // Undo the restore: the ignore rule comes back, and `.env` stays.
    let undo = s
        .restore("s", out.result.undo_checkpoint, 1, false)
        .await
        .unwrap();
    assert_eq!(read(r, ".gitignore").unwrap(), ".env\n");
    assert_eq!(
        read(r, ".env").as_deref(),
        Some("SECRET=new\n"),
        "the undo deleted it"
    );
    assert_eq!(undo.not_restored, [(wp(".env"), NotRestored::NotCaptured)]);
}

/// The check for a file on disk the snapshot lacks does not stop a restore of a name whose case
/// the agent changed (on a case-insensitive file system both names are the same file, captured
/// under the new name once the old one left the store's index).
#[tokio::test]
async fn checkpoint_restore_puts_back_a_name_whose_case_the_agent_changed() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "Readme.md", "old\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    std::fs::remove_file(r.join("Readme.md")).unwrap();
    s.record_post("s").await.unwrap();
    write(r, "README.md", "new\n");
    s.record_post("s").await.unwrap();
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert!(out.not_restored.is_empty(), "{:?}", out.not_restored);
    assert_eq!(read(r, "Readme.md").as_deref(), Some("old\n"));
}

/// Review H2: a restore writes nothing inside a nested repository, a path under one included. The
/// agent replaced `sub` with a clone and the person edited a file in it (uncommitted there, so in
/// no checkpoint); the restore leaves it and names the repository once.
#[tokio::test]
async fn checkpoint_restore_writes_nothing_under_a_nested_repository() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(r, "sub/f.txt", "v1\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    // The agent removes `sub`, then makes it a clone, and edits a file.
    std::fs::remove_dir_all(r.join("sub")).unwrap();
    s.record_post("s").await.unwrap();
    let sub = r.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    git(&sub, &["init", "-q"]);
    write(&sub, "f.txt", "cloned\n");
    git(&sub, &["add", "."]);
    git(&sub, &["commit", "-q", "-m", "c"]);
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    // The person edits a file in it.
    write(&sub, "f.txt", "PRECIOUS\n");
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(&sub, "f.txt").unwrap(), "PRECIOUS\n", "written over");
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(
        out.result
            .restored
            .iter()
            .map(|p| p.0.as_str())
            .collect::<Vec<_>>(),
        ["a.txt"]
    );
    assert_eq!(
        out.not_restored,
        [(wp("sub"), NotRestored::NestedRepository)]
    );
}

/// Review H8: the gate's `makes_git_dir` rule holds in a restore. It does not write one of git's
/// own entries into a folder that already holds what makes a git directory (`pkg/refs/`), nor
/// two of those into one folder (`src/HEAD` with `src/commondir`). One alone in an ordinary
/// folder (`docs/HEAD`) is restored, as the gate does not ask for it, and so is a file in a
/// folder whose name is one of them (`lib/refs/a.ts` beside `lib/refs/b.ts`), which adds none.
#[tokio::test]
async fn checkpoint_restore_does_not_make_a_git_directory() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(r, "src/HEAD", "ref: refs/heads/main\n");
    write(r, "src/commondir", "../elsewhere\n");
    write(r, "pkg/refs/README", "refs\n");
    write(r, "pkg/config", "[core]\n");
    write(r, "docs/HEAD", "a heading\n");
    write(r, "lib/refs/a.ts", "a\n");
    write(r, "lib/refs/b.ts", "b\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    for f in [
        "src/HEAD",
        "src/commondir",
        "pkg/config",
        "docs/HEAD",
        "lib/refs/a.ts",
    ] {
        std::fs::remove_file(r.join(f)).unwrap();
    }
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    let out = s.restore("s", 1, 1, false).await.unwrap();
    for f in ["src/HEAD", "src/commondir", "pkg/config"] {
        assert!(!r.join(f).exists(), "{f} was restored");
    }
    assert_eq!(read(r, "docs/HEAD").unwrap(), "a heading\n");
    assert_eq!(read(r, "lib/refs/a.ts").unwrap(), "a\n");
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    let mut restored: Vec<&str> = out.result.restored.iter().map(|p| p.0.as_str()).collect();
    restored.sort();
    assert_eq!(restored, ["a.txt", "docs/HEAD", "lib/refs/a.ts"]);
    assert_eq!(
        out.not_restored,
        [
            (wp("pkg/config"), NotRestored::GitDirectory),
            (wp("src/HEAD"), NotRestored::GitDirectory),
            (wp("src/commondir"), NotRestored::GitDirectory),
        ]
    );
}

/// Review H3: a `git` killed at its bound leaves no lock behind (git's own lock on the store's
/// index, or on the ref it was writing), so the next checkpoint succeeds. The stand-in takes the
/// lock as the real git does, then hangs.
#[cfg(unix)]
#[tokio::test]
async fn checkpoint_git_killed_at_its_bound_leaves_no_lock() {
    use std::os::unix::fs::PermissionsExt;
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    write(ws.path(), "a.txt", "a\n");
    let hang = bin.path().join("hang");
    let stand_in = bin.path().join("git");
    std::fs::write(
        &stand_in,
        format!(
            "#!/bin/sh\nmode=$(cat '{}' 2>/dev/null)\nprev=\nfor a in \"$@\"; do\n  \
             if [ \"$a\" = add ] && [ \"$mode\" = add ]; then : > \"$GIT_INDEX_FILE.lock\"; exec sleep 300; fi\n  \
             if [ \"$prev\" = update-ref ] && [ \"$mode\" = update-ref ]; then mkdir -p \"$(dirname \"$GIT_DIR/$a\")\"; : > \"$GIT_DIR/$a.lock\"; exec sleep 300; fi\n  \
             prev=$a\ndone\nexec git \"$@\"\n",
            hang.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stand_in, std::fs::Permissions::from_mode(0o755)).unwrap();
    let bound = Duration::from_secs(3);
    let s = Shadow::with_git(data.path(), ws.path(), stand_in).with_timeouts(bound, bound);
    let shadow = data
        .path()
        .join("checkpoints")
        .join(axiom_agent::workspace_key(ws.path()));
    let index_lock = shadow.join("axiom-index.lock");
    let ref_lock = shadow.join("refs/axiom/s/0001.lock");

    std::fs::write(&hang, "add").unwrap();
    let e = s.create("s", "start", 1).await.unwrap_err();
    assert!(
        matches!(&e, CheckpointError::TimedOut { command, .. } if command == "add"),
        "{e:?}"
    );
    assert!(!index_lock.exists(), "the index lock is left");

    std::fs::write(&hang, "update-ref").unwrap();
    let e = s.create("s", "start", 1).await.unwrap_err();
    assert!(
        matches!(&e, CheckpointError::TimedOut { command, .. } if command == "update-ref"),
        "{e:?}"
    );
    assert!(!ref_lock.exists(), "the ref lock is left");

    std::fs::remove_file(&hang).unwrap();
    assert_eq!(s.create("s", "start", 1).await.unwrap().number, 1);
    write(ws.path(), "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(ws.path(), "a.txt").unwrap(), "a\n");
}

/// Review H3: an index lock a killed git left in the store (killed with its daemon, say) is
/// stale: no git of ours runs while the store's lock is held. The next checkpoint, after a
/// restart, removes it and succeeds.
#[tokio::test]
async fn checkpoint_removes_a_stale_index_lock() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    write(ws.path(), "a.txt", "a\n");
    Shadow::new(data.path(), ws.path())
        .create("s", "start", 1)
        .await
        .unwrap();
    let lock = data
        .path()
        .join("checkpoints")
        .join(axiom_agent::workspace_key(ws.path()))
        .join("axiom-index.lock");
    std::fs::write(&lock, "").unwrap();
    write(ws.path(), "a.txt", "agent\n");
    let s = Shadow::new(data.path(), ws.path());
    assert_eq!(s.create("s", "next", 1).await.unwrap().number, 2);
    assert!(!lock.exists());
}

/// Review H2, on disk: a folder that became a repository while the store's index held its files
/// (the agent replaced `sub` with a clone within one tool) is still held as plain files by the
/// snapshots. A restore does not write into it either: it is a repository on disk.
#[tokio::test]
async fn checkpoint_restore_writes_nothing_in_a_folder_that_became_a_repository() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    let r = ws.path();
    write(r, "a.txt", "a\n");
    write(r, "sub/f.txt", "v1\n");
    let s = Shadow::new(data.path(), r);
    s.create("s", "start", 1).await.unwrap();
    std::fs::remove_dir_all(r.join("sub")).unwrap();
    let sub = r.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    git(&sub, &["init", "-q"]);
    write(&sub, "f.txt", "cloned\n");
    git(&sub, &["add", "."]);
    git(&sub, &["commit", "-q", "-m", "c"]);
    write(r, "a.txt", "agent\n");
    s.record_post("s").await.unwrap();
    let out = s.restore("s", 1, 1, false).await.unwrap();
    assert_eq!(read(&sub, "f.txt").unwrap(), "cloned\n", "written into it");
    assert_eq!(read(r, "a.txt").unwrap(), "a\n");
    assert_eq!(
        out.not_restored,
        [(wp("sub"), NotRestored::NestedRepository)]
    );
}
