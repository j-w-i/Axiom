mod common;
use axiom_protocol::fs::{EntryKind, FileVersion};
use axiom_protocol::ErrorKind;
use common::*;
use serde_json::json;

#[tokio::test]
async fn list_orders_directories_first_then_case_insensitive_name() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let root = fx.list("").await.unwrap();
    // dirs: .git data notes src target ; files: .gitignore README.md build.log
    assert_eq!(
        names(&root),
        [
            ".git",
            "data",
            "notes",
            "src",
            "target",
            ".gitignore",
            "build.log",
            "README.md"
        ]
    );
    assert!(root.iter().take(5).all(is_dir));
    assert!(root.iter().skip(5).all(|e| e.kind == EntryKind::File));
    let src = fx.list("src").await.unwrap();
    assert_eq!(names(&src), ["lib.rs", "main.rs"]);
}

#[tokio::test]
async fn list_reports_sizes_for_files_only_and_ignored_flags() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let root = fx.list("").await.unwrap();
    assert_eq!(entry(&root, "src").size, None);
    assert_eq!(
        entry(&root, "README.md").size,
        Some(std::fs::metadata(fx.path("README.md")).unwrap().len())
    );
    assert!(entry(&root, ".git").ignored, ".git is always ignored");
    assert!(entry(&root, "target").ignored, "target/ is in .gitignore");
    assert!(entry(&root, "build.log").ignored, "*.log is in .gitignore");
    assert!(!entry(&root, "src").ignored);
    assert!(!entry(&root, ".gitignore").ignored);
    // Ignored entries are listed, and children of an ignored directory are ignored.
    let target = fx.list("target").await.unwrap();
    assert_eq!(names(&target), ["debug.o"]);
    assert!(target[0].ignored);
}

#[tokio::test]
async fn list_errors() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    assert_eq!(
        kind(&fx.list("missing").await.unwrap_err()),
        ErrorKind::NotFound
    );
    assert_eq!(
        kind(&fx.list("README.md").await.unwrap_err()),
        ErrorKind::InvalidParams
    );
}

#[tokio::test]
async fn read_text_returns_content_and_sha256_version() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let r = fx.read("README.md").await.unwrap();
    assert_eq!(
        r.text,
        std::fs::read_to_string(fx.path("README.md")).unwrap()
    );
    assert_eq!(r.version.0.len(), 64);
    assert!(r
        .version
        .0
        .chars()
        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    assert_eq!(fx.read("README.md").await.unwrap().version, r.version);
}

#[tokio::test]
async fn read_preserves_crlf() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let r = fx.read("notes/crlf.txt").await.unwrap();
    assert_eq!(r.text, "line one\r\nline two\r\n");
}

#[tokio::test]
async fn read_errors_invalid_utf8_directory_missing() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    assert_eq!(
        kind(&fx.read("data/invalid-utf8.bin").await.unwrap_err()),
        ErrorKind::UnsupportedEncoding
    );
    assert_eq!(
        kind(&fx.read("src").await.unwrap_err()),
        ErrorKind::InvalidParams
    );
    assert_eq!(
        kind(&fx.read("nope.txt").await.unwrap_err()),
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn read_file_over_16_mib_is_too_large() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let f = std::fs::File::create(fx.path("big.bin")).unwrap();
    f.set_len(16 * 1024 * 1024 + 1).unwrap();
    drop(f);
    assert_eq!(
        kind(&fx.read("big.bin").await.unwrap_err()),
        ErrorKind::TooLarge
    );
}

// ---- the six rows of the fs/writeFile decision table -----------------------

#[tokio::test]
async fn write_row1_overwrite_true_always_writes() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let bogus = FileVersion("0".repeat(64));
    // existing file, bogus expected
    fx.write("README.md", "replaced", Some(&bogus), true)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(fx.path("README.md")).unwrap(),
        "replaced"
    );
    // missing file, bogus expected
    fx.write("fresh.txt", "new", Some(&bogus), true)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(fx.path("fresh.txt")).unwrap(),
        "new"
    );
}

#[tokio::test]
async fn write_row2_create_when_missing_and_no_expected() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let v = fx.write("created.txt", "hello", None, false).await.unwrap();
    assert_eq!(fx.read("created.txt").await.unwrap().version, v);
}

#[tokio::test]
async fn write_row3_missing_file_with_expected_conflicts_current_null() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let e = fx
        .write("ghost.txt", "x", Some(&FileVersion("ab".into())), false)
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Conflict);
    assert_eq!(e.data.details.unwrap(), json!({ "currentVersion": null }));
    assert!(!fx.path("ghost.txt").exists());
}

#[tokio::test]
async fn write_row4_existing_file_without_expected_conflicts_with_current_version() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let cur = fx.read("README.md").await.unwrap().version;
    let e = fx.write("README.md", "x", None, false).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Conflict);
    assert_eq!(e.data.details.unwrap(), json!({ "currentVersion": cur.0 }));
    assert_ne!(std::fs::read_to_string(fx.path("README.md")).unwrap(), "x");
}

#[tokio::test]
async fn write_row5_expected_equals_current_writes() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let cur = fx.read("README.md").await.unwrap().version;
    let v = fx
        .write("README.md", "next", Some(&cur), false)
        .await
        .unwrap();
    assert_ne!(v, cur);
    assert_eq!(
        std::fs::read_to_string(fx.path("README.md")).unwrap(),
        "next"
    );
    fx.write("README.md", "third", Some(&v), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn write_row6_expected_differs_conflicts_with_current_version() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let stale = fx.read("README.md").await.unwrap().version;
    std::fs::write(fx.path("README.md"), "changed behind our back").unwrap();
    let e = fx
        .write("README.md", "mine", Some(&stale), false)
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Conflict);
    let now = fx.read("README.md").await.unwrap().version;
    assert_eq!(e.data.details.unwrap(), json!({ "currentVersion": now.0 }));
    assert_eq!(
        std::fs::read_to_string(fx.path("README.md")).unwrap(),
        "changed behind our back"
    );
}

#[tokio::test]
async fn write_is_byte_for_byte_and_leaves_no_temp_files() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let before: Vec<_> = names_in(&fx.path("notes"));
    fx.write("notes/crlf.txt", "a\r\nb\nc\r", None, true)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(fx.path("notes/crlf.txt")).unwrap(),
        b"a\r\nb\nc\r"
    );
    assert_eq!(names_in(&fx.path("notes")), before);
}

fn names_in(p: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(p)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[tokio::test]
async fn write_missing_parent_is_not_found_and_too_large_is_rejected() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let e = fx
        .write("no/such/dir/f.txt", "x", None, true)
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    assert!(!fx.path("no").exists());
    let big = "x".repeat(16 * 1024 * 1024 + 1);
    let e = fx.write("big.txt", &big, None, true).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::TooLarge);
    assert!(!fx.path("big.txt").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn write_preserves_permission_bits() {
    use std::os::unix::fs::PermissionsExt;
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    std::fs::set_permissions(
        fx.path("src/main.rs"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fx.write("src/main.rs", "fn main() {}\n", None, true)
        .await
        .unwrap();
    assert_eq!(
        std::fs::metadata(fx.path("src/main.rs"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[tokio::test]
async fn concurrent_writes_with_same_expected_version_exactly_one_wins() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let base = fx.read("README.md").await.unwrap().version;
    let mut futs = Vec::new();
    for i in 0..8 {
        let fx_client = fx.client.clone();
        let id = fx.id.clone();
        let base = base.clone();
        futs.push(tokio::spawn(async move {
            fx_client
                .request::<axiom_protocol::fs::WriteFile>(axiom_protocol::fs::WriteFileParams {
                    workspace_id: id,
                    path: wp("README.md"),
                    text: format!("writer {i}"),
                    expected_version: Some(base),
                    overwrite: false,
                })
                .await
        }));
    }
    let mut ok = 0;
    for f in futs {
        match f.await.unwrap() {
            Ok(_) => ok += 1,
            Err(e) => assert_eq!(kind(&e), ErrorKind::Conflict),
        }
    }
    assert_eq!(ok, 1);
}

// ---- path safety ------------------------------------------------------------

#[tokio::test]
async fn traversal_absolute_and_backslash_paths_are_invalid_params() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    for bad in ["../x", "a/../b", "/etc/passwd", "a\\b", "./a", "a//b", "a/"] {
        assert_eq!(
            kind(&fx.read(bad).await.unwrap_err()),
            ErrorKind::InvalidParams,
            "read {bad}"
        );
        assert_eq!(
            kind(&fx.list(bad).await.unwrap_err()),
            ErrorKind::InvalidParams,
            "list {bad}"
        );
        assert_eq!(
            kind(&fx.write(bad, "x", None, true).await.unwrap_err()),
            ErrorKind::InvalidParams,
            "write {bad}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_escape_is_permission_denied_for_read_write_and_list() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "s3cret").unwrap();
    std::os::unix::fs::symlink(outside.path(), fx.path("out")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), fx.path("link.txt")).unwrap();

    assert_eq!(
        kind(&fx.read("out/secret.txt").await.unwrap_err()),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        kind(&fx.read("link.txt").await.unwrap_err()),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        kind(&fx.list("out").await.unwrap_err()),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        kind(&fx.write("out/pwn.txt", "x", None, true).await.unwrap_err()),
        ErrorKind::PermissionDenied
    );
    assert_eq!(
        kind(&fx.write("link.txt", "x", None, true).await.unwrap_err()),
        ErrorKind::PermissionDenied
    );
    assert!(!outside.path().join("pwn.txt").exists());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "s3cret"
    );

    // fs/list reports symlinks as symlinks and never follows them.
    let root = fx.list("").await.unwrap();
    assert_eq!(entry(&root, "out").kind, EntryKind::Symlink);
    assert_eq!(entry(&root, "link.txt").kind, EntryKind::Symlink);
}

#[tokio::test]
async fn error_messages_do_not_leak_absolute_paths() {
    let d = Daemon::ready().await;
    let fx = d.open_fixture().await;
    let root = fx.root.to_str().unwrap().to_string();
    let errs = vec![
        fx.read("missing.txt").await.unwrap_err(),
        fx.list("missing").await.unwrap_err(),
        fx.write("no/dir/f", "x", None, true).await.unwrap_err(),
        fx.read("data/invalid-utf8.bin").await.unwrap_err(),
    ];
    for e in errs {
        assert!(!e.message.contains(&root), "{}", e.message);
    }
}
