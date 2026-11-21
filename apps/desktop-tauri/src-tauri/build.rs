// The application command surface is pinned here: a command that is not in
// this list cannot be granted by any capability, and every one of them must be
// granted explicitly in capabilities/main.json.
fn main() {
    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "host_subscribe",
            "daemon_request",
            "daemon_cancel",
            "daemon_restart",
            "app_info",
            "open_external",
            "app_quit",
            "host_heartbeat",
            "snapshot_put",
            "snapshot_clear",
            "recovery_state",
            "local_all",
            "local_set",
        ]));
    if let Err(e) = tauri_build::try_build(attributes) {
        panic!("tauri-build failed: {e}");
    }
}
