//! Real installs from the internet. CI only: `cargo test -p axiom-env --features ci-install`.
#![cfg(feature = "ci-install")]

use axiom_env::registry::{Builtin, Registry, Sources};
use axiom_env::{Http, HttpConfig, Installer};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

/// A managed Rust toolchain installs, verifies and uninstalls with HOME's `.rustup` and
/// `.cargo` untouched (HOME is a fresh temp directory for this process).
#[tokio::test]
async fn rust_managed_install_roundtrip() {
    let home = tempfile::tempdir().unwrap();
    // This test binary runs only this test, so changing the process environment is safe.
    std::env::set_var("HOME", home.path());
    std::env::set_var("USERPROFILE", home.path());
    std::env::remove_var("RUSTUP_HOME");
    std::env::remove_var("CARGO_HOME");
    std::env::remove_var("RUSTUP_TOOLCHAIN");

    let r = Registry::load(&Sources {
        builtin: Builtin::Embedded,
        user_dir: None,
        workspace_dir: None,
        workspace_trusted: false,
    });
    let m = r.get("rust").unwrap().clone();
    let data = tempfile::tempdir().unwrap();
    let inst = Installer::new(
        data.path().to_path_buf(),
        Http::new(HttpConfig::default()).unwrap(),
    );
    let version = std::env::var("AXIOM_CI_RUST_VERSION").unwrap_or_else(|_| "1.84.0".into());

    let plan = inst.plan(&m, &version, None).await.unwrap();
    println!("plan:\n{}", serde_json::to_string_pretty(&plan).unwrap());
    let started = Instant::now();
    let result = inst
        .install(&plan.plan_id, Some(&m), &CancellationToken::new(), &|p| {
            if p.bytes_done.is_none() {
                println!("[{}/{}] {}", p.step + 1, p.steps, p.label);
            }
        })
        .await
        .unwrap();
    let took = started.elapsed();
    let root = data.path().join("toolchains").join("rust").join(&version);
    let size = dir_size(&root);
    println!(
        "installed {version} in {:.1} s, {:.1} MiB; verified {:?}",
        took.as_secs_f64(),
        size as f64 / 1048576.0,
        result
            .verified
            .iter()
            .map(|t| (&t.tool_id, &t.version))
            .collect::<Vec<_>>()
    );
    let tools: Vec<_> = result
        .verified
        .iter()
        .map(|t| (t.tool_id.as_str(), t.version.as_str()))
        .collect();
    assert!(tools.contains(&("cargo", version.as_str())), "{tools:?}");
    assert!(tools.contains(&("rustc", version.as_str())), "{tools:?}");

    // Detection finds the managed toolchain.
    let host = axiom_env::HostPaths {
        toolchains_dir: data.path().join("toolchains"),
        path: None,
    };
    let found = axiom_env::detect::detect_tool(&m, &m.tools[0], &host)
        .await
        .unwrap();
    assert_eq!(found.version, version);

    // M2-ENV-005: a project selecting this version resolves to it, and a process started in
    // the environment runs the managed cargo, first on PATH.
    let project =
        axiom_env::project::parse(&format!("[environment]\nrust = \"{version}\"\n")).unwrap();
    let (env, lock) = axiom_env::project::resolve(
        &r,
        &project,
        &data.path().join("toolchains"),
        std::env::var_os("PATH").as_deref(),
    )
    .await
    .unwrap();
    println!("environment: {env:#?}\nlock:\n{}", lock.to_text());
    assert!(std::path::Path::new(&env.path_prepend[0]).starts_with(&root));
    let vars = axiom_env::project::process_env(&env, std::env::var_os("PATH"));
    let out = std::process::Command::new("cargo")
        .arg("--version")
        .envs(&vars)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with(&format!("cargo {version}")), "{text}");

    inst.uninstall("rust", &version, Some(&m)).await.unwrap();
    assert!(!root.exists());
    assert!(
        !home.path().join(".rustup").exists(),
        "~/.rustup was created"
    );
    assert!(!home.path().join(".cargo").exists(), "~/.cargo was created");
    assert_eq!(
        std::fs::read_dir(home.path()).unwrap().count(),
        0,
        "HOME was written to"
    );
}

fn dir_size(p: &std::path::Path) -> u64 {
    let mut total = 0;
    for e in std::fs::read_dir(p).into_iter().flatten().flatten() {
        let md = match e.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        total += if md.is_dir() {
            dir_size(&e.path())
        } else {
            md.len()
        };
    }
    total
}

/// Linux CI only (`AXIOM_CI_SYSTEM_INSTALL=1`): a real `hello` package through the host's
/// apt or dnf, with a pkexec shim that runs the approved command through the runner's
/// passwordless `sudo -n` (CI has no polkit agent to show a prompt).
#[cfg(target_os = "linux")]
#[tokio::test]
async fn system_package_install_through_pkexec_shim() {
    use std::os::unix::fs::PermissionsExt;
    if std::env::var("AXIOM_CI_SYSTEM_INSTALL").as_deref() != Ok("1") {
        eprintln!("skipped: set AXIOM_CI_SYSTEM_INSTALL=1 (CI runners only; uses sudo)");
        return;
    }
    let t = tempfile::tempdir().unwrap();
    let shim = t.path().join("bin");
    std::fs::create_dir_all(&shim).unwrap();
    let log = t.path().join("pkexec.log");
    let pk = shim.join("pkexec");
    std::fs::write(
        &pk,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nexec sudo -n \"$@\"\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&pk, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut host = axiom_env::system::SystemHost::from_env();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let host_path =
        std::env::join_paths(std::iter::once(shim.clone()).chain(std::env::split_paths(&path)))
            .unwrap();
    host.path = Some(host_path.clone());

    let m = axiom_env::manifest::parse(
        r#"schema = "axiom.provider/1"
id = "hello"
name = "GNU Hello"
description = "CI fixture"
version = "0.1.0"

[[tools]]
id = "hello"
program = "hello"
version_regex = 'hello \(GNU Hello\) (\d+\.\d+(?:\.\d+)?)'

[[install]]
kind = "system_package"
packages = { apt = ["hello"], dnf = ["hello"] }
"#,
    )
    .unwrap();
    let data = tempfile::tempdir().unwrap();
    let inst = Installer::new(
        data.path().to_path_buf(),
        Http::new(HttpConfig::default()).unwrap(),
    )
    .with_system_host(host);
    let plan = inst.plan(&m, "system", Some(0)).await.unwrap();
    println!("plan:\n{}", serde_json::to_string_pretty(&plan).unwrap());
    let axiom_protocol::env::Step::SystemPackages { argv, .. } = &plan.steps[0] else {
        panic!()
    };
    assert_eq!(argv[0], pk.to_string_lossy());
    let r = inst
        .install(&plan.plan_id, Some(&m), &CancellationToken::new(), &|p| {
            println!("  {}", p.label)
        })
        .await;
    if r.is_err() {
        // What the host actually has, to tell a detection fault from an install fault.
        for dir in std::env::split_paths(host_path.as_os_str()) {
            let p = dir.join("hello");
            if p.is_file() {
                let out = std::process::Command::new(&p).arg("--version").output();
                println!(
                    "{}: {:?}",
                    p.display(),
                    out.map(|o| (
                        o.status,
                        String::from_utf8_lossy(&o.stdout).into_owned(),
                        String::from_utf8_lossy(&o.stderr).into_owned()
                    ))
                );
            }
        }
        println!("searched PATH: {}", host_path.to_string_lossy());
    }
    let r = r.unwrap();
    println!("verified {:?}", r.verified);
    assert_eq!(r.verified[0].tool_id, "hello");
    // The shim saw exactly the command the plan showed (after `pkexec`).
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        argv[1..].join(" ")
    );
}
