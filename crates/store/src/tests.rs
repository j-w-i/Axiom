use super::*;
use serde_json::json;

fn map(items: &[(&str, Option<Value>)]) -> BTreeMap<String, Option<Value>> {
    items
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

#[test]
fn settings_round_trip_and_delete() {
    let s = Store::open_in_memory().unwrap();
    assert_eq!(s.setting_get("a").unwrap(), None);
    s.settings_set(&map(&[
        ("a", Some(json!(1))),
        ("b", Some(json!({"x": [1, 2]}))),
    ]))
    .unwrap();
    assert_eq!(s.setting_get("a").unwrap(), Some(json!(1)));
    s.settings_set(&map(&[("a", Some(json!("two"))), ("b", None)]))
        .unwrap();
    let all = s.settings_all().unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all["a"], json!("two"));
    // Deleting a missing key is not an error.
    s.settings_set(&map(&[("zzz", None)])).unwrap();
}

#[test]
fn recent_ordering_limit_and_update() {
    let s = Store::open_in_memory().unwrap();
    s.recent_touch("/a", "a", 100).unwrap();
    s.recent_touch("/b", "b", 300).unwrap();
    s.recent_touch("/c", "c", 200).unwrap();
    let l = s.recent_list(10).unwrap();
    assert_eq!(
        l.iter().map(|r| r.root.as_str()).collect::<Vec<_>>(),
        ["/b", "/c", "/a"]
    );
    assert_eq!(s.recent_list(2).unwrap().len(), 2);
    assert_eq!(s.recent_list(0).unwrap().len(), 0);
    s.recent_touch("/a", "renamed", 400).unwrap();
    let l = s.recent_list(10).unwrap();
    assert_eq!(
        l[0],
        Recent {
            root: "/a".into(),
            name: "renamed".into(),
            last_opened_ms: 400
        }
    );
    assert_eq!(l.len(), 3);
    s.recent_remove("/a").unwrap();
    assert_eq!(s.recent_list(10).unwrap().len(), 2);
    s.recent_remove("/never").unwrap();
}

#[test]
fn recent_list_does_not_check_filesystem() {
    let s = Store::open_in_memory().unwrap();
    s.recent_touch("/definitely/not/a/real/dir", "x", 1)
        .unwrap();
    assert_eq!(s.recent_list(5).unwrap().len(), 1);
}

#[test]
fn workspace_state_keys_all_and_delete_and_isolation() {
    let s = Store::open_in_memory().unwrap();
    s.workspace_state_set(
        "/w1",
        &map(&[("tabs", Some(json!(["a"]))), ("sel", Some(json!("a")))]),
    )
    .unwrap();
    s.workspace_state_set("/w2", &map(&[("tabs", Some(json!(["z"])))]))
        .unwrap();
    assert_eq!(s.workspace_state_get("/w1", &[]).unwrap().len(), 2);
    let only = s
        .workspace_state_get("/w1", &["sel".into(), "missing".into()])
        .unwrap();
    assert_eq!(only.len(), 1);
    assert_eq!(only["sel"], json!("a"));
    s.workspace_state_set("/w1", &map(&[("sel", None)]))
        .unwrap();
    assert_eq!(s.workspace_state_get("/w1", &[]).unwrap().len(), 1);
    assert_eq!(
        s.workspace_state_get("/w2", &[]).unwrap()["tabs"],
        json!(["z"])
    );
}

#[test]
fn data_survives_reopen_and_creates_parent_dir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/deeper/axiom.db");
    {
        let s = Store::open(&path).unwrap();
        s.settings_set(&map(&[("k", Some(json!(true)))])).unwrap();
        s.recent_touch("/r", "r", 5).unwrap();
        s.workspace_state_set("/r", &map(&[("s", Some(json!(1)))]))
            .unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(s.setting_get("k").unwrap(), Some(json!(true)));
    assert_eq!(s.recent_list(5).unwrap().len(), 1);
    assert_eq!(s.workspace_state_get("/r", &[]).unwrap()["s"], json!(1));
    assert!(s.recovered_from_corruption().is_none());
}

#[test]
fn wal_and_user_version_are_set() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("axiom.db");
    let _s = Store::open(&path).unwrap();
    let c = Connection::open(&path).unwrap();
    let mode: String = c
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    let v: u32 = c
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    assert_eq!(v, latest_version());
}

#[test]
fn schema_too_new_is_rejected_and_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("axiom.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch("CREATE TABLE t(x); PRAGMA user_version = 999;")
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    let err = Store::open(&path).unwrap_err();
    assert!(
        matches!(err, StoreError::SchemaTooNew { found: 999, supported } if supported == latest_version()),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "file must be byte-identical"
    );
    let siblings: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(siblings.len(), 1, "no wal/shm/corrupt files created");
}

#[test]
fn corrupt_database_is_moved_aside_and_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("axiom.db");
    {
        let s = Store::open(&path).unwrap();
        s.settings_set(&map(&[("k", Some(json!(1)))])).unwrap();
    }
    std::fs::write(&path, vec![0xAB; 8192]).unwrap();
    let s = Store::open(&path).unwrap();
    let moved = s.recovered_from_corruption().expect("recovered");
    assert!(moved.exists());
    assert!(moved
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("axiom.db.corrupt-"));
    assert!(s.settings_all().unwrap().is_empty());
    s.settings_set(&map(&[("fresh", Some(json!(1)))])).unwrap();
    assert_eq!(std::fs::read(&moved).unwrap(), vec![0xAB; 8192]);
}

#[test]
fn too_large_is_atomic_and_reports_key() {
    let s = Store::open_in_memory().unwrap();
    let big = Value::String("x".repeat(MAX_VALUE_BYTES + 1));
    let err = s
        .settings_set(&map(&[("a", Some(json!(1))), ("big", Some(big.clone()))]))
        .unwrap_err();
    match err {
        StoreError::TooLarge { key, limit, .. } => {
            assert_eq!(key, "big");
            assert_eq!(limit, MAX_VALUE_BYTES);
        }
        e => panic!("{e:?}"),
    }
    assert!(s.settings_all().unwrap().is_empty(), "nothing written");
    let err = s
        .workspace_state_set("/w", &map(&[("a", Some(json!(1))), ("big", Some(big))]))
        .unwrap_err();
    assert!(matches!(err, StoreError::TooLarge { .. }));
    assert!(s.workspace_state_get("/w", &[]).unwrap().is_empty());
    // Exactly at the limit is fine (2 bytes of quotes + content).
    let ok = Value::String("x".repeat(MAX_VALUE_BYTES - 2));
    s.settings_set(&map(&[("ok", Some(ok))])).unwrap();
}

#[test]
fn default_db_path_honours_axiom_data_dir() {
    // Single test touching this variable; keep it that way.
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("AXIOM_DATA_DIR", dir.path());
    assert_eq!(default_db_path(), dir.path().join("axiom.db"));
    std::env::remove_var("AXIOM_DATA_DIR");
    let p = default_db_path();
    assert!(p.ends_with("axiom.db"));
    assert!(!p.starts_with(dir.path()));
}

#[test]
fn sqlite_version_is_reported() {
    let s = Store::open_in_memory().unwrap();
    let v: String = s
        .conn()
        .query_row("SELECT sqlite_version()", [], |r| r.get(0))
        .unwrap();
    eprintln!("sqlite_version={v}");
    assert!(!v.is_empty());
}

#[test]
fn workspace_trust_set_clear_and_isolation() {
    let s = Store::open_in_memory().unwrap();
    assert!(!s.workspace_trusted("/a").unwrap());
    s.set_workspace_trusted("/a", true, 1).unwrap();
    s.set_workspace_trusted("/a", true, 2).unwrap();
    assert!(s.workspace_trusted("/a").unwrap());
    assert!(!s.workspace_trusted("/b").unwrap());
    s.set_workspace_trusted("/a", false, 3).unwrap();
    assert!(!s.workspace_trusted("/a").unwrap());
}

/// A version-1 database (M1) migrates to the trust table and keeps its data.
#[test]
fn version_1_database_migrates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("axiom.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(MIGRATIONS[0].1).unwrap();
        c.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO settings (key, value) VALUES ('editor.fontSize', '14');",
        )
        .unwrap();
    }
    let s = Store::open(&path).unwrap();
    assert_eq!(
        s.setting_get("editor.fontSize").unwrap(),
        Some(Value::from(14))
    );
    s.set_workspace_trusted("/w", true, 1).unwrap();
    assert!(s.workspace_trusted("/w").unwrap());
}

#[test]
fn build_history_keeps_the_newest_twenty_per_workspace() {
    let s = Store::open_in_memory().unwrap();
    let run = |i: u64| BuildRun {
        provider: "rust".into(),
        task: "build".into(),
        started_ms: i,
        duration_ms: 10,
        outcome: "succeeded".into(),
        errors: 0,
        warnings: i as u32,
    };
    for i in 0..25 {
        s.build_history_add("/a", &run(i)).unwrap();
    }
    s.build_history_add("/b", &run(99)).unwrap();
    let a = s.build_history("/a").unwrap();
    assert_eq!(a.len(), BUILD_HISTORY_LIMIT);
    assert_eq!(a[0].started_ms, 24, "newest first");
    assert_eq!(a.last().unwrap().started_ms, 5);
    assert_eq!(s.build_history("/b").unwrap(), [run(99)]);
}

#[test]
fn agent_sessions_index_round_trip() {
    let s = Store::open_in_memory().unwrap();
    let row = |id: &str, root: Option<&str>, t: u64| SessionRow {
        id: id.into(),
        root: root.map(String::from),
        title: format!("t{id}"),
        provider: "p".into(),
        model: "m".into(),
        created_ms: 1,
        updated_ms: t,
    };
    s.session_put(&row("a", Some("/w"), 5)).unwrap();
    s.session_put(&row("b", Some("/w"), 9)).unwrap();
    s.session_put(&row("c", None, 7)).unwrap();
    let ids = |v: Vec<SessionRow>| v.into_iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(s.session_list(Some("/w")).unwrap()), ["b", "a"]);
    assert_eq!(ids(s.session_list(None).unwrap()), ["c"]);
    let mut a = s.session_get("a").unwrap().unwrap();
    a.title = "renamed".into();
    s.session_put(&a).unwrap();
    assert_eq!(s.session_get("a").unwrap().unwrap().title, "renamed");
    assert!(s.session_delete("a").unwrap());
    assert!(!s.session_delete("a").unwrap());
    assert_eq!(s.session_get("a").unwrap(), None);
}
