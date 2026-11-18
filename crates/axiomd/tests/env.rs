//! `env/providers`, `env/detect` and `env/trust` against the real daemon (M2-ENV-001).

mod common;
use axiom_protocol::env::{
    Detect, ProviderSource, Providers, ProvidersParams, ToolLocation, Trust, TrustParams,
};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::ErrorKind;
use axiom_protocol::WorkspaceId;
use common::*;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A Cargo workspace: `Cargo.toml` makes the built-in Rust provider apply.
fn cargo_ws() -> TempDir {
    let d = TempDir::new().unwrap();
    std::fs::write(d.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
    d
}

/// The directory holding the `cargo` running these tests (`$CARGO`).
fn cargo_dir() -> PathBuf {
    PathBuf::from(env!("CARGO")).parent().unwrap().to_path_buf()
}

async fn daemon_with_path(path: Option<OsString>) -> (Daemon, TempDir) {
    let data = TempDir::new().unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        match path {
            Some(p) => c.env("PATH", p),
            None => c.env_remove("PATH"),
        };
    })
    .await;
    d.initialize().await;
    (d, data)
}

#[tokio::test]
async fn initialize_lists_the_env_methods_now_served() {
    let d = Daemon::spawn().await;
    let r = d.initialize().await;
    for m in ["env/providers", "env/detect", "env/trust"] {
        assert!(r.methods.iter().any(|x| x == m), "{m}");
    }
}

#[tokio::test]
async fn providers_lists_the_builtin_rust_provider() {
    let d = Daemon::ready().await;
    let r = d
        .client
        .request::<Providers>(ProvidersParams::default())
        .await
        .unwrap();
    let rust = r
        .providers
        .iter()
        .find(|p| p.id == "rust")
        .expect("rust built in");
    assert_eq!(rust.source, ProviderSource::Builtin);
    assert!(rust.trusted && rust.errors.is_empty());
}

#[tokio::test]
async fn detect_finds_cargo_on_path_and_reports_it_missing_without_path() {
    // `$CARGO`'s directory holds cargo and (rustup or toolchain layout) rustc.
    let (d, _data) = daemon_with_path(Some(std::env::join_paths([cargo_dir()]).unwrap())).await;
    let ws = d.open_dir(cargo_ws()).await;
    let r = d
        .client
        .request::<Detect>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap();
    let rust = r
        .providers
        .iter()
        .find(|p| p.provider_id == "rust")
        .expect("rust applies");
    let cargo = rust.tools.iter().find(|t| t.tool_id == "cargo").unwrap();
    let found = cargo.found.as_ref().expect("cargo found on PATH");
    assert!(found.version.split('.').count() == 3, "{found:?}");
    assert!(
        matches!(&found.location, ToolLocation::System { path } if Path::new(path).starts_with(cargo_dir()))
    );

    let (d, _data) = daemon_with_path(None).await;
    let ws = d.open_dir(cargo_ws()).await;
    let r = d
        .client
        .request::<Detect>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap();
    let rust = r
        .providers
        .iter()
        .find(|p| p.provider_id == "rust")
        .unwrap();
    assert!(rust.tools.iter().all(|t| t.found.is_none()), "{rust:?}");
}

#[tokio::test]
async fn detect_on_a_non_rust_folder_reports_no_rust() {
    let d = Daemon::ready().await;
    let ws = d.open_dir(TempDir::new().unwrap()).await;
    let r = d
        .client
        .request::<Detect>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap();
    assert!(r.providers.iter().all(|p| p.provider_id != "rust"));
}

#[tokio::test]
async fn workspace_provider_is_untrusted_until_trusted_and_trust_persists() {
    let data = TempDir::new().unwrap();
    let dir = cargo_ws();
    let pdir = dir.path().join(".axiom/providers/local");
    std::fs::create_dir_all(&pdir).unwrap();
    std::fs::write(
        pdir.join("provider.toml"),
        "schema = \"axiom.provider/1\"\nid = \"local\"\nname = \"Local\"\ndescription = \"d\"\nversion = \"0.1.0\"\n\
         [project]\nany_file = [\"Cargo.toml\"]\n",
    )
    .unwrap();
    let root = dir.path().to_path_buf();
    let d = Daemon::ready_in(data.path()).await;
    let ws = d.open_dir(dir).await;
    let wsref = || WorkspaceRef {
        workspace_id: ws.id.clone(),
    };
    let list = |id: WorkspaceId| ProvidersParams {
        workspace_id: Some(id),
    };

    let r = d
        .client
        .request::<Providers>(list(ws.id.clone()))
        .await
        .unwrap();
    let local = r
        .providers
        .iter()
        .find(|p| p.id == "local")
        .expect("listed");
    assert_eq!(
        (local.source, local.trusted),
        (ProviderSource::Workspace, false)
    );
    let det = d.client.request::<Detect>(wsref()).await.unwrap();
    assert!(
        det.providers.iter().all(|p| p.provider_id != "local"),
        "not applied while untrusted"
    );

    d.client
        .request::<Trust>(TrustParams {
            workspace_id: ws.id.clone(),
            trusted: true,
        })
        .await
        .unwrap();
    let r = d
        .client
        .request::<Providers>(list(ws.id.clone()))
        .await
        .unwrap();
    assert!(
        r.providers
            .iter()
            .find(|p| p.id == "local")
            .unwrap()
            .trusted
    );
    let det = d.client.request::<Detect>(wsref()).await.unwrap();
    assert!(det.providers.iter().any(|p| p.provider_id == "local"));
    d.shutdown().await;

    // Trust is stored per workspace and survives a restart.
    let d = Daemon::ready_in(data.path()).await;
    let r: axiom_protocol::workspace::OpenResult = d
        .client
        .request::<axiom_protocol::workspace::Open>(axiom_protocol::workspace::OpenParams {
            path: root.to_str().unwrap().to_string(),
        })
        .await
        .unwrap();
    let p = d
        .client
        .request::<Providers>(list(r.workspace_id.clone()))
        .await
        .unwrap();
    assert!(
        p.providers
            .iter()
            .find(|p| p.id == "local")
            .unwrap()
            .trusted
    );
    drop(ws);
}

#[tokio::test]
async fn unknown_workspace_is_workspace_not_open() {
    let d = Daemon::ready().await;
    let e = d
        .client
        .request::<Detect>(WorkspaceRef {
            workspace_id: WorkspaceId("nope".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::WorkspaceNotOpen);
    let e = d
        .client
        .request::<Trust>(TrustParams {
            workspace_id: WorkspaceId("nope".into()),
            trusted: true,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::WorkspaceNotOpen);
}
