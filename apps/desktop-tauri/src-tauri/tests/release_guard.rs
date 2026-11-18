//! The end-to-end suite drives the app through an embedded WebDriver server
//! (`tauri-plugin-wdio-webdriver`): an HTTP server that can run arbitrary
//! JavaScript in the page. It must never reach a user. It is compiled in only
//! under the `e2e` feature; these tests make that a checked property instead of
//! a convention.
//!
//! 1. The manifest: the dependency is optional and only the `e2e` feature
//!    enables it; `default` must not.
//! 2. The resolved graph: `cargo tree` for a default build of this crate does
//!    not contain the plugin.
//! 3. The packaging script never passes `--features e2e` (or any feature).

use std::path::PathBuf;
use std::process::Command;

const PLUGIN: &str = "tauri-plugin-wdio-webdriver";

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = manifest_dir().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn the_plugin_is_optional_and_only_the_e2e_feature_enables_it() {
    let manifest: toml::Table = read("Cargo.toml")
        .parse()
        .expect("Cargo.toml is valid TOML");

    let dep = manifest["dependencies"][PLUGIN]
        .as_table()
        .unwrap_or_else(|| panic!("{PLUGIN} must be a table dependency with optional = true"));
    assert_eq!(
        dep.get("optional").and_then(|v| v.as_bool()),
        Some(true),
        "{PLUGIN} must be an optional dependency"
    );

    let features = manifest["features"].as_table().expect("[features] table");
    let default: Vec<&str> = features["default"]
        .as_array()
        .expect("default feature list")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(
        !default.iter().any(|f| *f == "e2e" || f.contains(PLUGIN)),
        "the default feature set must not enable the embedded WebDriver server: {default:?}"
    );

    // Only `e2e` may name the plugin.
    for (name, list) in features {
        let mentions = list
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .any(|s| s.contains(PLUGIN))
            })
            .unwrap_or(false);
        if mentions {
            assert_eq!(name, "e2e", "feature `{name}` must not enable {PLUGIN}");
        }
    }
}

#[test]
fn a_default_build_does_not_link_the_plugin() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args([
            "tree",
            "-p",
            "axiom-desktop-tauri",
            "-e",
            "normal",
            "--prefix",
            "none",
        ])
        .current_dir(manifest_dir())
        .output()
        .expect("cargo tree runs");
    assert!(
        out.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let tree = String::from_utf8_lossy(&out.stdout);
    assert!(
        tree.contains("axiom-desktop-tauri"),
        "unexpected cargo tree output:\n{tree}"
    );
    assert!(
        !tree.lines().any(|l| l.starts_with(PLUGIN)),
        "a default build depends on {PLUGIN} - the embedded WebDriver server would ship"
    );
}

#[test]
fn packaging_never_enables_a_feature_and_checks_the_shipped_binary() {
    let script = read("../scripts/bundle.sh");
    let commands: Vec<&str> = script
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .collect();
    assert!(
        !commands.iter().any(|l| l.contains("--features")),
        "scripts/bundle.sh must build packages with the default features only"
    );
    assert!(
        commands
            .iter()
            .any(|l| l.contains("grep") && l.contains("tauri-plugin-webdriver is ready")),
        "scripts/bundle.sh must check the packaged binary for the WebDriver server"
    );
}
