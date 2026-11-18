//! Tauri host for the Axiom desktop shell.
//!
//! Responsibilities (docs/migration/qt-to-tauri/architecture.md §Host):
//! - own the `axiomd` connection through `axiom-desktop-bridge`;
//! - expose a *minimal* command surface to the webview: forward an allowed
//!   protocol method, cancel it, read/restart the connection, and a few host
//!   services (folder dialog via plugin, external links, window geometry,
//!   confirmed quit). There is no command that reads or writes workspace
//!   files or runs processes: those stay in the daemon (governance rule 5);
//! - deliver every daemon event to the UI through one ordered `Channel`;
//! - keep the last unsaved text of each document (memory only) so a crashed
//!   webview can be reloaded without losing the user's typing (T-08), and
//!   watch a heartbeat from the page so that a dead or hung webview is
//!   noticed and reloaded (see [`Recovery`]).
//!
//! The command list is pinned in build.rs (`AppManifest::commands`) and the
//! window's capability (capabilities/main.json) grants exactly these.

use axiom_desktop_bridge::{locate_daemon, ui_callable, BridgeConfig, BridgeEvent, DaemonBridge};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use tauri::ipc::Channel;
use tauri::{Manager, PhysicalPosition, PhysicalSize, RunEvent, WindowEvent};

/// The complete command surface of the host. Pinned in `build.rs` (so no
/// capability can grant anything else) and in `capabilities/main.json`; a test
/// fails if the three lists disagree with this one.
pub const COMMANDS: [&str; 11] = [
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
];

/// A webview that has not sent a heartbeat for this long is considered dead or
/// hung and is reloaded (T-08). Generous on purpose: a reload is disruptive and
/// WebKit only guarantees one timer tick per second in a backgrounded page.
/// Linux does not need this path at all: the engine reports process death
/// directly (`install_web_process_watch`).
const UNRESPONSIVE_MS: u64 = 30_000;
/// A page that never sends its first heartbeat gets one reload attempt, then gives up.
const FIRST_BEAT_GRACE_MS: u64 = 30_000;
/// Never reload more often than this; a page that crashes on load must not loop.
const MAX_RELOADS: u32 = 3;
const HEARTBEAT_TICK: Duration = Duration::from_secs(2);

/// Answers `--version` and `--help` without creating a window or touching the
/// daemon (the Qt shell's equivalent is checked in `main.cpp` before the
/// application object exists). `Some(code)` means "exit with this code".
pub fn answer_meta_args(args: impl Iterator<Item = String>) -> Option<i32> {
    for arg in args {
        match arg.as_str() {
            "--version" | "-V" => {
                println!("axiom-tauri {}", env!("CARGO_PKG_VERSION"));
                return Some(0);
            }
            "--help" | "-h" => {
                println!(
                    "Axiom desktop shell (Tauri)\n\n\
                     USAGE: axiom-tauri [OPTIONS]\n\n\
                     OPTIONS:\n\
                     \x20 --daemon <PATH>     Path to the axiomd binary (default: $AXIOMD, a\n\
                     \x20                    sibling of this executable, then PATH)\n\
                     \x20 --data-dir <DIR>    Data directory forwarded to axiomd\n\
                     \x20 -h, --help          Print this help\n\
                     \x20 -V, --version       Print the version"
                );
                return Some(0);
            }
            _ => {}
        }
    }
    None
}

/// Events the UI receives on its subscription channel, in order.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
enum HostEvent {
    /// Anything from the daemon bridge (state, notification, response, log).
    Bridge { event: BridgeEvent },
    /// The user asked to close the window; the UI decides (unsaved work).
    CloseRequested,
}

/// Buffers events until the UI subscribes (and again across a page reload).
#[derive(Default)]
struct Outbox {
    channel: Option<Channel<HostEvent>>,
    buffer: VecDeque<HostEvent>,
}

const MAX_BUFFERED: usize = 10_000;

/// Crash recovery (T-08). The webview content process can die or hang; when it
/// does, everything in the page is lost — including the stores that know which
/// documents are open. Two mechanisms make that recoverable:
///
/// 1. `snapshots` keeps the latest unsaved text of each dirty document in the
///    *host* (memory only, never on disk), so it survives the page;
/// 2. a heartbeat lease: the page calls `host_heartbeat` every two seconds. No
///    beat for [`UNRESPONSIVE_MS`] means the page is dead or hung, and the host
///    reloads the webview. After the reload the UI restores its tabs from the
///    daemon's workspace state and asks for `recovery_state`, which returns the
///    snapshots so they can be re-opened dirty.
///
/// A heartbeat lease was chosen over engine-specific crash callbacks because it
/// behaves the same on WebKitGTK, WebView2 and WKWebView, and it also covers a
/// hung (not crashed) page, which no engine reports.
#[derive(Default)]
struct Recovery {
    snapshots: Mutex<std::collections::BTreeMap<String, String>>,
    /// True when the current page was loaded as part of a recovery.
    notice_pending: AtomicBool,
    reloads: AtomicU32,
    seen_beat: AtomicBool,
    last_beat_ns: AtomicU64,
    first_beat_deadline_ns: AtomicU64,
}

struct HostState {
    bridge: DaemonBridge,
    recovery: Recovery,
    outbox: Arc<Mutex<Outbox>>,
    started: Instant,
    data_dir: Option<PathBuf>,
    quitting: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn push(outbox: &Mutex<Outbox>, event: HostEvent) {
    let mut o = lock(outbox);
    if let Some(ch) = &o.channel {
        if ch.send(event.clone()).is_ok() {
            return;
        }
        o.channel = None; // the page went away; buffer until it subscribes again
    }
    if o.buffer.len() >= MAX_BUFFERED {
        o.buffer.pop_front();
    }
    o.buffer.push_back(event);
}

#[derive(Default)]
struct Cli {
    daemon: Option<PathBuf>,
    data_dir: Option<PathBuf>,
}

fn parse_cli() -> Cli {
    let mut cli = Cli::default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--daemon" => cli.daemon = args.next().map(PathBuf::from),
            "--data-dir" => cli.data_dir = args.next().map(PathBuf::from),
            _ => {
                if let Some(v) = a.strip_prefix("--daemon=") {
                    cli.daemon = Some(PathBuf::from(v));
                } else if let Some(v) = a.strip_prefix("--data-dir=") {
                    cli.data_dir = Some(PathBuf::from(v));
                }
            }
        }
    }
    if cli.data_dir.is_none() {
        cli.data_dir = std::env::var_os("AXIOM_DATA_DIR").map(PathBuf::from);
    }
    cli
}

// ---- commands -----------------------------------------------------------------

/// Subscribes the page to host events. Replaces any previous subscription
/// (page reload) and first delivers what was buffered meanwhile, plus the
/// current connection state.
#[tauri::command]
fn host_subscribe(state: tauri::State<'_, HostState>, channel: Channel<HostEvent>) {
    let (generation, current) = state.bridge.state();
    let mut o = lock(&state.outbox);
    let _ = channel.send(HostEvent::Bridge {
        event: BridgeEvent::State {
            generation,
            state: current,
        },
    });
    while let Some(e) = o.buffer.pop_front() {
        let _ = channel.send(e);
    }
    o.channel = Some(channel);
}

/// Sends one protocol request; the answer arrives on the channel as a
/// `response` event with the same `key`. Only protocol 0.1 methods a client
/// may issue are accepted (`initialize`/`shutdown` belong to the host).
#[tauri::command]
fn daemon_request(
    state: tauri::State<'_, HostState>,
    key: u64,
    method: String,
    params: Value,
    timeout_ms: Option<u64>,
) -> Result<(), String> {
    if !ui_callable(&method) {
        return Err(format!("method not allowed: {method}"));
    }
    state
        .bridge
        .request_forward(key, &method, params, timeout_ms.map(Duration::from_millis));
    Ok(())
}

#[tauri::command]
fn daemon_cancel(state: tauri::State<'_, HostState>, key: u64) {
    state.bridge.cancel_forward(key);
}

#[tauri::command]
async fn daemon_restart(state: tauri::State<'_, HostState>) -> Result<(), String> {
    let bridge = state.bridge.clone();
    bridge.restart().await;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    app_version: String,
    tauri_version: String,
    webview_version: String,
    protocol_version: String,
    data_dir: Option<String>,
    os: String,
    /// Milliseconds since the host process started (startup instrumentation).
    uptime_ms: u128,
    /// The host process id (diagnostics; end-to-end tests use it to kill the
    /// webview's content process and check crash recovery).
    pid: u32,
}

#[tauri::command]
fn app_info(state: tauri::State<'_, HostState>) -> AppInfo {
    AppInfo {
        app_version: env!("CARGO_PKG_VERSION").into(),
        tauri_version: tauri::VERSION.into(),
        webview_version: tauri::webview_version().unwrap_or_else(|_| "unknown".into()),
        protocol_version: "0.1".into(),
        data_dir: state.data_dir.as_ref().map(|d| d.display().to_string()),
        os: std::env::consts::OS.into(),
        uptime_ms: state.started.elapsed().as_millis(),
        pid: std::process::id(),
    }
}

/// Opens an http(s) link in the system browser (editor bridge `openLink`).
#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    let lower = url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err("only http and https links are opened".into());
    }
    let (program, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
        ("open", vec![url.as_str()])
    } else if cfg!(windows) {
        (
            "rundll32",
            vec!["url.dll,FileProtocolHandler", url.as_str()],
        )
    } else {
        ("xdg-open", vec![url.as_str()])
    };
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The UI confirmed (no unsaved work, or the user chose). Shuts the daemon
/// down within the bounded lifecycle and exits.
#[tauri::command]
async fn app_quit(app: tauri::AppHandle, state: tauri::State<'_, HostState>) -> Result<(), String> {
    state.quitting.store(true, Ordering::SeqCst);
    save_geometry(&app);
    state.bridge.shutdown().await;
    app.exit(0);
    Ok(())
}

// ---- native menu (macOS only, T-07) -------------------------------------------
//
// macOS expects an application menu; an in-window menu is not idiomatic there.
// Every item dispatches the *same* action id the command palette and the
// in-window menus use, delivered to the UI as the `axiom://menu` event, so there
// is exactly one implementation of each command.
//
// This module is compiled only on macOS, so it is compile-verified by the macOS
// CI job (T-09) and behaviour-verified by the macOS end-to-end run (T-10).
#[cfg(target_os = "macos")]
fn install_menu(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};

    fn item(
        app: &tauri::AppHandle,
        id: &str,
        text: &str,
        accelerator: Option<&str>,
    ) -> tauri::Result<tauri::menu::MenuItem<tauri::Wry>> {
        let mut builder = MenuItemBuilder::with_id(id, text);
        if let Some(a) = accelerator {
            builder = builder.accelerator(a);
        }
        builder.build(app)
    }

    let app_menu = SubmenuBuilder::new(app, "Axiom")
        .item(&PredefinedMenuItem::about(app, Some("About Axiom"), None)?)
        .separator()
        .item(&PredefinedMenuItem::hide(app, None)?)
        .item(&PredefinedMenuItem::hide_others(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::quit(app, Some("Quit Axiom"))?)
        .build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&item(
            app,
            "open-folder",
            "Open Folder…",
            Some("CmdOrCtrl+O"),
        )?)
        .item(&item(app, "save", "Save", Some("CmdOrCtrl+S"))?)
        .item(&item(app, "save-all", "Save All", None)?)
        .separator()
        .item(&item(app, "close-folder", "Close Folder", None)?)
        .item(&item(app, "settings", "Settings…", Some("CmdOrCtrl+,"))?)
        .build()?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .separator()
        .item(&item(app, "find", "Find", None)?)
        .build()?;
    let view = SubmenuBuilder::new(app, "View")
        .item(&item(
            app,
            "toggle-sidebar",
            "Toggle Side Bar",
            Some("CmdOrCtrl+B"),
        )?)
        .item(&item(
            app,
            "toggle-panel",
            "Toggle Panel",
            Some("CmdOrCtrl+J"),
        )?)
        .build()?;
    let run = SubmenuBuilder::new(app, "Run")
        .item(&item(
            app,
            "run-command",
            "Run Command…",
            Some("CmdOrCtrl+Shift+R"),
        )?)
        .item(&item(app, "rerun", "Rerun Last", Some("F5"))?)
        .item(&item(app, "stop", "Stop", Some("Shift+F5"))?)
        .build()?;
    let help = SubmenuBuilder::new(app, "Help")
        .item(&item(app, "about", "About Axiom", None)?)
        .build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &file, &edit, &view, &run, &help])
        .build()?;
    app.set_menu(menu)?;
    Ok(())
}

// ---- crash recovery (T-08) ---------------------------------------------------

/// The page is alive. Called every two seconds by the UI.
#[tauri::command]
fn host_heartbeat(state: tauri::State<'_, HostState>) {
    let now = state.started.elapsed().as_nanos() as u64;
    state.recovery.last_beat_ns.store(now, Ordering::SeqCst);
    state.recovery.seen_beat.store(true, Ordering::SeqCst);
}

/// Remembers the latest unsaved text of one document (memory only).
#[tauri::command]
fn snapshot_put(state: tauri::State<'_, HostState>, path: String, text: String) {
    lock(&state.recovery.snapshots).insert(path, text);
}

/// Forgets a document's unsaved text (saved, reverted to the saved state, or closed).
#[tauri::command]
fn snapshot_clear(state: tauri::State<'_, HostState>, path: String) {
    lock(&state.recovery.snapshots).remove(&path);
}

/// What the freshly loaded page needs to rebuild after a recovery: the unsaved
/// texts, and whether this load *is* a recovery (so the UI can say so).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecoverySnapshot {
    recovered: bool,
    snapshots: std::collections::BTreeMap<String, String>,
}

#[tauri::command]
fn recovery_state(state: tauri::State<'_, HostState>) -> RecoverySnapshot {
    RecoverySnapshot {
        recovered: state.recovery.notice_pending.swap(false, Ordering::SeqCst),
        snapshots: lock(&state.recovery.snapshots).clone(),
    }
}

/// Handles a webview that must be reloaded: remembers that the *next* page load
/// is a recovery (so the UI can say so), gives it a fresh grace period, and
/// resets the heartbeat clock so the monitor does not reload it twice.
fn schedule_recovery(app: &tauri::AppHandle) {
    let state = app.state::<HostState>();
    if state.quitting.load(Ordering::SeqCst) {
        return;
    }
    state.recovery.notice_pending.store(true, Ordering::SeqCst);
    state.recovery.seen_beat.store(false, Ordering::SeqCst);
    let now = state.started.elapsed().as_nanos() as u64;
    state
        .recovery
        .first_beat_deadline_ns
        .store(now + UNRESPONSIVE_MS * 1_000_000, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.reload() {
            eprintln!("[recovery] could not reload the webview: {e}");
        }
    }
}

/// Native crash detection where the engine reports it (Linux/WebKitGTK). This
/// is the immediate, false-positive-free signal; the heartbeat is the portable
/// fallback used on the other platforms until T-09/T-10 verify their hooks.
#[cfg(target_os = "linux")]
fn install_web_process_watch(app: &tauri::AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let handle = app.clone();
    let installed = window.with_webview(move |platform| {
        use webkit2gtk::WebViewExt as _;
        let view = platform.inner();
        let handle = handle.clone();
        view.connect_web_process_terminated(move |_, _reason| {
            eprintln!("[recovery] the webview process terminated; reloading");
            let h = handle.clone();
            let _ = handle.run_on_main_thread(move || schedule_recovery(&h));
        });
    });
    if let Err(e) = installed {
        eprintln!("[recovery] could not watch the webview process: {e}");
    }
}

/// Reloads the webview when the page stops proving it is alive. Runs for the
/// life of the process; stops in the background when the app is quitting.
fn spawn_heartbeat_monitor(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(HEARTBEAT_TICK);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let state = app.state::<HostState>();
            if state.quitting.load(Ordering::SeqCst) {
                return;
            }
            // Only enforce the lease for a window the user can actually see: a
            // hidden or minimised webview throttles its timers, and reloading it
            // would throw away the page the user is coming back to.
            let visible = app
                .get_webview_window("main")
                .and_then(|w| w.is_visible().ok())
                .unwrap_or(false);
            if !visible {
                state.recovery.seen_beat.store(false, Ordering::SeqCst);
                continue;
            }
            let now = state.started.elapsed().as_nanos() as u64;
            if !state.recovery.seen_beat.load(Ordering::SeqCst) {
                // The page has not booted yet: give it a generous grace period.
                let deadline = state.recovery.first_beat_deadline_ns.load(Ordering::SeqCst);
                if now < deadline {
                    continue;
                }
            } else if now.saturating_sub(state.recovery.last_beat_ns.load(Ordering::SeqCst))
                < UNRESPONSIVE_MS * 1_000_000
            {
                continue;
            }

            let reloads = state.recovery.reloads.fetch_add(1, Ordering::SeqCst) + 1;
            if reloads > MAX_RELOADS {
                eprintln!(
                    "[recovery] the webview is unresponsive and has been reloaded {MAX_RELOADS} times; giving up"
                );
                return;
            }
            eprintln!(
                "[recovery] no heartbeat for {} s; reloading the webview (attempt {reloads})",
                UNRESPONSIVE_MS / 1000
            );
            schedule_recovery(&app);
        }
    });
}

// ---- window geometry (per-machine UI state, like the Qt shell's QSettings) -----

#[derive(Serialize, Deserialize)]
struct Geometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    maximized: bool,
}

/// Where the window geometry is kept. With an explicit data directory
/// (`--data-dir` / `AXIOM_DATA_DIR`) it lives there, next to the daemon's
/// database, so an isolated instance (tests, a second profile) never reads or
/// writes the user's real one. Otherwise the per-machine app config dir.
fn geometry_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    let state = app.state::<HostState>();
    if let Some(dir) = &state.data_dir {
        return Some(dir.join("window.json"));
    }
    app.path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("window.json"))
}

fn save_geometry(app: &tauri::AppHandle) {
    let (Some(win), Some(file)) = (app.get_webview_window("main"), geometry_file(app)) else {
        return;
    };
    let maximized = win.is_maximized().unwrap_or(false);
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) else {
        return;
    };
    let g = Geometry {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized,
    };
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(&g) {
        let _ = std::fs::write(file, text);
    }
}

fn restore_geometry(app: &tauri::AppHandle) {
    let (Some(win), Some(file)) = (app.get_webview_window("main"), geometry_file(app)) else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(file) else {
        return;
    };
    let Ok(g) = serde_json::from_str::<Geometry>(&text) else {
        return;
    };
    if g.width < 720 || g.height < 480 {
        return;
    }
    // Never restore onto a monitor that is gone.
    let visible = win
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let p = m.position();
            let s = m.size();
            g.x < p.x + s.width as i32
                && g.x + g.width as i32 > p.x
                && g.y < p.y + s.height as i32
                && g.y + g.height as i32 > p.y
        });
    if visible {
        let _ = win.set_position(PhysicalPosition::new(g.x, g.y));
    }
    let _ = win.set_size(PhysicalSize::new(g.width, g.height));
    if g.maximized {
        let _ = win.maximize();
    }
}

// ---- entry point ---------------------------------------------------------------

pub fn run() {
    let started = Instant::now();
    let cli = parse_cli();
    let outbox: Arc<Mutex<Outbox>> = Arc::default();

    let mut config = BridgeConfig::new(locate_daemon(cli.daemon.clone()));
    if let Some(d) = &cli.data_dir {
        config.args = vec!["--data-dir".into(), d.display().to_string()];
    }
    let sink_outbox = outbox.clone();
    let tauri::async_runtime::RuntimeHandle::Tokio(rt) = tauri::async_runtime::handle();
    let bridge = DaemonBridge::with_runtime(
        rt,
        config,
        Arc::new(move |event| {
            if let BridgeEvent::Log { line } = &event {
                eprintln!("[axiomd] {line}");
            }
            push(&sink_outbox, HostEvent::Bridge { event });
        }),
    );

    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // macOS: WKWebView reports a dead WebContent process directly
    // (webViewWebContentProcessDidTerminate). Tauri's default handler reloads
    // silently; this one goes through schedule_recovery so the page knows it
    // was recovered and restores the host-held snapshots - the same path as
    // the Linux WebKitGTK signal.
    #[cfg(target_os = "macos")]
    let builder = builder.on_web_content_process_terminate(|webview| {
        eprintln!("[recovery] the webview process terminated; reloading");
        let app = webview.app_handle().clone();
        let h = app.clone();
        let _ = app.run_on_main_thread(move || schedule_recovery(&h));
    });
    // End-to-end builds only: an embedded W3C WebDriver server on 127.0.0.1
    // (port from TAURI_WEBDRIVER_PORT). Compiled out of every other build.
    #[cfg(feature = "e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    // End-to-end builds only, macOS: AXIOM_E2E_BACKGROUND=1 keeps the app under test from taking
    // the developer's focus (no activation, no Dock icon) and parks its window off-screen.
    #[cfg(all(feature = "e2e", target_os = "macos"))]
    let e2e_background = std::env::var("AXIOM_E2E_BACKGROUND").as_deref() == Ok("1");
    #[cfg(all(feature = "e2e", target_os = "macos"))]
    let builder = builder.activate_ignoring_other_apps(!e2e_background);

    let app = builder
        .manage(HostState {
            bridge: bridge.clone(),
            recovery: Recovery::default(),
            outbox: outbox.clone(),
            started,
            data_dir: cli.data_dir.clone(),
            quitting: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            host_subscribe,
            daemon_request,
            daemon_cancel,
            daemon_restart,
            app_info,
            open_external,
            app_quit,
            host_heartbeat,
            snapshot_put,
            snapshot_clear,
            recovery_state
        ])
        .setup(move |app| {
            restore_geometry(app.handle());
            #[cfg(all(feature = "e2e", target_os = "macos"))]
            if e2e_background {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.set_position(tauri::LogicalPosition::new(-20000.0, 20000.0));
                }
            }
            // The first page load gets FIRST_BEAT_GRACE_MS before the heartbeat
            // monitor considers it dead (it cannot have beaten yet).
            let state = app.state::<HostState>();
            state.recovery.first_beat_deadline_ns.store(
                state.started.elapsed().as_nanos() as u64 + FIRST_BEAT_GRACE_MS * 1_000_000,
                Ordering::SeqCst,
            );
            spawn_heartbeat_monitor(app.handle().clone());
            #[cfg(target_os = "linux")]
            install_web_process_watch(app.handle());
            #[cfg(target_os = "macos")]
            if let Err(e) = install_menu(app.handle()) {
                eprintln!("[menu] could not install the application menu: {e}");
            }
            // The daemon starts while the page loads; events are buffered until it subscribes.
            let bridge = bridge.clone();
            tauri::async_runtime::spawn(async move {
                bridge.start().await;
            });
            Ok(())
        })
        // Menu items (macOS) become the same action ids the UI already handles.
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            let id = event.id().as_ref().to_string();
            if let Err(e) = app.emit("axiom://menu", id) {
                eprintln!("[menu] could not deliver menu action: {e}");
            }
        })
        .on_window_event(move |window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<HostState>();
                if !state.quitting.load(Ordering::SeqCst) {
                    api.prevent_close();
                    push(&outbox, HostEvent::CloseRequested);
                }
            }
        })
        .build(tauri::generate_context!());

    let app = match app {
        Ok(a) => a,
        Err(e) => {
            eprintln!("axiom-tauri: cannot start: {e}");
            std::process::exit(1);
        }
    };
    app.run(|handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            // Any other exit path (e.g. the OS ending the session) still shuts the daemon down.
            let state = handle.state::<HostState>();
            if !state.quitting.swap(true, Ordering::SeqCst) {
                save_geometry(handle);
                let bridge = state.bridge.clone();
                tauri::async_runtime::block_on(bridge.shutdown());
            }
        }
    });
}
