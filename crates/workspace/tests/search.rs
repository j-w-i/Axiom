//! `Workspace::search` and `remove_file` (M3-TOOLS-001).

use axiom_protocol::WorkspacePath;
use axiom_workspace::{SearchQuery, Workspaces, WsError};
use std::fs;

fn q(p: &str) -> SearchQuery {
    SearchQuery {
        pattern: p.into(),
        regex: false,
        case_sensitive: false,
        glob: None,
        offset: 0,
        limit: 100,
    }
}

async fn fixture() -> (
    tempfile::TempDir,
    std::sync::Arc<axiom_workspace::Workspace>,
) {
    let d = tempfile::tempdir().unwrap();
    let r = d.path();
    fs::create_dir_all(r.join("src")).unwrap();
    fs::create_dir_all(r.join("target/debug")).unwrap();
    fs::create_dir_all(r.join(".git")).unwrap();
    fs::write(r.join(".gitignore"), "target/\n").unwrap();
    fs::write(r.join("src/main.rs"), "fn main() {\n    helper();\n}\n").unwrap();
    fs::write(
        r.join("src/lib.rs"),
        "pub fn helper() {}\n// Helper again\n",
    )
    .unwrap();
    fs::write(r.join("target/debug/out.rs"), "fn helper() {}\n").unwrap();
    fs::write(r.join(".git/config"), "helper\n").unwrap();
    fs::write(r.join("blob.bin"), b"helper\0\0").unwrap();
    fs::write(r.join(".env.example"), "HELPER=1\n").unwrap();
    let ws = Workspaces::new();
    let id = ws.open(r.to_str().unwrap()).await.unwrap().workspace_id;
    let w = ws.get(&id).unwrap();
    (d, w)
}

#[tokio::test]
async fn search_honours_ignores_and_skips_git_and_binaries() {
    let (_d, w) = fixture().await;
    let r = w.search(q("helper")).await.unwrap();
    let hits: Vec<(String, u32, u32)> = r
        .matches
        .iter()
        .map(|m| (m.path.0.clone(), m.line, m.column))
        .collect();
    assert_eq!(
        hits,
        [
            (".env.example".to_string(), 1, 1),
            ("src/lib.rs".into(), 1, 8),
            ("src/lib.rs".into(), 2, 4),
            ("src/main.rs".into(), 2, 5),
        ]
    );
    assert_eq!(r.next_offset, None);
}

#[tokio::test]
async fn search_case_regex_glob_and_paging() {
    let (_d, w) = fixture().await;
    let mut s = q("Helper");
    s.case_sensitive = true;
    assert_eq!(w.search(s).await.unwrap().matches.len(), 1);
    let mut s = q(r"fn \w+\(");
    s.regex = true;
    s.glob = Some("src/**".into());
    let r = w.search(s.clone()).await.unwrap();
    assert_eq!(r.matches.len(), 2);
    s.limit = 1;
    let p1 = w.search(s.clone()).await.unwrap();
    assert_eq!(p1.next_offset, Some(1));
    s.offset = 1;
    let p2 = w.search(s).await.unwrap();
    assert_eq!(p2.next_offset, None);
    assert_ne!(p1.matches[0], p2.matches[0]);
    let mut bad = q("(");
    bad.regex = true;
    assert!(matches!(w.search(bad).await, Err(WsError::InvalidPath(_))));
}

#[tokio::test]
async fn remove_file_checks_the_version() {
    let (_d, w) = fixture().await;
    let p = WorkspacePath("src/lib.rs".into());
    let v = w.read_text(&p).await.unwrap().version;
    let stale = axiom_protocol::fs::FileVersion("0".repeat(64));
    assert!(matches!(
        w.remove_file(&p, &stale).await,
        Err(WsError::Conflict { .. })
    ));
    w.remove_file(&p, &v).await.unwrap();
    assert!(matches!(w.read_text(&p).await, Err(WsError::NotFound(_))));
    assert!(matches!(
        w.remove_file(&WorkspacePath("src".into()), &v).await,
        Err(WsError::IsADirectory(_))
    ));
}
