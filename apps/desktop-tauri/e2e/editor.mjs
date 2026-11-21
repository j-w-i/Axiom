#!/usr/bin/env node
// The M4 exit scenario (M4-QA-001; milestones.md §M4) in one app session, on one workspace: a Git
// repository holding a Cargo crate and a TypeScript package (in web/).
//   "rust-analyzer and typescript-language-server provide completion, diagnostics,
//    go-to-definition and rename in the editor; the integrated terminal runs an interactive shell;
//    Git changes are visible and diffable."
// The language servers must be installed for their part (CI installs them and sets
// AXIOM_REQUIRE_LSP=1, which makes their absence a failure; elsewhere that part is reported as
// not run). Usage: scripts/e2e.sh editor.mjs

import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { Keys, TRUSTED_INPUT, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

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
const SPACE = "";
const REQUIRE = process.env.AXIOM_REQUIRE_LSP === "1";
const onLog = appLogger("editor");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const notRun = (name, why) => console.log(`N/A   ${name} — ${why}`);
const t0 = Date.now();
const appExpr = "window.__axiom";

// ---- the workspace ------------------------------------------------------------------------
const data = mkdtempSync(join(tmpdir(), "axiom-editor-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-editor-ws-"));
const files = {
  ".gitignore": "target/\nnode_modules/\n",
  "Cargo.toml": '[package]\nname = "exit_m4"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
  "src/main.rs": "mod util;\n\nfn main() {\n    let p = util::Point { x: 1 };\n    let n: i32 = util::helper();\n    let bad: i32 = \"text\";\n    println!(\"{} {n} {bad}\", p.x);\n    p.\n}\n",
  "src/util.rs": "pub struct Point {\n    pub x: i32,\n}\n\nimpl Point {\n    pub fn norm(&self) -> i32 {\n        self.x\n    }\n}\n\npub fn helper() -> i32 {\n    42\n}\n",
  "web/package.json": '{ "name": "exit-m4-web", "version": "1.0.0", "private": true }\n',
  "web/tsconfig.json": '{ "compilerOptions": { "strict": true, "target": "es2022", "module": "es2022", "moduleResolution": "bundler" } }\n',
  "web/src/main.ts": 'import { helper, point } from "./util";\n\nconst n: number = helper();\nconst bad: number = "text";\nconsole.log(n, bad);\npoint.\n',
  "web/src/util.ts": "export function helper(): number {\n  return 42;\n}\n\nexport const point = { norm(): number { return 1; } };\n",
  "notes.md": "# notes\n",
};
for (const [p, t] of Object.entries(files)) {
  mkdirSync(dirname(join(ws, p)), { recursive: true });
  writeFileSync(join(ws, p), t);
}
const ID = { GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@example.com", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@example.com" };
const git = (...args) => execFileSync("git", ["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", ...args], { cwd: ws, env: { ...process.env, ...ID }, stdio: "pipe" });
git("init", "-q");
git("add", ".");
git("commit", "-qm", "start");

// ---- helpers ------------------------------------------------------------------------------
async function openFile(wd, path) {
  await wd.exec(`${appExpr}.documents.open(arguments[0])`, [path]);
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === arguments[0] && !!window.__axiomEditor?.store.model(arguments[0])`, [path], 15000, `${path} open`);
}
const caret = (wd, line, column) => wd.exec(`const e = window.__axiomEditor.editor; e.setPosition({ lineNumber: arguments[0], column: arguments[1] }); e.focus();`, [line, column]);
async function suggestions(wd, line, column) {
  await caret(wd, line, column);
  await wd.chord(Keys.CTRL, SPACE);
  await wd.waitFor(`return document.querySelectorAll('.suggest-widget.visible .monaco-list-row').length > 0`, [], 30000, "the suggest widget");
  const labels = await wd.exec(`return [...document.querySelectorAll('.suggest-widget.visible .monaco-list-row')].map(r => r.getAttribute('aria-label') ?? r.textContent)`);
  await wd.keys(Keys.ESC);
  return labels;
}
const runs = (cmd, args) => spawnSync(cmd, args, { encoding: "utf8" }).status === 0;

async function server(wd, { name, ok, open, completion, error, definition, renameAt, renamedIn }) {
  if (!ok) {
    if (REQUIRE) check(`${name}: installed`, false, "AXIOM_REQUIRE_LSP=1 but it does not run");
    else notRun(`${name}: completion, diagnostics, definition, rename`, "not installed here");
    return;
  }
  await openFile(wd, open);
  await wd.waitFor(`return ${appExpr}.lsp.serverForPath(arguments[0])?.state === 'ready'`, [open], 120000, `${name} ready`);
  await wd.waitFor(`return ${appExpr}.problems.all().some(p => p.location?.file === arguments[0] && p.location.line === arguments[1] && p.severity === 'error')`, [error.file, error.line], 180000, `${name}: the type error`);
  check(`${name}: a diagnostic (the type error) is in Problems`, true);
  let labels = [];
  for (let i = 0; i < 20 && !labels.some((l) => l.includes(completion.expect)); i++) {
    labels = await suggestions(wd, completion.line, completion.column).catch(() => []);
    if (!labels.some((l) => l.includes(completion.expect))) await new Promise((r) => setTimeout(r, 3000));
  }
  check(`${name}: completion offers ${completion.expect}`, labels.some((l) => l.includes(completion.expect)), JSON.stringify(labels.slice(0, 5)));
  await caret(wd, definition.line, definition.column);
  await wd.keys(F12);
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === arguments[0]`, [definition.target], 30000, `${name}: the definition`);
  check(`${name}: F12 opens the definition in ${definition.target}`, true);
  await openFile(wd, open);
  await caret(wd, renameAt.line, renameAt.column);
  await wd.keys(F2);
  await wd.waitFor(`return !!document.activeElement?.closest('.rename-box')`, [], 30000, "the rename box");
  await wd.chord(MOD, "a");
  await wd.keys(`${name.split("-")[0]}Renamed`);
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return arguments[0].every(p => (window.__axiomEditor.store.model(p)?.getValue() ?? '').includes(arguments[1]))`, [renamedIn, `${name.split("-")[0]}Renamed`], 60000, `${name}: the rename`);
  check(`${name}: F2 renames across ${renamedIn.join(" and ")}`, true);
}

let wd;
let shellPids = [];
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 10000, "the workspace");

  // 1. Git: a change is visible (explorer, gutter, Source Control) and diffable.
  await openFile(wd, "notes.md");
  await caret(wd, 1, 8);
  await wd.keys(" (edited)");
  await wd.chord(MOD, "s");
  await wd.waitFor(`return document.querySelector('.tree-row[data-path="notes.md"]')?.dataset.git === 'modified' && !!document.querySelector('.monaco-editor .git-gutter--modified')`, [], 15000, "the change visible");
  await wd.click(await wd.find('.rail [data-destination="scm"]'));
  await wd.waitFor(`return !!document.querySelector('[data-testid=scm] .scm__row[data-path="notes.md"]')`, [], 10000, "Source Control lists it");
  await wd.exec(`document.querySelector('[data-testid=scm] .scm__row[data-path="notes.md"]').focus()`);
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return document.querySelectorAll('[data-testid=diff-view] .line-insert, [data-testid=diff-view] .char-insert').length > 0`, [], 15000, "the diff");
  check("1 Git: an edit is visible (explorer M, gutter mark, Source Control) and diffable (the diff view)", true);
  writeFileSync(join(SHOTS, "m4-git.png"), await wd.screenshot());
  await wd.exec(`document.querySelector('[data-testid=diff-view] .editor.modified textarea')?.focus()`);
  await wd.keys(Keys.ESC);
  await wd.waitFor(`return !document.querySelector('[data-testid=diff-view]')`, [], 5000, "the diff closed");
  await wd.click(await wd.find('.rail [data-destination="files"]'));

  // 2. The integrated terminal runs an interactive shell (in the workspace, with its toolchain).
  await wd.chord(MOD, Keys.SHIFT, "`");
  await wd.waitFor(`return ${appExpr}.terminals.getSnapshot().terminals.length === 1 && !!document.activeElement?.closest('[data-testid=terminal]')`, [], 15000, "a focused terminal");
  const line = "echo $((6*7)); git rev-parse --abbrev-ref HEAD";
  if (TRUSTED_INPUT) {
    await wd.keys(line);
    await wd.keys(Keys.ENTER);
  } else {
    await wd.exec(`const s = ${appExpr}.terminals.getSnapshot(); ${appExpr}.terminals.write(s.active, arguments[0] + "\\r")`, [line]);
  }
  await wd.waitFor(`const t = [...(document.querySelector('[data-testid=terminal] .xterm-rows')?.children ?? [])].map(r => r.textContent.trim()); return t.includes('42') && t.includes('main')`, [], 15000, "the shell's answers");
  shellPids = await wd.exec(`return ${appExpr}.terminals.getSnapshot().terminals.map(t => t.pid).filter(Boolean)`);
  check("2 the integrated terminal runs an interactive shell in the workspace", true);
  writeFileSync(join(SHOTS, "m4-terminal.png"), await wd.screenshot());

  // 3–4. The language servers.
  await server(wd, {
    name: "rust-analyzer",
    ok: runs("rust-analyzer", ["--version"]),
    open: "src/main.rs",
    completion: { line: 8, column: 7, expect: "norm" },
    error: { file: "src/main.rs", line: 6 },
    definition: { line: 5, column: 25, target: "src/util.rs" },
    renameAt: { line: 5, column: 25 },
    renamedIn: ["src/main.rs", "src/util.rs"],
  });
  await server(wd, {
    name: "typescript-language-server",
    ok: runs("typescript-language-server", ["--version"]),
    open: "web/src/main.ts",
    completion: { line: 6, column: 7, expect: "norm" },
    error: { file: "web/src/main.ts", line: 4 },
    definition: { line: 3, column: 21, target: "web/src/util.ts" },
    renameAt: { line: 3, column: 21 },
    renamedIn: ["web/src/main.ts", "web/src/util.ts"],
  });
  writeFileSync(join(SHOTS, "m4-editor.png"), await wd.screenshot());
} catch (e) {
  check("editor suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("editor", console.log);
} finally {
  if (wd) await wd.quit().catch(() => {});
  for (const p of shellPids) {
    try {
      process.kill(p, "SIGKILL");
    } catch {
      /* gone */
    }
  }
  removeDir(data);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} editor checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
