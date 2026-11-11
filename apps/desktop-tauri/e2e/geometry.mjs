#!/usr/bin/env node
// Window geometry (parity row S-4) and the minimum window size (S-6).
//
// The host persists geometry itself (window.json in the app config dir, the same
// per-machine state the Qt shell kept in QSettings), so this needs a real
// relaunch: set a size, quit through the app's own path, start again, read it back.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { WebDriver } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, "target/release/axiom-tauri");
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, "target/debug/axiomd");
const DRIVER = process.env.TAURI_DRIVER_URL ?? "http://127.0.0.1:4444";

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}

// One data dir for both launches, and one config dir so window.json survives.
const data = mkdtempSync(join(tmpdir(), "axiom-geom-data-"));
const config = join(data, "config");
const env = { XDG_CONFIG_HOME: config };

const TARGET = { width: 1000, height: 700, x: 40, y: 40 };

async function launch() {
  const wd = await WebDriver.start(DRIVER, APP, ["--daemon", DAEMON, "--data-dir", data], env);
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected");
  return wd;
}

let wd;
try {
  // ---- first launch: size the window, then quit through the app's own path
  wd = await launch();
  await wd.cmd("POST", "/window/rect", TARGET);
  await new Promise((r) => setTimeout(r, 300));
  const set = await wd.cmd("GET", "/window/rect");
  check("1 the window can be resized", Math.abs(set.width - TARGET.width) <= 2, `${set.width}x${set.height}`);

  // The minimum size is enforced by the window manager/window (720x480).
  await wd.cmd("POST", "/window/rect", { width: 400, height: 300, x: 40, y: 40 });
  await new Promise((r) => setTimeout(r, 300));
  const tiny = await wd.cmd("GET", "/window/rect");
  check(
    "2 the window refuses to go below its minimum size (720x480)",
    tiny.width >= 700 && tiny.height >= 460,
    `${tiny.width}x${tiny.height}`,
  );

  // Restore the target size and quit: app_quit saves the geometry.
  await wd.cmd("POST", "/window/rect", TARGET);
  await new Promise((r) => setTimeout(r, 300));
  await wd.execAsync(`const cb = arguments[arguments.length-1]; window.__axiomHost.quit().then(() => cb(true))`).catch(() => {});
  const gone = Date.now() + 10000;
  let exited = false;
  while (Date.now() < gone) {
    try {
      await wd.cmd("GET", "/window/rect");
    } catch {
      exited = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  check("3 the app exits when asked", exited);
  try {
    await wd.quit();
  } catch {
    /* already gone */
  }
  wd = null;

  // ---- second launch: the geometry comes back
  wd = await launch();
  const restored = await wd.cmd("GET", "/window/rect");
  check(
    "4 the window size is restored on the next launch",
    Math.abs(restored.width - TARGET.width) <= 2 && Math.abs(restored.height - TARGET.height) <= 2,
    `${restored.width}x${restored.height} (expected ${TARGET.width}x${TARGET.height})`,
  );
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
} finally {
  try {
    await wd?.quit();
  } catch {
    /* gone */
  }
  rmSync(data, { recursive: true, force: true });
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} geometry checks passed`);
process.exit(failed.length ? 1 : 0);
