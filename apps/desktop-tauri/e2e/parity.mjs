#!/usr/bin/env node
// Parity checks that need the real window: the Edit and Help menus, the task
// shortcuts and the About sheet. They live apart from smoke.mjs so a failure
// here names the row it belongs to (feature-parity.md).
//
//   E-10  Edit menu: undo / redo / find / replace / select all
//   T-7   F5 rerun, Shift+F5 stop, Ctrl/Cmd+Shift+R focus the command field
//   S-3   About: app, backend, protocol, toolkit versions, data directory
//   S-8   Accessibility: every interactive control has an accessible name and a
//         role, and keyboard focus is visible. (A screen reader's actual output
//         stays manual - agents/MANUAL_VERIFICATION.md §4.)
//
// Run through the runner: node e2e/runner.mjs e2e/parity.mjs

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const repo = resolve(import.meta.dirname, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const log = appLogger("parity");

const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}

const data = mkdtempSync(join(tmpdir(), "axiom-parity-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-parity-ws-"));
mkdirSync(join(ws, "src"), { recursive: true });
writeFileSync(join(ws, "src", "main.rs"), 'fn main() {\n    println!("hello");\n}\n');
writeFileSync(join(ws, "README.md"), "# Demo\n");
// A command for T-7: the command field records history (runArgv does not),
// and it is slow enough that a rerun is observable.
mkdirSync(join(ws, "tools"), { recursive: true });
writeFileSync(join(ws, "tools", "first.js"), "setTimeout(() => process.stdout.write('first\\n'), 800);\n");

/** Opens a menu by its trigger's accessible name and picks one item by its label. */
async function menu(wd, trigger, item) {
  await wd.click(await wd.find(`button[aria-label="${trigger}"]`));
  await wd.waitFor(`return document.querySelectorAll('.menu__item').length > 0`, [], 5000, "the menu opens");
  const seen = await wd.exec(`return [...document.querySelectorAll('.menu__item')].map(i => i.textContent)`);
  // Radix items respond to pointer events; a real click is down, up, click.
  const clicked = await wd.exec(
    `const el = [...document.querySelectorAll('.menu__item')].find(i => i.textContent.trim().startsWith(arguments[0]));
     if (!el) return false;
     for (const type of ['pointerdown', 'pointerup']) el.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, button: 0 }));
     el.click();
     return true;`,
    [item],
  );
  if (!clicked) throw new Error(`menu item "${item}" not found in ${trigger}; saw ${JSON.stringify(seen)}`);
  await new Promise((r) => setTimeout(r, 300));
}

/** The editor's text, with Monaco's non-breaking spaces normalised. */
const editorText = (wd) =>
  wd.exec(`return document.querySelector('.monaco-editor .view-lines')?.textContent.replace(/\\u00a0/g, ' ')`);

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], {}, log);
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected");
  await wd.exec(`window.__axiom.documents.requestOpenFolder(arguments[0])`, [ws]);
  // A root-level file: nested rows exist only after their folder is expanded.
  await wd.waitFor(`return document.querySelector('[data-path="README.md"]') !== null`, [], 10000, "tree");
  await wd.click(await wd.find('[data-path="README.md"]'));
  await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.includes('Demo')`, [], 20000, "editor");
  await wd.click(await wd.find(".monaco-editor .view-lines"));

  // ---- E-10: Edit menu drives the editor through the same commands ------------
  await wd.keys([..."XY"]);
  await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines').textContent.includes('XY')`, [], 5000, "typed");
  await menu(wd, "Edit menu", "Undo");
  const afterUndo = await editorText(wd);
  check("E-10 Edit ▸ Undo reverts the last edit", !afterUndo.includes("XY"), JSON.stringify(afterUndo).slice(0, 60));

  await menu(wd, "Edit menu", "Redo");
  const afterRedo = await editorText(wd);
  check("E-10 Edit ▸ Redo puts it back", afterRedo.includes("XY"), JSON.stringify(afterRedo).slice(0, 60));

  await menu(wd, "Edit menu", "Select All");
  // The status line carries the selection ("Ln 1, Col 1 (9 selected)"); the
  // selection itself lives inside Monaco.
  const status = await wd.waitFor(
    `const t = window.__axiom.documents.getSnapshot().cursorText; return /selected/.test(t) ? t : null`,
    [],
    5000,
    "a selection is reported",
  );
  const selectedCount = Number(/\((\d+) selected\)/.exec(status)?.[1] ?? 0);
  check("E-10 Edit ▸ Select All selects the document", selectedCount >= 6, JSON.stringify(status));

  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await menu(wd, "Edit menu", "Find");
  const findOpen = await wd.waitFor(
    `return document.querySelector('.find-widget')?.classList.contains('visible') ? 'visible' : null`,
    [],
    5000,
    "the find widget opens",
  ).catch(() => null);
  check("E-10 Edit ▸ Find opens Monaco's find widget", findOpen === "visible");
  await wd.keys([Keys.ESC]);

  await menu(wd, "Edit menu", "Replace");
  const replaceOpen = await wd.exec(
    `const w = document.querySelector('.find-widget'); return !!w && (w.classList.contains('visible') || w.querySelector('.replace-part'))`,
  );
  check("E-10 Edit ▸ Replace opens the replace field", replaceOpen === true);
  await wd.keys([Keys.ESC]);

  // ---- T-7: task shortcuts ----------------------------------------------------
  // Through the command field, which is what records history (F5 reruns that).
  await wd.click(await wd.find("[data-testid=command-input]"));
  await wd.keys([..."node tools/first.js", Keys.ENTER]);
  await wd.waitFor(`return window.__axiom.tasks.getSnapshot().lastOutcome === 'succeeded'`, [], 20000, "the first run finished");
  const history = await wd.exec(`return window.__axiom.tasks.getSnapshot().history`);
  check("T-7 the command field records history", history[0] === "node tools/first.js", JSON.stringify(history));
  await wd.exec(`document.querySelector('.monaco-editor .view-lines').focus()`);
  await wd.keys([Keys.F5]);
  // The rerun is visible while it lasts (the fixture takes 800 ms).
  const reran = await wd.waitFor(
    `return window.__axiom.tasks.getSnapshot().running ? 'running' : null`,
    [],
    5000,
    "F5 started a run",
  ).catch(() => null);
  await wd.waitFor(`return window.__axiom.tasks.getSnapshot().lastOutcome === 'succeeded'`, [], 20000, "the rerun finished");
  const outRows = await wd.exec(`return window.__axiom.tasks.getSnapshot().rows.map(r => r.text)`);
  check("T-7 F5 reruns the last command", reran === "running" && outRows.some((t) => t.includes("first")), `rerun=${reran}`);
  await wd.exec(`window.__axiom.tasks.runArgv(["node", "-e", "setTimeout(() => {}, 30000)"])`);
  await wd.waitFor(`return window.__axiom.tasks.getSnapshot().running`, [], 10000, "a long run started");
  await wd.exec(`document.querySelector('.monaco-editor .view-lines').focus()`);
  await wd.chord(Keys.SHIFT, Keys.F5);
  await wd.waitFor(`return window.__axiom.tasks.getSnapshot().lastOutcome === 'cancelled'`, [], 15000, "Shift+F5 stopped it");
  check("T-7 Shift+F5 stops the running command", true);

  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.chord(MOD, Keys.SHIFT, "r");
  const focused = await wd.waitFor(
    `return document.activeElement?.getAttribute('data-testid') === 'command-input' ? 'focused' : null`,
    [],
    5000,
    "the command field takes focus",
  ).catch(() => null);
  check("T-7 Ctrl/Cmd+Shift+R focuses the command field", focused === "focused");

  // ---- S-8: accessible names, roles and a visible focus ring ------------------
  // The same kind of guard the QML shell had (every interactive item needed an
  // Accessible.name), expressed for the DOM. A screen reader's output cannot be
  // asserted here; that is manual protocol §4.
  const audit = await wd.exec(`
    const interactive = [...document.querySelectorAll(
      'button, [role=button], a[href], input, select, textarea, [role=tab], [role=menuitem], [role=treeitem], [tabindex]:not([tabindex="-1"])'
    )].filter((el) => el.offsetParent !== null);
    const nameOf = (el) => {
      const label = el.getAttribute('aria-label');
      if (label && label.trim()) return label.trim();
      const by = el.getAttribute('aria-labelledby');
      if (by) {
        const t = by.split(/\s+/).map((id) => document.getElementById(id)?.textContent ?? '').join(' ').trim();
        if (t) return t;
      }
      const text = (el.innerText || el.textContent || '').trim();
      if (text) return text;
      const alt = el.getAttribute('alt') || el.getAttribute('title') || '';
      return alt.trim();
    };
    const unnamed = interactive.filter((el) => !nameOf(el))
      .map((el) => el.tagName.toLowerCase() + '.' + String(el.className || '').split(' ')[0]);
    const roles = [...document.querySelectorAll('[role]')].map((el) => el.getAttribute('role'));
    return { total: interactive.length, unnamed, roles: [...new Set(roles)].sort() };
  `);
  check(
    "S-8 every visible interactive control has an accessible name",
    audit.unnamed.length === 0,
    `${audit.total} controls${audit.unnamed.length ? "; unnamed: " + audit.unnamed.join(", ") : ""}`,
  );
  const neededRoles = ["tablist", "tab", "tree", "log"];
  const missingRoles = neededRoles.filter((r) => !audit.roles.includes(r));
  check("S-8 the main regions carry their roles", missingRoles.length === 0, `roles: ${audit.roles.join(", ")}`);

  // Keyboard focus must be visible: focus a toolbar button with Tab and read the
  // computed outline. Chromium reports `:focus-visible` styles here.
  await wd.exec(`document.querySelector('button[aria-label="Help menu"]').focus()`);
  const ring = await wd.exec(`
    const el = document.activeElement;
    if (!el || el.tagName !== 'BUTTON') return null;
    el.classList.add('focus-probe');
    const cs = getComputedStyle(el);
    return { outline: cs.outlineStyle + ' ' + cs.outlineWidth, shadow: cs.boxShadow };
  `);
  const visible = ring !== null && ((ring.outline && !ring.outline.includes("none 0px")) || (ring.shadow && ring.shadow !== "none"));
  check("S-8 keyboard focus is visible on a toolbar button", Boolean(visible), JSON.stringify(ring));

  // ---- S-3: About shows the versions and the data directory -------------------
  await menu(wd, "Help menu", "About Axiom");
  const about = await wd.waitFor(
    `return document.querySelector('.about-grid')?.textContent`,
    [],
    5000,
    "the About sheet",
  ).catch(() => null);
  const ok =
    about !== null &&
    /axiomd \d/.test(about) &&
    /0\.1/.test(about) &&
    /Tauri/.test(about) &&
    /Webview/.test(about) &&
    about.includes(data);
  check("S-3 About shows backend, protocol, toolkit versions and the data directory", ok, JSON.stringify(about ?? "").slice(0, 120));
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
  if (wd) {
    try {
      const shots = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
      mkdirSync(shots, { recursive: true });
      writeFileSync(join(shots, "zz-parity-failure.png"), await wd.screenshot());
    } catch {
      /* the page may be gone */
    }
    await wd.diagnose("parity failure", log);
  }
} finally {
  await wd?.quit();
  await removeDir(data);
  await removeDir(ws);
}

const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} parity checks passed`);
process.exit(failed.length ? 1 : 0);
