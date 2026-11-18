//! `env/providers`, `env/detect` and `env/trust` against the real daemon (M2-ENV-001).

mod common;
use axiom_protocol::env::{
    Detect, ProviderSource, Providers, ProvidersParams, Resolve, ToolLocation, Trust, TrustParams,
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

// ------------------------------------------------------------ env/resolve (M2-ENV-005)

/// A user provider `demo` with a managed 1.2.3 install (a scripted `demotool`).
fn demo_install(data: &Path) -> PathBuf {
    let p = data.join("providers/demo");
    std::fs::create_dir_all(&p).unwrap();
    std::fs::write(
        p.join("provider.toml"),
        "schema = \"axiom.provider/1\"\nid = \"demo\"\nname = \"Demo\"\ndescription = \"d\"\nversion = \"0.1.0\"\n\
         [project]\nany_file = [\"demo.txt\"]\n\
         [[tools]]\nid = \"demotool\"\nprogram = \"demotool\"\nversion_regex = 'demo-tool (\\d+\\.\\d+\\.\\d+)'\n\
         [[install]]\nkind = \"archive\"\nversions = { kind = \"static\", versions = [\"1.2.3\"] }\nbin_dirs = [\"bin\"]\nenv = { DEMO_HOME = \"${root}\" }\n\
         [[install.artifacts]]\nplatform = { os = \"linux\" }\nurl = \"https://example.invalid/d.tar.gz\"\nformat = \"tar.gz\"\nchecksum = { kind = \"sha256_url\", url = \"https://example.invalid/s\" }\n",
    )
    .unwrap();
    let root = data.join("toolchains").join("demo").join("1.2.3");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(
            bin.join("demotool"),
            "#!/bin/sh\necho \"demo-tool 1.2.3\"\n",
        )
        .unwrap();
        std::fs::set_permissions(bin.join("demotool"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    #[cfg(windows)]
    std::fs::write(bin.join("demotool.bat"), "@echo demo-tool 1.2.3\r\n").unwrap();
    root
}

fn project_ws(toml: &str) -> TempDir {
    let d = TempDir::new().unwrap();
    std::fs::create_dir_all(d.path().join(".axiom")).unwrap();
    std::fs::write(d.path().join(".axiom/project.toml"), toml).unwrap();
    std::fs::write(d.path().join("demo.txt"), "").unwrap();
    d
}

#[tokio::test]
async fn resolve_writes_the_lock_once_and_detect_shows_the_selection() {
    let data = TempDir::new().unwrap();
    let root = demo_install(data.path());
    let d = Daemon::ready_in(data.path()).await;
    let ws = d
        .open_dir(project_ws("[environment]\ndemo = \"1.2.3\"\n"))
        .await;
    let wsref = || WorkspaceRef {
        workspace_id: ws.id.clone(),
    };

    let env = d.client.request::<Resolve>(wsref()).await.unwrap();
    assert_eq!(
        env.path_prepend,
        [root.join("bin").to_string_lossy().into_owned()]
    );
    assert_eq!(env.vars["DEMO_HOME"], root.to_string_lossy());
    assert_eq!(env.toolchains[0].version, "1.2.3");
    let lock_path = ws.root.join(".axiom/project.lock");
    let lock = std::fs::read_to_string(&lock_path).unwrap();
    assert!(
        lock.contains("provider = \"demo\"") && lock.contains("version = \"1.2.3\""),
        "{lock}"
    );
    let first = std::fs::metadata(&lock_path).unwrap().modified().unwrap();

    // Unchanged resolution: the lock is not rewritten.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    d.client.request::<Resolve>(wsref()).await.unwrap();
    assert_eq!(
        std::fs::metadata(&lock_path).unwrap().modified().unwrap(),
        first
    );
    // No temp files left next to it (the write is temp file + rename).
    let names: Vec<_> = std::fs::read_dir(ws.root.join(".axiom"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");

    let det = d.client.request::<Detect>(wsref()).await.unwrap();
    let demo = det
        .providers
        .iter()
        .find(|p| p.provider_id == "demo")
        .unwrap();
    assert_eq!(demo.selected_version.as_deref(), Some("1.2.3"));
    assert_eq!(demo.tools[0].found.as_ref().unwrap().version, "1.2.3");
}

#[tokio::test]
async fn resolve_errors_name_the_line_or_the_missing_tools() {
    let data = TempDir::new().unwrap();
    demo_install(data.path());
    let d = Daemon::ready_in(data.path()).await;

    let ws = d
        .open_dir(project_ws("[environment]\ndemo = \"9.9.9\"\n"))
        .await;
    let e = d
        .client
        .request::<Resolve>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::PrerequisiteMissing);
    assert_eq!(
        e.data.details.unwrap()["missing"],
        serde_json::json!(["demotool"])
    );
    assert!(
        !ws.root.join(".axiom/project.lock").exists(),
        "no lock for a failed resolution"
    );

    let ws = d
        .open_dir(project_ws("[environment]\ndemo = \"1.2.3\"\nextra = [1]\n"))
        .await;
    let e = d
        .client
        .request::<Resolve>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
    assert_eq!(e.data.details.unwrap()["line"], 3);

    // No project file: an empty environment, and nothing written.
    let ws = d.open_dir(TempDir::new().unwrap()).await;
    let env = d
        .client
        .request::<Resolve>(WorkspaceRef {
            workspace_id: ws.id.clone(),
        })
        .await
        .unwrap();
    assert!(env.toolchains.is_empty() && env.path_prepend.is_empty());
    assert!(!ws.root.join(".axiom").exists());
}
