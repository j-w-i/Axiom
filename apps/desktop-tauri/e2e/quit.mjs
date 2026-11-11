#!/usr/bin/env node
// Quitting with unsaved work — the data-loss path (parity row S-10).
//
// A separate suite from smoke.mjs because it ends with the application
// *exiting*: the WebDriver session dies with it, so nothing may follow.
//
// The window's close button reaches the host (Rust intercepts CloseRequested and
// asks the UI), which then calls the same `requestQuit()` exercised here — one
// code path, driven from the UI side so the assertions can see it.

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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

const data = mkdtempSync(join(tmpdir(), "axiom-quit-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-quit-ws-"));
mkdirSync(join(ws, "src"), { recursive: true });
writeFileSync(join(ws, "README.md"), "# original\n");
writeFileSync(join(ws, "src", "main.rs"), "fn main() {}\n");

const alive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

let wd;
try {
  wd = await WebDriver.start(DRIVER, APP, ["--daemon", DAEMON, "--data-dir", data]);
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected");
  const pid = await wd.execAsync(`const cb = arguments[arguments.length-1]; window.__axiomHost.appInfo().then(i => cb(i.pid))`);

  await wd.exec(`window.__axiom.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return document.querySelector('[data-path="README.md"]') !== null`, [], 10000, "tree");
  await wd.click(await wd.find('[data-path="README.md"]'));
  await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.includes('original')`, [], 20000, "editor");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.keys([..."/edited"]);
  await wd.waitFor(`return document.querySelector('[data-path="README.md"][role=tab]')?.dataset.dirty === 'true'`, [], 8000, "dirty");
  check("1 unsaved work is on screen", true);

  // ---- Cancel: the app must stay, the text must stay
  await wd.exec(`window.__axiom.documents.requestQuit()`);
  await wd.waitFor(`return document.querySelector('[data-testid=unsaved-dialog]') !== null`, [], 8000, "unsaved dialog");
  const names = await wd.exec(`return document.querySelector('[data-testid=unsaved-dialog]').textContent`);
  check("2 quitting with unsaved work asks first", names.includes("README.md"), JSON.stringify(names).slice(0, 90));
  await wd.click(await wd.find('[data-testid=unsaved-dialog] [data-action=cancel]'));
  await new Promise((r) => setTimeout(r, 400));
  const stillDirty = await wd.exec(`return document.querySelector('[data-path="README.md"][role=tab]')?.dataset.dirty`);
  check("3 Cancel keeps the window, the tab and the unsaved text", alive(pid) && stillDirty === "true", `alive=${alive(pid)} dirty=${stillDirty}`);
  check("4 Cancel wrote nothing to disk", readFileSync(join(ws, "README.md"), "utf8") === "# original\n");

  // ---- Save: the file is written, then the app exits
  await wd.exec(`window.__axiom.documents.requestQuit()`);
  await wd.waitFor(`return document.querySelector('[data-testid=unsaved-dialog]') !== null`, [], 8000, "dialog again");
  await wd.click(await wd.find('[data-testid=unsaved-dialog] [data-action=save]'));
  const deadline = Date.now() + 15000;
  let exited = false;
  while (Date.now() < deadline) {
    if (!alive(pid)) {
      exited = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 100));
  }
  check("5 Save writes the document and the app exits", exited, `app exited=${exited}`);
  const onDisk = readFileSync(join(ws, "README.md"), "utf8");
  check("6 the saved bytes are on disk", onDisk.includes("/edited"), JSON.stringify(onDisk).slice(0, 40));
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
} finally {
  try {
    await wd?.quit();
  } catch {
    /* the app is already gone */
  }
  rmSync(data, { recursive: true, force: true });
  rmSync(ws, { recursive: true, force: true });
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} quit checks passed`);
process.exit(failed.length ? 1 : 0);
