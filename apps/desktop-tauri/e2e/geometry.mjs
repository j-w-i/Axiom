#!/usr/bin/env node
// Window geometry (parity row S-4) and the minimum window size (S-6).
//
// The host persists geometry itself (window.json in the app config dir, the same
// per-machine state the Qt shell kept in QSettings), so this needs a real
// relaunch: set a size, quit through the app's own path, start again, read it back.

import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { WebDriver, appLogger } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}

// One data dir for both launches. With --data-dir the host keeps window.json
// there (not in the user's profile), so this works and stays isolated on every OS.
const data = mkdtempSync(join(tmpdir(), "axiom-geom-data-"));

const TARGET = { width: 1000, height: 700, x: 40, y: 40 };

async function launch() {
  const wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, appLogger("geometry"));
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
  // The app exits while answering, so the script's own result never arrives.
  await wd.execAsync(`const cb = arguments[arguments.length-1]; window.__axiomHost.quit().then(() => cb(true))`).catch(() => {});
  const exited = await wd.waitForExit(10000);
  const code = wd.exitCode;
  check(
    "3 the app exits when asked" + (code === undefined ? "" : " cleanly"),
    exited && (code === undefined || code === 0),
    `exited=${exited} code=${code ?? "n/a (launched by WebKitWebDriver)"}`,
  );
  check("3b the geometry was written to the data directory", existsSync(join(data, "window.json")));
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
