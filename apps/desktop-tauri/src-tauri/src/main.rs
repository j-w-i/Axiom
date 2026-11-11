// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--version` / `--help` must work without a display, so answer before Tauri
    // initialises anything.
    if let Some(code) = axiom_desktop_tauri_lib::answer_meta_args(std::env::args().skip(1)) {
        std::process::exit(code);
    }
    axiom_desktop_tauri_lib::run();
}
