#!/usr/bin/env node
// Runs the end-to-end suites on any OS against the app built with the `e2e`
// Cargo feature, which embeds the WebDriver server: no tauri-driver, no
// WebKitWebDriver, no msedgedriver. Each suite launches the app itself
// (e2e/webdriver.mjs) and ends it.
//
//   Linux    a private Xvfb, always: an inherited DISPLAY (the developer's
//            screen) is ignored unless AXIOM_ALLOW_EXISTING_DISPLAY=1
//   Windows  the session's desktop (CI runner)
//   macOS    the session's GUI (CI runner)
//
// Usage: node e2e/runner.mjs [suite.mjs ...]   (default: smoke, quit, geometry, parity;
//        a bare name such as shots.mjs is looked up in e2e/; shots.mjs is not in the default set)
// Env:   AXIOM_TAURI_APP, AXIOMD_UNDER_TEST, AXIOM_SCREENSHOT_DIR, AXIOM_E2E_LOG,
//        AXIOM_XVFB (path to Xvfb), AXIOM_XVFB_SCREEN (e.g. 1440x900x24),
//        AXIOM_ALLOW_EXISTING_DISPLAY

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";

/** A suite named as `shots.mjs` is looked up in e2e/ when it is not a path that exists. */
const resolveSuite = (s) => (existsSync(s) ? resolve(s) : existsSync(join(here, s)) ? join(here, s) : s);
const customSuites = process.argv.slice(2).filter((a) => !a.startsWith("--")).map(resolveSuite);
const suites = customSuites.length
  ? customSuites
  : [join(here, "smoke.mjs"), join(here, "quit.mjs"), join(here, "geometry.mjs"), join(here, "parity.mjs")];

const children = [];
const log = (msg) => console.error(`[e2e] ${msg}`);

/** Ends a child and, on POSIX, everything in its process group (the app a suite launched). */
function killTree(child) {
  if (!child) return;
  if (process.platform === "win32") {
    if (child.exitCode === null && child.signalCode === null) {
      spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
    }
    return;
  }
  // The group outlives its leader: a suite that has exited may still have left
  // its app running, so signal the group even when the suite itself is gone.
  try {
    process.kill(-child.pid, "SIGTERM");
  } catch {
    /* no such group: nothing left */
  }
  if (child.exitCode === null && child.signalCode === null) {
    try {
      child.kill("SIGTERM");
    } catch {
      /* already gone */
    }
  }
}

function findXvfb() {
  if (process.env.AXIOM_XVFB) return process.env.AXIOM_XVFB;
  for (const p of ["/usr/bin/Xvfb", join(repo, ".toolchains/tauri-sysroot/usr/bin/Xvfb")]) {
    if (existsSync(p)) return p;
  }
  return null;
}

async function linuxDisplay() {
  if (process.env.AXIOM_ALLOW_EXISTING_DISPLAY === "1" && process.env.DISPLAY) {
    log(`using the existing DISPLAY ${process.env.DISPLAY} (AXIOM_ALLOW_EXISTING_DISPLAY=1)`);
    return { GDK_BACKEND: "x11" };
  }
  const xvfb = findXvfb();
  if (!xvfb) {
    throw new Error(
      "refusing to run: no private X server (install Xvfb, or run apps/desktop-tauri/scripts/sysroot.sh). " +
        "AXIOM_ALLOW_EXISTING_DISPLAY=1 uses the current display instead.",
    );
  }
  let n;
  for (let i = 0; i < 40; i++) {
    const candidate = 90 + Math.floor(Math.random() * 60);
    if (!existsSync(`/tmp/.X11-unix/X${candidate}`) && !existsSync(`/tmp/.X${candidate}-lock`)) {
      n = candidate;
      break;
    }
  }
  if (n === undefined) throw new Error("no free X display number");
  // The screenshot suite also takes a 200% pass (GDK_SCALE=2): a 1440x900
  // window then needs 2880x1800 device pixels. Other suites keep 1440x900.
  const screen = process.env.AXIOM_XVFB_SCREEN ?? (suites.some((s) => s.endsWith("shots.mjs")) ? "2880x1800x24" : "1440x900x24");
  const x = spawn(xvfb, [`:${n}`, "-screen", "0", screen, "-nolisten", "tcp", "-extension", "RANDR"], {
    stdio: "ignore",
  });
  children.push(x);
  for (let i = 0; i < 100 && !existsSync(`/tmp/.X11-unix/X${n}`); i++) {
    if (x.exitCode !== null) throw new Error(`Xvfb exited with code ${x.exitCode}`);
    await new Promise((r) => setTimeout(r, 50));
  }
  if (!existsSync(`/tmp/.X11-unix/X${n}`)) throw new Error("Xvfb did not start");
  // Never fall through to the developer's Wayland session.
  delete process.env.WAYLAND_DISPLAY;
  delete process.env.WAYLAND_SOCKET;
  log(`private Xvfb on :${n} (an inherited DISPLAY is ignored)`);
  return {
    DISPLAY: `:${n}`,
    GDK_BACKEND: "x11",
    XDG_SESSION_TYPE: "x11",
    // A virtual display has no GPU: keep WebKitGTK off the DMABUF renderer.
    WEBKIT_DISABLE_DMABUF_RENDERER: "1",
  };
}

async function main() {
  const app = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
  if (!existsSync(app)) {
    throw new Error(`the app is not built at ${app}; run: cargo build --release -p axiom-desktop-tauri --features e2e`);
  }
  const env = process.platform === "linux" ? await linuxDisplay() : {};
  // One app log per run (the host's stderr), uploaded by CI on failure.
  const appLog = process.env.AXIOM_E2E_LOG ?? join(tmpdir(), "axiom-tauri-e2e-app.log");
  mkdirSync(dirname(appLog), { recursive: true });
  writeFileSync(appLog, "");

  let code = 0;
  for (const suitePath of suites) {
    log(`suite: ${suitePath}`);
    // A private profile per suite (Linux): the webview's localStorage (the layout) and WebKit
    // caches live under XDG_DATA_HOME / XDG_CACHE_HOME, so a run never reads or writes the
    // developer's own profile (~/.local/share/<identifier>) and suites cannot leak state into
    // each other. macOS and Windows keep the per-user webview store (not isolated yet).
    const home = process.platform === "linux" ? mkdtempSync(join(tmpdir(), "axiom-e2e-home-")) : null;
    const xdg = home ? { XDG_DATA_HOME: join(home, "data"), XDG_CACHE_HOME: join(home, "cache"), XDG_CONFIG_HOME: join(home, "config") } : {};
    for (const d of Object.values(xdg)) mkdirSync(d, { recursive: true });
    const suiteCode = await new Promise((resolveExit) => {
      const suite = spawn(process.execPath, [suitePath], {
        env: { ...process.env, ...env, ...xdg, AXIOM_TAURI_APP: app, AXIOM_E2E_LOG: appLog },
        stdio: "inherit",
        // Own process group on POSIX, so a failing suite cannot leave its app behind.
        detached: process.platform !== "win32",
      });
      children.push(suite);
      suite.on("exit", (c) => resolveExit(c ?? 1));
    });
    if (suiteCode !== 0) code = suiteCode;
    // Anything the suite left running (its app after a crash check) ends here.
    killTree(children.pop());
    if (home) rmSync(home, { recursive: true, force: true });
  }
  log(`app log: ${appLog}`);
  return code;
}

let exitCode = 1;
try {
  exitCode = await main();
} catch (e) {
  log(e instanceof Error ? e.message : String(e));
} finally {
  for (const c of children.reverse()) killTree(c);
}
process.exit(exitCode);
