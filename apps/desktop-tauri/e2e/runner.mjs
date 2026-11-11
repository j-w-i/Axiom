#!/usr/bin/env node
// Runs the end-to-end suite on any OS: starts an X server when the platform
// needs one (Linux: a private Xvfb, never the developer's desktop), starts
// tauri-driver, runs e2e/smoke.mjs, then cleans everything up.
//
// Usage: node e2e/runner.mjs [--no-xvfb]
// Env:   AXIOM_XVFB (path to Xvfb), AXIOM_TAURI_APP, AXIOMD_UNDER_TEST,
//        AXIOM_SCREENSHOT_DIR, TAURI_DRIVER_LOG

import { spawn, spawnSync } from "node:child_process";
import { existsSync, openSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

const here = import.meta.dirname;
const repo = resolve(here, "../..");
const noXvfb = process.argv.includes("--no-xvfb");
// Any non-flag argument is an alternative suite (probes, one-off checks); the
// display, driver and cleanup are the same either way. With none, the default
// suites run in order: the main one, then quitting with unsaved work (which
// ends with the app exiting).
const customSuites = process.argv.filter(
  (a, i) => i >= 2 && !a.startsWith("--"),
);
const suites = customSuites.length
  ? customSuites
  : process.env.AXIOM_E2E_SCRIPT
    ? [process.env.AXIOM_E2E_SCRIPT]
    : [join(here, "smoke.mjs"), join(here, "quit.mjs")];
const children = [];

function log(msg) {
  console.error(`[e2e] ${msg}`);
}

async function freePort() {
  return new Promise((res, rej) => {
    const srv = createServer();
    srv.once("error", rej);
    srv.listen(0, "127.0.0.1", () => {
      const port = srv.address().port;
      srv.close(() => res(port));
    });
  });
}

function start(cmd, args, env = {}, stdio = "inherit") {
  const child = spawn(cmd, args, { env: { ...process.env, ...env }, stdio });
  children.push(child);
  return child;
}

function killTree(child) {
  if (!child || child.exitCode !== null) return;
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    try {
      process.kill(-child.pid, "SIGTERM"); // own process group
    } catch {
      try {
        child.kill("SIGTERM");
      } catch {
        /* already gone */
      }
    }
  }
}

async function waitForHttp(url, ms) {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(url);
      if (r.ok) return true;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  return false;
}

async function main() {
  const env = {};

  // ---- display -----------------------------------------------------------------
  // A test must NEVER put a window on the developer's screen. On Linux that means
  // always starting our own Xvfb: an inherited DISPLAY (:0, the desktop session)
  // is ignored unless AXIOM_ALLOW_EXISTING_DISPLAY=1 says otherwise (CI inside
  // `xvfb-run`, or a deliberate manual run).
  if (process.platform === "linux") {
    const allowExisting = process.env.AXIOM_ALLOW_EXISTING_DISPLAY === "1";
    if (allowExisting && process.env.DISPLAY) {
      log(`using the existing DISPLAY ${process.env.DISPLAY} (AXIOM_ALLOW_EXISTING_DISPLAY=1)`);
      env.GDK_BACKEND = "x11";
    } else {
      const xvfb = process.env.AXIOM_XVFB ?? (existsSync("/usr/bin/Xvfb") ? "/usr/bin/Xvfb" : null);
      if (!xvfb) {
        throw new Error(
          "refusing to run: no private X server available (install Xvfb, or run " +
            "apps/desktop-tauri/scripts/sysroot.sh; AXIOM_ALLOW_EXISTING_DISPLAY=1 overrides)",
        );
      }
      let display;
      for (let i = 0; i < 20; i++) {
        const n = 90 + Math.floor(Math.random() * 40);
        if (!existsSync(`/tmp/.X11-unix/X${n}`)) {
          display = `:${n}`;
          break;
        }
      }
      if (!display) throw new Error("no free X display number");
      const x = spawn(xvfb, [display, "-screen", "0", "1440x900x24", "-nolisten", "tcp", "-extension", "RANDR"], {
        stdio: "ignore",
      });
      children.push(x);
      for (let i = 0; i < 100 && !existsSync(`/tmp/.X11-unix/X${display.slice(1)}`); i++) {
        await new Promise((r) => setTimeout(r, 50));
      }
      // Wayland must not win over X11, and GTK must not probe DMABUF on a virtual display.
      Object.assign(env, {
        DISPLAY: display,
        GDK_BACKEND: "x11",
        XDG_SESSION_TYPE: "x11",
        WEBKIT_DISABLE_DMABUF_RENDERER: "1",
      });
      delete process.env.WAYLAND_DISPLAY;
      log(`private Xvfb on ${display} (DISPLAY from the environment is ignored)`);
    }
  }

  // ---- driver + suites ------------------------------------------------------------
  // One tauri-driver per suite: WebKitWebDriver keeps a single session, and the
  // crash test deliberately leaves its session behind (the page it was bound to
  // is dead), so a shared driver would refuse the next suite with
  // "Maximum number of active sessions".
  const logPath = process.env.TAURI_DRIVER_LOG ?? join(tmpdir(), "axiom-tauri-driver.log");
  let code = 0;
  for (const suitePath of suites) {
    const driverPort = await freePort();
    const logFd = openSync(logPath, "w");
    const driver = spawn("tauri-driver", ["--port", String(driverPort), "--native-port", String(driverPort + 1)], {
      env: { ...process.env, ...env },
      stdio: ["ignore", logFd, logFd],
    });
    children.push(driver);
    driver.on("error", (e) => log(`tauri-driver could not start: ${e.message}`));

    const ready = await waitForHttp(`http://127.0.0.1:${driverPort}/status`, 15000);
    if (!ready) {
      log(`tauri-driver did not answer on port ${driverPort}; see ${logPath}`);
      code = 1;
    } else {
      log(`suite: ${suitePath} (driver ${driverPort}, app log ${logPath})`);
      const suiteCode = await new Promise((res) => {
        const suite = spawn(process.execPath, [suitePath], {
          env: {
            ...process.env,
            ...env,
            TAURI_DRIVER_URL: `http://127.0.0.1:${driverPort}`,
            TAURI_DRIVER_LOG: logPath,
          },
          stdio: "inherit",
        });
        children.push(suite);
        suite.on("exit", (c) => res(c ?? 1));
      });
      if (suiteCode !== 0) code = suiteCode;
    }
    // Release this driver (and its native driver) before the next suite.
    killTree(driver);
    children.splice(children.indexOf(driver), 1);
    await new Promise((r) => setTimeout(r, 500));
  }
  return code;
}

let exitCode = 1;
try {
  exitCode = await main();
} catch (e) {
  log(e instanceof Error ? e.message : String(e));
} finally {
  // The crash test deliberately leaves the app running after its session dies.
  spawnSync(process.execPath, ["-e", "process.exit(0)"], { stdio: "ignore" });
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/IM", "axiom-tauri.exe", "/F"], { stdio: "ignore" });
  } else {
    spawnSync("pkill", ["-f", `${repo}/target/release/axiom-tauri`], { stdio: "ignore" });
  }
  for (const c of children.reverse()) killTree(c);
}
process.exit(exitCode);
