//! Delegate installs (M2-ENV-003): a scripted delegate bootstrapped from a `file` download,
//! offline; and the plan the real Rust manifest produces.

use axiom_env::registry::{Builtin, Registry, Sources};
use axiom_env::{Http, HttpConfig, Installer};
use axiom_protocol::env::{Sha256Source, Step};
use std::path::Path;

fn installer(data: &Path) -> Installer {
    let http = Http::new(HttpConfig {
        allow_http: true,
        ..HttpConfig::default()
    })
    .unwrap();
    Installer::new(data.to_path_buf(), http)
}

/// With rustup absent from managed storage, the Rust plan bootstraps it from rustup-init
/// first: download (checksum from static.rust-lang.org), place, set up, then the toolchain
/// install, all with homes inside the root, then verification.
#[tokio::test]
async fn rust_plan_bootstraps_rustup_first() {
    let r = Registry::load(&Sources {
        builtin: Builtin::Embedded,
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    });
    let m = r.get("rust").unwrap();
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let plan = inst.plan(m, "1.84.0", None).await.unwrap();
    let root = data.path().join("toolchains").join("rust").join("1.84.0");
    let root_s = root.to_string_lossy().into_owned();
    eprintln!("{}", serde_json::to_string_pretty(&plan).unwrap());

    let Step::Download { url, sha256, .. } = &plan.steps[0] else {
        panic!(
            "first step is the rustup-init download: {:?}",
            plan.steps[0]
        )
    };
    assert!(
        url.starts_with("https://static.rust-lang.org/rustup/dist/"),
        "{url}"
    );
    assert!(url.ends_with("/rustup-init") || url.ends_with("/rustup-init.exe"));
    assert!(matches!(sha256, Sha256Source::Url { url: u } if *u == format!("{url}.sha256")));
    assert!(matches!(
        plan.steps[1],
        Step::Extract {
            format: axiom_protocol::provider::ArchiveFormat::File,
            ..
        }
    ));
    let Step::Run { argv, env } = &plan.steps[2] else {
        panic!()
    };
    assert_eq!(argv[0], format!("{root_s}/rustup-init"));
    assert!(argv.contains(&"--no-modify-path".to_string()));
    assert!(
        argv.contains(&"none".to_string()),
        "no default toolchain from the bootstrap"
    );
    assert_eq!(env["RUSTUP_HOME"], format!("{root_s}/rustup"));
    assert_eq!(env["CARGO_HOME"], format!("{root_s}/cargo"));
    let Step::Run { argv, env } = &plan.steps[3] else {
        panic!()
    };
    assert!(
        Path::new(&argv[0]).starts_with(&root),
        "the root's own rustup: {argv:?}"
    );
    assert_eq!(
        &argv[1..],
        ["toolchain", "install", "1.84.0", "--profile", "minimal"]
    );
    assert_eq!(env["RUSTUP_TOOLCHAIN"], "1.84.0");
    let verify: Vec<_> = plan.steps[4..]
        .iter()
        .map(|s| match s {
            Step::Verify { tool_id } => tool_id.as_str(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(verify, ["cargo", "rustc"]);
    // Every variable a step sets points inside the root.
    for s in &plan.steps {
        if let Step::Run { env, .. } = s {
            for (k, v) in env {
                assert!(k == "RUSTUP_TOOLCHAIN" || v.starts_with(&root_s), "{k}={v}");
            }
        }
    }
}

#[cfg(unix)]
mod scripted {
    use super::*;
    use axiom_protocol::env::InstallProgress;
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    /// The bootstrap installer: creates `$FAKE_HOME/bin/fakeup`, which installs and
    /// uninstalls "toolchains" (a `demotool` printing its version) under `$FAKE_HOME`.
    const INIT: &str = r#"#!/bin/sh
set -e
[ "$1" = "--yes" ] || exit 3
mkdir -p "$FAKE_HOME/bin"
cat > "$FAKE_HOME/bin/fakeup" <<'UP'
#!/bin/sh
set -e
case "$1 $2" in
  "toolchain install")
    printf '#!/bin/sh\necho "demo-tool %s"\n' "$3" > "$FAKE_HOME/bin/demotool"
    chmod +x "$FAKE_HOME/bin/demotool"
    echo "installed $3 into $FAKE_HOME" ;;
  "toolchain uninstall")
    echo "$3" > "$FAKE_HOME/../../uninstalled" ;;
  "toolchain fail")
    echo "something went wrong" >&2; exit 7 ;;
  *) exit 2 ;;
esac
UP
chmod +x "$FAKE_HOME/bin/fakeup"
"#;

    async fn serve_one(body: Vec<u8>) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let body = Arc::new(body);
        tokio::spawn(async move {
            while let Ok((mut s, _)) = listener.accept().await {
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut c = [0u8; 1024];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match s.read(&mut c).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&c[..n]),
                        }
                    }
                    let h = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = s.write_all(h.as_bytes()).await;
                    if !buf.starts_with(b"HEAD") {
                        let _ = s.write_all(&body).await;
                    }
                });
            }
        });
        base
    }

    fn manifest(
        base: &str,
        digest: &str,
        install_verb: &str,
    ) -> axiom_protocol::provider::ProviderManifest {
        let os = serde_json::to_value(axiom_env::install::host_platform().os).unwrap();
        axiom_env::manifest::parse(&format!(
            r#"schema = "axiom.provider/1"
id = "demo"
name = "Demo"
description = "d"
version = "0.1.0"

[[tools]]
id = "demotool"
program = "demotool"
version_regex = 'demo-tool (\d+\.\d+\.\d+)'

[[tools]]
id = "fakeup"
program = "fakeup"
version_regex = 'fakeup (\d+)'
required = false

[[install]]
kind = "delegate"
using = "fakeup"
scope = "managed"
install = ["fakeup", "toolchain", "{install_verb}", "${{version}}"]
uninstall = ["fakeup", "toolchain", "uninstall", "${{version}}"]
list_versions = ["fakeup", "toolchain", "list"]
versions = {{ kind = "static", versions = ["2.0.0"] }}
bin_dirs = ["home/bin"]
env = {{ FAKE_HOME = "${{root}}/home" }}

[[install]]
kind = "archive"
provides = ["fakeup"]
versions = {{ kind = "static", versions = ["latest"] }}
bin_dirs = ["home/bin"]
setup = ["${{root}}/fakeup-init", "--yes"]
env = {{ FAKE_HOME = "${{root}}/home" }}

[[install.artifacts]]
platform = {{ os = "{os}" }}
url = "{base}/fakeup-init"
format = "file"
checksum = {{ kind = "sha256", values = {{ "2.0.0" = "{digest}" }} }}
"#,
            os = os.as_str().unwrap()
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn delegate_install_verify_and_uninstall() {
        let base = serve_one(INIT.as_bytes().to_vec()).await;
        let digest = hex::encode(Sha256::digest(INIT.as_bytes()));
        let m = manifest(&base, &digest, "install");
        let data = tempfile::tempdir().unwrap();
        let inst = installer(data.path());

        assert_eq!(inst.versions(&m).await.unwrap().available, ["2.0.0"]);
        let plan = inst.plan(&m, "2.0.0", None).await.unwrap();
        let kinds: Vec<&str> = plan
            .steps
            .iter()
            .map(|s| match s {
                Step::Download { .. } => "download",
                Step::Extract { .. } => "extract",
                Step::Run { .. } => "run",
                Step::Verify { .. } => "verify",
                Step::SystemPackages { .. } => "system",
            })
            .collect();
        assert_eq!(kinds, ["download", "extract", "run", "run", "verify"]);

        let labels = Arc::new(std::sync::Mutex::new(Vec::new()));
        let l = labels.clone();
        let progress = move |p: InstallProgress| l.lock().unwrap().push(p.label);
        let r = inst
            .install(
                &plan.plan_id,
                Some(&m),
                &CancellationToken::new(),
                &progress,
            )
            .await
            .unwrap();
        assert_eq!(r.verified[0].version, "2.0.0");
        let root = data.path().join("toolchains/demo/2.0.0");
        assert!(root.join("home/bin/fakeup").is_file());
        assert!(root.join("home/bin/demotool").is_file());
        let labels = labels.lock().unwrap().clone();
        assert!(
            labels.contains(&"Running fakeup-init --yes".to_string()),
            "{labels:?}"
        );
        assert!(
            labels.contains(&"Running fakeup toolchain install 2.0.0".to_string()),
            "{labels:?}"
        );
        // Nothing ran in, or was left at, the staging path.
        assert_eq!(
            std::fs::read_dir(data.path().join("toolchains/demo"))
                .unwrap()
                .count(),
            1
        );

        // Detection finds the managed tool through the delegate's layout and variables.
        let host = axiom_env::HostPaths {
            toolchains_dir: data.path().join("toolchains"),
            path: None,
        };
        let found = axiom_env::detect::detect_tool(&m, &m.tools[0], &host)
            .await
            .unwrap();
        assert_eq!(found.version, "2.0.0");

        // Uninstall runs the delegate's uninstall command, then removes the root.
        inst.uninstall("demo", "2.0.0", Some(&m)).await.unwrap();
        assert!(!root.exists());
        let marker = data.path().join("toolchains/demo/uninstalled");
        assert_eq!(std::fs::read_to_string(marker).unwrap().trim(), "2.0.0");
    }

    #[tokio::test]
    async fn a_failing_delegate_reports_its_output_and_cleans_up() {
        let base = serve_one(INIT.as_bytes().to_vec()).await;
        let digest = hex::encode(Sha256::digest(INIT.as_bytes()));
        let m = manifest(&base, &digest, "fail");
        let data = tempfile::tempdir().unwrap();
        let inst = installer(data.path());
        let plan = inst.plan(&m, "2.0.0", None).await.unwrap();
        let e = inst
            .install(&plan.plan_id, Some(&m), &CancellationToken::new(), &|_| {})
            .await
            .unwrap_err();
        match &e {
            axiom_env::EnvError::CommandFailed { code, output, .. } => {
                assert_eq!(*code, Some(7));
                assert!(output.contains("something went wrong"), "{output}");
            }
            other => panic!("{other:?}"),
        }
        let rpc: axiom_protocol::RpcError = e.into();
        assert_eq!(rpc.data.details.unwrap()["exitCode"], 7);
        assert_eq!(
            std::fs::read_dir(data.path().join("toolchains/demo"))
                .unwrap()
                .count(),
            0
        );
    }

    /// Without a bootstrap method the delegate must be on PATH; absent → prerequisite_missing.
    #[tokio::test]
    async fn delegate_without_bootstrap_needs_it_on_path() {
        let digest = "0".repeat(64);
        let mut m = manifest("http://127.0.0.1:9", &digest, "install");
        m.install.truncate(1);
        m.tools[1].program = "fakeup-not-on-path-anywhere".into();
        if let axiom_protocol::provider::InstallMethod::Delegate { install, .. } = &mut m.install[0]
        {
            install[0] = "fakeup-not-on-path-anywhere".into();
        }
        let data = tempfile::tempdir().unwrap();
        let e = installer(data.path())
            .plan(&m, "2.0.0", None)
            .await
            .unwrap_err();
        match e {
            axiom_env::EnvError::PrerequisiteMissing(_, missing) => assert_eq!(missing, ["fakeup"]),
            other => panic!("{other:?}"),
        }
    }
}
