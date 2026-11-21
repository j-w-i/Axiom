#!/usr/bin/env node
// The integrated terminal through the real app (M4-TERM-002; ADR-0018 §4): Mod+Shift+` shows the
// Terminal view with a shell in the workspace; typed commands run; a second terminal tab; the
// window's size reaches the shell; Mod+` (the console region) lands in the terminal; `exit`
// marks the tab ended; typing latency; accessibility; quitting leaves no shell. Command lines are
// typed with the trusted driver (Linux, WebKitWebDriver); see run() for the embedded one.
// Usage: scripts/e2e.sh terminal.mjs

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, TRUSTED_INPUT, WebDriver, appLogger, removeDir } from "./webdriver.mjs";
import { HELPERS } from "./a11y-helpers.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const onLog = appLogger("terminal");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";

/** The active terminal's screen text, one line per row (the DOM renderer's rows). */
const ROWS = `[...(document.querySelector('[data-testid=terminal] .terminal__body .xterm-rows')?.children ?? [])].map(r => r.textContent.replace(/\\s+$/, '')).join('\\n').replace(/\\n+$/, '')`;
const screen = (wd) => wd.exec(`return ${ROWS}`);
const waitScreen = (wd, re, what, ms = 15000) => wd.waitFor(`return new RegExp(arguments[0], 'm').test(${ROWS})`, [re], ms, what);
/** Types a command line and Enter. The embedded driver's synthetic key events carry no keyCode,
 * which xterm needs for spaces and capitals, so there the line goes to the active terminal's
 * input as written (the keyboard path is covered by the trusted WebKitWebDriver run). */
async function run(wd, line) {
  if (TRUSTED_INPUT) {
    await wd.keys(line);
    await wd.keys(Keys.ENTER);
  } else {
    await wd.exec(`const s = ${appExpr}.terminals.getSnapshot(); ${appExpr}.terminals.write(s.active, arguments[0] + "\\r")`, [line]);
  }
}
const alive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

const data = mkdtempSync(join(tmpdir(), "axiom-term-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-term-ws-"));
writeFileSync(join(ws, "hello.txt"), "hi\n");
let wd;
let pids = [];
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], { SHELL: "/bin/sh", PS1: "$ " }, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="hello.txt"]')`, [], 10000, "the workspace");
  await wd.click(await wd.find('[data-path="hello.txt"]'));
  // The file opens in the editor (which takes focus when it does) before the terminal is asked for.
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === 'hello.txt' && !!document.activeElement?.closest('.monaco-editor')`, [], 15000, "the editor");

  // 1. Mod+Shift+` shows the Terminal view with a shell, focused, in the workspace.
  await wd.chord(MOD, Keys.SHIFT, "`");
  await wd.waitFor(`return ${appExpr}.terminals.getSnapshot().terminals.length === 1 && document.activeElement?.closest('[data-testid=terminal]')`, [], 15000, "a focused terminal");
  await waitScreen(wd, "\\$", "the prompt");
  await run(wd, "echo $((6*7)); pwd");
  await waitScreen(wd, "42", "42").catch(async (e) => {
    console.log("SCREEN", JSON.stringify(await screen(wd)), JSON.stringify(await wd.exec(`return { active: document.activeElement?.className, tag: document.activeElement?.tagName, terms: ${appExpr}.terminals.getSnapshot() }`)));
    throw e;
  });
  const text = await screen(wd);
  check("1 Mod+Shift+` opens a terminal in the workspace; a typed command runs", /42/.test(text) && text.includes(ws.split("/").pop()), text.slice(-120));
  writeFileSync(join(SHOTS, "terminal-dark.png"), await wd.screenshot());

  // 2. The window's width reaches the shell.
  // Each reading has its own marker, so an older line cannot be taken for it.
  await run(wd, "echo C1=$(tput cols)");
  await waitScreen(wd, "^C1=\\d+$", "the first reading");
  const before = Number(/^C1=(\d+)$/m.exec(await screen(wd))?.[1] ?? "0");
  // Hiding the side bar widens the console (works under either driver).
  await wd.chord(MOD, "b");
  await wd.waitFor(`return !document.querySelector('.sidebar')`, [], 5000, "the side bar hidden");
  await new Promise((r) => setTimeout(r, 500));
  await run(wd, "echo C2=$(tput cols)");
  await waitScreen(wd, "^C2=\\d+$", "the second reading");
  const after = Number(/^C2=(\d+)$/m.exec(await screen(wd))?.[1] ?? "0");
  await wd.chord(MOD, "b");
  check("2 the console's width reaches the shell (side bar hidden: more columns)", before > 0 && after > before, JSON.stringify({ before, after }));

  // 3. Typing latency: keydown → the echo in the screen's DOM, measured in the page.
  await run(wd, "clear");
  const samples = [];
  for (let i = 0; i < 20; i++) {
    await wd.exec(`
      window.__lat = null;
      const rows = document.querySelector('[data-testid=terminal] .terminal__body .xterm-rows');
      let t0 = null;
      const kd = (e) => { if (t0 === null) t0 = e.timeStamp; };
      document.addEventListener('keydown', kd, { capture: true, once: true });
      const mo = new MutationObserver(() => { if (t0 !== null) { window.__lat = performance.now() - t0; mo.disconnect(); } });
      mo.observe(rows, { childList: true, subtree: true, characterData: true });`);
    await wd.keys("x");
    const lat = await wd.waitFor(`return window.__lat`, [], 5000, "the echo");
    samples.push(lat);
  }
  await wd.chord(Keys.CTRL, "u");
  samples.sort((a, b) => a - b);
  const p95 = samples[Math.floor(samples.length * 0.95) - 1];
  check("3 typing: keydown → echo painted ≤ 50 ms p95", p95 <= 50, `p50 ${samples[10].toFixed(1)} ms, p95 ${p95.toFixed(1)} ms (n=20)`);

  // 4. A second terminal; Mod+` (the console region) lands in the shown terminal.
  await wd.click(await wd.find("[data-testid=terminal-new]"));
  await wd.waitFor(`return ${appExpr}.terminals.getSnapshot().terminals.length === 2`, [], 10000, "a second terminal");
  const tabs = await wd.exec(`return [...document.querySelectorAll('[data-testid=terminal] [role=tab]')].map(t => t.textContent + ':' + t.getAttribute('aria-selected'))`);
  await wd.click(await wd.find('[data-path="hello.txt"]'));
  await wd.chord(MOD, "`");
  await new Promise((r) => setTimeout(r, 200));
  const focused = await wd.exec(`return !!document.activeElement?.closest('[data-testid=terminal] .terminal__body')`);
  check("4 a second terminal is a tab; Mod+` focuses the shown terminal", tabs.length === 2 && /2:true$/.test(tabs[1]) && focused, JSON.stringify({ tabs, focused }));

  // 5. Global chords are the app's: Mod+K opens the palette from the terminal.
  await wd.chord(MOD, "k");
  const palette = await wd.waitFor(`return !!document.querySelector('[role=dialog] input')`, [], 3000, "palette").then(() => true, () => false);
  await wd.keys(Keys.ESC);
  check("5 Mod+K from the terminal opens the palette (the shell does not get it)", palette);

  // 6. Light theme and accessibility: names, contrast of the screen text, focus.
  await wd.exec(`${appExpr}.settings.setValue("appearance.theme", "light")`);
  await wd.waitFor(`return document.documentElement.dataset.theme === 'light'`, [], 5000, "light");
  await new Promise((r) => setTimeout(r, 300));
  writeFileSync(join(SHOTS, "terminal-light.png"), await wd.screenshot());
  await wd.exec(HELPERS);
  const a11y = await wd.exec(`
    const named = [...document.querySelectorAll('[data-testid=terminal] button, [data-testid=terminal] [role=tab]')].map(e => window.__a11y.describe(e)).filter(d => !d.name);
    const ta = document.querySelector('[data-testid=terminal] .terminal__body textarea');
    // The screen's background: the nearest opaque one behind the rows, against the panel token.
    let el = document.querySelector('[data-testid=terminal] .xterm-rows');
    let bg = 'none';
    while (el) { const c = getComputedStyle(el).backgroundColor; if (c && c !== 'transparent' && !(c.startsWith('rgba') && c.endsWith(', 0)'))) { bg = c; break; } el = el.parentElement; }
    const probe = document.createElement('div'); probe.style.background = 'var(--surface-panel)'; document.body.appendChild(probe);
    const panel = getComputedStyle(probe).backgroundColor; probe.remove();
    const prompt = getComputedStyle(document.querySelector('.prompt')).display;
    return { unnamed: named.length, input: ta?.getAttribute('aria-label') ?? null, bg, panel, prompt };`);
  await wd.exec(`${appExpr}.settings.setValue("appearance.theme", "dark")`);
  check(
    "6 every control is named; the terminal's input is named; the screen is the panel colour of the light theme; no run prompt under it",
    a11y.unnamed === 0 && /^Terminal: /.test(a11y.input ?? "") && a11y.bg === a11y.panel && a11y.prompt === "none",
    JSON.stringify(a11y),
  );

  // 7. `exit` marks the tab ended.
  await wd.chord(MOD, "`");
  await run(wd, "exit");
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=terminal] [role=tab]')].some(t => /\\(ended\\)/.test(t.textContent))`, [], 10000, "an ended tab");
  check("7 `exit` marks the terminal's tab ended", true);

  // 8. Quitting the app leaves no shell.
  pids = await wd.exec(`return ${appExpr}.terminals.getSnapshot().terminals.map(t => t.pid).filter(Boolean)`);
  await wd.exec(`${appExpr}.documents.requestQuit()`);
  await wd.waitForExit(15000).catch(() => {});
  await wd.quit().catch(() => {});
  wd = null;
  const end = Date.now() + 5000;
  while (pids.some(alive) && Date.now() < end) await new Promise((r) => setTimeout(r, 100));
  check("8 quitting the app leaves no shell", pids.length > 0 && !pids.some(alive), JSON.stringify({ pids, alive: pids.filter(alive) }));
} catch (e) {
  check("terminal suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("terminal", console.log);
} finally {
  if (wd) await wd.quit().catch(() => {});
  for (const p of pids) if (alive(p)) execFileSync("kill", ["-9", String(p)]);
  removeDir(data);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} terminal checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
