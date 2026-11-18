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
// M1.6-EDITOR-002: one keyword, one string and one comment in known places.
writeFileSync(join(ws, "src", "colors.rs"), 'fn colors() {\n    let s = "quoted";\n}\n// a comment\n');
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
  // ---- 18. editor theme from tokens and region focus (M1.6-EDITOR-002) ------------------
  {
    await wd.click(await wd.find('[data-path="src/colors.rs"]'));
    await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.includes('quoted')`, [], 10000, "colors.rs in Monaco");
    await new Promise((r) => setTimeout(r, 300)); // tokenisation is asynchronous
    const colours = await wd.exec(`
      const ctx = document.createElement('canvas').getContext('2d');
      const css = getComputedStyle(document.documentElement);
      const want = (t) => { ctx.fillStyle = css.getPropertyValue(t).trim(); const h = ctx.fillStyle; return 'rgb(' + [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16)).join(', ') + ')'; };
      const spans = [...document.querySelectorAll('.monaco-editor .view-lines span span')];
      const got = (text) => { const s = spans.find(e => e.textContent.replace(/\u00a0/g, ' ').trim() === text); return s ? getComputedStyle(s).color : null; };
      return {
        keyword: [got('fn'), want('--syn-keyword')],
        string: [got('"quoted"'), want('--syn-string')],
        comment: [got('// a comment'), want('--syn-comment')],
      };`);
    check(
      "18a editor syntax colours follow the tokens (fn keyword, string, comment)",
      Object.values(colours).every(([g, w]) => g === w),
      JSON.stringify(colours),
    );
    await wd.exec(`${appExpr}.documents.close('src/colors.rs')`);
    await wd.waitFor(`return document.querySelector('[role=tab][data-path="src/colors.rs"]') === null`, [], 5000, "colors.rs closed");

    const where = () =>
      wd.exec(
        `const a = document.activeElement; return { region: document.querySelector('.workbench')?.dataset.focusedRegion ?? null, el: a?.getAttribute('role') || a?.dataset.testid || a?.tagName.toLowerCase() }`,
      );
    await wd.chord(MOD, "0");
    await new Promise((r) => setTimeout(r, 150));
    const files = await where();
    await wd.chord(MOD, "1");
    await new Promise((r) => setTimeout(r, 150));
    const editor = await where();
    await wd.chord(MOD, "`");
    await new Promise((r) => setTimeout(r, 150));
    const consoleFocus = await where();
    check(
      "18b region focus shortcuts: Mod+0 tree, Mod+1 editor, Mod+` console prompt (focused-region rule follows)",
      files.region === "files" && files.el === "tree" && editor.region === "editor" && editor.el === "textbox" /* Monaco's input textarea */ && consoleFocus.region === "console" && consoleFocus.el === "command-input",
      JSON.stringify({ files, editor, console: consoleFocus }),
    );
    await wd.click(await wd.find(".monaco-editor .view-lines"));

    // M1.6-INTERACTION-002: Monaco is laid out once per toggle (its box changes size once).
    await wd.exec(
      `window.__layouts = []; const ed = document.querySelector('[data-region="editor"] .monaco-editor'); let last = ed.offsetWidth + 'x' + ed.offsetHeight;` +
        `window.__layoutRO = new ResizeObserver(() => { const now = ed.offsetWidth + 'x' + ed.offsetHeight; if (now !== last) { window.__layouts.push(now); last = now; } }); window.__layoutRO.observe(ed);`,
    );
    const layoutsPer = [];
    for (const key of ["b", "b", "j", "j"]) {
      await wd.exec(`window.__layouts.length = 0`);
      await wd.chord(MOD, key);
      await new Promise((r) => setTimeout(r, 400));
      layoutsPer.push(await wd.exec(`return window.__layouts.length`));
    }
    await wd.exec(`window.__layoutRO.disconnect()`);
    check("18c Monaco is laid out once per side bar / console toggle (hide, show, hide, show)", layoutsPer.every((n) => n === 1), JSON.stringify(layoutsPer));
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
       // Monaco draws a 2 px line caret 1 px left of the glyph boundary and makes it up with
       // padding-left: 1px ("shift the cursor a bit between the characters", viewCursor.js).
       // The caret's position is its content edge, not its border box (D-X-1).
       const caretLeft = cursor ? cursor.getBoundingClientRect().left + parseFloat(getComputedStyle(cursor).paddingLeft) : null;
       return { textRight, caretLeft, line: first.textContent };`,
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
  // 4f: the region holding keyboard focus marks its header with the 2 px brass rule (SHELL-002).
  {
    const regions = [];
    for (const [region, focus, header] of [
      ["files", `document.querySelector('[data-testid=explorer]').focus()`, `[data-region="files"] > .panel__head`],
      ["editor", `document.querySelector('.monaco-editor textarea')?.focus()`, `[data-region="editor"] .tabs`],
      ["console", `document.querySelector('[data-testid=command-input]').focus()`, `[data-region="console"] .panel__head`],
    ]) {
      await wd.exec(focus);
      await new Promise((r) => setTimeout(r, 100));
      regions.push(
        await wd.exec(
          `const wb = document.querySelector('.workbench'); const h = document.querySelector(arguments[0]);
           return { want: arguments[1], got: wb.dataset.focusedRegion ?? null, rule: h ? /0px 2px 0px[^,]*inset|inset 0px 2px 0px/.test(getComputedStyle(h).boxShadow) : false, shadow: h ? getComputedStyle(h).boxShadow : null };`,
          [header, region],
        ),
      );
    }
    check("4f the focused region's header carries the brass rule (files, editor, console)", regions.every((r) => r.got === r.want && r.rule), JSON.stringify(regions));
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
  const ROW_DIRTY = `const r = document.querySelector('.tree-row[data-path="src/main.rs"]'); return r ? { dot: !!r.querySelector('.tree-row__dot'), name: r.getAttribute('aria-label') } : null;`;
  const dirtyRow = await wd.waitFor(`const v = (() => { ${ROW_DIRTY} })(); return v && v.dot ? v : null`, [], 3000, "unsaved dot").catch(() => null);
  await wd.chord(MOD, "s");
  await wd.waitFor(`return document.querySelector('[data-path="src/main.rs"][role=tab]')?.dataset.dirty === 'false'`, [], 5000, "clean after Ctrl+S");
  const savedRow = await wd.waitFor(`const v = (() => { ${ROW_DIRTY} })(); return v && !v.dot ? v : null`, [], 3000, "dot gone").catch(() => null);
  check(
    "5b explorer shows unsaved marker for dirty document (and drops it on save)",
    dirtyRow?.dot === true && dirtyRow.name === "main.rs, unsaved" && savedRow?.dot === false && savedRow.name === null,
    JSON.stringify({ dirtyRow, savedRow }),
  );
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

  // ---- 15. hint line (M1.6-SHELL-005): follows hover and focus; every shell control has a hint.
  // Here a command has run, so Rerun is enabled (a disabled button cannot take focus).
  {
    const HINT = `const h = document.querySelector('.sb__hint'); return { text: h.childNodes[0]?.textContent ?? '', keys: [...h.querySelectorAll('.kbd')].map((k) => k.textContent) };`;
    const rerun = `document.querySelector('[data-testid=transport] [data-hint^="Rerun"]')`;
    await wd.exec(`${rerun}.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))`);
    const hovered = await wd.exec(HINT);
    await wd.exec(`document.body.dispatchEvent(new PointerEvent('pointerover', { bubbles: true }))`);
    await wd.exec(`${rerun}.focus()`);
    const focused = await wd.exec(HINT);
    await wd.exec(`document.querySelector('.monaco-editor textarea').focus()`);
    const editor = await wd.exec(HINT);
    check(
      "15 hint line follows hover and focus",
      hovered.text === "Rerun the last command" && hovered.keys.join() === "F5" && focused.text === "Rerun the last command" && focused.keys.join() === "F5" && editor.text === "" && editor.keys.length === 0,
      JSON.stringify({ hovered, focused, editor }),
    );
    const missing = await wd.exec(
      `const scopes = document.querySelectorAll('.titlerow, .statusbar, .panel__tools, .tabs, [data-region=console], .workbench > [role=separator], .main > [role=separator]');
       const out = [];
       for (const scope of scopes) {
         const els = scope.matches('[role=separator]') ? [scope] : scope.querySelectorAll('button, input, [role=tab], [role=button], [role=separator]');
         for (const el of els) if (!el.getAttribute('data-hint')) out.push(el.tagName.toLowerCase() + '.' + String(el.className).split(' ')[0] + ' ' + (el.getAttribute('aria-label') ?? el.textContent.trim().slice(0, 20)));
       }
       return out;`,
    );
    check("15b every shell control has a hint (title row, status bar, panel tools, tabs, console, splitters)", missing.length === 0, JSON.stringify(missing));
  }
  // ---- 12. overlays (M1.6-SYSTEM-004): tooltip on focus, Escape and focus return, toasts --
  {
    // 12a: a focused icon button shows its tooltip, which repeats its accessible name.
    const tip = await wd.execAsync(
      `const cb = arguments[arguments.length - 1];
       const b = document.querySelector('button[aria-label^="Toggle panel"]');
       b.focus();
       setTimeout(() => cb({ name: b.getAttribute('aria-label'), title: b.getAttribute('title'), tip: document.querySelector('.tooltip')?.textContent ?? null }), 900);`,
    );
    check("12a icon button focus shows its tooltip with the name (and keeps the name as aria-label)", !!tip.tip && tip.tip.includes("Toggle panel") && tip.title === null, JSON.stringify(tip));
    await wd.exec(`document.activeElement?.blur()`);

    // 12b: Escape closes a menu and returns focus to its trigger.
    await wd.click(await wd.find('button[aria-label="File menu"]'));
    await wd.waitFor(`return document.querySelectorAll('.menu__item').length > 0`, [], 5000, "File menu open");
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return document.querySelectorAll('.menu__item').length === 0`, [], 5000, "File menu closed");
    const menuFocus = await wd.exec(`return document.activeElement?.getAttribute('aria-label')`);
    check("12b Escape closes the menu and focus returns to its trigger", menuFocus === "File menu", `focus on ${JSON.stringify(menuFocus)}`);

    // 12c: the output context menu: focus on its first item, arrows move, Escape closes and restores focus.
    await wd.exec(`document.querySelector('[data-testid=command-input]').focus()`);
    await wd.exec(
      `const el = document.querySelector('[data-testid=output]'); const r = el.getBoundingClientRect();
       el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: Math.round(r.left + 120), clientY: Math.round(r.top + 30) }));`,
    );
    await wd.waitFor(`return document.querySelector('[data-testid=output-menu]') !== null`, [], 5000, "context menu");
    const first = await wd.exec(`return document.activeElement?.textContent`);
    await wd.keys(["\uE015"]); // ArrowDown
    const second = await wd.exec(`return document.activeElement?.textContent`);
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return document.querySelector('[data-testid=output-menu]') === null`, [], 5000, "context menu closed");
    const back = await wd.exec(`return document.activeElement?.dataset.testid`);
    check(
      "12c context menu: arrows move, Escape closes, focus returns",
      first === "Copy selection" && second === "Copy all" && back === "command-input",
      `first=${JSON.stringify(first)} second=${JSON.stringify(second)} focus back on ${JSON.stringify(back)}`,
    );

    // 12d: toasts announce as status (ok) / alert (error) and never move focus.
    await wd.exec(`document.querySelector('[data-testid=command-input]').focus()`);
    await wd.exec(`window.__axiomToast({ tone: 'ok', text: 'smoke toast ok' }); window.__axiomToast({ tone: 'err', text: 'smoke toast err' });`);
    const toasts = await wd.waitFor(
      `const s = document.querySelector('.toast[role=status]'), a = document.querySelector('.toast[role=alert]');
       return s && a ? { status: s.textContent, alert: a.textContent, focus: document.activeElement?.dataset.testid } : null;`,
      [],
      5000,
      "toasts",
    );
    check("12d toasts use role status/alert and do not take focus", toasts.status.includes("smoke toast ok") && toasts.alert.includes("smoke toast err") && toasts.focus === "command-input", JSON.stringify(toasts));
  }

  // ---- 13. motion foundation (M1.6-INTERACTION-001) --------------------------------
  {
    const RUNNING = `return document.getAnimations().filter((a) => a.playState === 'running').length`;
    // 13a: motion "off" applies without a restart, and an idle app with a task running animates nothing.
    await wd.exec(`${appExpr}.settings.setValue("appearance.motion", "off")`);
    await wd.waitFor(`return document.documentElement.dataset.motion === 'off'`, [], 5000, "data-motion off");
    await wd.exec(`${appExpr}.tasks.runArgv(["sleep", "5"])`);
    await wd.waitFor(`return ${appExpr}.tasks.getSnapshot().running && !!document.querySelector('.statusbar .spinner')`, [], 5000, "a task running");
    await new Promise((r) => setTimeout(r, 300));
    const offRunning = await wd.exec(RUNNING);
    const probe = await wd.exec(`return window.__axiomMotion ? window.__axiomMotion.running() : null`);
    check("13a motion off leaves no running animations", offRunning === 0 && (probe === null || probe === 0), `running=${offRunning}, __axiomMotion=${probe === null ? "absent (production bundle)" : probe}`);

    // 13b: standard: the spinner runs, stepped; a window blur pauses it (synthetic events: the gate listens to window blur/focus).
    // A host whose OS asks for reduced motion (the macOS and Windows CI runners) resolves "standard" to
    // "reduced" (motion-system.md §6), where loops are static: 13b and 13b' cannot run there.
    const osReduced = await wd.exec(`return matchMedia('(prefers-reduced-motion: reduce)').matches`);
    await wd.exec(`${appExpr}.settings.setValue("appearance.motion", "standard")`);
    await wd.waitFor(`return document.documentElement.dataset.motion === arguments[0]`, [osReduced ? "reduced" : "standard"], 5000, "data-motion standard (or reduced by the OS)");
    if (osReduced) {
      notRun("13b' the spinner and the LED pulse are stepped", "the OS prefers reduced motion: loops are static by design");
      notRun("13b blur pauses loops", "the OS prefers reduced motion: no loops run");
    } else {
    await wd.exec(`window.dispatchEvent(new Event('focus'))`);
    await new Promise((r) => setTimeout(r, 200));
    const stepped = await wd.exec(
      `const led = document.createElement('span'); led.className = 'led'; led.dataset.state = 'run'; document.body.appendChild(led);
       // A detached spinner: the status bar's own is static while the title row's transport is visible (SHELL-003).
       const sp = document.createElement('span'); sp.className = 'spinner'; document.body.appendChild(sp);
       const out = { spinner: getComputedStyle(sp).animationTimingFunction, led: getComputedStyle(led).animationTimingFunction };
       led.remove(); sp.remove(); return out;`,
    );
    check("13b' the spinner and the LED pulse are stepped", /steps\(8/.test(stepped.spinner) && /steps\(6/.test(stepped.led), JSON.stringify(stepped));
    const before = await wd.exec(RUNNING);
    await wd.exec(`window.dispatchEvent(new Event('blur'))`);
    await new Promise((r) => setTimeout(r, 200));
    const paused = await wd.exec(`const a = document.getAnimations(); return { total: a.length, paused: a.filter((x) => x.playState === 'paused').length, idle: document.documentElement.dataset.idle ?? null }`);
    check("13b blur pauses loops", before > 0 && paused.total > 0 && paused.paused === paused.total && paused.idle === "true", `running before blur=${before}; after: ${JSON.stringify(paused)}`);
    }
    await wd.exec(`window.dispatchEvent(new Event('focus'))`);
    await wd.exec(`${appExpr}.tasks.stop()`);
    // Left at "reduced" so step 8 can check that it survives a restart.
    await wd.exec(`${appExpr}.settings.setValue("appearance.motion", "reduced")`);
    await wd.waitFor(`return document.documentElement.dataset.motion === 'reduced'`, [], 5000, "data-motion reduced");
  }

  // ---- 14. task transport (M1.6-SHELL-003) ------------------------------------------
  {
    const T = `const t = document.querySelector('[data-testid=transport]');
      return t ? { state: t.dataset.state, label: t.querySelector('.transport__label')?.textContent, time: t.querySelector('.transport__time')?.textContent ?? null,
                   bars: t.querySelectorAll('.rate-meter__bar').length, stop: t.querySelector('button[aria-label="Stop"]')?.disabled } : null;`;
    // runArgv: not recorded in the command history (check 8 asserts on it).
    await wd.exec(`${appExpr}.tasks.runArgv(["sh", "-c", "sleep 3; echo done"])`);
    const running = await wd.waitFor(`const r = (() => { ${T} })(); return r && r.state === 'run' && r.time ? r : null`, [], 5000, "transport running");
    const later = await wd.waitFor(`const r = (() => { ${T} })(); return r && r.time && r.time !== arguments[0] ? r : null`, [running.time], 4000, "elapsed time advances").catch(() => null);
    const done = await wd.waitFor(`const r = (() => { ${T} })(); return r && r.state === 'on' ? r : null`, [], 8000, "transport success");
    await wd.exec(`${appExpr}.tasks.runArgv(["sh", "-c", "exit 3"])`);
    const failed = await wd.waitFor(`const r = (() => { ${T} })(); return r && r.state === 'err' ? r : null`, [], 8000, "transport failure");
    check(
      "14 transport shows running state and returns to idle",
      running.label === "sh -c sleep 3; echo done" && running.bars === 16 && running.stop === false && !!later && /^00:0\d$/.test(later.time) &&
        done.time === null && done.stop === true && failed.stop === true,
      JSON.stringify({ running, later, done, failed }),
    );
  }

  // ---- 9. themes ---------------------------------------------------------------
  await wd.click(await wd.find('[data-path="README.md"]'));
  await wd.waitFor(`return document.querySelector('[data-path="README.md"][role=tab]') !== null`, [], 5000, "README tab");
  {
    // M1.6-EDITOR-001: breadcrumbs show the active document's path; the lit edge sits over the active tab.
    const crumbs = () => wd.exec(`return [...document.querySelectorAll('[data-testid=breadcrumbs] .crumbs__text')].map(e => e.textContent).join('/')`);
    const edge = () =>
      wd.exec(
        `const t = document.querySelector('[role=tab][aria-selected=true]').getBoundingClientRect(); const g = document.querySelector('.tabs__glide').getBoundingClientRect(); return { dx: Math.abs(t.left - g.left), dw: Math.abs(t.width - g.width) }`,
      );
    const readme = await crumbs();
    const edgeReadme = await edge();
    await wd.click(await wd.find('[role=tab][data-path="src/main.rs"]'));
    await wd.waitFor(`return document.querySelector('[role=tab][data-path="src/main.rs"]')?.getAttribute('aria-selected') === 'true'`, [], 5000, "main.rs active");
    const main = await crumbs();
    const edgeMain = await edge();
    const current = await wd.exec(`return document.querySelector('[data-testid=breadcrumbs] [aria-current]')?.textContent`);
    await wd.click(await wd.find('[role=tab][data-path="README.md"]'));
    await wd.waitFor(`return document.querySelector('[role=tab][data-path="README.md"]')?.getAttribute('aria-selected') === 'true'`, [], 5000, "README active again");
    const near = (e) => e.dx < 0.5 && e.dw < 0.5;
    check(
      "17 breadcrumbs follow the active tab; the lit edge sits over it",
      readme === "README.md" && main === "src/main.rs" && current === "main.rs" && near(edgeReadme) && near(edgeMain),
      `README → ${readme}; main.rs → ${main} (current ${current}); edge ${JSON.stringify(edgeReadme)} ${JSON.stringify(edgeMain)}`,
    );
  }
  await setTheme(wd, "light");
  await shot(wd, "09-workbench-light");
  const editorBgLight = await wd.exec(`return getComputedStyle(document.querySelector('.monaco-editor-background')).backgroundColor`);
  await setTheme(wd, "dark");
  await shot(wd, "09-workbench-dark");
  const editorBgDark = await wd.exec(`return getComputedStyle(document.querySelector('.monaco-editor-background')).backgroundColor`);
  check("9 light and dark themes apply to shell and editor", editorBgLight !== editorBgDark, `${editorBgLight} vs ${editorBgDark}`);
  {
    // M1.6-SURFACES-003 (D3): the native title bar follows the theme setting on Windows and Linux.
    const native = async (theme) => {
      await wd.exec(`${appExpr}.settings.setValue("appearance.theme", arguments[0])`, [theme]);
      return wd.waitFor(`return document.documentElement.dataset.nativeTheme === arguments[0] ? arguments[0] : null`, [theme], 3000, `native ${theme}`).catch(() => wd.exec(`return document.documentElement.dataset.nativeTheme ?? null`));
    };
    if (process.platform === "darwin") {
      notRun("9b theme switch calls setTheme", "macOS uses the overlay title bar (SHELL-003); no call is made");
    } else {
      const seen = [await native("light"), await native("system"), await native("dark")];
      check("9b theme switch calls setTheme (native title bar: light, system, dark; no capability error)", seen.join() === "light,system,dark", seen.join());
    }
    await setTheme(wd, "dark");
  }

  // command palette screenshot (10: polish evidence)
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.exec(`document.activeElement?.blur()`);
  await wd.chord(MOD, "k");
  await wd.waitFor(`return document.activeElement?.classList.contains('palette__input')`, [], 3000, "palette input focused");
  {
    // M1.6-SURFACES-001: one selection element follows the arrows (by transform); aria-selected stays the truth.
    await wd.keys(["\uE015", "\uE015"]); // ArrowDown x2
    await new Promise((r) => setTimeout(r, 250)); // the glide's --dur-1 transition
    const glide = await wd.exec(
      `const opts = [...document.querySelectorAll('[data-testid=palette] [role=option]')]; const g = document.querySelector('.palette__glide').getBoundingClientRect(); const sel = opts.findIndex(o => o.getAttribute('aria-selected') === 'true'); const o = opts[sel].getBoundingClientRect(); return { sel, dy: Math.round(g.top - o.top), active: document.querySelector('.palette__input').getAttribute('aria-activedescendant') === opts[sel].id }`,
    );
    check("19a palette glide follows arrow keys", glide.sel === 2 && glide.dy === 0 && glide.active, JSON.stringify(glide));
  }
  await wd.keys([..."theme"]);
  await wd.waitFor(`return document.querySelector('.palette__item[aria-selected=true]')?.textContent.includes('Theme')`, [], 3000, "palette filters to the theme command");
  await new Promise((r) => setTimeout(r, 400));
  const readmeDirty = await wd.exec(`return document.querySelector('[data-path="README.md"][role=tab]')?.dataset.dirty`);
  check("10a command palette takes focus and filters (no keystrokes leak into the editor)", readmeDirty === "false", `README dirty=${readmeDirty}`);
  await shot(wd, "10-palette-dark");
  await wd.keys([Keys.ENTER]);
  await wd.waitFor(`return document.documentElement.dataset.theme === 'light'`, [], 3000, "palette command ran");
  check("10b running a palette command works (theme switched)", true);
  // WebKit writes localStorage to disk on a ~1 s timer; a quit right after a write loses it
  // (19b below). Real use rarely quits within a second of running a command.
  await new Promise((r) => setTimeout(r, 1500));
  await setTheme(wd, "dark");

  // ---- 20. settings dialog (M1.6-SURFACES-002) ------------------------------------------
  {
    await wd.click(await wd.find(".monaco-editor .view-lines"));
    await wd.chord(MOD, ",");
    await wd.waitFor(`return !!document.querySelector('[data-testid=settings] [role=tablist]')`, [], 3000, "settings open");
    await new Promise((r) => setTimeout(r, 200));
    const landing = await wd.exec(
      `const a = document.activeElement; return { section: document.querySelector('[data-testid=settings] [role=tab][aria-selected=true]')?.textContent, focus: a?.closest('[role=radiogroup]')?.getAttribute('aria-label') ?? a?.tagName, checked: a?.getAttribute('aria-checked') }`,
    );
    // Shift+Tab from the first control reaches the selected section tab; arrows move between sections.
    await wd.chord(Keys.SHIFT, "\uE004"); // Shift+Tab
    const onTab = await wd.exec(`return document.activeElement?.getAttribute('role')`);
    await wd.keys(["\uE015"]); // ArrowDown → Editor
    const editor = await wd.exec(`return { tab: document.activeElement?.textContent, fontSize: !!document.querySelector('[role=tabpanel] #font-size') }`);
    await wd.keys(["\uE015", "\uE015"]); // → Workspace → About
    const about = await wd.exec(`return document.querySelector('[role=tabpanel] .about-grid')?.textContent ?? ''`);
    await wd.keys(["\uE015"]); // wraps to Appearance
    const wrapped = await wd.exec(`return document.activeElement?.textContent`);
    check(
      "20a settings sections keyboard navigation (lands on Appearance's first control; Shift+Tab to the tabs; arrows move and wrap)",
      landing.section === "Appearance" && landing.focus === "Theme" && landing.checked === "true" && onTab === "tab" && editor.tab === "Editor" && editor.fontSize && wrapped === "Appearance",
      JSON.stringify({ landing, onTab, editor, wrapped }),
    );
    check("20b about section shows versions", /axiomd \d/.test(about) && /Tauri/.test(about) && /Webview/.test(about), about.slice(0, 100));

    // The stored setting (the OS may resolve "standard" to "reduced"; Off always wins).
    const before = await wd.exec(`return ${appExpr}.settings.getSnapshot().motion`);
    await wd.click(await wd.find('[role=radiogroup][aria-label=Motion] [role=radio]:nth-child(4)')); // "Off" (the glide is child 1)
    const off = await wd.waitFor(`return document.documentElement.dataset.motion === 'off' ? 'off' : null`, [], 3000, "motion off").catch(() => null);
    const restore = { standard: 2, reduced: 3, off: 4 }[before] ?? 2;
    await wd.click(await wd.find(`[role=radiogroup][aria-label=Motion] [role=radio]:nth-child(${restore})`));
    const back = await wd.waitFor(`return ${appExpr}.settings.getSnapshot().motion === arguments[0] && document.documentElement.dataset.motion !== 'off' ? arguments[0] : null`, [before], 3000, "motion restored").catch(() => null);
    check("20c motion setting applies immediately (data-motion follows the control)", off === "off" && back === before, `before ${before} → off ${off} → back ${back}`);

    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return document.querySelector('[data-testid=settings]') === null`, [], 3000, "settings closed");
    const returned = await wd.exec(`return !!document.activeElement?.closest('.monaco-editor')`);
    const where = await wd.exec(`const a = document.activeElement; return a ? a.tagName + '.' + a.className + ' ' + (a.getAttribute('aria-label') ?? '') : null`);
    check("20d Escape closes settings and focus returns to the editor", returned, `focus in editor: ${returned}; active ${where}`);
  }

  // ---- 21. microinteractions (M1.6-INTERACTION-003) -------------------------------------
  {
    const edge = () =>
      wd.exec(
        `const t = document.querySelector('.tabs [role=tab][aria-selected=true]').getBoundingClientRect(); const g = document.querySelector('.tabs__glide').getBoundingClientRect(); return { dx: +Math.abs(t.left - g.left).toFixed(2), dw: +Math.abs(t.width - g.width).toFixed(2) }`,
      );
    await wd.click(await wd.find(".monaco-editor .view-lines"));
    const landings = [];
    for (let i = 0; i < 4; i++) {
      await wd.chord(Keys.CTRL, "\uE004"); // Ctrl+Tab (on every OS)
      await new Promise((r) => setTimeout(r, 320)); // --dur-3 (200 ms) and a margin
      landings.push(await edge());
    }
    const rect = await wd.cmd("GET", "/window/rect");
    await wd.cmd("POST", "/window/rect", { ...rect, width: rect.width - 160 });
    await new Promise((r) => setTimeout(r, 400));
    landings.push(await edge());
    await wd.cmd("POST", "/window/rect", rect);
    await new Promise((r) => setTimeout(r, 400));
    landings.push(await edge());
    check("21a tab glide lands on the active tab (Ctrl+Tab x4, then a resize and back; ±0.5 px)", landings.every((l) => l.dx <= 0.5 && l.dw <= 0.5), JSON.stringify(landings));

    await wd.click(await wd.find('button[aria-label="File menu"]'));
    await wd.waitFor(`return !!document.querySelector('.menu[data-state=open]')`, [], 3000, "File menu open");
    await wd.keys([Keys.ESC]);
    const focusAtClose = await wd.exec(`return document.activeElement?.getAttribute('aria-label')`);
    await new Promise((r) => setTimeout(r, 300));
    const menuLeft = await wd.exec(`return document.querySelectorAll('.menu').length`);
    check("21b menu exit leaves no mounted content, and focus is back on its trigger at once", menuLeft === 0 && focusAtClose === "File menu", `menus left ${menuLeft}; focus right after Escape: ${focusAtClose}`);

    const level = await wd.exec(`return ${appExpr}.settings.getSnapshot().motion`);
    await wd.exec(`${appExpr}.settings.setValue("appearance.motion", "off")`);
    await wd.waitFor(`return document.documentElement.dataset.motion === 'off'`, [], 3000, "motion off");
    await wd.click(await wd.find(".monaco-editor .view-lines"));
    await wd.chord(MOD, "k");
    await wd.waitFor(`return !!document.querySelector('[data-testid=palette]')`, [], 3000, "palette");
    const anims = await wd.exec(`return document.getAnimations().filter(a => a.effect?.target?.closest?.('.palette, .overlay')).length`);
    await wd.keys([Keys.ESC]);
    const gone = await wd.waitFor(`return document.querySelector('[data-testid=palette]') === null ? Date.now() : null`, [], 1000, "palette gone").then(() => true, () => false);
    // __axiomMotion.running() exists only in dev/e2e-mode builds; this is the same count, minus
    // Monaco's own caret blink (the caret is the editor's, not shell motion: motion-system.md §1).
    const running = await wd.exec(`return document.getAnimations().filter(a => a.playState === 'running' && !a.effect?.target?.closest?.('.monaco-editor')).map(a => (a.animationName || a.transitionProperty) + ' on ' + (a.effect?.target?.className || '?'))`);
    await wd.exec(`${appExpr}.settings.setValue("appearance.motion", arguments[0])`, [level]);
    await wd.waitFor(`return ${appExpr}.settings.getSnapshot().motion === arguments[0] && document.documentElement.dataset.motion !== 'off'`, [level], 3000, "motion restored");
    check("21c motion off: dialogs have no animations and close at once", anims === 0 && gone && running.length === 0, `animations on palette/scrim ${anims}; closed ${gone}; running anywhere after close: ${JSON.stringify(running)}`);
  }

  // ---- 22. reorder tabs (M1.6-INTERACTION-004, D7) ------------------------------------
  {
    const order = () => wd.exec(`return [...document.querySelectorAll('.tabs [role=tab]')].map(t => t.dataset.path).join(',')`);
    const centre = (path) => wd.exec(`const b = document.querySelector('.tabs [role=tab][data-path="' + arguments[0] + '"]').getBoundingClientRect(); return { x: Math.round(b.left + b.width / 2), y: Math.round(b.top + b.height / 2) }`, [path]);
    const drag = async (path, dx, { cancel = false } = {}) => {
      const c = await centre(path);
      const steps = Array.from({ length: 10 }, (_, i) => ({ type: "pointerMove", duration: 16, x: c.x + Math.round(((i + 1) * dx) / 10), y: c.y, origin: "viewport" }));
      await wd.cmd("POST", "/actions", { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [{ type: "pointerMove", duration: 0, x: c.x, y: c.y, origin: "viewport" }, { type: "pointerDown", button: 0 }, ...steps] }] });
      if (cancel) {
        const lifted = await wd.exec(`return document.querySelector('.tabs')?.dataset.dragging === 'true'`);
        await wd.keys([Keys.ESC]);
        await wd.cmd("POST", "/actions", { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [{ type: "pointerUp", button: 0 }] }] });
        return lifted;
      }
      await wd.cmd("POST", "/actions", { actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [{ type: "pointerUp", button: 0 }] }] });
      return true;
    };
    const before = await order(); // src/main.rs,README.md
    if (!TRUSTED_INPUT) {
      // Pointer drags need trusted input; the embedded WebDriver (macOS, Windows) does not move the pointer.
      notRun("22a drag tab reorders (transforms cleared after the drop)", "pointer drags need trusted input (embedded WebDriver)");
      notRun("22b escape cancels tab drag (order unchanged, no lost or duplicated tab)", "pointer drags need trusted input (embedded WebDriver)");
      // Reach the same order by keyboard, so 22c and check 8 start from README.md first.
      await wd.exec(`document.querySelector('.tabs [role=tab][data-path="README.md"]').focus()`);
      await wd.chord(MOD, Keys.SHIFT, "\uE00E"); // Mod+Shift+PageUp
      await new Promise((r) => setTimeout(r, 300));
    } else {
    const width = await wd.exec(`return document.querySelector('.tabs [role=tab][data-path="src/main.rs"]').getBoundingClientRect().width`);
    // Frame intervals during the drag (P3): a rAF recorder in the page.
    await wd.exec(`window.__dragFrames = []; let last = performance.now(); window.__dragRec = true; const tick = (t) => { window.__dragFrames.push(t - last); last = t; if (window.__dragRec) requestAnimationFrame(tick); }; requestAnimationFrame(tick);`);
    await drag("README.md", -Math.round(width) - 16); // past the neighbour's midpoint
    await new Promise((r) => setTimeout(r, 400));
    const frames = await wd.exec(`window.__dragRec = false; const f = window.__dragFrames.slice(1).sort((a, b) => a - b); return { n: f.length, p95: +f[Math.floor(f.length * 0.95)]?.toFixed(1), max: +f[f.length - 1]?.toFixed(1), over25: f.filter(x => x > 25).length }`);
    console.log(`INFO  tab drag frame intervals: ${JSON.stringify(frames)}`);
    const dragged = await order();
    const clean = await wd.exec(`return [...document.querySelectorAll('.tabs [role=tab]')].every(t => !t.style.transform) && document.querySelector('.tabs').dataset.dragging === undefined`);
    check("22a drag tab reorders (transforms cleared after the drop)", before === "src/main.rs,README.md" && dragged === "README.md,src/main.rs" && clean, `${before} → ${dragged}; clean ${clean}`);

    const lifted = await drag("src/main.rs", Math.round(width) + 16, { cancel: true });
    await new Promise((r) => setTimeout(r, 400));
    const afterCancel = await order();
    check("22b escape cancels tab drag (order unchanged, no lost or duplicated tab)", lifted && afterCancel === "README.md,src/main.rs", `lifted ${lifted}; ${afterCancel}`);
    }

    await wd.exec(`document.querySelector('.tabs [role=tab][data-path="README.md"]').focus()`);
    await wd.chord(MOD, Keys.SHIFT, "\uE00F"); // Mod+Shift+PageDown
    await new Promise((r) => setTimeout(r, 200));
    const down = await order();
    const said = await wd.exec(`return document.querySelector('.tabs__announce')?.textContent ?? ''`);
    const focusKept = await wd.exec(`return document.activeElement?.dataset.path`);
    await wd.chord(MOD, Keys.SHIFT, "\uE00E"); // Mod+Shift+PageUp
    await new Promise((r) => setTimeout(r, 200));
    const up = await order();
    check(
      "22c ctrl+shift+pagedown/pageup moves the focused tab and announces it",
      down === "src/main.rs,README.md" && said === "README.md moved to position 2 of 2" && focusKept === "README.md" && up === "README.md,src/main.rs",
      `↓ ${down} ("${said}", focus ${focusKept}); ↑ ${up}`,
    );
    // The reordered strip (README first) is what check 8 must find after the restart.
    await new Promise((r) => setTimeout(r, 1500)); // the tab save debounce (500 ms) and WebKit's storage
  }

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
    // README.md first: the order section 22 (tab reorder, D7) left behind, restored from the tab state.
    tabs.join(",") === "README.md,src/main.rs" && active === "README.md" && fontSize === 15 && history[0] === "node tools/slow.js",
    `tabs=${tabs} active=${active} fontSize=${fontSize} history[0]=${history[0]}`,
  );
  await shot(wd, "08-restored-dark");
  const motionAfter = await wd.waitFor(
    `const m = ${appExpr}.settings.getSnapshot().motion; return m === 'reduced' && document.documentElement.dataset.motion === m ? m : null`,
    [],
    5000,
    "motion restored",
  ).catch(() => null);
  check("13c appearance.motion persists across a restart (stored by the daemon as an unknown key)", motionAfter === "reduced", `after restart: ${motionAfter}`);
  {
    // M1.6-SURFACES-001: the last command run (10b, the theme command) is first in the palette after a restart.
    await wd.click(await wd.find(".monaco-editor .view-lines"));
    await wd.chord(MOD, "k");
    await wd.waitFor(`return !!document.querySelector('[data-testid=palette] [role=option]')`, [], 3000, "palette after restart");
    const first = await wd.exec(`return document.querySelector('[data-testid=palette] [role=option]').id`);
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return document.querySelector('[data-testid=palette]') === null`, [], 3000, "palette closed");
    check("19b recent commands come first across a restart (axiom.palette.mru.v1)", first === "pal-theme", `first option ${first}`);
  }

  // ---- 16. non-content states (M1.6-SHELL-007) -----------------------------------
  {
    await wd.exec(`${appExpr}.documents.requestCloseFolder()`);
    await wd.waitFor(`return document.querySelector('[data-testid=welcome] .welcome__item') !== null`, [], 8000, "Welcome with a recent folder");
    await new Promise((r) => setTimeout(r, 600)); // let presence transitions finish
    const anims = await wd.exec(
      `return document.getAnimations().filter(a => a.playState === 'running').map(a => (a.animationName || a.transitionProperty || 'animation') + ' on ' + (a.effect?.target?.className || a.effect?.target?.tagName || '?'))`,
    );
    check("16a Welcome and the connected status bar run no animation", anims.length === 0, anims.length ? JSON.stringify(anims) : "document.getAnimations(): none running");

    const empty = mkdtempSync(join(tmpdir(), "axiom-tauri-e2e-empty-"));
    try {
      await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [empty]);
      await wd.waitFor(`return document.querySelector('[data-testid=tree-empty]') !== null`, [], 8000, "empty-folder state");
      const state = await wd.exec(
        `const e = document.querySelector('[data-testid=tree-empty]'); return { text: e.querySelector('.tree-state__text')?.textContent, buttons: [...e.querySelectorAll('button')].map(b => b.textContent.trim()) }`,
      );
      writeFileSync(join(empty, "new.txt"), "x");
      await wd.click(await wd.find("[data-testid=tree-empty] button"));
      const listed = await wd
        .waitFor(`return document.querySelector('[data-path="new.txt"]') !== null`, [], 8000, "the new file after Refresh")
        .then(() => true, () => false);
      check(
        "16b empty folder state shows Refresh, and Refresh lists the folder again",
        state.text === "This folder is empty" && state.buttons.join() === "Refresh" && listed,
        `${JSON.stringify(state)}; new file listed=${listed}`,
      );

      await wd.exec(`${appExpr}.documents.requestCloseFolder()`);
      // By name: the row's title is the canonical root (/private/var/… on macOS, long names on Windows).
      const wsName = ws.split(/[\\/]/).pop();
      await wd.waitFor(`const b = [...document.querySelectorAll('.welcome__item')].find(b => b.querySelector('.welcome__item-name')?.textContent === arguments[0]); if (b) b.dataset.smokeTarget = ''; return !!b`, [wsName], 8000, "the workspace in Recent");
      await wd.click(await wd.find(`.welcome__item[data-smoke-target]`));
      await wd.waitFor(`return document.querySelector('[data-path="src"]') !== null`, [], 8000, "workspace reopened from Recent");
      await wd.waitFor(`return document.querySelectorAll('[role=tab]').length === 2`, [], 8000, "its tabs restored");
      check("16c a Welcome recent row opens its workspace", true, ws);
    } finally {
      await removeDir(empty);
    }
  }

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
