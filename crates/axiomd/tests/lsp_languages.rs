//! M5.5-LSP-001 (ADR-0021 §3, §4, §6): a server applies by language, not by project; a server
//! that is not on the workspace's PATH runs from its provider's managed install; `requires`;
//! a second server for a claimed language is on standby.

#![cfg(unix)]

mod common;

use axiom_protocol::lsp::{Request, RequestParams, ServerState, Servers, ServersResult};
use axiom_protocol::workspace::WorkspaceRef;
use common::Daemon;
use serde_json::json;
use std::path::{Path, PathBuf};

fn fake_lsp() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join("fake-lsp");
    if !p.exists() {
        assert!(std::process::Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "fake-lsp"])
            .status()
            .unwrap()
            .success());
    }
    p
}

/// A server-only provider: no `[project]`, an install method whose `bin/` holds the server.
fn server_provider(id: &str, server: &str, command: &str, extra: &str) -> String {
    format!(
        r#"schema = "axiom.provider/1"
id = "{id}"
name = "{id} server"
description = "Test language server provider."
version = "0.1.0"

[[tools]]
id = "{server}"
program = "{command}"
version_regex = '(\d+\.\d+\.\d+)'

[[install]]
kind = "archive"
versions = {{ kind = "static", versions = ["1.0.0"] }}
bin_dirs = ["bin"]
[[install.artifacts]]
platform = {{ os = "linux" }}
url = "https://example.invalid/{server}.tar.gz"
format = "tar.gz"
checksum = {{ kind = "sha256", values = {{ "1.0.0" = "{zero}" }} }}
[[install.artifacts]]
platform = {{ os = "macos" }}
url = "https://example.invalid/{server}.tar.gz"
format = "tar.gz"
checksum = {{ kind = "sha256", values = {{ "1.0.0" = "{zero}" }} }}

[[lsp]]
id = "{server}"
languages = ["fakelang"]
command = ["{command}"]
{extra}
"#,
        zero = "0".repeat(64)
    )
}

fn write_provider(data: &Path, id: &str, text: &str) {
    let dir = data.join("providers").join(id);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("provider.toml"), text).unwrap();
}

async fn servers(d: &Daemon, ws: &axiom_protocol::WorkspaceId) -> ServersResult {
    d.client
        .request::<Servers>(WorkspaceRef {
            workspace_id: ws.clone(),
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn a_server_only_provider_applies_to_any_workspace() {
    let data = tempfile::tempdir().unwrap();
    write_provider(
        data.path(),
        "lsp-fake",
        &server_provider("lsp-fake", "fake", "fake-lsp-not-installed", ""),
    );
    let d = Daemon::ready_in(data.path()).await;
    // An empty folder: nothing for the provider to detect.
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let s = servers(&d, &f.id).await;
    let fake = s.servers.iter().find(|x| x.server_id == "fake").unwrap();
    assert_eq!(fake.provider_id, "lsp-fake");
    assert_eq!(fake.languages, vec!["fakelang".to_string()]);
}

#[tokio::test]
async fn a_server_runs_from_its_managed_install() {
    let data = tempfile::tempdir().unwrap();
    write_provider(
        data.path(),
        "lsp-fake",
        &server_provider("lsp-fake", "fake", "fake-lsp", ""),
    );
    // The install a plan would have published: <data>/toolchains/lsp-fake/1.0.0/bin/fake-lsp.
    let bin = data.path().join("toolchains/lsp-fake/1.0.0/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(fake_lsp(), bin.join("fake-lsp")).unwrap();
    // The daemon's PATH does not have it.
    let d = Daemon::ready_in(data.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let r = d
        .client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "textDocument/hover".into(),
            params: json!({"textDocument": {"uri": "file:///x.fake"}, "position": {"line": 0, "character": 0}}),
        })
        .await
        .unwrap();
    let _ = r;
    let s = servers(&d, &f.id).await;
    let fake = s.servers.iter().find(|x| x.server_id == "fake").unwrap();
    assert_eq!(fake.state, ServerState::Ready, "{:?}", fake.detail);
}

#[tokio::test]
async fn a_missing_server_names_the_provider_to_install() {
    let data = tempfile::tempdir().unwrap();
    write_provider(
        data.path(),
        "lsp-fake",
        &server_provider("lsp-fake", "fake", "fake-lsp-not-installed", ""),
    );
    let d = Daemon::ready_in(data.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let _ = d
        .client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "textDocument/hover".into(),
            params: json!({}),
        })
        .await;
    let s = servers(&d, &f.id).await;
    let fake = s.servers.iter().find(|x| x.server_id == "fake").unwrap();
    assert_eq!(fake.state, ServerState::Missing);
    assert_eq!(fake.install_provider.as_deref(), Some("lsp-fake"));
}

/// A stopped server's children go with it (its process group is killed), and a closed
/// workspace's server stays stopped (audit finding 4).
#[tokio::test]
async fn closing_the_workspace_stops_the_server_and_its_children() {
    let data = tempfile::tempdir().unwrap();
    let pid_file = data.path().join("child.pid");
    let provider = server_provider("lsp-fake", "fake", "fake-lsp", "").replace(
        "command = [\"fake-lsp\"]",
        &format!(
            "command = [\"fake-lsp\", \"--child-pid-file\", \"{}\"]",
            pid_file.display()
        ),
    );
    write_provider(data.path(), "lsp-fake", &provider);
    let bin = data.path().join("toolchains/lsp-fake/1.0.0/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(fake_lsp(), bin.join("fake-lsp")).unwrap();
    let d = Daemon::ready_in(data.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    d.client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "textDocument/hover".into(),
            params: json!({"textDocument": {"uri": "file:///x.fake"}, "position": {"line": 0, "character": 0}}),
        })
        .await
        .unwrap();
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let alive = |pid: &str| {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid.trim()])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&out.stdout).trim().to_string();
        !stat.is_empty() && !stat.starts_with('Z')
    };
    assert!(alive(&pid), "the server's child runs");
    d.client
        .request::<axiom_protocol::workspace::Close>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while alive(&pid) && std::time::Instant::now() < end {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(!alive(&pid), "the server's child was left running");
}

/// A toolchain manager's stub for a component that is not installed (rustup's proxy) exits at
/// once and says so: the server is missing, stays missing (its exit is not a crash that restarts
/// it into "failed"), and names the provider to install (audit finding 3).
#[tokio::test]
async fn a_toolchain_stub_stays_missing() {
    let data = tempfile::tempdir().unwrap();
    write_provider(
        data.path(),
        "lsp-fake",
        &server_provider("lsp-fake", "fake", "fake-stub", ""),
    );
    let bin = data.path().join("toolchains/lsp-fake/1.0.0/bin");
    std::fs::create_dir_all(&bin).unwrap();
    let stub = bin.join("fake-stub");
    std::fs::write(
        &stub,
        "#!/bin/sh\necho \"error: Unknown binary 'fake-stub' in official toolchain\" >&2\nexit 1\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let d = Daemon::ready_in(data.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let _ = d
        .client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "textDocument/hover".into(),
            params: json!({}),
        })
        .await;
    // Long enough for a restart (and two more) had the exit been taken for a crash.
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let s = servers(&d, &f.id).await;
    let fake = s.servers.iter().find(|x| x.server_id == "fake").unwrap();
    assert_eq!(fake.state, ServerState::Missing, "{:?}", fake.detail);
    assert_eq!(fake.install_provider.as_deref(), Some("lsp-fake"));
}

#[tokio::test]
async fn a_required_tool_that_does_not_resolve_names_its_provider() {
    let data = tempfile::tempdir().unwrap();
    write_provider(
        data.path(),
        "lsp-fake",
        &server_provider("lsp-fake", "fake", "fake-lsp", "requires = [\"fakesdk\"]"),
    );
    // The SDK's provider: its tool is nowhere.
    write_provider(
        data.path(),
        "fake-sdk",
        r#"schema = "axiom.provider/1"
id = "fake-sdk"
name = "Fake SDK"
description = "The SDK the fake server needs."
version = "0.1.0"

[[tools]]
id = "fakesdk"
program = "fakesdk-not-installed"
version_regex = '(\d+\.\d+\.\d+)'
"#,
    );
    let bin = data.path().join("toolchains/lsp-fake/1.0.0/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(fake_lsp(), bin.join("fake-lsp")).unwrap();
    let d = Daemon::ready_in(data.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let _ = d
        .client
        .request::<Request>(RequestParams {
            workspace_id: f.id.clone(),
            server_id: "fake".into(),
            method: "textDocument/hover".into(),
            params: json!({}),
        })
        .await;
    let s = servers(&d, &f.id).await;
    let fake = s.servers.iter().find(|x| x.server_id == "fake").unwrap();
    assert_eq!(fake.state, ServerState::Missing);
    assert_eq!(fake.install_provider.as_deref(), Some("fake-sdk"));
    assert!(
        fake.detail
            .as_deref()
            .unwrap_or_default()
            .contains("Fake SDK"),
        "{:?}",
        fake.detail
    );
}

#[tokio::test]
async fn a_second_server_for_a_language_is_on_standby() {
    let data = tempfile::tempdir().unwrap();
    write_provider(
        data.path(),
        "lsp-a",
        &server_provider("lsp-a", "fake-a", "fake-lsp", ""),
    );
    write_provider(
        data.path(),
        "lsp-b",
        &server_provider("lsp-b", "fake-b", "fake-lsp", ""),
    );
    let d = Daemon::ready_in(data.path()).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let s = servers(&d, &f.id).await;
    let a = s.servers.iter().find(|x| x.server_id == "fake-a").unwrap();
    let b = s.servers.iter().find(|x| x.server_id == "fake-b").unwrap();
    assert_ne!(a.state, ServerState::Standby);
    assert_eq!(b.state, ServerState::Standby);
}
