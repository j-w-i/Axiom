#!/usr/bin/env node
// Window geometry (parity row S-4) and the minimum window size (S-6).
//
// The host persists geometry itself (window.json in the app config dir, the same
// per-machine state the Qt shell kept in QSettings), so this needs a real
// relaunch: set a size, quit through the app's own path, start again, read it back.

import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);

const results = [];
const skipped = [];
/** A check this platform cannot perform: reported, never counted as passed. */
function notRun(name, reason) {
  skipped.push({ name, reason });
  console.log(`N/A   ${name} — ${reason}`);
}
const log = appLogger("geometry");

/** Screenshot + page diagnostics into the artifacts, on failure only. */
async function failureEvidence(driver, suite) {
  if (!driver) return;
  try {
    const shots = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
    mkdirSync(shots, { recursive: true });
    writeFileSync(join(shots, `zz-${suite}-failure.png`), await driver.screenshot());
  } catch {
    /* the page may be gone */
  }
  await driver.diagnose(`${suite} failure`, log);
}
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}

// One data dir for both launches. With --data-dir the host keeps window.json
// there (not in the user's profile), so this works and stays isolated on every OS.
const data = mkdtempSync(join(tmpdir(), "axiom-geom-data-"));

const TARGET = { width: 1000, height: 700, x: 40, y: 40 };

async function launch() {
  const wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, log);
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

  // The minimum size (720x480, tauri.conf.json) bounds the *user's* resize.
  // A WebDriver resize is programmatic: X11 window managers apply the size
  // hints to it, so Linux can test it. Win32 enforces the minimum through
  // WM_GETMINMAXINFO, which bounds dragging (learn.microsoft.com,
  // "minimum tracking size"); AppKit's contentMinSize is documented to bound
  // setContentSize, yet tao applies it asynchronously. Neither is observable
  // here without asserting on our own config, so off Linux the check is not
  // run and the drag is in the manual protocol.
  await wd.cmd("POST", "/window/rect", { width: 400, height: 300, x: 40, y: 40 });
  await new Promise((r) => setTimeout(r, 300));
  const tiny = await wd.cmd("GET", "/window/rect");
  if (process.platform === "linux") {
    check("2 the window refuses to go below its minimum size (720x480)", tiny.width >= 700 && tiny.height >= 460, `${tiny.width}x${tiny.height}`);
  } else {
    notRun("2 the window refuses to go below its minimum size", `a programmatic resize gave ${tiny.width}x${tiny.height}; the user-drag minimum is checked by hand (agents/MANUAL_VERIFICATION.md)`);
  }

  // Restore the reference size, and remember what the OS actually gave us:
  // the restore must reproduce *that*, whatever the title bar takes.
  await wd.cmd("POST", "/window/rect", TARGET);
  await new Promise((r) => setTimeout(r, 300));
  const beforeQuit = await wd.cmd("GET", "/window/rect");

  // Quit: app_quit saves the geometry.
  // The app exits while answering, so the script's own result never arrives.
  const tQuit = Date.now();
  await wd.execAsync(`const cb = arguments[arguments.length-1]; window.__axiomHost.quit().then(() => cb(true))`).catch(() => {});
  // The same budget as the quit suite's own exit check: one operation, one
  // number. (Windows CI took longer than 10 s here once - the app had already
  // saved the geometry, so it was exiting, not hung: run 43146126579.)
  const exited = await wd.waitForExit(15000);
  const code = wd.exitCode;
  check(
    "3 the app exits when asked" + (code === undefined ? "" : " cleanly"),
    exited && (code === undefined || code === 0),
    `exited=${exited} after ${Date.now() - tQuit} ms; code=${!exited ? "still running" : code === undefined ? "not observable (WebKitWebDriver launched the app)" : code}`,
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
    Math.abs(restored.width - beforeQuit.width) <= 2 && Math.abs(restored.height - beforeQuit.height) <= 2,
    `${restored.width}x${restored.height} (before quit: ${beforeQuit.width}x${beforeQuit.height})`,
  );

  // ---- splitters (M1.6-SHELL-002): keyboard resize and double-click reset ----------
  const SPLIT = '[aria-label="Resize side bar"]';
  const now = () => wd.exec(`return Number(document.querySelector('${SPLIT}').getAttribute('aria-valuenow'))`);
  const width = () => wd.exec(`return Math.round(document.querySelector('.sidebar').getBoundingClientRect().width)`);
  await wd.waitFor(`return !!document.querySelector('${SPLIT}')`, [], 10000, "the side bar splitter");
  await wd.exec(`document.querySelector('${SPLIT}').focus()`);
  await wd.keys([Keys.HOME]);
  await wd.waitFor(`return document.querySelector('${SPLIT}').getAttribute('aria-valuenow') === '252'`, [], 3000, "Home resets to 252");
  await wd.keys(["\uE014"]); // ArrowRight
  const afterArrow = await now();
  await wd.chord(Keys.SHIFT, "\uE014");
  const afterShift = await now();
  await wd.keys(["\uE012"]); // ArrowLeft
  const afterLeft = await now();
  const shown = await width();
  check(
    "5 splitter keyboard resize (arrows ±16, Shift ±48, Home = default)",
    afterArrow === 268 && afterShift === 316 && afterLeft === 300 && shown === 300,
    `Home 252 → → ${afterArrow} → Shift+→ ${afterShift} → ← ${afterLeft}; side bar ${shown}px`,
  );
  await wd.exec(`document.querySelector('${SPLIT}').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))`);
  await wd.waitFor(`return document.querySelector('${SPLIT}').getAttribute('aria-valuenow') === '252'`, [], 3000, "double-click reset").catch(() => {});
  const afterDbl = await now();
  check("6 splitter double-click reset", afterDbl === 252 && (await width()) === 252, `aria-valuenow ${afterDbl}`);
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
  await failureEvidence(wd, "geometry");
} finally {
  try {
    await wd?.quit();
  } catch {
    /* gone */
  }
  await removeDir(data);
}

const failed = results.filter((r) => !r.ok);
console.log(
  `\n${results.length - failed.length}/${results.length} geometry checks passed` +
    (skipped.length ? `, ${skipped.length} not run on ${process.platform}` : ""),
);
process.exit(failed.length ? 1 : 0);
