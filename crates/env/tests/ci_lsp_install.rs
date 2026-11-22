//! M5.5-PROV-001: real language server installs from the internet. CI only, like ci_install.rs:
//! `cargo test -p axiom-env --features ci-install --test ci_lsp_install`.
#![cfg(feature = "ci-install")]

use axiom_env::registry::{Builtin, Registry, Sources};
use axiom_env::{Http, HttpConfig, Installer};
use std::path::Path;
use tokio_util::sync::CancellationToken;

fn registry() -> Registry {
    Registry::load(&Sources {
        builtin: Builtin::Embedded,
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    })
}

/// Installs [provider]'s first version and returns the install root.
async fn install(inst: &Installer, r: &Registry, provider: &str) -> std::path::PathBuf {
    let m = r.get(provider).unwrap().clone();
    let versions = inst.versions(&m).await.unwrap();
    let version = versions
        .available
        .first()
        .unwrap_or_else(|| panic!("{provider}: no versions ({versions:?})"))
        .clone();
    let plan = inst.plan(&m, &version, None).await.unwrap();
    inst.install(&plan.plan_id, Some(&m), &CancellationToken::new(), &|p| {
        if p.bytes_done.is_none() {
            println!("{provider} [{}/{}] {}", p.step + 1, p.steps, p.label);
        }
    })
    .await
    .unwrap();
    inst.toolchains_dir().join(provider).join(version)
}

/// The server at [exe] answers `initialize` on stdio.
fn initializes(exe: &Path, args: &[&str], path_dir: Option<&Path>) {
    use std::io::{BufRead, BufReader, Read, Write};
    let mut cmd = std::process::Command::new(exe);
    cmd.args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    if let Some(d) = path_dir {
        let mut parts = vec![d.to_path_buf()];
        parts.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        cmd.env("PATH", std::env::join_paths(parts).unwrap());
    }
    let mut child = cmd.spawn().unwrap();
    let body = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "processId": null, "rootUri": null, "capabilities": {}}})
    .to_string();
    let mut stdin = child.stdin.take().unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    stdin.flush().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    // Servers may log or ask things before they answer (basedpyright's window/logMessage): the
    // messages are read until the reply to id 1, at most 50.
    let mut reply = serde_json::Value::Null;
    for _ in 0..50 {
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            assert!(
                out.read_line(&mut line).unwrap() > 0,
                "the server closed its output"
            );
            if line.trim().is_empty() {
                break;
            }
            if let Some(v) = line.trim().strip_prefix("Content-Length:") {
                len = v.trim().parse().unwrap();
            }
        }
        let mut buf = vec![0; len];
        out.read_exact(&mut buf).unwrap();
        let msg: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        if msg["id"] == 1 && msg.get("method").is_none() {
            reply = msg;
            break;
        }
    }
    assert!(reply["result"]["capabilities"].is_object(), "{reply}");
    let _ = child.kill();
    let _ = child.wait();
}

#[tokio::test]
async fn language_servers_install_and_answer_initialize() {
    let r = registry();
    let data = tempfile::tempdir().unwrap();
    let inst = Installer::new(
        data.path().to_path_buf(),
        Http::new(HttpConfig::default()).unwrap(),
    );
    // A single executable (`file`), a gzip-compressed one (`gz`).
    let root = install(&inst, &r, "lsp-marksman").await;
    initializes(&root.join("marksman"), &["server"], None);
    let root = install(&inst, &r, "lsp-rust-analyzer").await;
    initializes(&root.join("rust-analyzer"), &[], None);
    // An npm server, where npm is on PATH (CI installs Node).
    if !axiom_env::detect::path_candidates("npm", std::env::var_os("PATH").as_deref()).is_empty() {
        let root = install(&inst, &r, "lsp-basedpyright").await;
        initializes(
            &root.join("node_modules/.bin/basedpyright-langserver"),
            &["--stdio"],
            None,
        );
    }
}

/// Every server provider with an install method plans on this host (or names what it needs).
#[tokio::test]
async fn every_server_provider_plans() {
    let r = registry();
    let data = tempfile::tempdir().unwrap();
    let inst = Installer::new(
        data.path().to_path_buf(),
        Http::new(HttpConfig::default()).unwrap(),
    );
    for (_, m) in r.active() {
        if m.lsp.is_empty() || m.install.is_empty() {
            continue;
        }
        let versions = inst.versions(m).await.unwrap();
        let Some(version) = versions.available.first() else {
            panic!("{}: no versions ({versions:?})", m.id);
        };
        match inst.plan(m, version, None).await {
            Ok(p) => println!("{}: {} steps", m.id, p.steps.len()),
            Err(axiom_env::error::EnvError::PrerequisiteMissing(why, _)) => {
                println!("{}: {why}", m.id)
            }
            Err(axiom_env::error::EnvError::UnsupportedPlatform(why)) => {
                println!("{}: {why}", m.id)
            }
            Err(e) => panic!("{}: {e}", m.id),
        }
    }
}
