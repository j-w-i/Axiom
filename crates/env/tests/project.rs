//! Project environments (M2-ENV-005): resolution of `.axiom/project.toml` against managed and
//! system toolchains, the lock, and processes started inside the environment.

use axiom_env::project::{parse, process_env, resolve, Lock};
use axiom_env::registry::{Builtin, Registry, Sources};
use axiom_env::EnvError;
use axiom_protocol::env::ToolLocation;
use std::path::{Path, PathBuf};

const MANIFEST: &str = r#"schema = "axiom.provider/1"
id = "demo"
name = "Demo"
description = "d"
version = "0.1.0"

[[tools]]
id = "demotool"
program = "demotool"
version_regex = 'demo-tool (\d+\.\d+\.\d+)'

[[install]]
kind = "archive"
versions = { kind = "static", versions = ["1.2.3"] }
bin_dirs = ["bin"]
env = { DEMO_HOME = "${root}/home", DEMO_VERSION = "${version}" }

[[install.artifacts]]
platform = { os = "linux" }
url = "https://example.invalid/demo-${version}.tar.gz"
format = "tar.gz"
checksum = { kind = "sha256_url", url = "https://example.invalid/sums" }
"#;

fn fake_tool(dir: &Path, version: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("demotool");
        std::fs::write(&p, format!("#!/bin/sh\necho \"demo-tool {version}\"\n")).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }
    #[cfg(windows)]
    {
        let p = dir.join("demotool.bat");
        std::fs::write(&p, format!("@echo demo-tool {version}\r\n")).unwrap();
        p
    }
}

struct Fixture {
    _t: tempfile::TempDir,
    registry: Registry,
    toolchains: PathBuf,
    system_bin: PathBuf,
}

fn fixture() -> Fixture {
    let t = tempfile::tempdir().unwrap();
    let b = t.path().join("providers/demo");
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(b.join("provider.toml"), MANIFEST).unwrap();
    let registry = Registry::load(&Sources {
        builtin: Builtin::Dir(t.path().join("providers")),
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    });
    let toolchains = t.path().join("toolchains");
    let root = toolchains.join("demo").join("1.2.3");
    fake_tool(&root.join("bin"), "1.2.3");
    std::fs::write(
        root.join(".axiom-install.toml"),
        "provider = \"demo\"\nversion = \"1.2.3\"\n[[downloads]]\nurl = \"https://example.invalid/demo-1.2.3.tar.gz\"\nsha256 = \"abababababababababababababababababababababababababababababababab\"\n",
    )
    .unwrap();
    let system_bin = t.path().join("sysbin");
    fake_tool(&system_bin, "9.9.9");
    Fixture {
        _t: t,
        registry,
        toolchains,
        system_bin,
    }
}

#[tokio::test]
async fn resolve_managed() {
    let f = fixture();
    let p = parse("[environment]\ndemo = \"1.2.3\"\n").unwrap();
    let (env, lock) = resolve(&f.registry, &p, &f.toolchains, None).await.unwrap();
    let root = f.toolchains.join("demo").join("1.2.3");
    assert_eq!(
        env.path_prepend,
        [root.join("bin").to_string_lossy().into_owned()]
    );
    assert_eq!(
        env.vars["DEMO_HOME"],
        format!("{}/home", root.to_string_lossy())
    );
    assert_eq!(env.vars["DEMO_VERSION"], "1.2.3");
    assert_eq!(env.toolchains.len(), 1);
    assert_eq!(env.toolchains[0].version, "1.2.3");
    assert!(
        matches!(&env.toolchains[0].location, ToolLocation::Managed { root: r } if Path::new(r) == root)
    );
    // The lock: exact versions and the install's download digests, no host paths.
    assert_eq!(lock.toolchains.len(), 1);
    assert_eq!(lock.toolchains[0].downloads[0].sha256, "ab".repeat(32));
    let text = lock.to_text();
    assert!(!text.contains(&*f.toolchains.to_string_lossy()), "{text}");
    assert_eq!(toml::from_str::<Lock>(&text).unwrap(), lock);

    // A process started in the environment finds the managed demotool before the system one.
    let inherited = std::env::join_paths([&f.system_bin]).unwrap();
    let vars = process_env(&env, Some(inherited));
    let program = if cfg!(windows) {
        "demotool.bat"
    } else {
        "demotool"
    };
    let out = std::process::Command::new(program)
        .env_clear()
        .envs(&vars)
        .envs(std::env::var_os("SYSTEMROOT").map(|v| ("SYSTEMROOT", v)))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "demo-tool 1.2.3"
    );
    // Without the environment, the system one runs.
    let out = std::process::Command::new(program)
        .env_clear()
        .env("PATH", std::env::join_paths([&f.system_bin]).unwrap())
        .envs(std::env::var_os("SYSTEMROOT").map(|v| ("SYSTEMROOT", v)))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "demo-tool 9.9.9"
    );
}

#[tokio::test]
async fn system_selection_uses_path() {
    let f = fixture();
    let p = parse("[environment]\ndemo = \"system\"\n").unwrap();
    let path = std::env::join_paths([&f.system_bin]).unwrap();
    let (env, lock) = resolve(&f.registry, &p, &f.toolchains, Some(&path))
        .await
        .unwrap();
    assert!(env.path_prepend.is_empty() && env.vars.is_empty());
    assert_eq!(env.toolchains[0].version, "9.9.9");
    assert_eq!(lock.toolchains[0].location, "system");
    assert!(lock.toolchains[0].downloads.is_empty());
}

async fn missing(f: &Fixture, text: &str, path: Option<&std::ffi::OsStr>) -> (String, Vec<String>) {
    let p = parse(text).unwrap();
    match resolve(&f.registry, &p, &f.toolchains, path).await {
        Err(EnvError::PrerequisiteMissing(msg, m)) => (msg, m),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn missing_is_prerequisite() {
    let f = fixture();
    let (msg, m) = missing(&f, "[environment]\ndemo = \"2.0.0\"\n", None).await;
    assert_eq!(m, ["demotool"]);
    assert!(msg.contains("demotool"), "{msg}");
    let (_, m) = missing(&f, "[environment]\ndemo = \"system\"\n", None).await;
    assert_eq!(m, ["demotool"]);
    let (_, m) = missing(
        &f,
        "[environment]\nnosuch = \"1.0.0\"\ndemo = \"2.0.0\"\n",
        None,
    )
    .await;
    assert_eq!(
        m,
        ["demotool", "nosuch"],
        "all missing pieces at once, in key order"
    );
    let rpc: axiom_protocol::RpcError =
        EnvError::PrerequisiteMissing("x".into(), vec!["demotool".into()]).into();
    assert_eq!(rpc.data.details.unwrap()["missing"][0], "demotool");
}

#[test]
fn project_errors_map_to_invalid_params_with_line() {
    let e = parse("[environment]\nrust = 3\n").unwrap_err();
    let rpc: axiom_protocol::RpcError = e.into();
    assert_eq!(rpc.data.kind, axiom_protocol::ErrorKind::InvalidParams);
    assert_eq!(rpc.data.details.as_ref().unwrap()["line"], 2);
    assert!(rpc.message.starts_with(".axiom/project.toml:"));
}
