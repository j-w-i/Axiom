#!/usr/bin/env node
// Language servers in the editor (M4-LSP-002; ADR-0018 §2–3), through the real app and daemon:
// 1. tests/helpers/fake-lsp, declared by a user provider for Rust files (every OS, always):
//    diagnostics as squiggles and in Problems (labelled), completion after `.`, F12 into another
//    file, F2 rename across two files (unsaved, undoable), Format Document, the status bar.
// 2. The real rust-analyzer, then typescript-language-server, where they run (CI installs them;
//    AXIOM_REQUIRE_LSP=1 makes their absence a failure): completion, a type error in Problems,
//    go to definition in another file, rename.
// Usage: scripts/e2e.sh lsp.mjs

import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, dirname, join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";
import { HELPERS } from "./a11y-helpers.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const F12 = "";
const F2 = "";
const ALT = "";
const SPACE = "";
const onLog = appLogger("lsp");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
function notRun(name, why) {
  console.log(`N/A   ${name} — ${why}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";
const fileUri = (p) => "file://" + p.split("/").map(encodeURIComponent).join("/");

const built = spawnSync("cargo", ["build", "-q", "-p", "fake-lsp", "--message-format=json"], { cwd: repo, encoding: "utf8" });
if (built.status !== 0) throw new Error("building fake-lsp failed");
const FAKE = built.stdout
  .split("\n")
  .map((l) => {
    try {
      return JSON.parse(l);
    } catch {
      return null;
    }
  })
  .find((m) => m?.target?.name === "fake-lsp" && m.executable)?.executable;

/** The text of an open document's model. */
const textOf = (wd, path) => wd.exec(`return window.__axiomEditor?.store.model(arguments[0])?.getValue() ?? null`, [path]);
const activePath = (wd) => wd.exec(`return ${appExpr}.documents.getSnapshot().activePath`);
/** Puts the caret at 1-based line/column of the active editor and focuses it. */
const caret = (wd, line, column) =>
  wd.exec(`const e = window.__axiomEditor.editor; e.setPosition({ lineNumber: arguments[0], column: arguments[1] }); e.focus();`, [line, column]);

async function openFile(wd, path) {
  await wd.exec(`${appExpr}.documents.open(arguments[0])`, [path]);
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === arguments[0] && !!window.__axiomEditor?.store.model(arguments[0])`, [path], 20000, `${path} open`);
}

async function suggestions(wd, line, column, typed) {
  await caret(wd, line, column);
  if (typed) await wd.keys(typed);
  await wd.chord(Keys.CTRL, SPACE);
  await wd.waitFor(`return document.querySelectorAll('.suggest-widget.visible .monaco-list-row').length > 0`, [], 60000, "the suggest widget");
  const labels = await wd.exec(`return [...document.querySelectorAll('.suggest-widget.visible .monaco-list-row')].map(r => r.getAttribute('aria-label') ?? r.textContent)`);
  await wd.keys(Keys.ESC);
  return labels;
}

async function rename(wd, line, column, name) {
  await caret(wd, line, column);
  await wd.keys(F2);
  await wd.waitFor(`return !!document.activeElement?.closest('.rename-box')`, [], 30000, "focus in the rename box");
  await wd.chord(MOD, "a");
  await wd.keys(name);
  await wd.keys(Keys.ENTER);
}

// ---- 1. the fake server -------------------------------------------------------------------
async function fakeServer() {
  const data = mkdtempSync(join(tmpdir(), "axiom-lsp-data-"));
  const ws = mkdtempSync(join(tmpdir(), "axiom-lsp-ws-"));
  const log = join(data, "fake-lsp.log");
  mkdirSync(join(data, "providers", "fakerust"), { recursive: true });
  writeFileSync(
    join(data, "providers", "fakerust", "provider.toml"),
    'schema = "axiom.provider/1"\nid = "fakerust"\nname = "Fake Rust"\ndescription = "Test: fake-lsp for Rust files."\nversion = "0.1.0"\n\n[project]\nany_file = ["fake.marker"]\n\n[[lsp]]\nid = "fake"\nlanguages = ["rust"]\ncommand = ["fake-lsp"]\n',
  );
  writeFileSync(join(ws, "fake.marker"), "");
  mkdirSync(join(ws, "src"));
  writeFileSync(join(ws, "src", "main.rs"), "fn helper() -> i32 { 1 }  \nfn main() {\n    let total = value();\n    ERROR\n}\n");
  writeFileSync(join(ws, "src", "other.rs"), "pub fn value() -> i32 {\n    2\n}\n");
  const env = {
    PATH: `${dirname(FAKE)}${delimiter}${process.env.PATH}`,
    FAKE_LSP_LOG: log,
    FAKE_LSP_DEFINITION: fileUri(join(ws, "src", "other.rs")),
  };
  let wd;
  try {
    wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], env, onLog);
    await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
    await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
    await wd.waitFor(`return ${appExpr}.lsp.getSnapshot().servers.some(s => s.serverId === 'fake')`, [], 15000, "the server listed");
    await openFile(wd, "src/main.rs");

    // Diagnostics: a squiggle and a Problems row from the server.
    await wd.waitFor(`return document.querySelectorAll('.monaco-editor .squiggly-error').length > 0`, [], 30000, "the squiggle");
    await wd.waitFor(`return ${appExpr}.problems.all().some(p => p.source === 'fake (fake)')`, [], 10000, "the problem");
    const p = await wd.exec(`return ${appExpr}.problems.all().find(p => p.source === 'fake (fake)')`);
    await wd.click(await wd.find("[data-view=problems]"));
    const row = await wd.exec(`return document.querySelector('[data-testid=problems] .problem')?.getAttribute('aria-label')`);
    check("1 a server diagnostic is a squiggle and a Problems row labelled with its source", p.location.line === 4 && /from fake \(fake\)/.test(row ?? ""), JSON.stringify({ at: p.location, row }));
    const status = await wd.exec(`const s = document.querySelector('[data-testid=lsp-status]'); return s && { state: s.dataset.state, text: s.textContent }`);
    check("1b the status bar shows the file's language server, ready", status?.state === "ready" && /fake/.test(status.text), JSON.stringify(status));
    writeFileSync(join(SHOTS, "lsp-fake.png"), await wd.screenshot());

    // Completion.
    const labels = await suggestions(wd, 3, 25, "");
    check("2 Ctrl+Space offers the server's completions", labels.some((l) => /alpha/.test(l)) && labels.some((l) => /beta/.test(l)), JSON.stringify(labels.slice(0, 5)));

    // Accessibility of what a server brings up: the suggest and hover widgets in both themes
    // (rendered contrast), and the status-bar segment's name.
    const a11y = [];
    await wd.exec(HELPERS);
    for (const theme of ["dark", "light"]) {
      await wd.exec(`${appExpr}.settings.setValue("appearance.theme", arguments[0])`, [theme]);
      await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [theme], 5000, theme);
      // Failures that appear when a widget opens are the widget's (the rest of the window is the
      // a11y suite's).
      const key = (f) => `${f.text}|${f.cls}|${f.region}`;
      const baseline = new Set((await wd.exec(`return window.__a11y.contrastScan()`)).map(key));
      await caret(wd, 3, 25);
      await wd.chord(Keys.CTRL, SPACE);
      await wd.waitFor(`return document.querySelectorAll('.suggest-widget.visible .monaco-list-row').length > 0`, [], 10000, "suggest");
      await new Promise((r) => setTimeout(r, 300));
      const bad = (await wd.exec(`return window.__a11y.contrastScan()`)).filter((f) => !baseline.has(key(f)));
      await wd.keys(Keys.ESC);
      await wd.exec(`window.__axiomEditor.editor.getAction('editor.action.showHover').run()`);
      await wd.waitFor(`return !!document.querySelector('.monaco-hover:not(.hidden) .hover-contents')`, [], 10000, "hover");
      await new Promise((r) => setTimeout(r, 300));
      const badHover = (await wd.exec(`return window.__a11y.contrastScan()`)).filter((f) => !baseline.has(key(f)));
      await wd.keys(Keys.ESC);
      a11y.push({ theme, suggest: bad.length, hover: badHover.length, detail: [...bad, ...badHover].map((f) => `"${f.text}" .${f.cls} ${f.ratio}:1 < ${f.need}`).slice(0, 4) });
    }
    await wd.exec(`${appExpr}.settings.setValue("appearance.theme", "dark")`);
    const named = await wd.exec(`const s = document.querySelector('[data-testid=lsp-status]'); return window.__a11y.describe(s)`);
    check("2b the suggest and hover widgets pass contrast in both themes; the status segment has a name", a11y.every((x) => x.suggest === 0 && x.hover === 0) && !!named?.name, JSON.stringify({ a11y, named }));

    // F12 into another file.
    await caret(wd, 3, 19);
    await wd.keys(F12);
    const went = await wd
      .waitFor(`return ${appExpr}.documents.getSnapshot().activePath === 'src/other.rs'`, [], 15000, "other.rs active")
      .then(() => true, () => false);
    const cursor = await wd.exec(`const p = window.__axiomEditor.editor.getPosition(); return p && 'Ln ' + p.lineNumber + ', Col ' + p.column`);
    const statusCursor = await wd
      .waitFor(`const c = ${appExpr}.documents.getSnapshot().cursorText; return /Ln/.test(c) && c`, [], 5000, "the status bar's cursor")
      .catch(() => "");
    check("3 F12 opens the definition in another file, and the status bar shows where", went && /Ln 1/.test(cursor ?? "") && statusCursor === cursor, JSON.stringify({ cursor, statusCursor, active: await activePath(wd), asked: readFileSync(log, "utf8").includes("textDocument/definition"), focus: await wd.exec(`return document.activeElement?.className`) }));
    if (!went) await openFile(wd, "src/other.rs");

    // Rename across both open files (the server renames in every open document).
    await openFile(wd, "src/main.rs");
    await rename(wd, 3, 19, "amount");
    await wd
      .waitFor(`return (window.__axiomEditor.store.model('src/other.rs')?.getValue() ?? '').includes('amount')`, [], 15000, "the rename in other.rs")
      .catch(async (e) => {
        console.log("RENAME", JSON.stringify({ asked: readFileSync(log, "utf8").includes("textDocument/rename"), main: await textOf(wd, "src/main.rs"), box: await wd.exec(`return !!document.querySelector('.rename-box')`) }));
        throw e;
      });
    const main = await textOf(wd, "src/main.rs");
    const other = await textOf(wd, "src/other.rs");
    const dirty = await wd.exec(`return ${appExpr}.documents.getSnapshot().docs.filter(d => d.dirty).map(d => d.path).sort()`);
    const onDisk = readFileSync(join(ws, "src", "other.rs"), "utf8");
    check(
      "4 F2 renames in both files; the edits are unsaved and the files on disk unchanged",
      main.includes("amount()") && other.includes("fn amount()") && dirty.join() === "src/main.rs,src/other.rs" && onDisk.includes("fn value()"),
      JSON.stringify({ dirty }),
    );
    await wd.chord(MOD, "z");
    const undone = await textOf(wd, "src/main.rs");
    check("4b the rename is undoable", undone.includes("value()"), undone.split("\n")[2]);

    // Formatting.
    await caret(wd, 1, 1);
    // Monaco's Format Document: Ctrl+Shift+I on Linux, Shift+Alt+F elsewhere.
    if (process.platform === "linux") await wd.chord(Keys.CTRL, Keys.SHIFT, "i");
    else await wd.chord(Keys.SHIFT, ALT, "f");
    await wd
      .waitFor(`return !(window.__axiomEditor.store.model('src/main.rs')?.getValue() ?? '').split('\\n')[0].endsWith(' ')`, [], 15000, "formatting")
      .catch(async (e) => {
        console.log("FORMAT", JSON.stringify({ asked: readFileSync(log, "utf8").includes("textDocument/formatting"), line: (await textOf(wd, "src/main.rs")).split("\n")[0], active: await activePath(wd), action: await wd.exec(`const a = window.__axiomEditor.editor.getAction('editor.action.formatDocument'); return a ? { supported: a.isSupported() } : null`) }));
        throw e;
      });
    check("5 Format Document (Ctrl+Shift+I on Linux, Shift+Alt+F elsewhere) formats through the server", true);

    // The daemon saw didOpen/didChange/rename/formatting (sanity of the sync).
    const got = readFileSync(log, "utf8");
    check("6 the server received the document sync and the requests", ["textDocument/didOpen", "textDocument/didChange", "textDocument/rename", "textDocument/formatting"].every((m) => got.includes(m)));
  } catch (e) {
    check("fake-server suite", false, String(e?.stack ?? e));
    if (wd) await wd.diagnose("lsp", console.log);
  } finally {
    if (wd) await wd.quit().catch(() => {});
    removeDir(data);
    removeDir(ws);
  }
}

// ---- 2. the real servers ------------------------------------------------------------------
function runs(cmd, args) {
  return spawnSync(cmd, args, { encoding: "utf8" }).status === 0;
}
const REQUIRE = process.env.AXIOM_REQUIRE_LSP === "1";

async function realServer({ name, ok, files, open, completion, error, definition, renameAt, renamedIn }) {
  if (!ok) {
    if (REQUIRE) check(`${name}: installed`, false, "AXIOM_REQUIRE_LSP=1 but it does not run");
    else notRun(name, "not installed here");
    return;
  }
  const data = mkdtempSync(join(tmpdir(), "axiom-lsp-data-"));
  const ws = mkdtempSync(join(tmpdir(), "axiom-lsp-ws-"));
  for (const [p, t] of Object.entries(files)) {
    mkdirSync(dirname(join(ws, p)), { recursive: true });
    writeFileSync(join(ws, p), t);
  }
  let wd;
  try {
    wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, onLog);
    await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
    await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
    await wd.waitFor(`return ${appExpr}.lsp.getSnapshot().servers.length > 0`, [], 15000, "the servers");
    await openFile(wd, open);
    await wd.waitFor(`return ${appExpr}.lsp.getSnapshot().servers.some(s => s.state === 'ready')`, [], 120000, `${name} ready`);
    // Diagnostics (servers may need a while to load the project).
    await wd.waitFor(`return ${appExpr}.problems.all().some(p => p.location?.file === arguments[0] && p.location.line === arguments[1] && p.severity === 'error')`, [error.file, error.line], 180000, "the type error");
    check(`${name}: a type error is in Problems`, true);
    let labels = [];
    for (let i = 0; i < 20 && !labels.some((l) => l.includes(completion.expect)); i++) {
      labels = await suggestions(wd, completion.line, completion.column, "").catch(() => []);
      if (!labels.some((l) => l.includes(completion.expect))) await new Promise((r) => setTimeout(r, 3000));
    }
    check(`${name}: completion offers ${completion.expect}`, labels.some((l) => l.includes(completion.expect)), JSON.stringify(labels.slice(0, 6)));
    await caret(wd, definition.line, definition.column);
    await wd.keys(F12);
    await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === arguments[0]`, [definition.target], 30000, "the definition's file");
    check(`${name}: F12 opens the definition in ${definition.target}`, true);
    await openFile(wd, open);
    await rename(wd, renameAt.line, renameAt.column, "renamedThing");
    await wd
      .waitFor(`return arguments[0].every(p => (window.__axiomEditor.store.model(p)?.getValue() ?? '').includes('renamedThing'))`, [renamedIn], 60000, "the rename in every file")
      .catch(async (e) => {
        const texts = {};
        for (const p of renamedIn) texts[p] = await textOf(wd, p).catch(() => null);
        console.log("RENAME", JSON.stringify(texts));
        throw e;
      });
    check(`${name}: F2 renames across ${renamedIn.join(" and ")}`, true);
    writeFileSync(join(SHOTS, `lsp-${name}.png`), await wd.screenshot());
  } catch (e) {
    check(`${name} suite`, false, String(e?.stack ?? e));
    if (wd) await wd.diagnose(`lsp-${name}`, console.log);
  } finally {
    if (wd) await wd.quit().catch(() => {});
    removeDir(data);
    removeDir(ws);
  }
}

await fakeServer();
await realServer({
  name: "rust-analyzer",
  ok: runs("rust-analyzer", ["--version"]),
  files: {
    "Cargo.toml": '[package]\nname = "lsp_e2e"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
    "src/main.rs": "mod util;\n\nfn main() {\n    let p = util::Point { x: 1 };\n    let n: i32 = util::helper();\n    let bad: i32 = \"text\";\n    println!(\"{} {n} {bad}\", p.x);\n    p.\n}\n",
    "src/util.rs": "pub struct Point {\n    pub x: i32,\n}\n\nimpl Point {\n    pub fn norm(&self) -> i32 {\n        self.x\n    }\n}\n\npub fn helper() -> i32 {\n    42\n}\n",
  },
  open: "src/main.rs",
  completion: { line: 8, column: 7, expect: "norm" },
  error: { file: "src/main.rs", line: 6 },
  definition: { line: 5, column: 25, target: "src/util.rs" },
  renameAt: { line: 5, column: 25 },
  renamedIn: ["src/main.rs", "src/util.rs"],
});
await realServer({
  name: "typescript-language-server",
  ok: runs("typescript-language-server", ["--version"]),
  files: {
    "package.json": '{ "name": "lsp-e2e", "version": "1.0.0", "private": true }\n',
    "tsconfig.json": '{ "compilerOptions": { "strict": true, "target": "es2022", "module": "es2022", "moduleResolution": "bundler" } }\n',
    "src/main.ts": 'import { helper, point } from "./util";\n\nconst n: number = helper();\nconst bad: number = "text";\nconsole.log(n, bad);\npoint.\n',
    "src/util.ts": "export function helper(): number {\n  return 42;\n}\n\nexport const point = { norm(): number { return 1; } };\n",
  },
  open: "src/main.ts",
  completion: { line: 6, column: 7, expect: "norm" },
  error: { file: "src/main.ts", line: 4 },
  definition: { line: 3, column: 21, target: "src/util.ts" },
  renameAt: { line: 3, column: 21 },
  renamedIn: ["src/main.ts", "src/util.ts"],
});

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} lsp checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
