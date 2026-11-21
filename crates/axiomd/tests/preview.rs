//! A preview through the daemon (M5-PREVIEW-001), with the React + Vite fixture
//! (`tests/fixtures/inspect-react`, `pnpm install` there first; CI does it) and a real page:
//! headless Chrome loads the preview's URL with a throw-away profile, the injected adapter dials
//! the daemon, and `ui/*` answers from it. `preview/start` → ready with the URL; the page
//! connects (level 3: line and column); stamps resolve exactly (`expected-map.json`), the
//! library element gets an uncertain owner; inspect mode turns on; `preview/stop` leaves no dev
//! server; the user's `vite.config.js` is never modified.
//!
//! Without the fixture's `node_modules` or a Chrome, the test is skipped with a note, unless
//! `AXIOM_REQUIRE_PREVIEW=1` (CI), where it fails.
#![cfg(unix)]

mod common;

use axiom_protocol::inspection::{MappingMethod, SourceRelation, SurfaceId, UiNode};
use axiom_protocol::preview::{List, PreviewRef, PreviewState, Start, StartParams, Stop};
use axiom_protocol::task::{ListTasks, ListTasksParams};
use axiom_protocol::ui::{
    HitTest, HitTestParams, InspectMode, InspectModeParams, SnapshotParams, TakeSnapshot,
};
use axiom_protocol::workspace::WorkspaceRef;
use common::Daemon;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/inspect-react")
}

fn chrome() -> Option<PathBuf> {
    [
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ]
    .iter()
    .find_map(|n| {
        std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(n))
                .find(|f| f.is_file())
        })
    })
}

fn skip(why: &str) -> bool {
    if std::env::var("AXIOM_REQUIRE_PREVIEW").as_deref() == Ok("1") {
        panic!("{why}");
    }
    eprintln!("SKIPPED: {why}");
    true
}

/// A copy of the fixture's sources (its `node_modules` linked) in a temp dir.
fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let src = fixture();
    for f in [
        "package.json",
        "vite.config.js",
        "index.html",
        "expected-map.json",
    ] {
        std::fs::copy(src.join(f), dir.path().join(f)).unwrap();
    }
    for d in ["src", "lib"] {
        std::fs::create_dir(dir.path().join(d)).unwrap();
        for e in std::fs::read_dir(src.join(d)).unwrap().flatten() {
            std::fs::copy(e.path(), dir.path().join(d).join(e.file_name())).unwrap();
        }
    }
    std::os::unix::fs::symlink(src.join("node_modules"), dir.path().join("node_modules")).unwrap();
    dir
}

#[tokio::test]
async fn a_vite_preview_runs_and_its_page_is_inspected() {
    if !fixture().join("node_modules/.bin/vite").is_file()
        && skip("the fixture's dependencies are not installed (pnpm install in tests/fixtures/inspect-react)")
    {
        return;
    }
    let Some(chrome) = chrome() else {
        if skip("no Chrome or Chromium on PATH") {
            return;
        }
        unreachable!();
    };
    let d = Daemon::ready().await;
    let dir = workspace();
    let config_before = std::fs::read(dir.path().join("vite.config.js")).unwrap();
    let f = d.open_dir(dir).await;

    // Start: ready with a loopback URL.
    let info = d
        .client
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            argv: None,
        })
        .await
        .unwrap();
    assert_eq!(info.state, PreviewState::Ready, "{info:?}");
    let url = info.url.clone().unwrap();
    assert!(url.starts_with("http://127.0.0.1:"), "{url}");
    let id = info.preview_id.clone();
    // A second start returns the same preview.
    let again = d
        .client
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            argv: None,
        })
        .await
        .unwrap();
    assert_eq!(again.preview_id, id);

    // A real page: headless Chrome, a throw-away profile.
    let profile = tempfile::tempdir().unwrap();
    let mut browser = tokio::process::Command::new(&chrome)
        .args([
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-gpu",
            "--window-size=1024,768",
            &format!("--user-data-dir={}", profile.path().display()),
            &url,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();

    // The adapter connects: `preview/state` with its capabilities (level 3: line and column).
    let mut adapter = None;
    for _ in 0..150 {
        let l = d
            .client
            .request::<List>(WorkspaceRef {
                workspace_id: f.id.clone(),
            })
            .await
            .unwrap();
        adapter = l
            .previews
            .into_iter()
            .find(|p| p.preview_id == id)
            .and_then(|p| p.adapter);
        if adapter.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let caps = adapter.expect("the page's adapter connected");
    assert_eq!(
        caps.source,
        axiom_protocol::inspection::SourcePrecision::LineColumn
    );
    assert!(!caps.screenshot);

    // The snapshot: the expected map's elements, exact; the library element, an uncertain owner.
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(fixture().join("expected-map.json")).unwrap(),
    )
    .unwrap();
    let mut snap = None;
    for _ in 0..50 {
        let s = d
            .client
            .request::<TakeSnapshot>(SnapshotParams {
                preview_id: id.clone(),
                max_nodes: None,
                include_invisible: false,
            })
            .await
            .unwrap();
        if s.nodes.iter().any(|n| n.type_name == "button") {
            snap = Some(s);
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let snap = snap.expect("the app rendered");
    let stamp = |n: &UiNode| {
        n.sources.first().map(|c| {
            format!(
                "{}:{}:{}",
                c.location.file.0,
                c.location.line,
                c.location.column.unwrap_or(0)
            )
        })
    };
    for (sel, want) in expected["exact"].as_object().unwrap() {
        let (tag, _class) = sel.split_once('.').unwrap();
        let n = snap
            .nodes
            .iter()
            .find(|n| n.type_name == tag && stamp(n).as_deref() == want.as_str())
            .unwrap_or_else(|| panic!("{sel} at {want}"));
        assert_eq!(n.sources[0].method, MappingMethod::BuildInstrumentation);
        assert_eq!(n.sources[0].confidence, 1.0);
    }
    let em = snap
        .nodes
        .iter()
        .find(|n| n.type_name == "em")
        .expect("the badge");
    assert!(em
        .sources
        .iter()
        .all(|c| c.relation == SourceRelation::Owner));
    assert_eq!(
        stamp(em).as_deref(),
        expected["uncertain"]["em.badge"]["location"].as_str()
    );
    assert!(em.sources[0].confidence < 0.6);

    // A hit test at the button's centre names it first.
    let b = snap
        .nodes
        .iter()
        .find(|n| n.type_name == "button")
        .unwrap()
        .bounds
        .unwrap();
    let hit = d
        .client
        .request::<HitTest>(HitTestParams {
            preview_id: id.clone(),
            surface: SurfaceId("page".into()),
            x: b.x + b.width / 2.0,
            y: b.y + b.height / 2.0,
        })
        .await
        .unwrap();
    assert_eq!(hit.chain[0].type_name, "button");
    d.client
        .request::<InspectMode>(InspectModeParams {
            preview_id: id.clone(),
            on: true,
        })
        .await
        .unwrap();

    // Stop: the dev server ends; the user's config is untouched.
    d.client
        .request::<Stop>(PreviewRef {
            preview_id: id.clone(),
        })
        .await
        .unwrap();
    let task = info.task_id.unwrap();
    let mut ended = false;
    for _ in 0..50 {
        let tasks = d
            .client
            .request::<ListTasks>(ListTasksParams {
                workspace_id: f.id.clone(),
            })
            .await
            .unwrap();
        if tasks
            .tasks
            .iter()
            .any(|t| t.task_id == task && t.exit.is_some())
        {
            ended = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(ended, "the dev server ended");
    let l = d
        .client
        .request::<List>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(
        l.previews.iter().all(|p| p.preview_id != id),
        "a stopped preview is not listed"
    );
    assert_eq!(
        std::fs::read(f.path("vite.config.js")).unwrap(),
        config_before
    );
    let _ = browser.kill().await;
}
