//! `lsp/*` through the daemon (M4-LSP-001): a user provider declares tests/helpers/fake-lsp; the
//! servers come from every provider (M5.5: by language); requests, sync, diagnostics relayed to the
//! workspace's clients, errors, reads outside the workspace, and closing the workspace. Then the
//! real rust-analyzer, where it is installed (CI installs it).

// Unix only for now: Windows (paths in `file://` URIs, `.cmd` launchers) is deferred with the
// rest of Windows support.
#![cfg(unix)]

mod common;

use axiom_protocol::fs::{ReadFile, ReadFileParams};
use axiom_protocol::lsp::{
    Notify, NotifyParams, Request, RequestParams, ServerState, Servers, ServersResult,
};
use axiom_protocol::workspace::{Close, WorkspaceRef};
use axiom_protocol::{ErrorKind, WorkspacePath};
use common::{kind, Daemon};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fake_lsp() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join(if cfg!(windows) {
        "fake-lsp.exe"
    } else {
        "fake-lsp"
    });
    // Always built: a no-op when up to date, and a binary left from before a change to the
    // helper would not have it.
    assert!(std::process::Command::new(env!("CARGO"))
        .args(["build", "-q", "-p", "fake-lsp"])
        .status()
        .unwrap()
        .success());
    p
}

const MANIFEST: &str = r#"schema = "axiom.provider/1"
id = "fakelang"
name = "Fake language"
description = "Test provider with a language server."
version = "0.1.0"

[project]
any_file = ["fake.marker"]

[[lsp]]
id = "fake"
languages = ["fakelang"]
command = ["fake-lsp"]
install_hint = "Build tests/helpers/fake-lsp."
"#;

fn path_with(dir: &Path) -> std::ffi::OsString {
    let mut parts = vec![dir.to_path_buf()];
    if let Some(p) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&p));
    }
    std::env::join_paths(parts).unwrap()
}

fn uri(p: &Path) -> String {
    axiom_lsp::file_uri(p)
}

async fn diagnostics_for(d: &Daemon, uri: &str) -> Value {
    loop {
        let v = d
            .next_notification("lsp/notification", Duration::from_secs(30))
            .await;
        if v["method"] == "textDocument/publishDiagnostics" && v["params"]["uri"] == uri {
            return v;
        }
    }
}

#[tokio::test]
async fn servers_requests_sync_diagnostics_and_external_reads() {
    let data = tempfile::tempdir().unwrap();
    let pdir = data.path().join("providers").join("fakelang");
    std::fs::create_dir_all(&pdir).unwrap();
    std::fs::write(pdir.join("provider.toml"), MANIFEST).unwrap();
    // A file outside the workspace that the server's definitions point to.
    let outside = tempfile::tempdir().unwrap();
    let ext = dunce::canonicalize(outside.path())
        .unwrap()
        .join("ext.fake");
    std::fs::write(&ext, "external text\n").unwrap();
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "no").unwrap();
    let bin = fake_lsp();
    let log = data.path().join("lsp.log");
    let (ext_uri, log2) = (uri(&ext), log.clone());
    let d = Daemon::spawn_configured(data.path(), move |c| {
        c.env("PATH", path_with(bin.parent().unwrap()))
            .env("FAKE_LSP_DEFINITION", &ext_uri)
            .env("FAKE_LSP_LOG", &log2);
    })
    .await;
    d.initialize().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fake.marker"), "").unwrap();
    std::fs::write(dir.path().join("a.fake"), "fine\nERROR here\n").unwrap();
    let f = d.open_dir(dir).await;
    let ws = f.id.clone();

    // The server of the provider that applies, idle until used.
    let s: ServersResult = d
        .client
        .request::<Servers>(WorkspaceRef {
            workspace_id: ws.clone(),
        })
        .await
        .unwrap();
    let fake = s
        .servers
        .iter()
        .find(|s| s.server_id == "fake")
        .expect("listed");
    assert_eq!(
        (fake.provider_id.as_str(), fake.state),
        ("fakelang", ServerState::Idle)
    );
    // Since M5.5 (ADR-0021 §3) every server applies by language: rust-analyzer is listed in
    // this non-Rust folder, and stays unstarted; in this untrusted folder it is `stopped` until
    // the folder is trusted, because it would run the project's build scripts (AUD-008).
    if let Some(ra) = s.servers.iter().find(|s| s.server_id == "rust-analyzer") {
        assert_eq!(ra.state, ServerState::Stopped, "{ra:?}");
        assert!(ra
            .detail
            .as_deref()
            .unwrap_or_default()
            .starts_with(axiom_lsp::TRUST_TO_ENABLE));
    }

    // didOpen starts it; its diagnostics reach this client.
    let a = uri(&f.root.join("a.fake"));
    d.client
        .request::<Notify>(NotifyParams {
            workspace_id: ws.clone(),
            server_id: "fake".into(),
            method: "textDocument/didOpen".into(),
            params: json!({"textDocument":{"uri":a,"languageId":"fakelang","version":1,"text":"fine\nERROR here\n"}}),
        })
        .await
        .unwrap();
    let diag = diagnostics_for(&d, &a).await;
    assert_eq!(diag["serverId"], "fake");
    assert_eq!(
        diag["params"]["diagnostics"][0]["range"]["start"]["line"],
        1
    );

    let req = |method: &str, params: Value| RequestParams {
        workspace_id: ws.clone(),
        server_id: "fake".into(),
        method: method.into(),
        params,
    };
    let c = d
        .client
        .request::<Request>(req("textDocument/completion", json!({})))
        .await
        .unwrap();
    assert_eq!(c.result[1]["label"], "beta");

    // The server error keeps its code.
    let e = d
        .client
        .request::<Request>(req("fake/fail", json!({})))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::LspError);
    assert_eq!(e.data.details.as_ref().unwrap()["code"], -32001);

    // Before the server pointed at it, the outside file cannot be read; after, it can (read-only),
    // and nothing else outside can.
    let read = |p: String| ReadFileParams {
        workspace_id: ws.clone(),
        path: WorkspacePath(p),
    };
    let e = d
        .client
        .request::<ReadFile>(read(uri(&ext)))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::PermissionDenied);
    let def = d
        .client
        .request::<Request>(req(
            "textDocument/definition",
            json!({"textDocument":{"uri":a},"position":{"line":0,"character":0}}),
        ))
        .await
        .unwrap();
    assert_eq!(def.result[0]["uri"], uri(&ext));
    let text = d.client.request::<ReadFile>(read(uri(&ext))).await.unwrap();
    assert_eq!(text.text, "external text\n");
    let e = d
        .client
        .request::<ReadFile>(read(uri(&secret)))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::PermissionDenied);

    // Unknown server.
    let mut bad = req("x", json!({}));
    bad.server_id = "nope".into();
    let e = d.client.request::<Request>(bad).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);

    // Closing the workspace shuts the server down.
    d.client
        .request::<Close>(WorkspaceRef {
            workspace_id: ws.clone(),
        })
        .await
        .unwrap();
    let got = std::fs::read_to_string(&log).unwrap();
    assert!(got.contains("shutdown\nexit\n"), "{got}");
    d.shutdown().await;
}

#[tokio::test]
async fn a_missing_server_is_reported_with_its_hint() {
    let data = tempfile::tempdir().unwrap();
    let pdir = data.path().join("providers").join("fakelang");
    std::fs::create_dir_all(&pdir).unwrap();
    std::fs::write(
        pdir.join("provider.toml"),
        MANIFEST.replace("\"fake-lsp\"", "\"fake-lsp-not-installed\""),
    )
    .unwrap();
    let d = Daemon::ready_in(data.path()).await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("fake.marker"), "").unwrap();
    let f = d.open_dir(dir).await;
    let e = d
        .client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "textDocument/hover".into(),
            params: json!({}),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::ServerUnavailable);
    assert_eq!(e.data.details.as_ref().unwrap()["state"], "missing");
    let s: ServersResult = d
        .client
        .request::<Servers>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    // Built-in providers' servers are listed too since M5.5 (every server applies by language).
    let fake = s.servers.iter().find(|x| x.server_id == "fake").unwrap();
    assert_eq!(fake.state, ServerState::Missing);
    assert_eq!(
        fake.install_hint.as_deref(),
        Some("Build tests/helpers/fake-lsp.")
    );
    assert_eq!(fake.install_provider.as_deref(), Some("fakelang"));
}

/// Whether the real rust-analyzer runs here. With `AXIOM_REQUIRE_LSP=1` (CI) its absence fails.
fn rust_analyzer_available() -> bool {
    let ok = std::process::Command::new("rust-analyzer")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        assert!(
            std::env::var("AXIOM_REQUIRE_LSP").as_deref() != Ok("1"),
            "AXIOM_REQUIRE_LSP=1 but rust-analyzer does not run"
        );
        eprintln!("NOT RUN: rust-analyzer is not installed (rustup component add rust-analyzer)");
        common::note_skipped(
            "real rust-analyzer",
            "rust-analyzer is not installed (rustup component add rust-analyzer)",
        );
    }
    ok
}

#[tokio::test]
async fn real_rust_analyzer_definition_and_diagnostics() {
    if !rust_analyzer_available() {
        return;
    }
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname = \"ra_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n").unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let main = "mod util;\n\nfn main() {\n    let n: i32 = util::helper();\n    let s: i32 = \"text\";\n    println!(\"{n} {s}\");\n}\n";
    std::fs::write(root.join("src/main.rs"), main).unwrap();
    std::fs::write(
        root.join("src/util.rs"),
        "pub fn helper() -> i32 {\n    42\n}\n",
    )
    .unwrap();
    let f = d.open_dir(dir).await;
    let s: ServersResult = d
        .client
        .request::<Servers>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(
        s.servers.iter().any(|s| s.server_id == "rust-analyzer"),
        "{s:?}"
    );
    let main_uri = uri(&f.root.join("src/main.rs"));
    d.client
        .request::<Notify>(NotifyParams {
            workspace_id: f.id.clone(),
            server_id: "rust-analyzer".into(),
            method: "textDocument/didOpen".into(),
            params: json!({"textDocument":{"uri":main_uri,"languageId":"rust","version":1,"text":main}}),
        })
        .await
        .unwrap();
    // Go to the definition of `helper` (line 3, inside "helper"), in the other file. Retried
    // while rust-analyzer is still loading the project (it answers null or an empty list then).
    let mut found = Value::Null;
    for _ in 0..60 {
        let r = match d
            .client
            .request::<Request>(RequestParams {
                workspace_id: f.id.clone(),
                server_id: "rust-analyzer".into(),
                method: "textDocument/definition".into(),
                params: json!({"textDocument":{"uri":main_uri},"position":{"line":3,"character":25}}),
            })
            .await
        {
            Ok(r) => r,
            // ContentModified (-32801) while it loads the project: clients ask again.
            Err(e) if e.data.details.as_ref().is_some_and(|d| d["code"] == -32801) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
            Err(e) => panic!("{e:?}"),
        };
        let first = if r.result.is_array() {
            r.result[0].clone()
        } else {
            r.result.clone()
        };
        if !first.is_null() {
            found = first;
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let target = found
        .get("targetUri")
        .or_else(|| found.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(target.ends_with("/src/util.rs"), "{found}");
    // The type error is a diagnostic of main.rs (native, or from cargo check).
    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        assert!(
            tokio::time::Instant::now() < deadline,
            "no type-error diagnostic for main.rs"
        );
        let v = d
            .next_notification("lsp/notification", Duration::from_secs(120))
            .await;
        if v["method"] == "textDocument/publishDiagnostics"
            && v["params"]["uri"] == main_uri
            && v["params"]["diagnostics"]
                .as_array()
                .is_some_and(|a| a.iter().any(|x| x["range"]["start"]["line"] == 4))
        {
            break;
        }
    }
}
