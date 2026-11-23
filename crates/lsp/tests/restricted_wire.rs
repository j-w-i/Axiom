//! What a restricted server is actually started with (AUD-008, review N8): the
//! `initialize` request it reads and the environment it runs in, for the
//! typescript-language-server and gopls restrictions (jdtls's settings are covered at the
//! daemon, `crates/axiomd/tests/lsp_server.rs`).
//!
//! The "server" is a shell script that records its environment and the first message it reads,
//! then exits: nothing of a real server is needed to see what Axiom sends.
#![cfg(unix)]

use axiom_lsp::{restrict, Definition, Launch, Manager, Restriction};
use axiom_protocol::WorkspaceId;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Records `env` to `$RECORD/env` and the first LSP message's body to `$RECORD/initialize`.
const RECORDER: &str = r#"#!/bin/sh
env > "$RECORD/env.tmp" && mv "$RECORD/env.tmp" "$RECORD/env"
IFS= read -r header
len=${header#Content-Length: }
len=${len%?}
IFS= read -r blank
head -c "$len" > "$RECORD/initialize.tmp" && mv "$RECORD/initialize.tmp" "$RECORD/initialize"
"#;

fn script(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, RECORDER).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn def(id: &str) -> Definition {
    Definition {
        id: id.into(),
        provider_id: "builtin".into(),
        languages: vec![],
        command: vec![id.into()],
        initialization_options: Some(json!({"fromManifest": true})),
        install_hint: None,
        requires: vec![],
    }
}

fn launch(root: &Path, bin: &Path, record: &Path) -> Launch {
    Launch {
        root: root.to_path_buf(),
        env: BTreeMap::from([
            (
                "PATH".to_string(),
                format!("{}:/usr/bin:/bin", bin.to_string_lossy()),
            ),
            ("RECORD".to_string(), record.to_string_lossy().into_owned()),
        ]),
        settings: json!({}),
        initialization_options: Some(json!({"fromSettings": true})),
        blocked: None,
        install_provider: None,
        restriction: Restriction::None,
    }
}

/// Starts `def` with `launch` and returns the `initialize` params and the environment the
/// process saw.
async fn started_with(
    def: Definition,
    launch: Launch,
    record: &Path,
) -> (Value, BTreeMap<String, String>) {
    std::fs::create_dir_all(record).unwrap();
    let manager = Manager::default();
    let s = manager.server(&WorkspaceId("w".into()), def, launch);
    let cancel = CancellationToken::new();
    let req = {
        let s = s.clone();
        let cancel = cancel.clone();
        tokio::spawn(async move { s.request("x", Value::Null, &cancel).await })
    };
    let (init, env) = (record.join("initialize"), record.join("env"));
    tokio::time::timeout(Duration::from_secs(20), async {
        while !(init.is_file() && env.is_file()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the server did not record what it was started with");
    cancel.cancel();
    let _ = req.await;
    s.close();
    s.stop().await;
    let msg: Value = serde_json::from_slice(&std::fs::read(&init).unwrap()).unwrap();
    assert_eq!(msg["method"], "initialize", "{msg}");
    let env = std::fs::read_to_string(&env)
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    (msg["params"].clone(), env)
}

#[tokio::test]
async fn restricted_gopls_runs_with_the_installed_go_only() {
    let d = tempfile::tempdir().unwrap();
    let (root, bin) = (d.path().join("w"), d.path().join("bin"));
    std::fs::create_dir_all(&root).unwrap();
    script(&bin.join("gopls"));

    let l = restrict(&def("gopls"), launch(&root, &bin, &d.path().join("r1")));
    assert!(matches!(l.restriction, Restriction::Restricted { .. }));
    let (params, env) = started_with(def("gopls"), l, &d.path().join("r1")).await;
    assert_eq!(
        env.get("GOTOOLCHAIN").map(String::as_str),
        Some("local"),
        "{env:?}"
    );
    // The rest is as configured.
    assert_eq!(
        params["initializationOptions"],
        json!({"fromManifest": true, "fromSettings": true})
    );

    // Trusted (no restriction), the environment is the workspace's own.
    let l = launch(&root, &bin, &d.path().join("r2"));
    let (_, env) = started_with(def("gopls"), l, &d.path().join("r2")).await;
    assert_eq!(env.get("GOTOOLCHAIN"), None, "{env:?}");
}

#[tokio::test]
async fn restricted_typescript_server_is_told_its_own_typescript_and_no_plugins() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("w");
    // The folder's own TypeScript, which must not be the one named.
    std::fs::create_dir_all(root.join("node_modules/typescript/lib")).unwrap();
    std::fs::write(root.join("node_modules/typescript/lib/tsserver.js"), "").unwrap();
    // The server as installed: `<prefix>/node_modules/.bin/…` beside its own TypeScript.
    let prefix = d.path().join("prefix");
    let bin = prefix.join("node_modules/.bin");
    script(&bin.join("typescript-language-server"));
    let lib = prefix.join("node_modules/typescript/lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(lib.join("tsserver.js"), "").unwrap();

    let d_ts = def("typescript-language-server");
    let l = restrict(&d_ts, launch(&root, &bin, &d.path().join("r1")));
    assert!(matches!(l.restriction, Restriction::Restricted { .. }));
    let (params, _) = started_with(d_ts.clone(), l, &d.path().join("r1")).await;
    let o = &params["initializationOptions"];
    let own: PathBuf = std::fs::canonicalize(lib.join("tsserver.js")).unwrap();
    assert_eq!(o["tsserver"]["path"], own.to_string_lossy().as_ref(), "{o}");
    assert_eq!(o["plugins"], json!([]), "{o}");
    assert_eq!(o["fromManifest"], true, "{o}");
    assert_eq!(o["fromSettings"], true, "{o}");

    // Trusted: as configured, with no TypeScript named.
    let l = launch(&root, &bin, &d.path().join("r2"));
    let (params, _) = started_with(d_ts, l, &d.path().join("r2")).await;
    assert_eq!(
        params["initializationOptions"],
        json!({"fromManifest": true, "fromSettings": true})
    );
}
