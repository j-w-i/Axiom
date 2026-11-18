//! Registry and detection against real directories and real (scripted) tools.

use axiom_env::detect::{detect, detect_tool};
use axiom_env::registry::{Builtin, Registry, Sources};
use axiom_env::HostPaths;
use axiom_protocol::env::{ProviderSource, ToolLocation};
use std::path::{Path, PathBuf};

fn manifest(id: &str, name: &str, program: &str) -> String {
    format!(
        r#"schema = "axiom.provider/1"
id = "{id}"
name = "{name}"
description = "test provider"
version = "0.1.0"

[project]
any_file = ["demo.toml", "*.demoproj"]

[[tools]]
id = "tool"
program = "{program}"
version_regex = 'demo-tool (\d+\.\d+\.\d+)'

[[tools]]
id = "extra"
program = "{program}-extra-missing"
version_regex = '(\d+)'
required = false

[[install]]
kind = "archive"
versions = {{ kind = "static", versions = ["1.0.0"] }}
bin_dirs = ["bin"]

[[install.artifacts]]
platform = {{ os = "linux" }}
url = "https://example.invalid/demo-${{version}}.tar.gz"
format = "tar.gz"
checksum = {{ kind = "sha256_url", url = "https://example.invalid/demo-${{version}}.sha256" }}
"#
    )
}

fn put(dir: &Path, sub: &str, text: &str) {
    let d = dir.join(sub);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("provider.toml"), text).unwrap();
}

/// An executable that prints `demo-tool <version>`, run directly (shebang / batch file).
fn fake_tool(dir: &Path, program: &str, version: &str) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(program);
        std::fs::write(
            &p,
            format!("#!/bin/sh\necho \"demo-tool {version} (test)\"\n"),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }
    #[cfg(windows)]
    {
        let p = dir.join(format!("{program}.bat"));
        std::fs::write(&p, format!("@echo demo-tool {version} (test)\r\n")).unwrap();
        p
    }
}

fn sources(builtin: &Path, user: &Path, ws: Option<&Path>, trusted: bool) -> Sources {
    Sources {
        builtin: Builtin::Dir(builtin.to_path_buf()),
        user_dir: Some(user.to_path_buf()),
        workspace_dir: ws.map(Path::to_path_buf),
        workspace_trusted: trusted,
    }
}

#[test]
fn override_order() {
    let t = tempfile::tempdir().unwrap();
    let (b, u, w) = (t.path().join("b"), t.path().join("u"), t.path().join("w"));
    put(&b, "demo", &manifest("demo", "Built-in demo", "demo"));
    put(&b, "other", &manifest("other", "Other", "other"));
    put(&u, "demo", &manifest("demo", "User demo", "demo"));
    put(&w, "demo", &manifest("demo", "Workspace demo", "demo"));

    let r = Registry::load(&sources(&b, &u, Some(&w), true));
    assert_eq!(r.entries().len(), 4);
    assert_eq!(r.get("demo").unwrap().name, "Workspace demo");
    assert_eq!(r.get("other").unwrap().name, "Other");
    let active: Vec<_> = r
        .active()
        .map(|(e, _)| (e.info.id.clone(), e.info.source))
        .collect();
    assert_eq!(
        active,
        [
            ("other".to_string(), ProviderSource::Builtin),
            ("demo".to_string(), ProviderSource::Workspace)
        ]
    );

    // Without a workspace, the user provider wins over the built-in one.
    let r = Registry::load(&sources(&b, &u, None, false));
    assert_eq!(r.get("demo").unwrap().name, "User demo");
}

#[test]
fn invalid_manifest_reported() {
    let t = tempfile::tempdir().unwrap();
    let (b, u) = (t.path().join("b"), t.path().join("u"));
    put(&b, "demo", &manifest("demo", "Demo", "demo"));
    put(
        &u,
        "broken",
        "schema = \"axiom.provider/1\"\nid = \"broken\"\nsurprise = 1\n",
    );
    put(
        &u,
        "badregex",
        &manifest("badregex", "Bad", "x").replace(r"(\d+\.\d+\.\d+)", r"\d+"),
    );
    // A user manifest that fails validation does not override a valid built-in one.
    put(
        &u,
        "demo",
        &manifest("demo", "Demo", "demo").replace("0.1.0\"", "0.1.0\"\nhosts = 3"),
    );

    let r = Registry::load(&sources(&b, &u, None, false));
    let infos = r.infos();
    let bad: Vec<_> = infos.iter().filter(|i| !i.errors.is_empty()).collect();
    assert_eq!(bad.len(), 3, "{infos:#?}");
    let broken = infos.iter().find(|i| i.id == "broken").unwrap();
    assert_eq!(broken.source, ProviderSource::User);
    assert!(broken.errors[0].contains("surprise"), "{:?}", broken.errors);
    let badregex = infos.iter().find(|i| i.id == "badregex").unwrap();
    assert!(badregex.errors[0].contains("exactly one capture group"));
    assert_eq!(r.get("demo").unwrap().name, "Demo");
    assert!(r.get("broken").is_none() && r.get("badregex").is_none());
}

#[test]
fn untrusted_workspace_provider() {
    let t = tempfile::tempdir().unwrap();
    let (b, u, w) = (t.path().join("b"), t.path().join("u"), t.path().join("w"));
    std::fs::create_dir_all(&b).unwrap();
    put(
        &w,
        "wsonly",
        &manifest("wsonly", "Workspace only", "wsonly"),
    );

    let r = Registry::load(&sources(&b, &u, Some(&w), false));
    let info = &r.infos()[0];
    assert_eq!(
        (info.id.as_str(), info.source, info.trusted),
        ("wsonly", ProviderSource::Workspace, false)
    );
    assert!(info.errors.is_empty());
    assert!(r.get("wsonly").is_none(), "listed but not loaded");
    assert_eq!(r.active().count(), 0);

    let r = Registry::load(&sources(&b, &u, Some(&w), true));
    assert!(r.infos()[0].trusted);
    assert!(r.get("wsonly").is_some());
}

#[tokio::test]
async fn detect_found_and_missing() {
    let t = tempfile::tempdir().unwrap();
    let (b, u) = (t.path().join("b"), t.path().join("u"));
    put(&b, "demo", &manifest("demo", "Demo", "demotool"));
    let r = Registry::load(&sources(&b, &u, None, false));

    let ws = t.path().join("ws");
    std::fs::create_dir_all(ws.join("sub/app.demoproj")).unwrap();
    let other_ws = t.path().join("other");
    std::fs::create_dir_all(&other_ws).unwrap();
    std::fs::write(other_ws.join("README"), "").unwrap();

    let sys = t.path().join("sysbin");
    let tool = fake_tool(&sys, "demotool", "2.0.1");
    let toolchains = t.path().join("toolchains");
    let host = HostPaths {
        toolchains_dir: toolchains.clone(),
        path: Some(std::env::join_paths([&sys]).unwrap()),
    };

    // A first-level directory matching `*.demoproj` makes the provider apply.
    let d = detect(&r, &ws, &host).await;
    assert_eq!(d.providers.len(), 1);
    let p = &d.providers[0];
    assert_eq!(p.provider_id, "demo");
    let found = p.tools[0].found.as_ref().expect("tool on PATH found");
    assert_eq!(found.version, "2.0.1");
    assert_eq!(
        found.location,
        ToolLocation::System {
            path: tool.to_string_lossy().into_owned()
        }
    );
    assert!(!p.tools[1].required && p.tools[1].found.is_none());

    // A workspace without a marker file: the provider does not apply.
    assert!(detect(&r, &other_ws, &host).await.providers.is_empty());

    // PATH emptied: missing.
    let empty = HostPaths {
        toolchains_dir: toolchains.clone(),
        path: None,
    };
    let d = detect(&r, &ws, &empty).await;
    assert!(d.providers[0].tools[0].found.is_none());

    // A managed install is preferred to PATH, newest version first; staging dirs are ignored.
    fake_tool(&toolchains.join("demo/1.9.0/bin"), "demotool", "1.9.0");
    fake_tool(&toolchains.join("demo/1.10.0/bin"), "demotool", "1.10.0");
    fake_tool(
        &toolchains.join("demo/.staging-2.0.0/bin"),
        "demotool",
        "9.9.9",
    );
    let m = r.get("demo").unwrap();
    let found = detect_tool(m, &m.tools[0], &host).await.unwrap();
    assert_eq!(found.version, "1.10.0");
    assert_eq!(
        found.location,
        ToolLocation::Managed {
            root: toolchains
                .join("demo")
                .join("1.10.0")
                .to_string_lossy()
                .into_owned()
        }
    );
}

#[cfg(unix)]
#[tokio::test]
async fn a_hanging_version_command_times_out() {
    use std::os::unix::fs::PermissionsExt;
    let t = tempfile::tempdir().unwrap();
    let b = t.path().join("b");
    put(&b, "demo", &manifest("demo", "Demo", "slowtool"));
    let r = Registry::load(&Sources {
        builtin: Builtin::Dir(b),
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    });
    let sys = t.path().join("sys");
    std::fs::create_dir_all(&sys).unwrap();
    let p = sys.join("slowtool");
    std::fs::write(&p, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    let host = HostPaths {
        toolchains_dir: t.path().join("tc"),
        path: Some(sys.into_os_string()),
    };
    let m = r.get("demo").unwrap();
    let start = std::time::Instant::now();
    assert!(detect_tool(m, &m.tools[0], &host).await.is_none());
    let took = start.elapsed();
    assert!(
        took >= axiom_env::detect::VERSION_TIMEOUT && took.as_secs() < 10,
        "{took:?}"
    );
}
