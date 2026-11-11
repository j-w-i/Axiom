use axiom_protocol::fs::{EntryKind, FileVersion};
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId, WorkspacePath};
use axiom_workspace::{Workspace, Workspaces, WsError};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

fn wp(s: &str) -> WorkspacePath {
    WorkspacePath(s.to_string())
}

async fn open(dir: &Path) -> Arc<Workspace> {
    let wss = Workspaces::new();
    let r = wss.open(dir.to_str().unwrap()).await.unwrap();
    wss.get(&r.workspace_id).unwrap()
}

fn kind(e: WsError) -> ErrorKind {
    let r: RpcError = e.into();
    assert_eq!(r.code, r.data.kind.code());
    r.data.kind
}

fn names(entries: &[axiom_protocol::fs::DirEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

fn leftovers(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".axiom-tmp-") || n.starts_with(".tmp"))
        .collect()
}

// ---------------------------------------------------------------- open

#[tokio::test]
async fn open_rejects_relative_path() {
    let e = Workspaces::new().open("relative/dir").await.unwrap_err();
    assert!(matches!(e, WsError::InvalidPath(_)));
    assert_eq!(kind(e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn open_missing_path_is_not_found() {
    let d = TempDir::new().unwrap();
    let missing = d.path().join("nope");
    let e = Workspaces::new()
        .open(missing.to_str().unwrap())
        .await
        .unwrap_err();
    assert_eq!(kind(e), ErrorKind::NotFound);
}

#[tokio::test]
async fn open_file_path_is_invalid_params() {
    let d = TempDir::new().unwrap();
    let f = d.path().join("f.txt");
    fs::write(&f, "x").unwrap();
    let e = Workspaces::new()
        .open(f.to_str().unwrap())
        .await
        .unwrap_err();
    assert_eq!(kind(e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn open_is_idempotent_and_ids_increment() {
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let wss = Workspaces::new();
    let r1 = wss.open(a.path().to_str().unwrap()).await.unwrap();
    let r2 = wss.open(a.path().to_str().unwrap()).await.unwrap();
    let r3 = wss.open(b.path().to_str().unwrap()).await.unwrap();
    assert_eq!(r1.workspace_id.0, "w1");
    assert_eq!(r1, r2);
    assert_eq!(r3.workspace_id.0, "w2");
    assert_eq!(r1.name, a.path().file_name().unwrap().to_string_lossy());
}

#[cfg(unix)]
#[tokio::test]
async fn open_symlinked_root_resolves_to_canonical_root() {
    let d = TempDir::new().unwrap();
    let real = d.path().join("real");
    fs::create_dir(&real).unwrap();
    let link = d.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let wss = Workspaces::new();
    let via_link = wss.open(link.to_str().unwrap()).await.unwrap();
    let direct = wss.open(real.to_str().unwrap()).await.unwrap();
    assert_eq!(
        via_link.root,
        dunce::canonicalize(&real).unwrap().to_str().unwrap()
    );
    assert_eq!(
        via_link.workspace_id, direct.workspace_id,
        "same canonical root"
    );
    assert_eq!(via_link.name, "real");
}

#[tokio::test]
async fn close_and_get_unknown_are_workspace_not_open() {
    let d = TempDir::new().unwrap();
    let wss = Workspaces::new();
    let r = wss.open(d.path().to_str().unwrap()).await.unwrap();
    wss.close(&r.workspace_id).await.unwrap();
    assert_eq!(
        kind(wss.get(&r.workspace_id).err().unwrap()),
        ErrorKind::WorkspaceNotOpen
    );
    assert_eq!(
        kind(wss.close(&r.workspace_id).await.unwrap_err()),
        ErrorKind::WorkspaceNotOpen
    );
    assert_eq!(
        kind(wss.get(&WorkspaceId("zzz".into())).err().unwrap()),
        ErrorKind::WorkspaceNotOpen
    );
    // Reopening after close mints a fresh id.
    let again = wss.open(d.path().to_str().unwrap()).await.unwrap();
    assert_ne!(again.workspace_id, r.workspace_id);
}

// -------------------------------------------------------------- resolve

#[tokio::test]
async fn resolve_rejects_every_lexically_bad_path() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    for bad in [
        "/a", "a/", "a//b", "../a", "a/../b", "./a", "a\\b", "a/./b", "a\0b",
    ] {
        let e = ws.resolve(&wp(bad)).unwrap_err();
        assert!(matches!(e, WsError::InvalidPath(_)), "{bad:?} gave {e:?}");
    }
}

#[tokio::test]
async fn resolve_nonexistent_nested_path_under_existing_dir() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("src")).unwrap();
    let ws = open(d.path()).await;
    let p = ws.resolve(&wp("src/new/deeper/file.rs")).unwrap();
    assert!(p.starts_with(ws.root()));
    assert!(p.ends_with("src/new/deeper/file.rs"));
    assert_eq!(ws.resolve(&wp("")).unwrap(), ws.root());
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_pointing_outside_escapes_for_read_write_and_list() {
    let outside = TempDir::new().unwrap();
    fs::write(outside.path().join("secret.txt"), "s3cret").unwrap();
    let d = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path(), d.path().join("out")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), d.path().join("link.txt"))
        .unwrap();
    let ws = open(d.path()).await;

    for path in ["out", "out/secret.txt", "link.txt", "out/newfile.txt"] {
        assert!(
            matches!(ws.resolve(&wp(path)), Err(WsError::Escapes(_))),
            "{path}"
        );
    }
    let e = ws.read_text(&wp("out/secret.txt")).await.unwrap_err();
    assert!(matches!(e, WsError::Escapes(_)), "{e:?}");
    assert_eq!(kind(e), ErrorKind::PermissionDenied);
    assert!(matches!(
        ws.read_text(&wp("link.txt")).await,
        Err(WsError::Escapes(_))
    ));
    assert!(matches!(
        ws.write_text(&wp("out/pwn.txt"), "x", None, false).await,
        Err(WsError::Escapes(_))
    ));
    assert!(matches!(
        ws.write_text(&wp("link.txt"), "x", None, true).await,
        Err(WsError::Escapes(_))
    ));
    assert!(matches!(
        ws.list(&wp("out")).await,
        Err(WsError::Escapes(_))
    ));
    assert!(!outside.path().join("pwn.txt").exists());
    assert_eq!(
        fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
        "s3cret"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn dangling_symlink_cannot_be_written_through() {
    let outside = TempDir::new().unwrap();
    let d = TempDir::new().unwrap();
    std::os::unix::fs::symlink(outside.path().join("nothing"), d.path().join("dangling")).unwrap();
    let ws = open(d.path()).await;
    assert!(matches!(
        ws.write_text(&wp("dangling"), "x", None, true).await,
        Err(WsError::Escapes(_))
    ));
    assert!(!outside.path().join("nothing").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn symlink_inside_workspace_is_allowed() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("real")).unwrap();
    fs::write(d.path().join("real/f.txt"), "hi").unwrap();
    std::os::unix::fs::symlink(d.path().join("real"), d.path().join("alias")).unwrap();
    let ws = open(d.path()).await;
    assert_eq!(ws.read_text(&wp("alias/f.txt")).await.unwrap().text, "hi");
}

// ----------------------------------------------------------------- list

#[tokio::test]
async fn list_orders_dirs_first_then_case_insensitive_name() {
    let d = TempDir::new().unwrap();
    for dir in ["zdir", "Adir", "bdir"] {
        fs::create_dir(d.path().join(dir)).unwrap();
    }
    for f in ["b.txt", "A.txt", "C.txt"] {
        fs::write(d.path().join(f), "x").unwrap();
    }
    // The tie-break by exact name needs a case-sensitive filesystem (macOS and
    // Windows treat a.txt and A.txt as one file).
    let case_sensitive = {
        fs::write(d.path().join("a.txt"), "x").unwrap();
        fs::read_dir(d.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    == "a.txt"
            })
            .count()
            == 2
    };
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("")).await.unwrap();
    let expected: &[&str] = if case_sensitive {
        &["Adir", "bdir", "zdir", "A.txt", "a.txt", "b.txt", "C.txt"]
    } else {
        &["Adir", "bdir", "zdir", "a.txt", "b.txt", "C.txt"]
    };
    // On a case-insensitive filesystem the surviving name keeps its case.
    let got: Vec<String> = names(&entries).iter().map(|n| n.to_lowercase()).collect();
    let want: Vec<String> = expected.iter().map(|n| n.to_lowercase()).collect();
    assert_eq!(got, want);
    if case_sensitive {
        assert_eq!(names(&entries), expected);
    }
}

#[tokio::test]
async fn list_reports_kinds_and_sizes() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join("sub")).unwrap();
    fs::write(d.path().join("five.txt"), "12345").unwrap();
    fs::write(d.path().join("empty"), "").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("five.txt", d.path().join("ln")).unwrap();
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("")).await.unwrap();
    let get = |n: &str| entries.iter().find(|e| e.name == n).unwrap().clone();
    assert_eq!(get("sub").kind, EntryKind::Directory);
    assert_eq!(get("sub").size, None);
    assert_eq!(get("five.txt").kind, EntryKind::File);
    assert_eq!(get("five.txt").size, Some(5));
    assert_eq!(get("empty").size, Some(0));
    #[cfg(unix)]
    {
        assert_eq!(get("ln").kind, EntryKind::Symlink);
        assert_eq!(get("ln").size, None);
    }
}

#[tokio::test]
async fn list_errors() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("f"), "x").unwrap();
    let ws = open(d.path()).await;
    assert_eq!(
        kind(ws.list(&wp("missing")).await.unwrap_err()),
        ErrorKind::NotFound
    );
    assert_eq!(
        kind(ws.list(&wp("f")).await.unwrap_err()),
        ErrorKind::InvalidParams
    );
    assert_eq!(
        kind(ws.list(&wp("../x")).await.unwrap_err()),
        ErrorKind::InvalidParams
    );
}

#[tokio::test]
async fn list_marks_dot_git_ignored_even_without_gitignore() {
    let d = TempDir::new().unwrap();
    fs::create_dir(d.path().join(".git")).unwrap();
    fs::write(d.path().join("a.txt"), "x").unwrap();
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("")).await.unwrap();
    assert!(entries.iter().find(|e| e.name == ".git").unwrap().ignored);
    assert!(!entries.iter().find(|e| e.name == "a.txt").unwrap().ignored);
    // Listed, not hidden.
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn list_applies_root_gitignore_including_dir_patterns() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "*.log\ntarget/\n").unwrap();
    fs::write(d.path().join("a.log"), "").unwrap();
    fs::write(d.path().join("a.txt"), "").unwrap();
    fs::create_dir(d.path().join("target")).unwrap();
    fs::create_dir(d.path().join("src")).unwrap();
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("")).await.unwrap();
    let ign = |n: &str| entries.iter().find(|e| e.name == n).unwrap().ignored;
    assert!(ign("a.log"));
    assert!(ign("target"));
    assert!(!ign("a.txt"));
    assert!(!ign("src"));
    assert!(!ign(".gitignore"));
}

#[tokio::test]
async fn list_applies_nested_gitignore_and_inherits_root_rules() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "*.log\n").unwrap();
    fs::create_dir_all(d.path().join("sub/deep")).unwrap();
    fs::write(d.path().join("sub/.gitignore"), "*.tmp\n").unwrap();
    for f in ["x.log", "x.tmp", "x.txt"] {
        fs::write(d.path().join("sub").join(f), "").unwrap();
        fs::write(d.path().join("sub/deep").join(f), "").unwrap();
        fs::write(d.path().join(f), "").unwrap();
    }
    let ws = open(d.path()).await;
    for dir in ["sub", "sub/deep"] {
        let entries = ws.list(&wp(dir)).await.unwrap();
        let ign = |n: &str| entries.iter().find(|e| e.name == n).unwrap().ignored;
        assert!(ign("x.log"), "{dir}: root rule inherited");
        assert!(ign("x.tmp"), "{dir}: nested rule applies");
        assert!(!ign("x.txt"), "{dir}");
    }
    // The nested rule must not leak upward.
    let root = ws.list(&wp("")).await.unwrap();
    assert!(!root.iter().find(|e| e.name == "x.tmp").unwrap().ignored);
}

#[tokio::test]
async fn list_negation_pattern_keeps_file() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "*.log\n!keep.log\n").unwrap();
    fs::write(d.path().join("drop.log"), "").unwrap();
    fs::write(d.path().join("keep.log"), "").unwrap();
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("")).await.unwrap();
    assert!(
        entries
            .iter()
            .find(|e| e.name == "drop.log")
            .unwrap()
            .ignored
    );
    assert!(
        !entries
            .iter()
            .find(|e| e.name == "keep.log")
            .unwrap()
            .ignored
    );
}

#[tokio::test]
async fn list_nested_negation_overrides_parent_rule() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "*.log\n").unwrap();
    fs::create_dir(d.path().join("sub")).unwrap();
    fs::write(d.path().join("sub/.gitignore"), "!important.log\n").unwrap();
    fs::write(d.path().join("sub/important.log"), "").unwrap();
    fs::write(d.path().join("sub/other.log"), "").unwrap();
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("sub")).await.unwrap();
    assert!(
        !entries
            .iter()
            .find(|e| e.name == "important.log")
            .unwrap()
            .ignored
    );
    assert!(
        entries
            .iter()
            .find(|e| e.name == "other.log")
            .unwrap()
            .ignored
    );
}

#[tokio::test]
async fn list_honours_dot_ignore_and_git_info_exclude_but_listing_stays_complete() {
    let d = TempDir::new().unwrap();
    fs::create_dir_all(d.path().join(".git/info")).unwrap();
    fs::write(d.path().join(".git/info/exclude"), "secret.txt\n").unwrap();
    fs::write(d.path().join(".ignore"), "*.bak\n").unwrap();
    for f in ["secret.txt", "a.bak", "ok.txt"] {
        fs::write(d.path().join(f), "").unwrap();
    }
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("")).await.unwrap();
    let ign = |n: &str| entries.iter().find(|e| e.name == n).unwrap().ignored;
    assert!(ign("secret.txt"));
    assert!(ign("a.bak"));
    assert!(!ign("ok.txt"));
}

#[tokio::test]
async fn list_children_of_ignored_directory_are_ignored() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join(".gitignore"), "build/\n").unwrap();
    fs::create_dir_all(d.path().join("build/inner")).unwrap();
    fs::write(d.path().join("build/x.txt"), "").unwrap();
    let ws = open(d.path()).await;
    let entries = ws.list(&wp("build")).await.unwrap();
    assert!(entries.iter().all(|e| e.ignored), "{entries:?}");
    assert_eq!(entries.len(), 2);
}

#[tokio::test]
async fn ignore_cache_is_invalidated_by_writes_through_this_crate() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "").unwrap();
    let ws = open(d.path()).await;
    assert!(!ws.list(&wp("")).await.unwrap()[0].ignored);
    ws.write_text(&wp(".gitignore"), "a.txt\n", None, false)
        .await
        .unwrap();
    let entries = ws.list(&wp("")).await.unwrap();
    assert!(entries.iter().find(|e| e.name == "a.txt").unwrap().ignored);
    ws.write_text(&wp(".gitignore"), "", None, true)
        .await
        .unwrap();
    let entries = ws.list(&wp("")).await.unwrap();
    assert!(!entries.iter().find(|e| e.name == "a.txt").unwrap().ignored);
}

// ----------------------------------------------------------------- read

#[tokio::test]
async fn read_utf8_with_version_and_bom_kept() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "héllo ✓").unwrap();
    fs::write(d.path().join("bom.txt"), b"\xEF\xBB\xBFhi").unwrap();
    let ws = open(d.path()).await;
    let r = ws.read_text(&wp("a.txt")).await.unwrap();
    assert_eq!(r.text, "héllo ✓");
    assert_eq!(r.version.0.len(), 64);
    assert!(r
        .version
        .0
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    let again = ws.read_text(&wp("a.txt")).await.unwrap();
    assert_eq!(r.version, again.version);
    let bom = ws.read_text(&wp("bom.txt")).await.unwrap();
    assert_eq!(bom.text, "\u{feff}hi");
}

#[tokio::test]
async fn read_version_is_sha256_of_bytes() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("abc"), "abc").unwrap();
    let ws = open(d.path()).await;
    assert_eq!(
        ws.read_text(&wp("abc")).await.unwrap().version.0,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[tokio::test]
async fn read_preserves_crlf_byte_for_byte() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("crlf.txt"), b"a\r\nb\r\n").unwrap();
    let ws = open(d.path()).await;
    let r = ws.read_text(&wp("crlf.txt")).await.unwrap();
    assert_eq!(r.text, "a\r\nb\r\n");
    ws.write_text(&wp("crlf.txt"), &r.text, Some(&r.version), false)
        .await
        .unwrap();
    assert_eq!(fs::read(d.path().join("crlf.txt")).unwrap(), b"a\r\nb\r\n");
}

#[tokio::test]
async fn read_errors() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("bad.bin"), b"\xff\xfe\x00bad").unwrap();
    fs::create_dir(d.path().join("dir")).unwrap();
    let ws = open(d.path()).await;
    assert_eq!(
        kind(ws.read_text(&wp("bad.bin")).await.unwrap_err()),
        ErrorKind::UnsupportedEncoding
    );
    let e = ws.read_text(&wp("dir")).await.unwrap_err();
    assert!(matches!(e, WsError::IsADirectory(_)));
    assert_eq!(kind(e), ErrorKind::InvalidParams);
    assert_eq!(
        kind(ws.read_text(&wp("nope.txt")).await.unwrap_err()),
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn read_too_large_sparse_file() {
    let d = TempDir::new().unwrap();
    let f = fs::File::create(d.path().join("big")).unwrap();
    f.set_len(17 * 1024 * 1024).unwrap();
    drop(f);
    let ws = open(d.path()).await;
    let e = ws.read_text(&wp("big")).await.unwrap_err();
    assert!(matches!(e, WsError::TooLarge { bytes } if bytes == 17 * 1024 * 1024));
    assert_eq!(kind(e), ErrorKind::TooLarge);
}

#[tokio::test]
async fn error_messages_never_contain_absolute_paths() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let root = ws.root().to_string_lossy().into_owned();
    let errs = vec![
        ws.read_text(&wp("missing/file.txt")).await.unwrap_err(),
        ws.list(&wp("missing")).await.unwrap_err(),
        ws.write_text(&wp("missing/dir/f"), "x", None, false)
            .await
            .unwrap_err(),
    ];
    for e in errs {
        let r: RpcError = e.into();
        assert!(!r.message.contains(&root), "{}", r.message);
        assert!(r.message.ends_with('.'));
    }
}

// ---------------------------------------------------------------- write

#[tokio::test]
async fn write_table_overwrite_true_always_writes() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    // Row 1: file exists or not, any expected.
    ws.write_text(&wp("a.txt"), "one", None, true)
        .await
        .unwrap();
    let bogus = FileVersion("deadbeef".into());
    let v = ws
        .write_text(&wp("a.txt"), "two", Some(&bogus), true)
        .await
        .unwrap();
    assert_eq!(fs::read_to_string(d.path().join("a.txt")).unwrap(), "two");
    assert_eq!(v, ws.read_text(&wp("a.txt")).await.unwrap().version);
    ws.write_text(&wp("b.txt"), "new", Some(&bogus), true)
        .await
        .unwrap();
    assert!(d.path().join("b.txt").exists());
}

#[tokio::test]
async fn write_table_create_new_file() {
    // Row 2: no file, overwrite=false, expected absent → write.
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let v = ws
        .write_text(&wp("new.txt"), "hello", None, false)
        .await
        .unwrap();
    assert_eq!(
        fs::read_to_string(d.path().join("new.txt")).unwrap(),
        "hello"
    );
    assert_eq!(v.0.len(), 64);
    assert!(leftovers(d.path()).is_empty());
}

#[tokio::test]
async fn write_table_missing_file_with_expected_conflicts_with_null_current() {
    // Row 3.
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let e = ws
        .write_text(&wp("new.txt"), "x", Some(&FileVersion("abc".into())), false)
        .await
        .unwrap_err();
    assert!(matches!(&e, WsError::Conflict { current: None }));
    let r: RpcError = e.into();
    assert_eq!(r.data.kind, ErrorKind::Conflict);
    assert_eq!(
        r.data.details.unwrap()["currentVersion"],
        serde_json::Value::Null
    );
    assert!(!d.path().join("new.txt").exists());
    assert!(leftovers(d.path()).is_empty());
}

#[tokio::test]
async fn write_table_existing_file_without_expected_conflicts() {
    // Row 4.
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "orig").unwrap();
    let ws = open(d.path()).await;
    let cur = ws.read_text(&wp("a.txt")).await.unwrap().version;
    let e = ws
        .write_text(&wp("a.txt"), "x", None, false)
        .await
        .unwrap_err();
    match &e {
        WsError::Conflict { current: Some(c) } => assert_eq!(c, &cur),
        other => panic!("{other:?}"),
    }
    let r: RpcError = e.into();
    assert_eq!(r.data.details.unwrap()["currentVersion"], cur.0);
    assert_eq!(fs::read_to_string(d.path().join("a.txt")).unwrap(), "orig");
    assert!(leftovers(d.path()).is_empty());
}

#[tokio::test]
async fn write_table_expected_equals_current_writes() {
    // Row 5.
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "orig").unwrap();
    let ws = open(d.path()).await;
    let cur = ws.read_text(&wp("a.txt")).await.unwrap().version;
    let v = ws
        .write_text(&wp("a.txt"), "next", Some(&cur), false)
        .await
        .unwrap();
    assert_ne!(v, cur);
    assert_eq!(fs::read_to_string(d.path().join("a.txt")).unwrap(), "next");
    // The returned version is usable for the following save.
    ws.write_text(&wp("a.txt"), "third", Some(&v), false)
        .await
        .unwrap();
    assert!(leftovers(d.path()).is_empty());
}

#[tokio::test]
async fn write_table_expected_differs_conflicts() {
    // Row 6.
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "orig").unwrap();
    let ws = open(d.path()).await;
    let stale = ws.read_text(&wp("a.txt")).await.unwrap().version;
    fs::write(d.path().join("a.txt"), "changed elsewhere").unwrap();
    let e = ws
        .write_text(&wp("a.txt"), "mine", Some(&stale), false)
        .await
        .unwrap_err();
    assert!(matches!(e, WsError::Conflict { current: Some(ref c) } if *c != stale));
    assert_eq!(
        fs::read_to_string(d.path().join("a.txt")).unwrap(),
        "changed elsewhere"
    );
    assert!(leftovers(d.path()).is_empty());
}

#[tokio::test]
async fn write_missing_parent_is_not_found_and_creates_nothing() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let e = ws
        .write_text(&wp("no/such/dir/f.txt"), "x", None, true)
        .await
        .unwrap_err();
    assert_eq!(kind(e), ErrorKind::NotFound);
    assert!(!d.path().join("no").exists());
}

#[tokio::test]
async fn write_parent_is_a_file_and_target_is_a_directory() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("f"), "x").unwrap();
    fs::create_dir(d.path().join("dir")).unwrap();
    let ws = open(d.path()).await;
    let e = ws
        .write_text(&wp("f/child"), "x", None, true)
        .await
        .unwrap_err();
    assert!(matches!(
        kind(e),
        ErrorKind::InvalidParams | ErrorKind::NotFound
    ),);
    let e = ws
        .write_text(&wp("dir"), "x", None, true)
        .await
        .unwrap_err();
    assert_eq!(kind(e), ErrorKind::InvalidParams);
    assert!(leftovers(d.path()).is_empty());
}

#[tokio::test]
async fn write_too_large_is_rejected_without_touching_file() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "orig").unwrap();
    let ws = open(d.path()).await;
    let big = "x".repeat(16 * 1024 * 1024 + 1);
    let e = ws
        .write_text(&wp("a.txt"), &big, None, true)
        .await
        .unwrap_err();
    assert_eq!(kind(e), ErrorKind::TooLarge);
    assert_eq!(fs::read_to_string(d.path().join("a.txt")).unwrap(), "orig");
    // Exactly at the limit is accepted.
    let max = "y".repeat(16 * 1024 * 1024);
    ws.write_text(&wp("a.txt"), &max, None, true).await.unwrap();
}

#[tokio::test]
async fn write_is_byte_for_byte_no_newline_conversion() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    ws.write_text(&wp("m.txt"), "a\r\nb\nc\r", None, false)
        .await
        .unwrap();
    assert_eq!(fs::read(d.path().join("m.txt")).unwrap(), b"a\r\nb\nc\r");
}

#[cfg(unix)]
#[tokio::test]
async fn write_preserves_permission_bits() {
    use std::os::unix::fs::PermissionsExt;
    let d = TempDir::new().unwrap();
    let p = d.path().join("run.sh");
    fs::write(&p, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    let ws = open(d.path()).await;
    ws.write_text(&wp("run.sh"), "#!/bin/sh\necho\n", None, true)
        .await
        .unwrap();
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o755
    );

    fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
    ws.write_text(&wp("run.sh"), "x", None, true).await.unwrap();
    assert_eq!(
        fs::metadata(&p).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test]
async fn concurrent_writers_with_same_expected_version_exactly_one_succeeds() {
    let d = TempDir::new().unwrap();
    fs::write(d.path().join("a.txt"), "base").unwrap();
    let ws = open(d.path()).await;
    let base = ws.read_text(&wp("a.txt")).await.unwrap().version;
    for round in 0..20 {
        ws.write_text(&wp("a.txt"), "base", None, true)
            .await
            .unwrap();
        let mut handles = Vec::new();
        for i in 0..8 {
            let ws = ws.clone();
            let base = base.clone();
            handles.push(tokio::spawn(async move {
                ws.write_text(&wp("a.txt"), &format!("writer {i}"), Some(&base), false)
                    .await
            }));
        }
        let mut ok = 0;
        let mut conflicts = 0;
        for h in handles {
            match h.await.unwrap() {
                Ok(_) => ok += 1,
                Err(WsError::Conflict { .. }) => conflicts += 1,
                Err(e) => panic!("round {round}: {e:?}"),
            }
        }
        assert_eq!((ok, conflicts), (1, 7), "round {round}");
        assert!(leftovers(d.path()).is_empty());
    }
}

#[tokio::test]
async fn concurrent_creates_exactly_one_succeeds() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    let mut handles = Vec::new();
    for i in 0..8 {
        let ws = ws.clone();
        handles.push(tokio::spawn(async move {
            ws.write_text(&wp("n.txt"), &format!("{i}"), None, false)
                .await
        }));
    }
    let mut ok = 0;
    for h in handles {
        if h.await.unwrap().is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 1);
}

#[tokio::test]
async fn workspace_accessors() {
    let d = TempDir::new().unwrap();
    let ws = open(d.path()).await;
    assert_eq!(ws.id().0, "w1");
    assert_eq!(ws.root(), dunce::canonicalize(d.path()).unwrap());
    assert_eq!(ws.name(), d.path().file_name().unwrap().to_string_lossy());
}
