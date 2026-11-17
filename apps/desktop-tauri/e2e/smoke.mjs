#!/usr/bin/env node
// End-to-end smoke test of the real Tauri app (release build) against the real
// axiomd, driven through WebDriver. Covers the prototype's ten demonstration
// points (prototype-plan.md §Demonstrations) and writes screenshots.
//
// Usage: node e2e/runner.mjs   (starts a private X server on Linux, then this)
// Env:   AXIOM_TAURI_APP (built with --features e2e), AXIOMD_UNDER_TEST,
//        AXIOM_SCREENSHOT_DIR, AXIOM_E2E_LOG

import { findDescendantsByName, findWebContentProcesses, killNow, readProcessTable } from "./processes.mjs";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { INPUT_RECORDER, Keys, PROVIDER, TRUSTED_INPUT, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

// The app's command modifier: Cmd on macOS, Ctrl elsewhere (App.tsx IS_MAC;
// Monaco uses the same rule for Ctrl/Cmd+S and Ctrl/Cmd+Home).
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
// "Go to the start of the document": Ctrl+Home, but Cmd+Up on macOS
// (Monaco's cursorTop binding; Cmd+Home is not bound there).
const DOC_START = process.platform === "darwin" ? "\uE013" : "\uE011";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });
// The app's own stderr (the host's `[axiomd]` and `[recovery]` lines), kept in
// memory for assertions and appended to a file for CI artifacts.
const appLog = [];
const onLog = appLogger("smoke", (line) => appLog.push(line));

const results = [];
const skipped = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
/** A check that cannot be performed on this platform: reported, never counted as passed. */
function notRun(name, reason) {
  skipped.push({ name, reason });
  console.log(`N/A   ${name} — ${reason}`);
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
// Commands for checks 7a/7b run through the *user's* shell, which is `sh -c`
// on Unix and `cmd /C` on Windows (docs/roadmap/verification/M1.5.md §5b: the
// shell form is unverified on Windows). A program the workspace holds is the
// one command line both shells run identically, so the check tests the app's
// streaming and cancellation and not a shell dialect.
mkdirSync(join(ws, "tools"), { recursive: true });
writeFileSync(
  join(ws, "tools", "emit.js"),
  `process.stdout.write("out1\\n");\nprocess.stderr.write("err1\\n");\nprocess.stdout.write("out2\\n");\nsetTimeout(() => { process.stderr.write("err2\\n"); process.stdout.write("out3\\n"); process.exit(2); }, 100);\n`,
);
writeFileSync(join(ws, "tools", "slow.js"), `setTimeout(() => {}, 30000);\n`);

const S = (sel) => JSON.stringify(sel);
/** Wait-for snippet for a tab's dirty state. */
const docsTab = (path, dirty) =>
  `return document.querySelector('[data-path="${path}"][role=tab]')?.dataset.dirty === '${dirty}'`;
const appExpr = "window.__axiom";

async function launch() {
  return WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], {}, onLog);
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
  for (const pid of victims) killNow(pid);
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
    logged = appLog.some((l) => l.includes("[recovery]"));
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
    daemonAlive: findDescendantsByName(readProcessTable(), appPid, /^axiomd(\.exe)?$/i).length > 0,
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
  // Record which input path Monaco chose (EditContext vs. textarea) and the
  // platform it believes it is on: both decide how key events become text.
  {
    const env = await wd.exec(
      `const me = document.querySelector('.monaco-editor'); return { editContextApi: typeof globalThis.EditContext === 'function', input: me?.querySelector('.native-edit-context') ? 'native-edit-context' : me?.querySelector('textarea.inputarea') ? 'textarea' : 'none', ua: navigator.userAgent }`,
    );
    onLog(`ENV editor input=${env.input} EditContext=${env.editContextApi} ua=${env.ua}`);
    console.log(`INFO  editor input path: ${env.input} (EditContext API ${env.editContextApi ? "present" : "absent"})`);
    // Parity, not a detail: the editor must take the same input path on every
    // engine (src/editor/surface.ts sets editContext: false), or the Windows
    // build's typing differs from the other two and cannot be driven by tests.
    check(
      "4a' the editor uses the classic (textarea) input path on every engine",
      env.input === "textarea",
      `${env.input}${env.editContextApi ? " (EditContext API present but not used)" : ""}`,
    );
  }
  await shot(wd, "04-editor-dark");
  // 4c: the legacy token aliases (M1.6-SYSTEM-001) resolve on real elements in both
  // themes: a missing alias would leave a property empty and a component uncoloured.
  {
    const tokensCss = readFileSync(join(here, "../src/styles/tokens.css"), "utf8");
    const legacy = tokensCss.slice(tokensCss.indexOf("legacy: removed by M1.6-QA-006"));
    const aliases = [...new Set([...legacy.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]))];
    const empty = [];
    for (const theme of ["dark", "light"]) {
      await setTheme(wd, theme);
      const missing = await wd.exec(
        `const out = [];
         for (const sel of ['.shell', '.tab', '.tree-row', '.statusbar']) {
           const el = document.querySelector(sel);
           if (!el) { out.push(sel + ' (no element)'); continue; }
           const cs = getComputedStyle(el);
           for (const name of arguments[0]) if (!cs.getPropertyValue(name).trim()) out.push(sel + ' ' + name);
         }
         return out;`,
        [aliases],
      );
      empty.push(...missing.map((m) => `${theme}: ${m}`));
    }
    await setTheme(wd, "dark");
    check("4c every legacy token alias resolves on .shell, .tab, .tree-row and .statusbar (dark and light)", aliases.length > 30 && empty.length === 0, `${aliases.length} aliases; empty: ${JSON.stringify(empty.slice(0, 8))}`);
  }
  // 4d/4e: the bundled IBM Plex faces (M1.6-SYSTEM-002) are active, and Monaco
  // measured the code face after it swapped in (caret at the end of typed text).
  {
    const fonts = await wd.execAsync(
      `const cb = arguments[arguments.length - 1];
       document.fonts.ready.then(() => cb({
         ui: document.fonts.check('13px "Axiom Plex Sans"'),
         code: document.fonts.check('13px "Axiom Plex Mono"'),
         loaded: [...document.fonts].filter((f) => f.family.replace(/"/g, '').startsWith('Axiom Plex') && f.status === 'loaded').length,
         tree: getComputedStyle(document.querySelector('.tree-row')).fontFamily,
         lines: getComputedStyle(document.querySelector('.monaco-editor .view-lines')).fontFamily,
       }));`,
    );
    const starts = (f, fam) => f.replace(/"/g, "").startsWith(fam);
    check(
      "4d bundled UI and code fonts are active",
      fonts.ui && fonts.code && fonts.loaded >= 2 && starts(fonts.tree, "Axiom Plex Sans") && starts(fonts.lines, "Axiom Plex Mono"),
      JSON.stringify(fonts),
    );
    await wd.click(await wd.find(".monaco-editor .view-lines"));
    await wd.chord(MOD, DOC_START);
    await wd.keys(["\uE010", ..."abc"]); // End, then type
    await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines').textContent.includes('abc')`, [], 5000, "abc typed");
    await new Promise((r) => setTimeout(r, 200));
    const caret = await wd.exec(
      `const ed = document.querySelector('.monaco-editor');
       const lines = [...ed.querySelectorAll('.view-line')].sort((a, b) => a.getBoundingClientRect().top - b.getBoundingClientRect().top);
       const first = lines[0];
       const spans = first.querySelectorAll('span span');
       const textRight = spans.length ? spans[spans.length - 1].getBoundingClientRect().right : first.getBoundingClientRect().left;
       const cursor = ed.querySelector('.cursors-layer .cursor');
       return { textRight, caretLeft: cursor ? cursor.getBoundingClientRect().left : null, line: first.textContent };`,
    );
    check(
      "4e caret aligns after font swap",
      caret.caretLeft !== null && Math.abs(caret.caretLeft - caret.textRight) <= 1 && caret.line.endsWith("abc"),
      `caret x ${caret.caretLeft?.toFixed(1)}, end of text ${caret.textRight.toFixed(1)}, line ${JSON.stringify(caret.line)}`,
    );
    // Undo, so the later checks see the file as it was.
    await wd.chord(MOD, "z");
    await wd.waitFor(`return !document.querySelector('.monaco-editor .view-lines').textContent.includes('abc')`, [], 5000, "abc undone");
  }
  await wd.exec(INPUT_RECORDER);
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.chord(MOD, DOC_START); // Ctrl+Home / Cmd+Up
  // Count how the keystrokes reach the page: trusted (OS-level, WebKitWebDriver)
  // or synthetic (the embedded driver dispatches them by script).
  await wd.exec(`window.__keyTrust = []; document.addEventListener('keydown', (e) => window.__keyTrust.push(e.isTrusted), true)`);
  await wd.keys([..."// edited", "\uE007"]);
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.dataset.dirty === 'true'`, [], 5000, "dirty tab");
  const trust = await wd.exec(`return window.__keyTrust`);
  const allTrusted = trust.length > 0 && trust.every(Boolean);
  if (TRUSTED_INPUT) {
    check("4b typing with real (trusted) key events marks the tab dirty", allTrusted, `${trust.length} keydown events, all trusted=${allTrusted}`);
  } else {
    // The edit itself is still asserted byte-for-byte by check 5; what this
    // server cannot prove is that the events came from the OS.
    check("4b typing marks the tab dirty", true, `${trust.length} keydown events (synthetic: ${PROVIDER} driver)`);
    notRun("4b' typing with trusted key events", `the ${PROVIDER} WebDriver dispatches keys by script; covered on Linux and by the manual protocol`);
  }

  // ---- 5. save through the daemon (Ctrl+S inside Monaco) -----------------------
  // Check 4b passes on the *first* keystroke (the tab turns dirty); the whole
  // line must be in the model before saving, or check 5 compares a partial
  // edit. (On macOS the embedded driver's typing lagged behind the request.)
  await wd.waitFor(
    `return document.querySelector('.monaco-editor .view-lines')?.textContent.replace(/\u00a0/g, ' ').includes('// edited')`,
    [],
    5000,
    "the typed line is in the editor",
  );
  await wd.chord(MOD, "s");
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
  await wd.chord(MOD, "s");
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
  await wd.keys([..."node tools/emit.js", Keys.ENTER]);
  await wd.waitFor(`return document.querySelector('[data-testid=output]')?.textContent.includes('Failed with exit code 2')`, [], 20000, "task exit");
  const out = await wd.exec(`return [...document.querySelectorAll('[data-testid=output] .output__row')].map(r => (r.dataset.stream||'') + ':' + r.textContent)`);
  const okStreams = out.some((l) => l.startsWith("stdout:") && l.includes("out3")) && out.some((l) => l.startsWith("stderr:") && l.includes("err2"));
  check("7a command streams stdout and stderr distinctly with exit status", okStreams, `${out.length} rows`);
  await shot(wd, "07-output-dark");
  await wd.click(input);
  await wd.chord(MOD, "a");
  await wd.keys(["\uE017"]); // Delete: clear the previous command
  await wd.keys([..."node tools/slow.js", Keys.ENTER]);
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
  await wd.chord(MOD, "k");
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
    tabs.join(",") === "src/main.rs,README.md" && active === "README.md" && fontSize === 15 && history[0] === "node tools/slow.js",
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
  if (hostPid !== wd.pid) {
    throw new Error(`app_info reports pid ${hostPid}, but the launched process is ${wd.pid}`);
  }
  const before = contentProcessesOf(hostPid);
  if (process.platform === "darwin" && before.length === 0) {
    // WKWebView's WebContent process is a launchd-owned XPC service, not a
    // child of the app, so it cannot be told apart from another app's (see
    // tests/processes.test.ts). Killing "a" WebContent would be wrong, and
    // passing without a crash would be a lie: the check is reported as not run.
    notRun(
      "11b/11c crash recovery on macOS",
      "WebContent is not a descendant of the app; no safe way to kill only ours (the host uses the heartbeat on macOS)",
    );
  } else {
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
  }
  // The session is dead from here on (its page is gone); nothing else may run.
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
  if (wd) {
    await shot(wd, "zz-failure").catch(() => {});
    const state = await wd.diagnose("smoke failure", onLog);
    if (state) console.log(`DIAG ${JSON.stringify({ active: state.active, monacoInput: state.monacoInput, editContextApi: state.editContextApi })}`);
  }
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
  await removeDir(dataDir);
  await removeDir(ws);
}

const failed = results.filter((r) => !r.ok);
console.log(
  `\n${results.length - failed.length}/${results.length} checks passed` +
    (skipped.length ? `, ${skipped.length} not run on ${process.platform}` : "") +
    `; screenshots in ${SHOTS}`,
);
process.exit(failed.length ? 1 : 0);
