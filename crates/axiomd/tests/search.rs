//! `workspace/search` through the daemon (M4-UX-001).

mod common;

use axiom_protocol::workspace::{Search, SearchMode, SearchParams};
use axiom_protocol::ErrorKind;
use common::{kind, Daemon};

fn params(ws: &axiom_protocol::WorkspaceId, query: &str) -> SearchParams {
    SearchParams {
        workspace_id: ws.clone(),
        query: query.into(),
        mode: SearchMode::Text,
        regex: false,
        case_sensitive: false,
        whole_word: false,
        include: None,
        exclude: None,
        limit: None,
    }
}

#[tokio::test]
async fn text_files_and_errors() {
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    std::fs::write(f.path("needle.txt"), "a needle here\n").unwrap();
    let r = d
        .client
        .request::<Search>(params(&f.id, "needle"))
        .await
        .unwrap();
    let m = r
        .matches
        .iter()
        .find(|m| m.path.0 == "needle.txt")
        .expect("found");
    assert_eq!((m.line, m.column, m.end_column), (1, 3, 9));
    assert_eq!(m.preview, "a needle here");
    // The file list honours ignores (the fixture's .gitignore skips target/ and build.log).
    let mut p = params(&f.id, "");
    p.mode = SearchMode::Files;
    let r = d.client.request::<Search>(p).await.unwrap();
    assert!(r.files.iter().any(|f| f.0 == "needle.txt"));
    assert!(
        r.files
            .iter()
            .all(|f| !f.0.starts_with("target/") && f.0 != "build.log"),
        "{:?}",
        r.files
    );
    let mut bad = params(&f.id, "(");
    bad.regex = true;
    let e = d.client.request::<Search>(bad).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}
