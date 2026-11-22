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

use axiom_protocol::inspection::Verdict;
use axiom_protocol::inspection::{MappingMethod, SourceRelation, SurfaceId, UiNode};
use axiom_protocol::preview::{List, PreviewRef, PreviewState, Start, StartParams, Stop};
use axiom_protocol::task::{ListTasks, ListTasksParams};
use axiom_protocol::ui::{
    Assertion, Comparison, Edge, EdgeRef, HitTest, HitTestParams, InspectMode, InspectModeParams,
    NodeTarget, SnapshotParams, TakeSnapshot, Verify, VerifyParams,
};
use axiom_protocol::workspace::WorkspaceRef;
use common::{note_skipped, Daemon};
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
    note_skipped("preview", why);
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

/// Headless Chrome on `url` with a throw-away profile, and the adapter's capabilities once the
/// page has connected (`preview/list`), or `None` after 30 s.
async fn open_page(
    d: &Daemon,
    chrome: &Path,
    ws: &axiom_protocol::WorkspaceId,
    id: &axiom_protocol::preview::PreviewId,
    url: &str,
) -> (
    tokio::process::Child,
    tempfile::TempDir,
    Option<axiom_protocol::inspection::Capabilities>,
) {
    let profile = tempfile::tempdir().unwrap();
    let browser = tokio::process::Command::new(chrome)
        .args([
            "--headless=new",
            "--no-first-run",
            "--no-default-browser-check",
            "--disable-gpu",
            "--window-size=1024,768",
            &format!("--user-data-dir={}", profile.path().display()),
            url,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut adapter = None;
    for _ in 0..150 {
        let l = d
            .client
            .request::<List>(WorkspaceRef {
                workspace_id: ws.clone(),
            })
            .await
            .unwrap();
        adapter = l
            .previews
            .into_iter()
            .find(|p| &p.preview_id == id)
            .and_then(|p| p.adapter);
        if adapter.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    (browser, profile, adapter)
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
    let (mut browser, _profile, adapter) = open_page(&d, &chrome, &f.id, &id, &url).await;
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

    // The loop: a spacing patch, the hot reload in the page, and objective verification.
    let key_of = |tag: &str| {
        snap.nodes
            .iter()
            .find(|n| n.type_name == tag)
            .and_then(|n| n.stable_key.clone())
            .unwrap()
    };
    let (button, amount) = (key_of("button"), key_of("span"));
    let app_jsx = f.path("src/App.jsx");
    let text = std::fs::read_to_string(&app_jsx).unwrap();
    assert!(text.contains("marginLeft: 8 }"));
    std::fs::write(
        &app_jsx,
        text.replace("marginLeft: 8 }", "marginLeft: 24 }"),
    )
    .unwrap();
    let target = |k: &str| NodeTarget {
        stable_key: Some(k.into()),
        node: None,
    };
    let claim = |offset: f64| VerifyParams {
        preview_id: id.clone(),
        assertions: vec![Assertion::Geometry {
            node: target(&button),
            edge: Edge::Left,
            op: Comparison::Eq,
            value: None,
            other: Some(EdgeRef {
                node: target(&amount),
                edge: Edge::Right,
            }),
            offset,
        }],
        target: Some(target(&button)),
        timeout_ms: Some(5000),
        since_ms: None,
    };
    let r = d.client.request::<Verify>(claim(24.0)).await.unwrap();
    assert_eq!(r.verdict, Verdict::Verified, "{r:#?}");
    let r = d.client.request::<Verify>(claim(8.0)).await.unwrap();
    assert_eq!(r.verdict, Verdict::Failed, "{r:#?}");
    let r = d
        .client
        .request::<Verify>(VerifyParams {
            assertions: vec![Assertion::Property {
                node: target(&button),
                name: "css.margin-left".into(),
                equals: serde_json::json!("24px"),
            }],
            ..claim(0.0)
        })
        .await
        .unwrap();
    assert_eq!(r.verdict, Verdict::Verified, "{r:#?}");

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

/// The agent's loop over a real page (M5-AGENT-001), with the scripted model: `preview.start`
/// (the running preview), `ui.selection` (nothing selected), `ui.snapshot` and `ui.inspect` by
/// stable key (the button's margin), `fs.applyPatch`, then `ui.verify` with the geometry check
/// stated before the patch → `verified`.
#[tokio::test]
async fn the_agent_inspects_patches_and_verifies_a_ui_change() {
    use axiom_protocol::agent::{
        AgentEvent, Decision, EventBody, Policy, Send, SendParams, SessionCreate,
        SessionCreateParams, SetPolicy, TurnEndReason,
    };
    use serde_json::json;

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
    const BUTTON: &str = "src/App.jsx:11:9#0";
    const AMOUNT: &str = "src/App.jsx:10:9#0";
    let data = tempfile::tempdir().unwrap();
    let script = data.path().join("script.json");
    let key = |k: &str| json!({"stableKey": k});
    std::fs::write(
        &script,
        json!({"turns": [
            {"expect": {"userContains": "gap", "systemContains": "ui.verify",
                        "toolsInclude": ["preview.start", "ui.snapshot", "ui.inspect", "ui.selection", "ui.verify"]},
             "events": [{"type":"toolCall","id":"c1","name":"preview.start","input":{}}]},
            {"expect": {"toolResult": {"callId":"c1","contains":"page: connected"}},
             "events": [{"type":"toolCall","id":"c2","name":"ui.selection","input":{}}]},
            {"expect": {"toolResult": {"callId":"c2","contains":"Nothing is selected"}},
             "events": [{"type":"toolCall","id":"c3","name":"ui.snapshot","input":{}}]},
            {"expect": {"toolResult": {"callId":"c3","contains":format!("key={BUTTON}")}},
             "events": [{"type":"toolCall","id":"c4","name":"ui.inspect","input":{"stableKey": BUTTON}}]},
            {"expect": {"toolResult": {"callId":"c4","contains":"css.margin-left: 8px"}},
             "events": [{"type":"text","text":"Success means button.left == amount.right + 24."},
                        {"type":"toolCall","id":"c5","name":"fs.applyPatch","input":{
                 "path":"src/App.jsx",
                 "edits":[{"search":"marginLeft: 8 }","replace":"marginLeft: 24 }"}]}}]},
            {"expect": {"toolResult": {"callId":"c5","isError":false}},
             "events": [{"type":"toolCall","id":"c6","name":"ui.verify","input":{
                 "assertions":[{"kind":"geometry","node":key(BUTTON),"edge":"left","op":"eq",
                                "other":{"node":key(AMOUNT),"edge":"right"},"offset":24}],
                 "target": key(BUTTON)}}]},
            {"expect": {"toolResult": {"callId":"c6","contains":"verdict: verified"}},
             "events": [{"type":"text","text":"Verified: the gap is 24 px."}]}
        ]})
        .to_string(),
    )
    .unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("AXIOM_TEST_SCRIPTED_MODEL", &script);
    })
    .await;
    d.initialize().await;
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Allow,
            rules: vec![],
        })
        .await
        .unwrap();
    let dir = workspace();
    // Checkpoints leave the linked dependencies alone.
    std::fs::write(dir.path().join(".gitignore"), "node_modules\n").unwrap();
    let f = d.open_dir(dir).await;

    // The person's side: the preview runs and its page is open.
    let info = d
        .client
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            argv: None,
        })
        .await
        .unwrap();
    assert_eq!(info.state, PreviewState::Ready, "{info:?}");
    let (mut browser, _profile, adapter) = open_page(
        &d,
        &chrome,
        &f.id,
        &info.preview_id,
        info.url.as_deref().unwrap(),
    )
    .await;
    adapter.expect("the page's adapter connected");
    // The page has rendered (the adapter connects before React mounts): the agent's first
    // snapshot must see the app, not an empty root.
    let mut rendered = false;
    for _ in 0..50 {
        let s = d
            .client
            .request::<TakeSnapshot>(SnapshotParams {
                preview_id: info.preview_id.clone(),
                max_nodes: None,
                include_invisible: false,
            })
            .await
            .unwrap();
        if s.nodes.iter().any(|n| n.type_name == "button") {
            rendered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(rendered, "the app rendered");

    let session = d
        .client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: Some(f.id.clone()),
            provider_id: "scripted".into(),
            model: "scripted".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id;
    d.client
        .request::<Send>(SendParams {
            workspace_id: Some(f.id.clone()),
            session_id: session.clone(),
            text: "Make the gap between the amount and the Pay button 24 px.".into(),
            attachments: vec![],
            images: vec![],
            from_turn: None,
        })
        .await
        .unwrap();
    let mut evs = Vec::new();
    loop {
        let v = d
            .next_notification("agent/event", Duration::from_secs(120))
            .await;
        let e: AgentEvent = serde_json::from_value(v).unwrap();
        if e.session_id != session {
            continue;
        }
        let end = matches!(e.event, EventBody::TurnEnd { .. });
        evs.push(e);
        if end {
            break;
        }
    }
    let errors: Vec<_> = evs
        .iter()
        .filter_map(|e| match &e.event {
            EventBody::Error { message, .. } => Some(message.clone()),
            _ => None,
        })
        .collect();
    assert!(
        matches!(
            evs.last().map(|e| &e.event),
            Some(EventBody::TurnEnd {
                reason: TurnEndReason::EndTurn
            })
        ),
        "errors: {errors:?}"
    );
    let tools: Vec<String> = evs
        .iter()
        .filter_map(|e| match &e.event {
            EventBody::ToolCall { tool, .. } => Some(tool.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        tools,
        [
            "preview.start",
            "ui.selection",
            "ui.snapshot",
            "ui.inspect",
            "fs.applyPatch",
            "ui.verify"
        ]
    );
    assert!(std::fs::read_to_string(f.path("src/App.jsx"))
        .unwrap()
        .contains("marginLeft: 24 }"));

    d.client
        .request::<Stop>(PreviewRef {
            preview_id: info.preview_id,
        })
        .await
        .unwrap();
    let _ = browser.kill().await;
}
