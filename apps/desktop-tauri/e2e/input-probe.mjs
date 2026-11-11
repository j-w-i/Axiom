#!/usr/bin/env node
// Measures which kinds of keyboard input the current WebDriver server delivers
// faithfully to the real app. The end-to-end suites use the answer to decide
// which checks are meaningful on that server: an embedded (script-dispatched)
// server cannot prove "real key events reach Monaco" the way WebKitWebDriver
// does on Linux. Run through the runner: node e2e/runner.mjs e2e/input-probe.mjs

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver, appLogger } from "./webdriver.mjs";

const repo = resolve(import.meta.dirname, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;

const data = mkdtempSync(join(tmpdir(), "axiom-probe-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-probe-ws-"));
mkdirSync(join(ws, "src"), { recursive: true });
writeFileSync(join(ws, "a.txt"), "line one\n");

const results = {};
let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, appLogger("probe"));
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected");
  await wd.exec(`window.__axiom.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return document.querySelector('[data-path="a.txt"]') !== null`, [], 10000, "tree");
  await wd.click(await wd.find('[data-path="a.txt"]'));
  await wd.waitFor(`return document.querySelector('[data-path="a.txt"][role=tab]') !== null`, [], 10000, "tab");
  await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.replace(/\u00a0/g, ' ').includes('line one')`, [], 20000, "editor");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.chord(MOD, "\uE011"); // Ctrl/Cmd+Home

  // Monaco renders spaces as U+00A0; compare the text as typed.
  const text = () =>
    wd.exec(`return document.querySelector('.monaco-editor .view-lines').textContent.replace(/\u00a0/g, ' ')`);

  // Record how the keystrokes arrive: trusted or synthetic events, and
  // whether Monaco's own input path (its hidden textarea's input events) ran.
  await wd.exec(`
    window.__probe = { keydownTrusted: [], inputTrusted: [], beforeinput: 0 };
    const ta = document.querySelector('.monaco-editor textarea');
    document.addEventListener('keydown', (e) => window.__probe.keydownTrusted.push(e.isTrusted), true);
    ta.addEventListener('input', (e) => window.__probe.inputTrusted.push(e.isTrusted), true);
    ta.addEventListener('beforeinput', () => window.__probe.beforeinput++, true);
  `);
  await wd.keys([..."abc"]);
  await new Promise((r) => setTimeout(r, 300));
  results.monacoPrintable = (await text()).startsWith("abcline one");
  results.events = await wd.exec(`return window.__probe`);

  await wd.keys(["\uE003"]); // Backspace
  await new Promise((r) => setTimeout(r, 300));
  results.monacoBackspace = (await text()).startsWith("abline one");

  await wd.keys([Keys.ENTER]);
  await new Promise((r) => setTimeout(r, 300));
  results.monacoEnter = await wd.exec(
    `return document.querySelectorAll('.monaco-editor .view-line').length >= 3`,
  );

  // Typing with the caret in the middle of a line (as after a mouse click),
  // not at the start of the document.
  await wd.chord(MOD, "\uE010"); // Ctrl/Cmd+End
  await wd.keys(["\uE013", "\uE010"]); // Up, End: end of the "line one" line
  await wd.keys(["\uE012", "\uE012", "\uE012"]); // Left x3: inside "one"
  await wd.keys([..."XYZ"]);
  await new Promise((r) => setTimeout(r, 300));
  results.midLineText = await text();
  results.monacoMidLine = results.midLineText.includes("XYZone") || results.midLineText.includes("lXYZine");

  // A longer run, then read it back: does every keystroke land in order?
  await wd.chord(MOD, "\uE010");
  await wd.keys([..."// unsaved-crash-marker"]);
  await new Promise((r) => setTimeout(r, 500));
  results.longRun = await text();
  results.monacoLongRun = results.longRun.includes("// unsaved-crash-marker");

  // Which printable characters survive: the whole set typed into a fresh
  // file, saved, and compared with the bytes on disk (Monaco virtualises its
  // DOM, so the DOM is not evidence).
  const alphabet = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_=+[]{};:'\",./?\\|~!@#$%^&*() ";
  writeFileSync(join(ws, "chars.txt"), "");
  await wd.waitFor(`return document.querySelector('[data-path="chars.txt"]') !== null`, [], 10000, "chars.txt listed");
  await wd.click(await wd.find('[data-path="chars.txt"]'));
  await wd.waitFor(`return document.querySelector('[data-path="chars.txt"][role=tab][aria-selected=true]') !== null`, [], 10000, "chars.txt active");
  await new Promise((r) => setTimeout(r, 500));
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  // One actions request for the whole set: the way the suites type.
  // (tauri-plugin-wdio-webdriver 1.5.1 sent keyCode = char code, so `p` (112)
  // arrived as F1 and opened Monaco's command palette mid-typing: fixed in
  // 2.0.0-next.0, upstream PR #828. This probe is what found it.)
  await wd.keys([...alphabet]);
  await new Promise((r) => setTimeout(r, 500));
  await wd.chord(MOD, "s");
  await wd.waitFor(`return document.querySelector('[data-path="chars.txt"][role=tab]')?.dataset.dirty === 'false'`, [], 5000, "chars.txt saved");
  const saved = readFileSync(join(ws, "chars.txt"), "utf8");
  results.alphabetSaved = saved;
  results.alphabetExact = saved === alphabet;

  // The same run, one key per request with a pause: a race between
  // keystrokes would disappear; a mapping problem would not.
  await wd.keys([Keys.ENTER]);
  for (const ch of "unsaved-crash-marker") {
    await wd.keys([ch]);
    await new Promise((r) => setTimeout(r, 40));
  }
  await new Promise((r) => setTimeout(r, 300));
  results.pacedRun = (await text()).split("\n").pop();
  results.monacoPacedRun = (await text()).includes("unsaved-crash-marker");

  // A shortcut handled by the app (not Monaco): Ctrl/Cmd+K opens the palette.
  await wd.chord(MOD, "k");
  await new Promise((r) => setTimeout(r, 300));
  results.appShortcut = await wd.exec(`return document.querySelector('[role=dialog] input') !== null`);
  if (results.appShortcut) {
    await wd.keys([..."zz"]);
    await new Promise((r) => setTimeout(r, 200));
    results.plainInput = await wd.exec(`return document.querySelector('[role=dialog] input')?.value === 'zz'`);
    await wd.keys([Keys.ESC]);
  }

  // A shortcut Monaco handles itself: Ctrl/Cmd+S saves the *active* document.
  await new Promise((r) => setTimeout(r, 300));
  await wd.click(await wd.find('[data-path="a.txt"][role=tab]'));
  await wd.waitFor(`return document.querySelector('[data-path="a.txt"][role=tab][aria-selected=true]') !== null`, [], 5000, "a.txt active");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.chord(MOD, "s");
  await new Promise((r) => setTimeout(r, 800));
  results.monacoSaveShortcut = await wd.exec(
    `return document.querySelector('[data-path="a.txt"][role=tab]')?.dataset.dirty === 'false'`,
  );
} catch (e) {
  results.error = e instanceof Error ? e.message : String(e);
  try {
    const shots = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
    mkdirSync(shots, { recursive: true });
    writeFileSync(join(shots, "probe-failure.png"), await wd.screenshot());
    results.dom = await wd.exec(`return document.querySelector('main, #root')?.innerText.slice(0, 300)`);
  } catch {
    /* best effort */
  }
} finally {
  await wd?.quit();
  rmSync(data, { recursive: true, force: true });
  rmSync(ws, { recursive: true, force: true });
}
console.log(JSON.stringify({ platform: process.platform, ...results }, null, 2));
