#!/usr/bin/env node
// Git in the UI (M4-GIT-002; ADR-0018 §5) on a temporary repository, through the real app:
// the branch in the status bar; an edit shows a gutter mark at once and, saved, an M in the
// explorer and on the tab; a new file is U; Source Control lists the changes and Enter opens the
// diff (Monaco's diff editor), Esc closes it; a commit made outside the app clears the marks;
// accessibility of Source Control and the diff in both themes.
// Usage: scripts/e2e.sh git.mjs

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
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
const onLog = appLogger("git");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";

const ID = { GIT_AUTHOR_NAME: "t", GIT_AUTHOR_EMAIL: "t@example.com", GIT_COMMITTER_NAME: "t", GIT_COMMITTER_EMAIL: "t@example.com" };
const git = (cwd, ...args) =>
  execFileSync("git", ["-c", "init.defaultBranch=main", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null", ...args], { cwd, env: { ...process.env, ...ID }, stdio: "pipe" });

const data = mkdtempSync(join(tmpdir(), "axiom-git-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-git-ws-"));
mkdirSync(join(ws, "src"));
writeFileSync(join(ws, "src", "lib.rs"), "pub fn one() -> i32 {\n    1\n}\n\npub fn two() -> i32 {\n    2\n}\n");
writeFileSync(join(ws, "README.md"), "# demo\n");
git(ws, "init", "-q");
git(ws, "add", ".");
git(ws, "commit", "-qm", "first");

/** Contrast failures of the elements whose class starts with `prefix`, and unnamed controls in
 * `root`. */
async function audit(wd, prefix, root) {
  await wd.exec(HELPERS);
  const contrast = (await wd.exec(`return window.__a11y.contrastScan()`)).filter((f) => f.cls.startsWith(prefix));
  const unnamed = await wd.exec(`return [...document.querySelectorAll(arguments[0] + ' button, ' + arguments[0] + ' [role=tab]')].filter(e => e.getBoundingClientRect().width > 0).map(e => window.__a11y.describe(e)).filter(d => !d.name).length`, [root]);
  return { contrast: contrast.length, unnamed, detail: contrast.slice(0, 3).map((f) => `"${f.text}" ${f.ratio}:1`) };
}

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 10000, "the workspace");

  // 1. The branch in the status bar.
  const branch = await wd.waitFor(`return document.querySelector('[data-testid=git-branch]')?.textContent`, [], 10000, "the branch");
  check("1 the status bar shows the branch", /main/.test(branch), branch);

  // 2. An edit: a gutter mark before saving; saved, M on the explorer row and the tab.
  await wd.click(await wd.find('[data-path="src"]'));
  await wd.waitFor(`return !!document.querySelector('[data-path="src/lib.rs"]')`, [], 5000, "src/lib.rs listed");
  await wd.click(await wd.find('[data-path="src/lib.rs"]'));
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === 'src/lib.rs' && !!window.__axiomEditor?.store.model('src/lib.rs')`, [], 15000, "the editor");
  await wd.exec(`const e = window.__axiomEditor.editor; e.setPosition({ lineNumber: 2, column: 6 }); e.focus();`);
  await wd.keys(" + 10");
  await wd.waitFor(`return !!document.querySelector('.monaco-editor .git-gutter--modified')`, [], 5000, "the gutter mark");
  check("2 an unsaved edit shows a modified mark in the gutter", true);
  await wd.chord(MOD, "s");
  await wd.waitFor(`return document.querySelector('.tree-row[data-path="src/lib.rs"]')?.dataset.git === 'modified' && document.querySelector('.tab[data-path="src/lib.rs"]')?.dataset.git === 'modified'`, [], 10000, "the M marks");
  const marks = await wd.exec(`const r = document.querySelector('.tree-row[data-path="src/lib.rs"]'); const t = document.querySelector('.tab[data-path="src/lib.rs"]'); return { row: r.querySelector('.tree-row__git')?.textContent, rowName: r.getAttribute('aria-label'), folder: document.querySelector('.tree-row[data-path="src"]')?.dataset.git, tab: t.querySelector('.tab__git')?.textContent, tabName: t.textContent }`);
  check(
    "2b saved, the file is M in the explorer (its folder coloured) and on its tab, in words for assistive technology",
    marks.row === "M" && /modified/.test(marks.rowName) && marks.folder === "modified" && marks.tab === "M" && /modified/.test(marks.tabName),
    JSON.stringify(marks),
  );

  // 3. A new file on disk is untracked.
  writeFileSync(join(ws, "NEW.md"), "new\n");
  await wd.waitFor(`return document.querySelector('.tree-row[data-path="NEW.md"]')?.dataset.git === 'untracked'`, [], 10000, "the U mark");
  check("3 a new file is U (untracked)", true);
  writeFileSync(join(SHOTS, "git-explorer.png"), await wd.screenshot());

  // 4. Source Control lists the changes; Enter opens the diff; Esc closes it.
  await wd.click(await wd.find('.rail [data-destination="scm"]'));
  await wd.waitFor(`return document.querySelectorAll('[data-testid=scm] .scm__row').length === 2`, [], 10000, "two changes");
  const rows = await wd.exec(`return [...document.querySelectorAll('[data-testid=scm] .scm__row')].map(r => r.getAttribute('aria-label'))`);
  await wd.exec(`document.querySelector('[data-testid=scm] .scm__row[data-path="src/lib.rs"]').focus()`);
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return !!document.querySelector('[data-testid=diff-view] .monaco-diff-editor') && document.querySelectorAll('[data-testid=diff-view] .line-insert, [data-testid=diff-view] .char-insert').length > 0`, [], 15000, "the diff");
  // Side by side or inline (Monaco goes inline when the editor is narrow): the new text, and the
  // change marked as an insertion against a deletion.
  const sides = await wd.exec(`const d = document.querySelector('[data-testid=diff-view]'); const t = (d.querySelector('.monaco-diff-editor')?.textContent ?? '').replace(/\\u00a0/g, ' '); return { label: d.getAttribute('aria-label'), modified: t.includes('1 + 10'), inserted: d.querySelectorAll('.line-insert, .char-insert').length > 0, deleted: d.querySelectorAll('.line-delete, .char-delete, .view-zones .line-delete, .inline-deleted-text').length > 0 || /\\b1\\b/.test(t) }`);
  check("4 Source Control lists both changes; Enter opens the diff (HEAD ↔ working tree)", rows.length === 2 && sides.modified && sides.inserted && /^Changes in src\/lib\.rs since HEAD$/.test(sides.label), JSON.stringify({ rows, sides }));
  writeFileSync(join(SHOTS, "git-diff-dark.png"), await wd.screenshot());

  // 5. Accessibility, both themes: Source Control and the diff view.
  const a11y = {};
  for (const theme of ["dark", "light"]) {
    await wd.exec(`${appExpr}.settings.setValue("appearance.theme", arguments[0])`, [theme]);
    await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [theme], 5000, theme);
    await new Promise((r) => setTimeout(r, 300));
    a11y[theme] = { scm: await audit(wd, "scm", "[data-testid=scm]"), diff: await audit(wd, "diff-host", "[data-testid=diff-view] .diff-host__bar") };
  }
  writeFileSync(join(SHOTS, "git-diff-light.png"), await wd.screenshot());
  await wd.exec(`${appExpr}.settings.setValue("appearance.theme", "dark")`);
  const ok5 = Object.values(a11y).every((t) => t.scm.contrast === 0 && t.scm.unnamed === 0 && t.diff.contrast === 0 && t.diff.unnamed === 0);
  check("5 Source Control and the diff bar: named controls, contrast in both themes", ok5, JSON.stringify(a11y));

  await wd.exec(`document.querySelector('[data-testid=diff-view] .editor.modified textarea')?.focus()`);
  await wd.keys(Keys.ESC);
  await wd.waitFor(`return !document.querySelector('[data-testid=diff-view]')`, [], 5000, "the diff closed");
  check("6 Esc closes the diff", true);

  // 7. A commit made outside the app clears the marks.
  git(ws, "add", ".");
  git(ws, "commit", "-qm", "second");
  await wd.waitFor(`return !document.querySelector('.tree-row[data-git]') && !document.querySelector('.tab[data-git]') && !document.querySelector('.monaco-editor .git-gutter')`, [], 10000, "the marks cleared");
  const left = await wd.exec(`return document.querySelectorAll('[data-testid=scm] .scm__row').length`);
  check("7 a commit made outside the app clears the explorer, tab and gutter marks and the list", left === 0, `${left} rows`);
} catch (e) {
  check("git suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("git", console.log);
} finally {
  if (wd) await wd.quit().catch(() => {});
  removeDir(data);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} git checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
