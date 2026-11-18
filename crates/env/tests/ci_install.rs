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
