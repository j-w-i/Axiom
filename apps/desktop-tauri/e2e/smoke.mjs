#!/usr/bin/env node
// End-to-end smoke test of the real Tauri app (release build) against the real
// axiomd, driven through WebDriver. Covers the prototype's ten demonstration
// points (prototype-plan.md §Demonstrations) and writes screenshots.
//
// Usage: node e2e/smoke.mjs   (after scripts/e2e.sh has started Xvfb + tauri-driver)
// Env:   AXIOM_TAURI_APP, AXIOMD_UNDER_TEST, TAURI_DRIVER_URL, AXIOM_SCREENSHOT_DIR

import { findDescendantsByName, findWebContentProcesses, readProcessTable } from "./processes.mjs";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, "target/release/axiom-tauri");
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, "target/debug/axiomd");
const DRIVER = process.env.TAURI_DRIVER_URL ?? "http://127.0.0.1:4444";
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}

const dataDir = mkdtempSync(join(tmpdir(), "axiom-tauri-e2e-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-tauri-e2e-ws-"));
mkdirSync(join(ws, "src", "nested"), { recursive: true });
writeFileSync(join(ws, "src", "main.rs"), 'fn main() {\n    println!("hello");\n}\n');
writeFileSync(join(ws, "src", "nested", "deep.ts"), "export const x: number = 1;\n");
writeFileSync(join(ws, "README.md"), "# Demo\n\nA small workspace.\n");
writeFileSync(join(ws, ".gitignore"), "target/\n");
mkdirSync(join(ws, "target"));
writeFileSync(join(ws, "Cargo.toml"), '[package]\nname = "demo"\nversion = "0.1.0"\n');

const S = (sel) => JSON.stringify(sel);
/** Wait-for snippet for a tab's dirty state. */
const docsTab = (path, dirty) =>
  `return document.querySelector('[data-path="${path}"][role=tab]')?.dataset.dirty === '${dirty}'`;
const appExpr = "window.__axiom";

async function launch() {
  return WebDriver.start(DRIVER, APP, ["--daemon", DAEMON, "--data-dir", dataDir]);
}

async function shot(wd, name) {
  writeFileSync(join(SHOTS, `${name}.png`), await wd.screenshot());
}

/** Pids of the app's webview content processes (portable, unit-tested). */
function contentProcessesOf(appPid) {
  return findWebContentProcesses(readProcessTable(), appPid);
}

/**
 * Kills the webview's content process — the real failure that recovery must
 * survive. The lookup is descendant-only (see e2e/processes.mjs and its tests):
 * another application's web process is never touched.
 */
function killWebContentProcess(appPid) {
  const victims = findWebContentProcesses(readProcessTable(), appPid);
  if (victims.length === 0) {
    throw new Error(`no web content process under app ${appPid}; none found in the process table`);
  }
  for (const pid of victims) process.kill(pid, "SIGKILL");
  return victims;
}

const alive = (pid) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

function readDriverLog() {
  if (!process.env.TAURI_DRIVER_LOG) return "";
  try {
    return readFileSync(process.env.TAURI_DRIVER_LOG, "utf8");
  } catch {
    return "";
  }
}

/**
 * Waits until the host has noticed the dead content process and loaded a new
 * one, and reports what it observed. The host's own `[recovery]` log line is
 * the attribution: a web process that WebKit restarts by itself would not
 * produce it.
 */
async function waitForRecovery(appPid, before, killed, ms) {
  const deadline = Date.now() + ms;
  let newPids = [];
  let logged = false;
  while (Date.now() < deadline) {
    newPids = contentProcessesOf(appPid).filter((p) => !before.includes(p) && !killed.includes(p));
    logged = readDriverLog().includes("[recovery]");
    if (newPids.length > 0 && logged) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  return {
    newPids,
    logged,
    reloaded: newPids.length > 0,
    appAlive: alive(appPid),
    // The daemon is a child of the app: it must still be *this* app's daemon,
    // not one belonging to another instance (or to a developer's terminal).
    daemonAlive: findDescendantsByName(readProcessTable(), appPid, /^axiomd/i).length > 0,
  };
}

async function setTheme(wd, theme) {
  await wd.exec(`${appExpr}.settings.setValue("appearance.theme", arguments[0])`, [theme]);
  await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [theme], 5000, `theme ${theme}`);
  await new Promise((r) => setTimeout(r, 250));
}

let wd;
try {
  // ---- 1. launch + connect ---------------------------------------------------
  const t0 = Date.now();
  wd = await launch();
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 15000, "Connected");
  const status = await wd.exec(`return document.querySelector('[data-testid=status]').textContent`);
  check("1 launch and daemon connection", /^Connected — axiomd \d/.test(status), `${status} (${Date.now() - t0} ms to connected incl. WebDriver session)`);
  await setTheme(wd, "light");
  await shot(wd, "01-welcome-light");
  await setTheme(wd, "dark");
  await shot(wd, "01-welcome-dark");

  // ---- 2. open workspace (the same store call the folder dialog result makes) --
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return document.querySelector('[data-path="src"]') !== null`, [], 8000, "root listing");
  const title = await wd.exec("return document.title");
  check("2 open a workspace", title.endsWith("— Axiom"), `title "${title}"`);
  const ignored = await wd.exec(`return document.querySelector('[data-path="target"]')?.dataset.ignored`);
  check("3a explorer flags ignored entries", ignored === "true");

  // ---- 3. explorer navigation with the keyboard -------------------------------
  await wd.click(await wd.find('[data-path="src"]'));
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"]') !== null`, [], 5000, "src expanded");
  const tree = await wd.find('[data-testid="explorer"]');
  await wd.click(tree);
  // Focus is on "src" (just clicked): ArrowDown → "nested", ArrowRight expands it.
  await wd.keys(["\uE015", "\uE014"]);
  await wd.waitFor(`return document.querySelector('[data-path="src/nested/deep.ts"]') !== null`, [], 5000, "keyboard expand of src/nested");
  check("3b explorer lazy expand by click and keyboard", true);

  // ---- 4. open + edit in Monaco ------------------------------------------------
  await wd.click(await wd.find('[data-path="src/main.rs"]'));
  await wd.waitFor(`return !!document.querySelector('.monaco-editor .view-lines') && document.querySelector('.monaco-editor .view-lines').textContent.includes('hello')`, [], 15000, "Monaco shows main.rs");
  // At least two distinct token classes means Monaco classified the content;
  // a plain-text editor would leave every span as `mtk1`. The exact count
  // depends on the tokenizer's version, so it is not asserted.
  const coloured = await wd.exec(`return new Set([...document.querySelectorAll('.monaco-editor .view-lines span span')].map(s => s.className)).size`);
  check("4a file opens in Monaco with syntax colouring", coloured >= 2, `${coloured} token classes`);
  await shot(wd, "04-editor-dark");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.chord(Keys.CTRL, "\uE011"); // Ctrl+Home
  await wd.keys([..."// edited", "\uE007"]);
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.dataset.dirty === 'true'`, [], 5000, "dirty tab");
  check("4b typing with real key events marks the tab dirty", true);

  // ---- 5. save through the daemon (Ctrl+S inside Monaco) -----------------------
  await wd.chord(Keys.CTRL, "s");
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.dataset.dirty === 'false'`, [], 5000, "clean after Ctrl+S");
  const onDisk = readFileSync(join(ws, "src", "main.rs"), "utf8");
  check("5 Ctrl+S saves through fs/writeFile; bytes on disk match", onDisk === '// edited\nfn main() {\n    println!("hello");\n}\n', JSON.stringify(onDisk.slice(0, 30)));

  // ---- 6. external modifications -------------------------------------------------
  writeFileSync(join(ws, "src", "main.rs"), "fn main() { /* external */ }\n");
  await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.includes('external')`, [], 5000, "clean doc reloads");
  check("6a clean document reloads on external change", true);
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.keys([..."x"]);
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.dataset.dirty === 'true'`, [], 5000, "dirty again");
  writeFileSync(join(ws, "src", "main.rs"), "fn main() { /* theirs */ }\n");
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.textContent.includes('changed on disk')`, [], 5000, "changed-on-disk badge");
  await wd.chord(Keys.CTRL, "s");
  await wd.waitFor(`return document.querySelector('[data-testid=conflict-dialog]') !== null`, [], 5000, "conflict dialog");
  await new Promise((r) => setTimeout(r, 400)); // let the 160 ms entry animation finish
  const dlgOpacity = await wd.exec(`return getComputedStyle(document.querySelector('[data-testid=conflict-dialog]')).opacity`);
  check("6b' conflict dialog is fully visible and focused on a button", dlgOpacity === "1" && (await wd.exec(`return document.activeElement?.closest('[data-testid=conflict-dialog]') !== null`)), `opacity ${dlgOpacity}`);
  await shot(wd, "06-conflict-dark");
  await wd.click(await wd.find('[data-testid=conflict-dialog] [data-action=overwrite]'));
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.dataset.dirty === 'false'`, [], 5000, "clean after overwrite");
  const afterOverwrite = readFileSync(join(ws, "src", "main.rs"), "utf8");
  check("6b dirty + external change → conflict dialog → Overwrite writes ours", afterOverwrite.includes("external") && afterOverwrite.includes("x") && !afterOverwrite.includes("theirs"), JSON.stringify(afterOverwrite));
  writeFileSync(join(ws, "src", "added.rs"), "\n");
  await wd.waitFor(`return document.querySelector('[data-path="src/added.rs"]') !== null`, [], 5000, "new file in explorer");
  check("6c explorer updates when a file appears on disk", true);

  // ---- 7. run a command with streamed output -----------------------------------
  const input = await wd.find("[data-testid=command-input]");
  await wd.click(input);
  await wd.keys([..."for i in 1 2 3; do echo out$i; echo err$i >&2; sleep 0.1; done; exit 2", Keys.ENTER]);
  await wd.waitFor(`return document.querySelector('[data-testid=output]')?.textContent.includes('Failed with exit code 2')`, [], 10000, "task exit");
  const out = await wd.exec(`return [...document.querySelectorAll('[data-testid=output] .output__row')].map(r => (r.dataset.stream||'') + ':' + r.textContent)`);
  const okStreams = out.some((l) => l.startsWith("stdout:") && l.includes("out3")) && out.some((l) => l.startsWith("stderr:") && l.includes("err2"));
  check("7a command streams stdout and stderr distinctly with exit status", okStreams, `${out.length} rows`);
  await shot(wd, "07-output-dark");
  await wd.click(input);
  await wd.chord(Keys.CTRL, "a");
  await wd.keys(["\uE017"]); // Delete: clear the previous command
  await wd.keys([..."sleep 30 & sleep 30; wait", Keys.ENTER]);
  await wd.waitFor(`return document.querySelector('[data-testid=stop]') !== null`, [], 5000, "running");
  await new Promise((r) => setTimeout(r, 300));
  const tStop = Date.now();
  await wd.click(await wd.find("[data-testid=stop]"));
  await wd.waitFor(`return document.querySelector('[data-testid=output]')?.textContent.includes('Cancelled')`, [], 6000, "cancelled");
  check("7b Stop cancels a running command", true, `${Date.now() - tStop} ms`);
  await wd.exec(`${appExpr}.tasks.runArgv(["no-such-program-axiom"])`);
  await wd.waitFor(`return document.querySelector('[data-testid=output]')?.textContent.includes('Could not start no-such-program-axiom')`, [], 5000, "spawn_failed");
  check("7c a non-existent program produces a structured error", true);

  // ---- 9. themes ---------------------------------------------------------------
  await wd.click(await wd.find('[data-path="README.md"]'));
  await wd.waitFor(`return document.querySelector('[data-path="README.md"][role=tab]') !== null`, [], 5000, "README tab");
  await setTheme(wd, "light");
  await shot(wd, "09-workbench-light");
  const editorBgLight = await wd.exec(`return getComputedStyle(document.querySelector('.monaco-editor-background')).backgroundColor`);
  await setTheme(wd, "dark");
  await shot(wd, "09-workbench-dark");
  const editorBgDark = await wd.exec(`return getComputedStyle(document.querySelector('.monaco-editor-background')).backgroundColor`);
  check("9 light and dark themes apply to shell and editor", editorBgLight !== editorBgDark, `${editorBgLight} vs ${editorBgDark}`);

  // command palette screenshot (10: polish evidence)
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.exec(`document.activeElement?.blur()`);
  await wd.chord(Keys.CTRL, "k");
  await wd.waitFor(`return document.activeElement?.classList.contains('palette__input')`, [], 3000, "palette input focused");
  await wd.keys([..."theme"]);
  await wd.waitFor(`return document.querySelector('.palette__item[aria-selected=true]')?.textContent.includes('Theme')`, [], 3000, "palette filters to the theme command");
  await new Promise((r) => setTimeout(r, 400));
  const readmeDirty = await wd.exec(`return document.querySelector('[data-path="README.md"][role=tab]')?.dataset.dirty`);
  check("10a command palette takes focus and filters (no keystrokes leak into the editor)", readmeDirty === "false", `README dirty=${readmeDirty}`);
  await shot(wd, "10-palette-dark");
  await wd.keys([Keys.ENTER]);
  await wd.waitFor(`return document.documentElement.dataset.theme === 'light'`, [], 3000, "palette command ran");
  check("10b running a palette command works (theme switched)", true);
  await setTheme(wd, "dark");

  // ---- 8. restore state after restart --------------------------------------------
  await wd.exec(`${appExpr}.settings.setValue("editor.fontSize", 15)`);
  await wd.waitFor(`return ${appExpr}.settings.getSnapshot().editorFontSize === 15`, [], 3000, "font size");
  // Quit through the app's own path (flushes explorer + tab state, shuts the daemon down).
  await wd.execAsync(`const done = arguments[arguments.length-1]; ${appExpr}.workspace.flushState().then(() => ${appExpr}.documents.saveTabState()).then(() => done(true))`);
  await wd.quit();
  wd = null;
  await new Promise((r) => setTimeout(r, 500));

  wd = await launch();
  await wd.waitFor(`return document.querySelector('[data-path="src/nested"]') !== null`, [], 15000, "last workspace + expanded folders restored");
  await wd.waitFor(`return document.querySelectorAll('[role=tab]').length === 2`, [], 8000, "tabs restored");
  const tabs = await wd.exec(`return [...document.querySelectorAll('[role=tab]')].map(t => t.dataset.path)`);
  const active = await wd.exec(`return document.querySelector('[role=tab][aria-selected=true]')?.dataset.path`);
  const fontSize = await wd.exec(`return ${appExpr}.settings.getSnapshot().editorFontSize`);
  const history = await wd.exec(`return ${appExpr}.tasks.getSnapshot().history`);
  check(
    "8 state restored: last workspace, expanded folders, tabs, active tab, settings, command history",
    tabs.join(",") === "src/main.rs,README.md" && active === "README.md" && fontSize === 15 && history[0]?.startsWith("sleep 30"),
    `tabs=${tabs} active=${active} fontSize=${fontSize} history[0]=${history[0]}`,
  );
  await shot(wd, "08-restored-dark");

  // ---- 11. webview crash recovery (T-08) ---------------------------------------
  // Killing the content process also kills the WebDriver session (it is bound to
  // that process), so recovery is verified from outside: the host must notice the
  // missing heartbeat, reload the webview, and a fresh content process must serve
  // the window again. That the *unsaved text* comes back from the host's snapshots
  // is verified at store level ("unsaved text comes back from the host snapshot
  // after a webview crash") — the same split as the Qt shell, whose crash test
  // drives the real renderer and whose model test covers the text.
  await wd.click(await wd.find('[data-path="README.md"]'));
  await wd.waitFor(docsTab("README.md", "false"), [], 10000, "README tab clean");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.keys([..."// unsaved-crash-marker"]);
  await wd.waitFor(docsTab("README.md", "true"), [], 10000, "README tab dirty");
  // Every keystroke must have landed: the tab is dirty after the first one.
  await wd.waitFor(
    `return document.querySelector('.monaco-editor .view-lines')?.textContent.includes("unsaved-crash-marker")`,
    [],
    10000,
    "marker rendered",
  );
  const crashBefore = await wd.exec(`return document.querySelector('.monaco-editor .view-lines').textContent`);
  check(
    "11a the unsaved marker is on screen before the crash",
    crashBefore.includes("unsaved-crash-marker"),
    JSON.stringify(crashBefore).slice(0, 120),
  );
  await shot(wd, "11a-before-crash-dark");
  const hostPid = await wd.execAsync(`const cb = arguments[arguments.length-1]; window.__axiomHost.appInfo().then(i => cb(i.pid))`);
  const before = contentProcessesOf(hostPid);
  const victims = killWebContentProcess(hostPid);
  const tCrash = Date.now();
  // The page is gone; the next calls must not be made on this session.
  const recovered = await waitForRecovery(hostPid, before, victims, 45000);
  check(
    "11b the host detects the dead webview and reloads it (new content process)",
    recovered.reloaded && recovered.logged,
    `killed ${victims.join(",")}; new content process ${recovered.newPids.join(",") || "none"} after ${Date.now() - tCrash} ms; host logged=${recovered.logged}`,
  );
  check(
    "11c the app and its daemon survive the crash",
    recovered.appAlive && recovered.daemonAlive,
    `app alive=${recovered.appAlive} daemon alive=${recovered.daemonAlive}`,
  );
  // The session is dead from here on (its page is gone); nothing else may run.
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
  if (wd) await shot(wd, "zz-failure").catch(() => {});
} finally {
  // The crash check leaves a session whose page is dead; try to release it
  // anyway so the driver is not left holding it.
  if (wd) {
    try {
      await wd.quit();
    } catch {
      /* already gone */
    }
  }
  rmSync(dataDir, { recursive: true, force: true });
  rmSync(ws, { recursive: true, force: true });
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} checks passed; screenshots in ${SHOTS}`);
process.exit(failed.length ? 1 : 0);
