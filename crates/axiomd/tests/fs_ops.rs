//! Workspace file operations through the daemon (protocol 0.6, AUD-022): `fs/createFile`,
//! `fs/createDirectory`, `fs/rename`, `fs/trash`. Containment and symlink escapes are
//! `permission_denied`, existing targets are never replaced (`already_exists`), the trash is the
//! freedesktop.org home trash under `$XDG_DATA_HOME`, and a watching client gets the usual
//! `fs/didChange` for each.
#![cfg(unix)]

mod common;

use axiom_protocol::fs::{
    ChangeKind, CreateDirectory, CreateDirectoryParams, CreateFile, CreateFileParams,
    DidChangeParams, Rename, RenameParams, Trash, TrashParams, Watch, WatchParams,
};
use axiom_protocol::ErrorKind;
use common::{kind, wp, Daemon, Fixture, WAIT};
use std::path::Path;
use std::time::Duration;

async fn daemon_with_trash(xdg: &Path) -> (Daemon, tempfile::TempDir) {
    let data = tempfile::tempdir().unwrap();
    let xdg = xdg.to_path_buf();
    let d = Daemon::spawn_configured(data.path(), move |c| {
        c.env("XDG_DATA_HOME", &xdg);
        // macOS trashes into `$HOME/.Trash`: never the developer's own trash.
        if cfg!(target_vendor = "apple") {
            c.env("HOME", &xdg);
        }
    })
    .await;
    d.initialize().await;
    (d, data)
}

/// Where `name`, trashed, lands under the test's trash home: `Trash/files/` (freedesktop.org,
/// Linux) or `.Trash/` (macOS).
fn trashed(home: &Path, name: &str) -> std::path::PathBuf {
    if cfg!(target_vendor = "apple") {
        home.join(".Trash").join(name)
    } else {
        home.join("Trash/files").join(name)
    }
}

async fn create(f: &Fixture, path: &str, text: &str) -> Result<(), axiom_protocol::RpcError> {
    f.client
        .request::<CreateFile>(CreateFileParams {
            workspace_id: f.id.clone(),
            path: wp(path),
            text: text.into(),
        })
        .await
        .map(|_| ())
}

async fn mkdir(f: &Fixture, path: &str, parents: bool) -> Result<(), axiom_protocol::RpcError> {
    f.client
        .request::<CreateDirectory>(CreateDirectoryParams {
            workspace_id: f.id.clone(),
            path: wp(path),
            parents,
        })
        .await
        .map(|_| ())
}

async fn rename(f: &Fixture, from: &str, to: &str) -> Result<(), axiom_protocol::RpcError> {
    f.client
        .request::<Rename>(RenameParams {
            workspace_id: f.id.clone(),
            from: wp(from),
            to: wp(to),
        })
        .await
        .map(|_| ())
}

async fn trash(f: &Fixture, path: &str) -> Result<(), axiom_protocol::RpcError> {
    f.client
        .request::<Trash>(TrashParams {
            workspace_id: f.id.clone(),
            path: wp(path),
        })
        .await
        .map(|_| ())
}

#[tokio::test]
async fn the_methods_are_served() {
    let d = Daemon::spawn().await;
    let init = d.initialize().await;
    for m in [
        "fs/createFile",
        "fs/createDirectory",
        "fs/rename",
        "fs/trash",
    ] {
        assert!(init.methods.iter().any(|x| x == m), "{m}");
    }
}

#[tokio::test]
async fn create_rename_and_trash_with_change_notifications() {
    let xdg = tempfile::tempdir().unwrap();
    let (d, _data) = daemon_with_trash(xdg.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    d.client
        .request::<Watch>(WatchParams {
            workspace_id: f.id.clone(),
            enabled: true,
        })
        .await
        .unwrap();
    d.drain_until_quiet(Duration::from_millis(300)).await;

    // createFile: content, version, a notification.
    create(&f, "new.txt", "hello\n").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(f.path("new.txt")).unwrap(),
        "hello\n"
    );
    let n: DidChangeParams =
        serde_json::from_value(d.next_notification("fs/didChange", WAIT).await).unwrap();
    assert!(n
        .changes
        .iter()
        .any(|c| c.path.0 == "new.txt" && c.kind == ChangeKind::Created));
    assert_eq!(f.read("new.txt").await.unwrap().text, "hello\n");

    // createDirectory, with parents.
    mkdir(&f, "src/deep/er", true).await.unwrap();
    assert!(f.path("src/deep/er").is_dir());
    d.drain_until_quiet(Duration::from_millis(300)).await;

    // rename: moves; notified as removed + created.
    rename(&f, "new.txt", "src/deep/moved.txt").await.unwrap();
    assert!(!f.path("new.txt").exists());
    assert!(f.path("src/deep/moved.txt").is_file());
    let mut removed = false;
    let mut created = false;
    let end = tokio::time::Instant::now() + WAIT;
    while !(removed && created) && tokio::time::Instant::now() < end {
        let n: DidChangeParams =
            serde_json::from_value(d.next_notification("fs/didChange", WAIT).await).unwrap();
        for c in n.changes {
            removed |= c.path.0 == "new.txt" && c.kind == ChangeKind::Removed;
            created |= c.path.0 == "src/deep/moved.txt" && c.kind == ChangeKind::Created;
        }
    }
    assert!(removed && created);
    d.drain_until_quiet(Duration::from_millis(300)).await;

    // trash: into $XDG_DATA_HOME/Trash with its .trashinfo (Linux), ~/.Trash (macOS); notified
    // as removed.
    trash(&f, "src/deep/moved.txt").await.unwrap();
    assert!(!f.path("src/deep/moved.txt").exists());
    assert_eq!(
        std::fs::read_to_string(trashed(xdg.path(), "moved.txt")).unwrap(),
        "hello\n"
    );
    if !cfg!(target_vendor = "apple") {
        let info =
            std::fs::read_to_string(xdg.path().join("Trash/info/moved.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(
            info.contains(&format!(
                "Path={}\n",
                f.path("src/deep/moved.txt").to_string_lossy()
            )),
            "{info}"
        );
        assert!(info.contains("\nDeletionDate="), "{info}");
    }
    let n: DidChangeParams =
        serde_json::from_value(d.next_notification("fs/didChange", WAIT).await).unwrap();
    assert!(n
        .changes
        .iter()
        .any(|c| c.path.0 == "src/deep/moved.txt" && c.kind == ChangeKind::Removed));

    // A directory goes to the trash whole.
    trash(&f, "src").await.unwrap();
    assert!(trashed(xdg.path(), "src/deep/er").is_dir());
    assert!(!f.path("src").exists());
}

#[tokio::test]
async fn existing_targets_are_never_replaced() {
    let xdg = tempfile::tempdir().unwrap();
    let (d, _data) = daemon_with_trash(xdg.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    std::fs::write(f.path("a.txt"), "a").unwrap();
    std::fs::write(f.path("b.txt"), "b").unwrap();
    std::fs::create_dir(f.path("dir")).unwrap();
    let exists = |e: axiom_protocol::RpcError| assert_eq!(kind(&e), ErrorKind::AlreadyExists);
    exists(create(&f, "a.txt", "x").await.unwrap_err());
    exists(create(&f, "dir", "x").await.unwrap_err());
    exists(mkdir(&f, "dir", false).await.unwrap_err());
    exists(mkdir(&f, "a.txt", false).await.unwrap_err());
    exists(rename(&f, "a.txt", "b.txt").await.unwrap_err());
    exists(rename(&f, "a.txt", "dir").await.unwrap_err());
    assert_eq!(std::fs::read_to_string(f.path("a.txt")).unwrap(), "a");
    assert_eq!(std::fs::read_to_string(f.path("b.txt")).unwrap(), "b");
    // Missing parents, missing sources, the root, moving into itself.
    assert_eq!(
        kind(&create(&f, "no/such/f", "").await.unwrap_err()),
        ErrorKind::NotFound
    );
    assert_eq!(
        kind(&mkdir(&f, "no/such", false).await.unwrap_err()),
        ErrorKind::NotFound
    );
    assert_eq!(
        kind(&rename(&f, "missing", "x").await.unwrap_err()),
        ErrorKind::NotFound
    );
    assert_eq!(
        kind(&trash(&f, "missing").await.unwrap_err()),
        ErrorKind::NotFound
    );
    assert_eq!(
        kind(&trash(&f, "").await.unwrap_err()),
        ErrorKind::InvalidParams
    );
    assert_eq!(
        kind(&rename(&f, "dir", "dir/inner").await.unwrap_err()),
        ErrorKind::InvalidParams
    );
}

#[tokio::test]
async fn nothing_leaves_the_workspace() {
    let xdg = tempfile::tempdir().unwrap();
    let (d, _data) = daemon_with_trash(xdg.path()).await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "s").unwrap();
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    std::os::unix::fs::symlink(outside.path(), f.path("out")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), f.path("link")).unwrap();
    std::fs::write(f.path("a.txt"), "a").unwrap();
    let denied = |e: axiom_protocol::RpcError| assert_eq!(kind(&e), ErrorKind::PermissionDenied);
    denied(create(&f, "out/new", "x").await.unwrap_err());
    denied(mkdir(&f, "out/d", false).await.unwrap_err());
    denied(mkdir(&f, "out/d/e", true).await.unwrap_err());
    denied(rename(&f, "a.txt", "out/a.txt").await.unwrap_err());
    denied(rename(&f, "out/secret", "here").await.unwrap_err());
    denied(trash(&f, "out/secret").await.unwrap_err());
    // Lexically invalid paths.
    for bad in ["../x", "/etc/passwd", "a/../b"] {
        assert_eq!(
            kind(&create(&f, bad, "").await.unwrap_err()),
            ErrorKind::InvalidParams,
            "{bad}"
        );
    }
    let left: Vec<_> = std::fs::read_dir(outside.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, vec![std::ffi::OsString::from("secret")]);
    // Trashing a link moves the link, never what it points to.
    trash(&f, "link").await.unwrap();
    let in_trash = trashed(xdg.path(), "link");
    assert!(std::fs::symlink_metadata(&in_trash)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret")).unwrap(),
        "s"
    );
}
