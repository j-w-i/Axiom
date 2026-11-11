//! The host's external surface is small on purpose (ADR-0012, security.md
//! §Host commands): every command it exposes is pinned in three places that
//! must not drift — `COMMANDS` in `src/lib.rs`, the `AppManifest` in `build.rs`
//! and the window capability. This test fails if a command, permission or
//! capability is added without updating all of them.

use axiom_desktop_tauri_lib::COMMANDS;

const CAPABILITY: &str = "capabilities/main.json";

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
}

/// Command names listed in build.rs's `AppManifest::new().commands(&[...])`.
fn commands_in_build_rs() -> Vec<String> {
    let src = read("build.rs");
    let start = src
        .find(".commands(&[")
        .expect("build.rs must pin the command list with AppManifest::commands");
    let rest = &src[start..];
    let end = rest.find(']').expect("unterminated command list");
    rest[..end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_string)
        .collect()
}

fn permissions_in_capability() -> Vec<String> {
    let text = read(CAPABILITY);
    let json: serde_json::Value = serde_json::from_str(&text).expect("capability is valid JSON");
    json["permissions"]
        .as_array()
        .expect("capability has a permissions array")
        .iter()
        .map(|p| p.as_str().expect("permission is a string").to_string())
        .collect()
}

#[test]
fn capability_grants_exactly_the_pinned_commands() {
    let mut expected: Vec<String> = COMMANDS
        .iter()
        .map(|c| format!("allow-{}", c.replace('_', "-")))
        .collect();
    expected.push("dialog:allow-open".into());
    expected.push("core:app:allow-version".into());
    // macOS menu actions arrive as events (T-07); listening is the only event
    // permission the page has, and it can only hear what the host emits.
    expected.push("core:event:allow-listen".into());
    let mut actual = permissions_in_capability();
    actual.sort();
    expected.sort();
    assert_eq!(
        actual, expected,
        "capabilities/main.json must grant exactly the host commands plus the folder picker"
    );
}

#[test]
fn build_rs_pins_the_same_commands() {
    let mut actual = commands_in_build_rs();
    actual.sort();
    let mut expected: Vec<String> = COMMANDS.iter().map(|c| c.to_string()).collect();
    expected.sort();
    assert_eq!(actual, expected, "build.rs and COMMANDS disagree");
}

#[test]
fn capability_targets_the_main_window_only() {
    let json: serde_json::Value = serde_json::from_str(&read(CAPABILITY)).expect("valid JSON");
    let windows: Vec<&str> = json["windows"]
        .as_array()
        .expect("windows array")
        .iter()
        .map(|w| w.as_str().expect("window label"))
        .collect();
    assert_eq!(windows, vec!["main"]);
}

#[test]
fn no_command_can_touch_files_processes_or_arbitrary_urls() {
    // The names are the contract: nothing here may grow a filesystem, process
    // or network capability without an ADR (governance: "supervise, don't
    // reach around the daemon").
    for forbidden in [
        "fs_",
        "read_file",
        "write_file",
        "spawn",
        "exec",
        "shell",
        "open_path",
    ] {
        for command in COMMANDS {
            assert!(
                !command.contains(forbidden),
                "command {command} looks like it bypasses the daemon"
            );
        }
    }
    for permission in permissions_in_capability() {
        assert!(
            !permission.starts_with("fs:")
                && !permission.starts_with("shell:")
                && !permission.starts_with("process:"),
            "permission {permission} would give the webview direct platform access"
        );
    }
}

#[test]
fn meta_arguments_answer_without_a_display() {
    assert_eq!(
        axiom_desktop_tauri_lib::answer_meta_args(["--version".to_string()].into_iter()),
        Some(0)
    );
    assert_eq!(
        axiom_desktop_tauri_lib::answer_meta_args(["-V".to_string()].into_iter()),
        Some(0)
    );
    assert_eq!(
        axiom_desktop_tauri_lib::answer_meta_args(["--help".to_string()].into_iter()),
        Some(0)
    );
    assert_eq!(
        axiom_desktop_tauri_lib::answer_meta_args(
            ["--daemon".to_string(), "/x".to_string()].into_iter()
        ),
        None
    );
}
