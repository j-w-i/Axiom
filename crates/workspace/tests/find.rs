//! `workspace/search`'s engine (M4-UX-001): text matches with Monaco columns, whole word,
//! include/exclude, the file list for quick open, .gitignore, limits, cancellation, previews, and
//! the latency on 10 000 files.

use axiom_workspace::{FindQuery, Workspaces};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

async fn ws(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<axiom_workspace::Workspace>) {
    let d = tempfile::tempdir().unwrap();
    for (p, t) in files {
        let path = d.path().join(p);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, t).unwrap();
    }
    let reg = Workspaces::new();
    let o = reg.open(d.path().to_str().unwrap()).await.unwrap();
    let w = reg.get(&o.workspace_id).unwrap();
    (d, w)
}

fn q(query: &str) -> FindQuery {
    FindQuery {
        query: query.into(),
        ..Default::default()
    }
}

fn no() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[tokio::test]
async fn text_matches_with_monaco_columns_and_every_match_on_a_line() {
    let (_d, w) = ws(&[
        ("a.rs", "let 😀 = foo(foo);\nnothing\n"),
        ("b/c.ts", "const FOO = 1;\n"),
    ])
    .await;
    let r = w.find(q("foo"), no()).await.unwrap();
    let m: Vec<_> = r
        .matches
        .iter()
        .map(|m| (m.path.0.as_str(), m.line, m.column, m.end_column))
        .collect();
    // 😀 is two UTF-16 units: "let 😀 = " is 9 units, so foo starts at column 10.
    assert_eq!(
        m,
        vec![
            ("a.rs", 1, 10, 13),
            ("a.rs", 1, 14, 17),
            ("b/c.ts", 1, 7, 10)
        ]
    );
    assert_eq!(r.files_searched, 2);
    let r = w
        .find(
            FindQuery {
                case_sensitive: true,
                ..q("foo")
            },
            no(),
        )
        .await
        .unwrap();
    assert_eq!(r.matches.len(), 2);
}

#[tokio::test]
async fn whole_word_regex_include_exclude_and_errors() {
    let (_d, w) = ws(&[
        ("src/a.rs", "foo foobar barfoo foo\n"),
        ("src/b.rs", "foo\n"),
        ("tests/t.rs", "foo\n"),
        ("docs/x.md", "foo\n"),
    ])
    .await;
    let r = w
        .find(
            FindQuery {
                whole_word: true,
                include: Some("src/a.rs".into()),
                ..q("foo")
            },
            no(),
        )
        .await
        .unwrap();
    assert_eq!(
        r.matches.iter().map(|m| m.column).collect::<Vec<_>>(),
        vec![1, 19]
    );
    let r = w
        .find(
            FindQuery {
                regex: true,
                ..q(r"fo+bar")
            },
            no(),
        )
        .await
        .unwrap();
    assert_eq!(r.matches.len(), 1);
    let r = w
        .find(
            FindQuery {
                include: Some("src/**, tests/**".into()),
                exclude: Some("src/b.rs".into()),
                ..q("foo")
            },
            no(),
        )
        .await
        .unwrap();
    let files: std::collections::BTreeSet<_> = r.matches.iter().map(|m| m.path.0.clone()).collect();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        vec!["src/a.rs", "tests/t.rs"]
    );
    let r = w
        .find(
            FindQuery {
                exclude: Some("**/*.md".into()),
                ..q("foo")
            },
            no(),
        )
        .await
        .unwrap();
    assert!(r.matches.iter().all(|m| !m.path.0.ends_with(".md")));
    assert!(w
        .find(
            FindQuery {
                regex: true,
                ..q("(")
            },
            no()
        )
        .await
        .is_err());
}

#[tokio::test]
async fn the_file_list_honours_gitignore_and_limits() {
    let (_d, w) = ws(&[
        (".gitignore", "target/\n"),
        ("src/main.rs", ""),
        ("target/debug/x", ""),
        ("README.md", ""),
    ])
    .await;
    let r = w
        .find(
            FindQuery {
                files: true,
                ..Default::default()
            },
            no(),
        )
        .await
        .unwrap();
    let f: Vec<_> = r.files.iter().map(|p| p.0.as_str()).collect();
    assert_eq!(f, vec![".gitignore", "README.md", "src/main.rs"]);
    let r = w
        .find(
            FindQuery {
                files: true,
                limit: 2,
                ..Default::default()
            },
            no(),
        )
        .await
        .unwrap();
    assert!(r.truncated && r.files.len() == 2);
}

#[tokio::test]
async fn truncated_cancelled_and_previews() {
    let long = format!("{}needle{}\n", "x".repeat(500), "y".repeat(500));
    let many = "needle\n".repeat(50);
    let (_d, w) = ws(&[("long.txt", &long), ("many.txt", &many)]).await;
    let r = w
        .find(
            FindQuery {
                limit: 10,
                ..q("needle")
            },
            no(),
        )
        .await
        .unwrap();
    assert!(r.truncated && r.matches.len() == 10);
    let lm = w
        .find(
            FindQuery {
                include: Some("long.txt".into()),
                ..q("needle")
            },
            no(),
        )
        .await
        .unwrap();
    let m = &lm.matches[0];
    assert_eq!(m.column, 501);
    assert!(m.preview.chars().count() <= 300);
    assert_eq!(&m.preview[(m.preview_column - 1) as usize..][..6], "needle");
    let cancelled = Arc::new(AtomicBool::new(true));
    let r = w.find(q("needle"), cancelled).await.unwrap();
    assert!(r.matches.is_empty());
}

/// AUD-006: a minified bundle (one 2 MiB line) searched for one character used to rescan the line
/// from its start for every match (quadratic: minutes to hours). One forward pass, capped at the
/// limit, answers at once, with the same columns as before.
#[tokio::test]
async fn a_two_mebibyte_single_line_file_is_searched_at_once() {
    let line = "a".repeat(2 * 1024 * 1024 - 1) + "\n";
    let (_d, w) = ws(&[("bundle.min.js", &line)]).await;
    let t = std::time::Instant::now();
    let r = w
        .find(
            FindQuery {
                limit: 2000,
                ..q("a")
            },
            no(),
        )
        .await
        .unwrap();
    assert!(
        t.elapsed() < std::time::Duration::from_secs(1),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(r.matches.len(), 2000);
    assert!(r.truncated);
    let last = &r.matches[1999];
    assert_eq!((last.line, last.column, last.end_column), (1, 2000, 2001));
    assert_eq!(last.preview_column, 61);
    assert!(last.preview.chars().count() <= 300);
}

/// Columns stay exact across many matches on one line with characters of several widths.
#[tokio::test]
async fn columns_across_many_matches_on_a_line_with_wide_characters() {
    let line = "é😀x".repeat(100);
    let (_d, w) = ws(&[("w.txt", &line)]).await;
    let r = w.find(q("x"), no()).await.unwrap();
    assert_eq!(r.matches.len(), 100);
    for (k, m) in r.matches.iter().enumerate() {
        // Each "é😀x" is 1 + 2 + 1 = 4 UTF-16 units.
        let col = 4 * k as u32 + 4;
        assert_eq!((m.column, m.end_column), (col, col + 1));
        let p: Vec<u16> = m.preview.encode_utf16().collect();
        assert_eq!(p[(m.preview_column - 1) as usize], 'x' as u16);
    }
}

/// The budget (M4-UX-001): the first page of a regex search over 10 000 files under 300 ms.
#[tokio::test]
async fn ten_thousand_files_first_page_is_fast() {
    let d = tempfile::tempdir().unwrap();
    for i in 0..10_000 {
        let dir = d.path().join(format!("m{}", i / 100));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("f{i}.rs")),
            format!("fn item_{i}() -> u32 {{\n    {i}\n}}\n"),
        )
        .unwrap();
    }
    let reg = Workspaces::new();
    let o = reg.open(d.path().to_str().unwrap()).await.unwrap();
    let w = reg.get(&o.workspace_id).unwrap();
    let _ = w
        .find(
            FindQuery {
                regex: true,
                ..q(r"item_9\d{3}\b")
            },
            no(),
        )
        .await
        .unwrap(); // warm the cache
    let t = std::time::Instant::now();
    let r = w
        .find(
            FindQuery {
                regex: true,
                ..q(r"item_9\d{3}\b")
            },
            no(),
        )
        .await
        .unwrap();
    let ms = t.elapsed().as_millis();
    eprintln!(
        "regex search over 10 000 files: {ms} ms, {} matches",
        r.matches.len()
    );
    assert_eq!(r.matches.len(), 1000);
    assert!(ms < 300, "{ms} ms");
    let t = std::time::Instant::now();
    let f = w
        .find(
            FindQuery {
                files: true,
                ..Default::default()
            },
            no(),
        )
        .await
        .unwrap();
    eprintln!("file list of 10 000 files: {} ms", t.elapsed().as_millis());
    assert_eq!(f.files.len(), 10_000);
}

/// The parallel walk does not change the answer: a truncated search holds the first matches in
/// path order, every time.
#[tokio::test]
async fn truncated_results_are_the_first_in_path_order() {
    let files: Vec<(String, String)> = (0..300)
        .map(|i| {
            (
                format!("d{:02}/f{i:03}.txt", i % 37),
                "hit\nhit\n".to_string(),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (_d, w) = ws(&refs).await;
    let mut want: Vec<String> = files.iter().map(|f| f.0.clone()).collect();
    want.sort();
    for _ in 0..5 {
        let r = w
            .find(
                FindQuery {
                    limit: 51,
                    ..q("hit")
                },
                no(),
            )
            .await
            .unwrap();
        assert!(r.truncated);
        assert_eq!(r.matches.len(), 51);
        let got: Vec<(String, u32)> = r
            .matches
            .iter()
            .map(|m| (m.path.0.clone(), m.line))
            .collect();
        let expect: Vec<(String, u32)> = want
            .iter()
            .flat_map(|p| [(p.clone(), 1), (p.clone(), 2)])
            .take(51)
            .collect();
        assert_eq!(got, expect);
    }
}
