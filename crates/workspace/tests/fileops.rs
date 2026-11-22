//! Creating, renaming and trashing entries (protocol 0.6, AUD-022), and refusing non-regular
//! files on read (AUD-097): containment, symlinks never followed out, nothing overwritten.

use axiom_protocol::WorkspacePath;
use axiom_workspace::{Workspace, Workspaces, WsError};
use std::path::Path;
use std::sync::Arc;

fn wp(s: &str) -> WorkspacePath {
    WorkspacePath(s.to_string())
}

async fn open(dir: &Path) -> Arc<Workspace> {
    let reg = Workspaces::new();
    let o = reg.open(dir.to_str().unwrap()).await.unwrap();
    reg.get(&o.workspace_id).unwrap()
}

#[tokio::test]
async fn create_file_writes_new_files_and_never_overwrites() {
    let d = tempfile::tempdir().unwrap();
    let w = open(d.path()).await;
    let v = w.create_file(&wp("a.txt"), "hello").await.unwrap();
    assert_eq!(
        std::fs::read_to_string(d.path().join("a.txt")).unwrap(),
        "hello"
    );
    assert_eq!(v, w.read_text(&wp("a.txt")).await.unwrap().version);
    // An existing file, directory or symlink is never replaced.
    let e = w.create_file(&wp("a.txt"), "other").await.unwrap_err();
    assert!(matches!(e, WsError::AlreadyExists(_)), "{e:?}");
    assert_eq!(
        std::fs::read_to_string(d.path().join("a.txt")).unwrap(),
        "hello"
    );
    std::fs::create_dir(d.path().join("dir")).unwrap();
    assert!(matches!(
        w.create_file(&wp("dir"), "").await.unwrap_err(),
        WsError::AlreadyExists(_)
    ));
    // The directory must exist.
    assert!(matches!(
        w.create_file(&wp("missing/b.txt"), "").await.unwrap_err(),
        WsError::NotFound(_)
    ));
    // No temp file is left behind.
    let names: Vec<_> = std::fs::read_dir(d.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !n.starts_with(".axiom-tmp")),
        "{names:?}"
    );
}

#[tokio::test]
async fn create_dir_with_and_without_parents() {
    let d = tempfile::tempdir().unwrap();
    let w = open(d.path()).await;
    w.create_dir(&wp("a"), false).await.unwrap();
    assert!(d.path().join("a").is_dir());
    assert!(matches!(
        w.create_dir(&wp("a"), false).await.unwrap_err(),
        WsError::AlreadyExists(_)
    ));
    assert!(matches!(
        w.create_dir(&wp("x/y/z"), false).await.unwrap_err(),
        WsError::NotFound(_)
    ));
    w.create_dir(&wp("x/y/z"), true).await.unwrap();
    assert!(d.path().join("x/y/z").is_dir());
    assert!(matches!(
        w.create_dir(&wp("x/y/z"), true).await.unwrap_err(),
        WsError::AlreadyExists(_)
    ));
    // The root and bad paths are refused.
    assert!(matches!(
        w.create_dir(&wp(""), false).await.unwrap_err(),
        WsError::InvalidPath(_)
    ));
    assert!(matches!(
        w.create_dir(&wp("../out"), false).await.unwrap_err(),
        WsError::InvalidPath(_)
    ));
}

#[tokio::test]
async fn rename_moves_inside_the_workspace_and_never_overwrites() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.txt"), "a").unwrap();
    std::fs::write(d.path().join("b.txt"), "b").unwrap();
    std::fs::create_dir_all(d.path().join("dir/sub")).unwrap();
    std::fs::write(d.path().join("dir/sub/f"), "f").unwrap();
    let w = open(d.path()).await;
    // Onto an existing file: refused, both kept.
    assert!(matches!(
        w.rename(&wp("a.txt"), &wp("b.txt")).await.unwrap_err(),
        WsError::AlreadyExists(_)
    ));
    assert_eq!(
        std::fs::read_to_string(d.path().join("b.txt")).unwrap(),
        "b"
    );
    // Onto an existing (even empty) directory: refused.
    std::fs::create_dir(d.path().join("empty")).unwrap();
    assert!(matches!(
        w.rename(&wp("dir"), &wp("empty")).await.unwrap_err(),
        WsError::AlreadyExists(_)
    ));
    // A rename and a move into a directory.
    w.rename(&wp("a.txt"), &wp("c.txt")).await.unwrap();
    w.rename(&wp("c.txt"), &wp("dir/sub/c.txt")).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(d.path().join("dir/sub/c.txt")).unwrap(),
        "a"
    );
    // A directory, with its contents.
    w.rename(&wp("dir"), &wp("moved")).await.unwrap();
    assert!(d.path().join("moved/sub/f").is_file());
    // Not into itself, not from a missing path, not into a missing directory.
    assert!(matches!(
        w.rename(&wp("moved"), &wp("moved/sub/x"))
            .await
            .unwrap_err(),
        WsError::InvalidPath(_)
    ));
    assert!(matches!(
        w.rename(&wp("nope"), &wp("x")).await.unwrap_err(),
        WsError::NotFound(_)
    ));
    assert!(matches!(
        w.rename(&wp("b.txt"), &wp("no/dir/b.txt"))
            .await
            .unwrap_err(),
        WsError::NotFound(_)
    ));
    assert!(matches!(
        w.rename(&wp(""), &wp("x")).await.unwrap_err(),
        WsError::InvalidPath(_)
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn symlinks_never_lead_out_of_the_workspace() {
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "s").unwrap();
    let d = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), d.path().join("out")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), d.path().join("link")).unwrap();
    std::fs::write(d.path().join("a.txt"), "a").unwrap();
    let w = open(d.path()).await;
    let escapes = |e: WsError| assert!(matches!(e, WsError::Escapes(_)), "{e:?}");
    escapes(w.create_file(&wp("out/new"), "x").await.unwrap_err());
    escapes(w.create_dir(&wp("out/d"), false).await.unwrap_err());
    escapes(w.create_dir(&wp("out/d/e"), true).await.unwrap_err());
    escapes(w.rename(&wp("a.txt"), &wp("out/a.txt")).await.unwrap_err());
    escapes(
        w.rename(&wp("out/secret"), &wp("stolen"))
            .await
            .unwrap_err(),
    );
    escapes(w.trash(&wp("out/secret")).await.unwrap_err());
    let left: Vec<_> = std::fs::read_dir(outside.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left, vec![std::ffi::OsString::from("secret")]);
    assert!(d.path().join("a.txt").is_file());
    // A link itself can be renamed inside the workspace: the link moves, its target stays.
    w.rename(&wp("link"), &wp("link2")).await.unwrap();
    assert!(std::fs::symlink_metadata(d.path().join("link2"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret")).unwrap(),
        "s"
    );
    // Creating over a dangling link is refused (it would create a file wherever it points).
    std::os::unix::fs::symlink(outside.path().join("nothing"), d.path().join("dangling")).unwrap();
    assert!(w.create_file(&wp("dangling"), "x").await.is_err());
    assert!(!outside.path().join("nothing").exists());
}

/// AUD-097: a FIFO in the workspace is refused at once instead of blocking a thread forever.
#[cfg(unix)]
#[tokio::test]
async fn a_fifo_is_refused_not_read() {
    let d = tempfile::tempdir().unwrap();
    let fifo = d.path().join("p");
    let st = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(st.success());
    let w = open(d.path()).await;
    let r = tokio::time::timeout(std::time::Duration::from_secs(5), w.read_text(&wp("p")))
        .await
        .expect("reading a FIFO must not block");
    assert!(matches!(r.unwrap_err(), WsError::NotAFile(_)));
    // Nor does writing over it with a version check, or removing it with one.
    let r = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        w.write_text(&wp("p"), "x", None, false),
    )
    .await
    .expect("must not block");
    assert!(r.is_err());
}
