//! The built-in agent tools through the registry, in process, against temp workspaces
//! (M3-TOOLS-001). No model and no gate: `Allowed::for_tests` (feature `test-support`) stands in
//! for the gate's decision, which M3-GATE-001 tests.

use axiom_agent::tools::{Allowed, ToolCtx, ToolError, ToolOutput};
use axiom_protocol::agent::Effect;
use axiom_protocol::{ErrorKind, WorkspaceId};
use axiom_store::Store;
use axiomd::App;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

struct Env {
    app: Arc<App>,
    id: WorkspaceId,
    dir: tempfile::TempDir,
    _data: tempfile::TempDir,
}

impl Env {
    async fn new(files: &[(&str, &str)]) -> Env {
        let dir = tempfile::tempdir().unwrap();
        for (p, c) in files {
            let f = dir.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(f, c).unwrap();
        }
        let data = tempfile::tempdir().unwrap();
        let app = App::with_data_dir(Store::open_in_memory().unwrap(), data.path().into());
        let id = app
            .workspaces
            .open(dir.path().to_str().unwrap())
            .await
            .unwrap()
            .workspace_id;
        Env {
            app,
            id,
            dir,
            _data: data,
        }
    }

    fn ctx(&self) -> ToolCtx {
        ToolCtx {
            workspace_id: self.id.clone(),
            principal: "test".into(),
            cancel: CancellationToken::new(),
        }
    }

    fn path(&self, p: &str) -> std::path::PathBuf {
        self.dir.path().join(p)
    }

    async fn prepare(
        &self,
        tool: &str,
        input: Value,
    ) -> Result<axiom_agent::tools::Prepared, ToolError> {
        self.app.agent.tools.prepare(&self.ctx(), tool, input).await
    }

    async fn call(&self, tool: &str, input: Value) -> Result<ToolOutput, ToolError> {
        let p = self.prepare(tool, input).await?;
        self.app
            .agent
            .tools
            .execute(&self.ctx(), p, Allowed::for_tests())
            .await
    }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap()
}

#[tokio::test]
async fn tools_registry_lists_every_m3_tool_with_its_effect() {
    let e = Env::new(&[]).await;
    let infos = e.app.agent.tools.infos();
    let got: Vec<(String, Effect)> = infos.iter().map(|t| (t.name.clone(), t.effect)).collect();
    let want = [
        ("build.diagnostics", Effect::Read),
        ("build.run", Effect::Execute),
        ("editor.open", Effect::Read),
        ("env.detect", Effect::Read),
        ("env.install", Effect::Install),
        ("env.planInstall", Effect::Read),
        ("fs.applyPatch", Effect::WriteWorkspace),
        ("fs.list", Effect::Read),
        ("fs.read", Effect::Read),
        ("task.run", Effect::Execute),
        ("workspace.info", Effect::Read),
        ("workspace.search", Effect::Read),
    ];
    assert_eq!(
        got,
        want.iter()
            .map(|(n, e)| (n.to_string(), *e))
            .collect::<Vec<_>>()
    );
    for t in &infos {
        assert_eq!(t.input_schema["type"], "object", "{}", t.name);
        assert!(!t.description.is_empty());
    }
}

#[tokio::test]
async fn tools_info_list_read_and_search() {
    let big: String = (1..=2500).map(|i| format!("line {i}\n")).collect();
    let e = Env::new(&[
        (
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        (
            "src/main.rs",
            "fn main() {\n    helper();\n}\nfn helper() {}\n",
        ),
        ("big.txt", &big),
    ])
    .await;
    let info = e.call("workspace.info", json!({})).await.unwrap();
    assert!(info.output["entries"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x["name"] == "src"));
    assert!(
        info.output["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["provider"] == "rust" && t["task"] == "build"),
        "{}",
        info.output
    );

    let l = e.call("fs.list", json!({"path": "src"})).await.unwrap();
    assert_eq!(l.text, "main.rs\n");

    let r = e
        .call(
            "fs.read",
            json!({"path": "src/main.rs", "startLine": 2, "endLine": 3}),
        )
        .await
        .unwrap();
    assert_eq!(r.text, "    2\t    helper();\n    3\t}\n");
    assert_eq!(r.output["totalLines"], 4);

    let p1 = e.call("fs.read", json!({"path": "big.txt"})).await.unwrap();
    assert!(p1.truncated);
    let next = p1.output["nextStartLine"].as_u64().unwrap();
    assert!(next > 1 && next <= 2001, "{next}");
    let p2 = e
        .call("fs.read", json!({"path": "big.txt", "startLine": next}))
        .await
        .unwrap();
    assert!(p2.text.starts_with(&format!("{next:>5}\tline {next}\n")));

    let s = e
        .call("workspace.search", json!({"query": "helper"}))
        .await
        .unwrap();
    assert_eq!(
        s.text,
        "src/main.rs:2:5:     helper();\nsrc/main.rs:4:4: fn helper() {}\n"
    );
    let none = e
        .call("workspace.search", json!({"query": "zzz"}))
        .await
        .unwrap();
    assert!(none.text.starts_with("No matches"));
    let bad = e
        .call("workspace.search", json!({"query": "(", "regex": true}))
        .await
        .unwrap_err();
    assert_eq!(bad.kind, ErrorKind::InvalidParams);
}

#[tokio::test]
async fn tools_paths_outside_the_workspace_never_reach_the_gate() {
    let e = Env::new(&[("a.txt", "x")]).await;
    for (tool, input) in [
        ("fs.read", json!({"path": "../etc/passwd"})),
        ("fs.applyPatch", json!({"path": "../x", "create": "y"})),
        ("fs.list", json!({"path": "/etc"})),
    ] {
        let r = e.prepare(tool, input).await;
        let err = match r {
            Err(e) => e,
            Ok(p) => e
                .app
                .agent
                .tools
                .execute(&e.ctx(), p, Allowed::for_tests())
                .await
                .unwrap_err(),
        };
        assert!(
            matches!(
                err.kind,
                ErrorKind::InvalidParams | ErrorKind::PermissionDenied
            ),
            "{tool}: {err:?}"
        );
    }
}

#[tokio::test]
async fn tools_apply_patch_edit_create_delete_and_failures() {
    let e = Env::new(&[("src/lib.rs", "fn a() {}\nfn b() {}\nfn b() {}\n")]).await;

    // Zero matches: a hint pointing at the closest region; file unchanged.
    let err = e
        .call(
            "fs.applyPatch",
            json!({"path": "src/lib.rs", "edits": [{"search": "fn c() {}", "replace": ""}]}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Conflict);
    assert!(err.message.contains("not found"));
    // Two matches.
    let err = e
        .call(
            "fs.applyPatch",
            json!({"path": "src/lib.rs", "edits": [{"search": "fn b() {}", "replace": ""}]}),
        )
        .await
        .unwrap_err();
    assert!(err.message.contains("2 times"), "{}", err.message);

    // A good edit: the prepared preview is the diff, and the result carries it.
    let p = e
        .prepare("fs.applyPatch", json!({"path": "src/lib.rs", "edits": [{"search": "fn a() {}", "replace": "pub fn a() {}"}]}))
        .await
        .unwrap();
    assert_eq!(p.effect, Effect::WriteWorkspace);
    let preview = p.targets.preview.clone().unwrap();
    assert!(preview.contains("-fn a() {}\n+pub fn a() {}"), "{preview}");
    let out = e
        .app
        .agent
        .tools
        .execute(&e.ctx(), p, Allowed::for_tests())
        .await
        .unwrap();
    assert_eq!(out.output["diff"], preview);
    assert_eq!(out.summary, "+1 −1");
    assert!(read(&e.path("src/lib.rs")).starts_with("pub fn a() {}\n"));

    // Stale: the file changes between approval (prepare) and execution.
    let p = e
        .prepare(
            "fs.applyPatch",
            json!({"path": "src/lib.rs", "edits": [{"search": "pub fn a", "replace": "fn a"}]}),
        )
        .await
        .unwrap();
    std::fs::write(
        e.path("src/lib.rs"),
        "pub fn a() {}\n// edited by the user\n",
    )
    .unwrap();
    let err = e
        .app
        .agent
        .tools
        .execute(&e.ctx(), p, Allowed::for_tests())
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Conflict);
    assert!(err.message.contains("changed after"));
    assert!(read(&e.path("src/lib.rs")).contains("edited by the user"));

    // Create, and create over an existing file.
    e.call(
        "fs.applyPatch",
        json!({"path": "src/new.rs", "create": "pub fn n() {}\n"}),
    )
    .await
    .unwrap();
    assert_eq!(read(&e.path("src/new.rs")), "pub fn n() {}\n");
    let err = e
        .call(
            "fs.applyPatch",
            json!({"path": "src/new.rs", "create": "x"}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::AlreadyExists);

    // Delete is the `delete` effect.
    let p = e
        .prepare(
            "fs.applyPatch",
            json!({"path": "src/new.rs", "delete": true}),
        )
        .await
        .unwrap();
    assert_eq!(p.effect, Effect::Delete);
    e.app
        .agent
        .tools
        .execute(&e.ctx(), p, Allowed::for_tests())
        .await
        .unwrap();
    assert!(!e.path("src/new.rs").exists());

    // Exactly one mode.
    let err = e
        .call(
            "fs.applyPatch",
            json!({"path": "src/lib.rs", "create": "x", "delete": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
}

#[cfg(unix)]
#[tokio::test]
async fn tools_task_run_exit_code_output_and_timeout() {
    let e = Env::new(&[]).await;
    let p = e
        .prepare(
            "task.run",
            json!({"argv": ["sh", "-c", "echo hi; echo oops >&2; exit 3"]}),
        )
        .await
        .unwrap();
    assert_eq!(p.targets.argv.as_deref().unwrap()[0], "sh");
    let out = e
        .app
        .agent
        .tools
        .execute(&e.ctx(), p, Allowed::for_tests())
        .await
        .unwrap();
    assert_eq!(out.output["exitCode"], 3);
    assert_eq!(out.output["outcome"], "failed");
    assert!(out.text.contains("hi") && out.text.contains("oops"));
    assert!(out.summary.starts_with("exit 3"));

    let t = std::time::Instant::now();
    let out = e
        .call(
            "task.run",
            json!({"argv": ["sleep", "30"], "timeoutSecs": 1}),
        )
        .await
        .unwrap();
    assert_eq!(out.output["timedOut"], true);
    assert!(t.elapsed() < std::time::Duration::from_secs(10));

    let err = e.call("task.run", json!({"argv": []})).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
}

/// A real `cargo build` of a crate with a type error (cargo is on PATH wherever this suite runs:
/// developer machines and every CI Rust job).
#[tokio::test]
async fn tools_build_run_returns_diagnostics_and_build_diagnostics_keeps_them() {
    let e = Env::new(&[
        (
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        ),
        (
            "src/main.rs",
            "fn main() {\n    let x: u32 = \"no\";\n    let _ = x;\n}\n",
        ),
    ])
    .await;
    let p = e
        .prepare("build.run", json!({"task": "build"}))
        .await
        .unwrap();
    assert_eq!(p.targets.argv.as_deref().unwrap()[0], "cargo");
    let out = e
        .app
        .agent
        .tools
        .execute(&e.ctx(), p, Allowed::for_tests())
        .await
        .unwrap();
    assert_eq!(out.output["outcome"], "failed", "{}", out.text);
    assert!(
        out.text
            .contains("src/main.rs:2:18: error [E0308]: mismatched types"),
        "{}",
        out.text
    );
    let d = e.call("build.diagnostics", json!({})).await.unwrap();
    assert!(d.text.contains("E0308"));

    let err = e
        .call("build.run", json!({"task": "nope"}))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(err.hint.unwrap().contains("rust:build"));
}

#[tokio::test]
async fn tools_editor_open_and_env_detect() {
    let e = Env::new(&[("Cargo.toml", "[package]\nname = \"d\"\n")]).await;
    let out = e
        .call("editor.open", json!({"path": "Cargo.toml", "line": 2}))
        .await
        .unwrap();
    assert_eq!(out.summary, "no editor attached");
    let d = e.call("env.detect", json!({})).await.unwrap();
    assert!(d.text.contains("rust"), "{}", d.text);
}
