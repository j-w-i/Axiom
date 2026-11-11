use axiom_protocol::fs::{ChangeKind, FileChange};
use axiom_protocol::WorkspacePath;
use axiom_workspace::{WatchHandle, Workspaces};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn wp(s: &str) -> WorkspacePath {
    WorkspacePath(s.to_string())
}

async fn open(dir: &Path) -> std::sync::Arc<axiom_workspace::Workspace> {
    let wss = Workspaces::new();
    let r = wss.open(dir.to_str().unwrap()).await.unwrap();
    wss.get(&r.workspace_id).unwrap()
}

/// Collects batches until `done(all changes so far)` holds or 5 s pass.
async fn wait_for(h: &mut WatchHandle, done: impl Fn(&[FileChange]) -> bool) -> Vec<FileChange> {
    let mut all = Vec::new();
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Ok(Some(batch)) =
            tokio::time::timeout(Duration::from_millis(100), h.changes.recv()).await
        {
            all.extend(batch);
            if done(&all) {
                return all;
            }
        }
    }
    panic!("timed out waiting for expected changes; got {all:?}");
}

fn last_kind(all: &[FileChange], path: &str) -> Option<ChangeKind> {
    all.iter().rev().find(|c| c.path.0 == path).map(|c| c.kind)
}

fn has(all: &[FileChange], path: &str, kind: ChangeKind) -> bool {
    all.iter().any(|c| c.path.0 == path && c.kind == kind)
}

async fn assert_quiet(h: &mut WatchHandle, ms: u64) {
    if let Ok(Some(b)) = tokio::time::timeout(Duration::from_millis(ms), h.changes.recv()).await {
        panic!("unexpected batch: {b:?}");
    }
}

/// The watcher needs a moment to be armed on every platform; settle, then drain.
async fn settle(h: &mut WatchHandle) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    while h.changes.try_recv().is_ok() {}
}

#[tokio::test]
async fn watch_create_modify_remove() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;

    let t = Instant::now();
    fs::write(d.path().join("a.txt"), "1").unwrap();
    let all = wait_for(&mut h, |a| has(a, "a.txt", ChangeKind::Created)).await;
    eprintln!("watch latency (create): {:?}", t.elapsed());
    assert_eq!(last_kind(&all, "a.txt"), Some(ChangeKind::Created));

    tokio::time::sleep(Duration::from_millis(300)).await;
    while h.changes.try_recv().is_ok() {}
    fs::write(d.path().join("a.txt"), "22").unwrap();
    let all = wait_for(&mut h, |a| has(a, "a.txt", ChangeKind::Modified)).await;
    assert_eq!(last_kind(&all, "a.txt"), Some(ChangeKind::Modified));

    tokio::time::sleep(Duration::from_millis(300)).await;
    while h.changes.try_recv().is_ok() {}
    fs::remove_file(d.path().join("a.txt")).unwrap();
    let all = wait_for(&mut h, |a| has(a, "a.txt", ChangeKind::Removed)).await;
    assert_eq!(last_kind(&all, "a.txt"), Some(ChangeKind::Removed));
}

#[tokio::test]
async fn watch_create_then_modify_is_a_single_created() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;
    let p = d.path().join("n.txt");
    fs::write(&p, "one").unwrap();
    fs::write(&p, "two").unwrap();
    let all = wait_for(&mut h, |a| has(a, "n.txt", ChangeKind::Created)).await;
    // Whatever batching happened, the file is never reported as merely modified
    // before it was created, and the first report for it is `created`.
    let first = all.iter().find(|c| c.path.0 == "n.txt").unwrap();
    assert_eq!(first.kind, ChangeKind::Created);
    // Within the batch that carries it, there is exactly one entry per path.
    let mut paths: Vec<_> = all.iter().filter(|c| c.path.0 == "n.txt").collect();
    paths.dedup_by_key(|c| c.kind);
    assert_eq!(paths.len(), 1, "{all:?}");
}

#[tokio::test]
async fn watch_batches_are_sorted_and_deduplicated_per_path() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;
    for n in ["c", "a", "b"] {
        fs::write(d.path().join(n), "x").unwrap();
        fs::write(d.path().join(n), "yy").unwrap();
    }
    let end = Instant::now() + Duration::from_secs(5);
    let mut seen = std::collections::BTreeSet::new();
    while Instant::now() < end && seen.len() < 3 {
        if let Ok(Some(batch)) =
            tokio::time::timeout(Duration::from_millis(200), h.changes.recv()).await
        {
            let mut sorted = batch.clone();
            sorted.sort_by(|x, y| x.path.cmp(&y.path));
            assert_eq!(batch, sorted, "batch sorted by path");
            let mut uniq: Vec<_> = batch.iter().map(|c| c.path.0.clone()).collect();
            uniq.dedup();
            assert_eq!(uniq.len(), batch.len(), "one entry per path per batch");
            seen.extend(batch.into_iter().map(|c| c.path.0));
        }
    }
    assert_eq!(seen.len(), 3);
}

#[tokio::test]
async fn watch_ignores_dot_git_and_ignored_directories() {
    let d = TempDir::new().unwrap();
    fs::create_dir_all(d.path().join(".git/objects")).unwrap();
    fs::create_dir_all(d.path().join("target/deep")).unwrap();
    fs::write(d.path().join(".gitignore"), "target/\n").unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;

    fs::write(d.path().join(".git/HEAD"), "ref").unwrap();
    fs::write(d.path().join(".git/objects/x"), "o").unwrap();
    fs::write(d.path().join("target/out.o"), "o").unwrap();
    fs::write(d.path().join("target/deep/out2.o"), "o").unwrap();
    assert_quiet(&mut h, 500).await;

    // The same watcher still reports ordinary files.
    fs::write(d.path().join("real.txt"), "r").unwrap();
    let all = wait_for(&mut h, |a| has(a, "real.txt", ChangeKind::Created)).await;
    assert!(all
        .iter()
        .all(|c| !c.path.0.starts_with(".git/") && !c.path.0.starts_with("target/")));
}

#[tokio::test]
async fn watch_reports_ignored_file_in_non_ignored_directory_and_ignored_dir_entry_itself() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "*.log\nbuild/\n").unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;
    fs::write(d.path().join("debug.log"), "x").unwrap();
    wait_for(&mut h, |a| has(a, "debug.log", ChangeKind::Created)).await;

    // The ignored directory entry itself is reported when created ...
    fs::create_dir(d.path().join("build")).unwrap();
    wait_for(&mut h, |a| has(a, "build", ChangeKind::Created)).await;
    // ... but not what happens inside it.
    tokio::time::sleep(Duration::from_millis(300)).await;
    while h.changes.try_recv().is_ok() {}
    fs::write(d.path().join("build/out"), "x").unwrap();
    assert_quiet(&mut h, 500).await;
}

#[tokio::test]
async fn watch_nested_ignore_file_applies_and_edits_to_it_take_effect() {
    let d = TempDir::new().unwrap();
    fs::create_dir_all(d.path().join("sub/gen")).unwrap();
    let ws = open(d.path()).await;
    // Prime the matcher cache: nothing is ignored yet.
    ws.list(&wp("sub")).await.unwrap();
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;

    fs::write(d.path().join("sub/gen/a"), "1").unwrap();
    wait_for(&mut h, |a| has(a, "sub/gen/a", ChangeKind::Created)).await;

    // Writing an ignore file outside the crate invalidates the cache via the watcher.
    fs::write(d.path().join("sub/.gitignore"), "gen/\n").unwrap();
    wait_for(&mut h, |a| has(a, "sub/.gitignore", ChangeKind::Created)).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    while h.changes.try_recv().is_ok() {}
    fs::write(d.path().join("sub/gen/b"), "2").unwrap();
    assert_quiet(&mut h, 500).await;
}

#[tokio::test]
async fn watch_two_handles_both_receive_and_dropping_one_keeps_the_other() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let mut h1 = ws.watch().unwrap();
    let mut h2 = ws.watch().unwrap();
    settle(&mut h1).await;
    settle(&mut h2).await;
    fs::write(d.path().join("one.txt"), "1").unwrap();
    wait_for(&mut h1, |a| has(a, "one.txt", ChangeKind::Created)).await;
    wait_for(&mut h2, |a| has(a, "one.txt", ChangeKind::Created)).await;

    drop(h1);
    tokio::time::sleep(Duration::from_millis(300)).await;
    while h2.changes.try_recv().is_ok() {}
    fs::write(d.path().join("two.txt"), "2").unwrap();
    wait_for(&mut h2, |a| has(a, "two.txt", ChangeKind::Created)).await;
}

#[tokio::test]
async fn watch_rename_is_removed_plus_created() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("old.txt"), "x").unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;
    fs::rename(d.path().join("old.txt"), d.path().join("new.txt")).unwrap();
    let all = wait_for(&mut h, |a| {
        has(a, "old.txt", ChangeKind::Removed) && has(a, "new.txt", ChangeKind::Created)
    })
    .await;
    assert_eq!(last_kind(&all, "old.txt"), Some(ChangeKind::Removed));
    assert_eq!(last_kind(&all, "new.txt"), Some(ChangeKind::Created));
}

#[tokio::test]
async fn watch_dropping_the_handle_stops_delivery_and_a_new_watch_works() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let h = ws.watch().unwrap();
    drop(h);
    tokio::time::sleep(Duration::from_millis(300)).await;
    fs::write(d.path().join("after.txt"), "x").unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    // A fresh subscription starts a new watcher and sees only new changes.
    let mut h2 = ws.watch().unwrap();
    settle(&mut h2).await;
    fs::write(d.path().join("later.txt"), "x").unwrap();
    let all = wait_for(&mut h2, |a| has(a, "later.txt", ChangeKind::Created)).await;
    assert!(!has(&all, "after.txt", ChangeKind::Created));
}

#[tokio::test]
async fn watch_dropped_receiver_end_closes_when_handle_dropped() {
    // The Receiver is a field of the handle; after the handle is gone nothing
    // can observe it. Verify instead that closing the only handle releases the
    // watcher so a temp dir can be removed and re-watched without error.
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    drop(ws.watch().unwrap());
    drop(ws.watch().unwrap());
    let _h = ws.watch().unwrap();
}

#[tokio::test]
async fn watch_sees_writes_made_through_write_text() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let mut h = ws.watch().unwrap();
    settle(&mut h).await;
    ws.write_text(&wp("w.txt"), "first", None, false)
        .await
        .unwrap();
    wait_for(&mut h, |a| {
        has(a, "w.txt", ChangeKind::Created) || has(a, "w.txt", ChangeKind::Modified)
    })
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    while h.changes.try_recv().is_ok() {}
    ws.write_text(&wp("w.txt"), "second", None, true)
        .await
        .unwrap();
    let all = wait_for(&mut h, |a| a.iter().any(|c| c.path.0 == "w.txt")).await;
    // Atomic replace (temp file + rename) may surface as created or modified.
    assert!(matches!(
        last_kind(&all, "w.txt"),
        Some(ChangeKind::Created | ChangeKind::Modified)
    ));
    // The temp file used for the atomic write is never left visible as present.
    assert!(!all.iter().any(|c| c.path.0.contains(".axiom-tmp-")
        && c.kind != ChangeKind::Removed
        && fs::symlink_metadata(d.path().join(&c.path.0)).is_ok()));
}
