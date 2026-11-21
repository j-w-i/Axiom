#!/usr/bin/env node
// Workspace search and quick open (M4-UX-001), through the real app: Mod+Shift+F focuses the
// query; typing finds matches grouped by file; ↓ and Enter open the file at the match; Match case
// and Regular expression by keyboard; Go to File (Mod+P) lists files the explorer has not loaded
// and ranks them fuzzily; accessibility in both themes.
// Usage: scripts/e2e.sh search.mjs

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
const SPACE = "";
const onLog = appLogger("search");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";

const data = mkdtempSync(join(tmpdir(), "axiom-search-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-search-ws-"));
const file = (p, t) => {
  mkdirSync(join(ws, p, ".."), { recursive: true });
  writeFileSync(join(ws, p), t);
};
file("src/main.rs", "fn main() {\n    let needle = 1;\n    let NEEDLE = 2;\n}\n");
file("src/deep/nested/needles.rs", "// needle in a haystack\n");
file("docs/notes.md", "No match here.\n");
file(".gitignore", "target/\n");
file("target/debug/needle.txt", "needle (ignored)\n");

const rowsOf = (wd) => wd.exec(`return [...document.querySelectorAll('[data-testid=search] .search__match')].map(b => b.getAttribute('aria-label'))`);

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 10000, "the workspace");
  await wd.click(await wd.find('[data-path="src"]'));

  // 1. Mod+Shift+F: the Search page, its query focused; typing finds matches grouped by file.
  await wd.chord(MOD, Keys.SHIFT, "f");
  await wd.waitFor(`return document.activeElement?.matches('[data-testid=search] input[aria-label=Search]')`, [], 5000, "the query focused");
  await wd.keys("needle");
  await wd.waitFor(`return document.querySelectorAll('[data-testid=search] .search__match').length === 3`, [], 10000, "three matches");
  const groups = await wd.exec(`return [...document.querySelectorAll('[data-testid=search] .search__group')].map(g => g.getAttribute('aria-label'))`);
  const status = await wd.exec(`return document.querySelector('[data-testid=search] .search__status').textContent.trim()`);
  check("1 Mod+Shift+F focuses the query; matches are grouped by file; ignored files are not searched", groups.join() === "src/deep/nested/needles.rs, 1 match,src/main.rs, 2 matches" && /3 results in 2 files/.test(status), JSON.stringify({ groups, status }));
  writeFileSync(join(SHOTS, "search-dark.png"), await wd.screenshot());

  // 2. ↓ then ↓ and Enter: src/main.rs opens at line 3 (the second match there).
  await wd.keys(Keys.DOWN ?? "");
  await wd.keys("");
  await wd.keys("");
  const focusRow = await wd.exec(`return document.activeElement?.getAttribute('aria-label')`);
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === 'src/main.rs' && /^Ln 3, Col 9/.test(${appExpr}.documents.getSnapshot().cursorText)`, [], 10000, "main.rs at 3:9");
  check("2 arrows move through the matches; Enter opens the file at the match", /line 3/.test(focusRow ?? ""), focusRow);

  // 3. Match case by keyboard: NEEDLE no longer matches.
  await wd.exec(`document.querySelector('[data-testid=search] input[aria-label=Search]').focus()`);
  await wd.keys(Keys.TAB);
  const onToggle = await wd.exec(`return document.activeElement?.getAttribute('aria-label')`);
  await wd.keys(SPACE);
  await wd.waitFor(`return document.querySelector('[data-testid=search] [aria-label="Match case"]').getAttribute('aria-pressed') === 'true' && document.querySelectorAll('[data-testid=search] .search__match').length === 2`, [], 10000, "case-sensitive results");
  check("3 Tab reaches Match case; Space turns it on and the upper-case line drops out", onToggle === "Match case");

  // 4. Regular expression.
  await wd.exec(`document.querySelector('[data-testid=search] [aria-label="Regular expression"]').click()`);
  await wd.exec(`document.querySelector('[data-testid=search] input[aria-label=Search]').focus()`);
  await wd.chord(MOD, "a");
  await wd.keys("ne+dles?\\b");
  await wd.waitFor(`return document.querySelectorAll('[data-testid=search] .search__match').length === 2 && ${appExpr}.search.getSnapshot().regex`, [], 10000, "regex results");
  const hit = await wd.exec(`return [...document.querySelectorAll('[data-testid=search] .search__hit')].map(h => h.textContent)`);
  check("4 a regular expression finds and highlights its matches", hit.join() === "needle,needle", JSON.stringify(hit));

  // 5. Accessibility in both themes.
  const a11y = {};
  for (const theme of ["light", "dark"]) {
    await wd.exec(`${appExpr}.settings.setValue("appearance.theme", arguments[0])`, [theme]);
    await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [theme], 5000, theme);
    await new Promise((r) => setTimeout(r, 300));
    await wd.exec(HELPERS);
    const contrast = (await wd.exec(`return window.__a11y.contrastScan()`)).filter((f) => f.cls.startsWith("search"));
    const unnamed = await wd.exec(`return [...document.querySelectorAll('[data-testid=search] button, [data-testid=search] input')].map(e => window.__a11y.describe(e)).filter(d => !d.name).length`);
    a11y[theme] = { contrast: contrast.map((f) => `"${f.text}" ${f.ratio}:1`), unnamed };
  }
  writeFileSync(join(SHOTS, "search-light.png"), await wd.screenshot());
  check("5 the Search page: every control named, contrast in both themes", Object.values(a11y).every((x) => x.contrast.length === 0 && x.unnamed === 0), JSON.stringify(a11y));

  // 6. Go to File lists files the explorer never loaded, ranked fuzzily.
  await wd.click(await wd.find('.rail [data-destination="files"]'));
  const loaded = await wd.exec(`return !!document.querySelector('[data-path="src/deep/nested/needles.rs"]')`);
  await wd.chord(MOD, "p");
  await wd.find("[role=dialog] input", 5000);
  await wd.keys("ndlsrs");
  await wd.waitFor(`return document.querySelector('[role=dialog] [role=option]')?.textContent.includes('needles.rs')`, [], 10000, "the nested file first");
  const first = await wd.exec(`return [...document.querySelectorAll('[role=dialog] [role=option]')].slice(0, 3).map(o => o.textContent)`);
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return ${appExpr}.documents.getSnapshot().activePath === 'src/deep/nested/needles.rs'`, [], 10000, "it opens");
  check("6 Go to File finds a file the explorer has not loaded, ranked first for a fuzzy query, and opens it", !loaded && /needles\.rs/.test(first[0]), JSON.stringify({ loaded, first }));
} catch (e) {
  check("search suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("search", console.log);
} finally {
  if (wd) await wd.quit().catch(() => {});
  removeDir(data);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} search checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
