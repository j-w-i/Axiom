#!/usr/bin/env node
// The split editor (M4-UX-002), through the real app: Mod+\ shows the active document in a right
// group (a second editor over the same buffer); typing in either side is the one document;
// Mod+S on the right saves; Mod+2 / Mod+1 move between the groups; the splitter answers the keys
// the other splitters do; the split comes back after a restart; closing the right group's last tab
// ends it.
// Usage: scripts/e2e.sh split.mjs

import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const onLog = appLogger("split");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";

const data = mkdtempSync(join(tmpdir(), "axiom-split-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-split-ws-"));
writeFileSync(join(ws, "a.txt"), "alpha\n");
writeFileSync(join(ws, "b.txt"), "beta\n");

const model = (wd, path) => wd.exec(`return window.__axiomEditor.store.model(arguments[0])?.getValue() ?? null`, [path]);
const focusIn = (wd) => wd.exec(`const a = document.activeElement; return a?.closest('[data-testid=editor-right]') ? 'right' : a?.closest('[data-testid=editor]') ? 'left' : a?.tagName ?? null`);

async function open(wd, path) {
  await wd.exec(`${appExpr}.documents.open(arguments[0])`, [path]);
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === arguments[0] && !!window.__axiomEditor?.store.model(arguments[0])`, [path], 15000, `${path} open`);
}

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="a.txt"]')`, [], 10000, "the workspace");
  await open(wd, "b.txt");
  await open(wd, "a.txt");

  // 1. Mod+\ splits: the right group shows a.txt in a second editor.
  await wd.exec(`window.__axiomEditor.focus()`);
  await wd.chord(MOD, "\\");
  await wd.waitFor(`return !!document.querySelector('[data-testid=editor-right] .monaco-editor') && ${appExpr}.documents.getSnapshot().split?.active === 'a.txt'`, [], 10000, "the right group");
  const tabs = await wd.exec(`return [...document.querySelectorAll('[role=tablist][aria-label="Right editor group"] [role=tab]')].map(t => t.dataset.path)`);
  check("1 Mod+\\ shows the active document in a right group with its own editor", tabs.join() === "a.txt", JSON.stringify(tabs));
  writeFileSync(join(SHOTS, "split.png"), await wd.screenshot());

  // 2. Mod+2 focuses the right group; typing there is the same buffer as on the left; Mod+S saves.
  await wd.chord(MOD, "2");
  await new Promise((r) => setTimeout(r, 150));
  const right = await focusIn(wd);
  await wd.keys("R");
  await wd.waitFor(`return (window.__axiomEditor.store.model('a.txt')?.getValue() ?? '').includes('R')`, [], 5000, "the edit");
  const leftView = await wd.exec(`return window.__axiomEditor.editor.getModel()?.getValue() ?? ''`);
  await wd.chord(MOD, "s");
  await wd.waitFor(`return !${appExpr}.documents.getSnapshot().docs.find(d => d.path === 'a.txt').dirty`, [], 10000, "saved");
  const onDisk = readFileSync(join(ws, "a.txt"), "utf8");
  check("2 Mod+2 focuses the right editor; its edit is the document's (the left shows it too); Mod+S there saves", right === "right" && leftView.includes("R") && onDisk.includes("R"), JSON.stringify({ right, onDisk }));

  // 3. Mod+1 back to the left; a different document there while the right keeps a.txt.
  await wd.chord(MOD, "1");
  await new Promise((r) => setTimeout(r, 150));
  const left = await focusIn(wd);
  await open(wd, "b.txt");
  const rightShows = await wd.exec(`return document.querySelector('[data-testid=editor-right] .view-lines')?.textContent.replace(/\\u00a0/g, ' ')`);
  check("3 Mod+1 back to the left editor; the left can show another document while the right keeps its own", left === "left" && /alpha/.test(rightShows ?? ""), JSON.stringify({ left, rightShows }));

  // 4. The splitter answers ←/→ (±16) and Home (half) like the other splitters.
  const sep = '[role=separator][aria-label="Resize the editor groups"]';
  const v0 = Number(await wd.exec(`return document.querySelector(arguments[0]).getAttribute('aria-valuenow')`, [sep]));
  await wd.exec(`document.querySelector(arguments[0]).focus()`, [sep]);
  await wd.keys(Keys.RIGHT);
  const v1 = Number(await wd.exec(`return document.querySelector(arguments[0]).getAttribute('aria-valuenow')`, [sep]));
  await wd.keys(Keys.HOME);
  const v2 = Number(await wd.exec(`return document.querySelector(arguments[0]).getAttribute('aria-valuenow')`, [sep]));
  const w = await wd.exec(`return document.querySelector('[data-group="1"]').getBoundingClientRect().width`);
  check("4 the splitter: → widens the left group by 16; Home resets it", v1 === v0 + 16 && v2 === v0 && Math.abs(w - v2) <= 1, JSON.stringify({ v0, v1, v2, w }));

  // 5. After a restart the split is back.
  await wd.exec(`${appExpr}.documents.requestQuit()`);
  await wd.waitForExit(15000).catch(() => {});
  await wd.quit().catch(() => {});
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready && ${appExpr}.documents.getSnapshot().split?.active === 'a.txt'`, [], 20000, "the split restored");
  await wd.waitFor(`return !!document.querySelector('[data-testid=editor-right] .monaco-editor')`, [], 15000, "the right editor");
  check("5 the split comes back after a restart", true);

  // 6. Closing the right group's last tab ends the split; the document stays open on the left.
  await wd.exec(`document.querySelector('[aria-label="Close a.txt in the right group"]').click()`);
  await wd.waitFor(`return !${appExpr}.documents.getSnapshot().split && !document.querySelector('[data-testid=editor-right]')`, [], 5000, "the split ended");
  const still = await wd.exec(`return ${appExpr}.documents.getSnapshot().docs.map(d => d.path)`);
  check("6 closing the right group's last tab ends the split; the document stays open on the left", still.includes("a.txt"), JSON.stringify(still));
} catch (e) {
  check("split suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("split", console.log);
} finally {
  if (wd) await wd.quit().catch(() => {});
  removeDir(data);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} split checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
