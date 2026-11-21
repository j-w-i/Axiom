#!/usr/bin/env node
// Accessibility verification of the M1.6 shell (M1.6-QA-003). Each check runs in dark and
// light and reports every violation; nothing here changes product code (failures become
// defect notes on the owning task).
//
//   1 Tab traversal       forward and reverse through the regions, in order; no trap outside dialogs
//   2 Focus visible       a ≥ 2 px outline or ring on every element reached, not clipped
//   3 Names and roles     every element reached has an accessible name and a role
//   4 Rendered contrast   text colour against the composited background, S2/S4/S7/S11/S12:
//                         ≥ 4.5:1, or ≥ 3:1 for non-essential text (rendered in --text-faint, §2.3)
//   5 Not by colour alone dirty tabs, stderr rows, LEDs, failed status carry text
//   6 Reduced motion      Off: nothing running once settled; Reduced: presence ≤ 80 ms
//   7 Forced colours      not available in WebKitGTK: recorded as unverified (QA-005, Windows)
//   + Hit targets         interactive boxes under 24 × 24 px at 100% (listed, spacing not evaluated)
//   + 200% screenshots    S4, S7, S11 with GDK_SCALE=2 for the manual review
//
// Run through the runner: scripts/e2e.sh a11y.mjs (results also in $AXIOM_A11Y_OUT, JSON)

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const repo = resolve(import.meta.dirname, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const OUT = process.env.AXIOM_A11Y_OUT ?? join(tmpdir(), "axiom-a11y");
mkdirSync(OUT, { recursive: true });
const log = appLogger("a11y");

const results = [];
function check(name, ok, detail = "", violations = []) {
  results.push({ name, ok, detail, violations });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
  for (const v of violations.slice(0, 25)) console.log(`        · ${typeof v === "string" ? v : JSON.stringify(v)}`);
  if (violations.length > 25) console.log(`        · … ${violations.length - 25} more (see ${join(OUT, "a11y.json")})`);
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const data = mkdtempSync(join(tmpdir(), "axiom-a11y-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-a11y-ws-"));
// S20 (Chat, M3-UI-001): the scripted model answers once per theme with Markdown and code.
const chatScript = join(data, "chat-script.json");
const chatAnswer = { type: "text", text: "A **short** answer:\n\n```sh\necho hi\n```\n\n- one\n- two" };
// S21 (approval card, M3-UI-002): an edit asks; Esc denies it; the model hears the denial.
const ask = { events: [{ type: "toolCall", id: "a1", name: "fs.applyPatch", input: { path: "README.md", edits: [{ search: "# Demo", replace: "# Demo!" }] } }] };
const heard = { expect: { toolResult: { callId: "a1", isError: true } }, events: [{ type: "text", text: "Left it." }] };
writeFileSync(chatScript, JSON.stringify({ turns: [{ events: [chatAnswer] }, ask, heard, { events: [chatAnswer] }, ask, heard] }));
mkdirSync(join(ws, "src"), { recursive: true });
writeFileSync(join(ws, "src", "main.rs"), 'fn main() {\n    println!("hello");\n}\n');
writeFileSync(join(ws, "README.md"), "# Demo\n\nA small workspace.\n");
writeFileSync(join(ws, ".gitignore"), "target/\n");
// A Cargo workspace, so the Environments page (S18) has a toolchain row.
writeFileSync(join(ws, "Cargo.toml"), '[package]\nname = "demo"\nversion = "0.1.0"\n');
mkdirSync(join(ws, "target"), { recursive: true });

// ---- in-page helpers -------------------------------------------------------------------

const HELPERS = `
window.__a11y = (() => {
  const region = (el) => {
    if (!el || el === document.body) return "body";
    if (el.closest('[role=dialog]')) return "dialog";
    if (el.closest('.titlerow')) return "title";
    if (el.closest('[data-region=files]')) return "files";
    if (el.closest('.tabs')) return "tabs";
    if (el.closest('.monaco-editor')) return "editor";
    if (el.closest('[data-region=editor]')) return "editor-area";
    if (el.closest('[data-region=console]')) return "console";
    if (el.closest('.statusbar')) return "status";
    return "other";
  };
  const text = (el) => (el?.textContent ?? "").replace(/\\s+/g, " ").trim();
  const name = (el) => {
    if (!el) return "";
    const by = el.getAttribute("aria-labelledby");
    if (by) return by.split(/\\s+/).map((id) => text(document.getElementById(id))).join(" ").trim();
    return (el.getAttribute("aria-label") || (el.labels && el.labels[0] && text(el.labels[0])) || text(el) || el.getAttribute("title") || el.getAttribute("placeholder") || "").trim();
  };
  const IMPLICIT = { BUTTON: "button", A: "link", INPUT: "textbox", TEXTAREA: "textbox", SELECT: "combobox" };
  const role = (el) => el?.getAttribute("role") || IMPLICIT[el?.tagName] || "";
  const describe = (el) => ({ region: region(el), role: role(el), name: name(el).slice(0, 60), tag: el?.tagName?.toLowerCase(), cls: (el?.className?.baseVal ?? el?.className ?? "").toString().slice(0, 40) });

  // Focus indication: an outline ≥ 2 px, a box-shadow ring, or (rows) the focused row's ring.
  const focusVisible = (el) => {
    const cs = getComputedStyle(el);
    if (cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) >= 2) return "outline " + cs.outlineWidth;
    if (cs.boxShadow && cs.boxShadow !== "none") return "ring";
    if (el.getAttribute("role") === "tree") {
      const row = document.getElementById(el.getAttribute("aria-activedescendant") || "");
      if (row && getComputedStyle(row).boxShadow !== "none") return "row ring";
      if (row && getComputedStyle(row, "::after").content !== "none") return "row marker";
    }
    if (el.closest(".monaco-editor")) return "caret";
    // Borderless inputs inside a recessed well: the well draws the ring on focus-within.
    const well = el.closest(".prompt, .palette__bar");
    if (well && getComputedStyle(well).boxShadow.split("inset").length > 2) return "well ring";
    // The Chat composer's box draws a 2 px focus outline on focus-within (M3-UI-001).
    const box = el.closest(".composer__box");
    if (box) {
      const bs = getComputedStyle(box);
      if (bs.outlineStyle !== "none" && parseFloat(bs.outlineWidth) >= 2) return "box outline";
    }
    return "";
  };
  const clipped = (el) => {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return "zero-size";
    for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
      const cs = getComputedStyle(p);
      if (!/(auto|scroll|hidden)/.test(cs.overflow + cs.overflowX + cs.overflowY)) continue;
      const pr = p.getBoundingClientRect();
      if (r.right < pr.left + 1 || r.left > pr.right - 1 || r.bottom < pr.top + 1 || r.top > pr.bottom - 1) return "outside its scroll container";
    }
    return "";
  };

  // ---- colour ----
  const parse = (c) => {
    const m = /rgba?\\(([^)]+)\\)/.exec(c || "");
    if (!m) return null;
    const p = m[1].split(/[\\s,/]+/).filter(Boolean).map(Number);
    return [p[0], p[1], p[2], p.length > 3 ? p[3] : 1];
  };
  const over = (top, bottom) => {
    const a = top[3];
    return [0, 1, 2].map((i) => top[i] * a + bottom[i] * (1 - a)).concat(1);
  };
  const lum = (c) => {
    const f = (v) => { v /= 255; return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4); };
    return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
  };
  const ratio = (a, b) => { const x = lum(a), y = lum(b); return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05); };
  // The backgrounds under an element: fills from the element up, composited; a gradient
  // contributes each of its stops (the check takes the worst).
  const backgrounds = (el) => {
    const layers = [];
    for (let p = el; p; p = p.parentElement) {
      const cs = getComputedStyle(p);
      const stops = (cs.backgroundImage.match(/rgba?\\([^)]+\\)/g) || []).map(parse).filter(Boolean);
      const fill = parse(cs.backgroundColor);
      layers.push({ fill: fill && fill[3] > 0 ? fill : null, stops });
      if ((fill && fill[3] >= 1) || stops.some((s) => s[3] >= 1)) break;
    }
    let bases = [[255, 255, 255, 1]];
    for (const l of layers.reverse()) {
      if (l.stops.length) bases = bases.flatMap((b) => l.stops.map((s) => over(s, b)));
      if (l.fill) bases = bases.map((b) => over(l.fill, b));
    }
    return bases;
  };
  const opacity = (el) => { let o = 1; for (let p = el; p; p = p.parentElement) o *= parseFloat(getComputedStyle(p).opacity); return o; };
  const visuallyHidden = (el) => { const r = el.getBoundingClientRect(); return r.width <= 1 || r.height <= 1; };
  const faint = () => { const s = document.createElement("span"); s.style.color = "var(--text-faint)"; document.body.appendChild(s); const c = getComputedStyle(s).color; s.remove(); return c; };

  const contrastScan = () => {
    const FAINT = faint();
    const out = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const seen = new Set();
    let n;
    while ((n = walker.nextNode())) {
      if (!n.nodeValue.trim()) continue;
      const el = n.parentElement;
      if (!el || seen.has(el)) continue;
      seen.add(el);
      if (el.closest("[aria-hidden=true], [hidden], .visually-hidden, script, style, .monaco-editor .view-overlays, .minimap")) continue;
      if (el.closest("[disabled], [aria-disabled=true], :disabled")) continue; // inactive controls are exempt (WCAG 1.4.3)
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0 || r.bottom < 0 || r.top > innerHeight || visuallyHidden(el)) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility === "hidden" || parseFloat(cs.opacity) === 0) continue;
      const fg = parse(cs.color);
      if (!fg) continue;
      const o = opacity(el);
      const nonessential = cs.color === FAINT || el.closest(".line-numbers") !== null;
      const large = parseFloat(cs.fontSize) >= 24 || (parseFloat(cs.fontSize) >= 18.66 && parseInt(cs.fontWeight) >= 700);
      const need = nonessential || large ? 3 : 4.5;
      let worst = Infinity;
      for (const bg of backgrounds(el)) worst = Math.min(worst, ratio(over([fg[0], fg[1], fg[2], fg[3] * o], bg), bg));
      if (worst < need) out.push({ text: n.nodeValue.trim().slice(0, 40), ratio: +worst.toFixed(2), need, region: region(el), cls: (el.className?.baseVal ?? el.className ?? "").toString().slice(0, 40), color: cs.color });
    }
    return out;
  };

  const targets = () => {
    const out = [];
    for (const el of document.querySelectorAll("button, [role=button], [role=tab], [role=menuitem], [role=switch], [role=radio], input, a[href], [role=treeitem]")) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0 || el.closest("[aria-hidden=true], .monaco-editor")) continue;
      if (r.width < 24 || r.height < 24) out.push({ ...describe(el), w: Math.round(r.width), h: Math.round(r.height) });
    }
    return out;
  };
  return { region, describe, focusVisible, clipped, contrastScan, targets };
})();
true;`;

async function setTheme(wd, theme) {
  await wd.exec(`window.__axiom.settings.setValue("appearance.theme", arguments[0])`, [theme]);
  await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [theme], 5000, `theme ${theme}`);
  await sleep(300);
}

/** Tab (or Shift+Tab) through the window, recording each focused element; in the editor, turns on
 * Monaco's "Tab key moves focus" (Ctrl+M) once and goes on with Tab. */
let tabFocusMode = false;
async function traverse(wd, reverse) {
  const seen = [];
  let monacoStops = 0;
  for (let i = 0; i < 90; i++) {
    if (reverse) await wd.chord(Keys.SHIFT, "");
    else await wd.keys([""]);
    await sleep(40);
    const d = await wd.exec(`const a = document.activeElement; const d = window.__a11y.describe(a); d.focus = window.__a11y.focusVisible(a); d.clipped = window.__a11y.clipped(a); return d;`);
    if (d.region === "editor" && !tabFocusMode) {
      // The editor keeps Tab for indentation until "Toggle Tab Key Moves Focus" (Monaco's
      // Ctrl+M; Ctrl+Shift+M on macOS) is on: the documented way out (D-A11Y-3).
      monacoStops++;
      seen.push(d);
      if (process.platform === "darwin") await wd.chord(Keys.CTRL, Keys.SHIFT, "m");
      else await wd.chord(Keys.CTRL, "m");
      tabFocusMode = true;
      await sleep(80);
      continue;
    }
    if (seen.length > 3 && JSON.stringify(d) === JSON.stringify(seen[0])) break;
    seen.push(d);
  }
  return { seen, monacoStops };
}

/** The order regions are first reached in. */
const regionOrder = (seen) => seen.map((d) => d.region).filter((r, i, a) => r !== a[i - 1]).filter((r, i, a) => a.indexOf(r) === i);

let wd;
const record = { themes: {} };
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], { AXIOM_TEST_SCRIPTED_MODEL: chatScript }, log);
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected");
  await wd.exec(HELPERS);
  // S2: Welcome with one recent folder (open and close it once).
  await wd.exec(`window.__axiom.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="README.md"]')`, [], 10000, "tree");
  await wd.exec(`window.__axiom.documents.requestCloseFolder()`);
  await wd.waitFor(`return !!document.querySelector('[data-testid=welcome] .welcome__item')`, [], 8000, "welcome");

  const contrast = {};
  for (const theme of ["dark", "light"]) {
    await setTheme(wd, theme);
    contrast[`S2 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
  }

  // S4: the workspace with a dirty tab, a failed run with stderr, a second tab.
  // By name: the row's title is the canonical root (/private/var/… on macOS, long names on Windows).
  await wd.exec(`[...document.querySelectorAll('.welcome__item')].find(b => b.querySelector('.welcome__item-name')?.textContent === arguments[0])?.setAttribute('data-a11y-target', '')`, [basename(ws)]);
  await wd.click(await wd.find(`.welcome__item[data-a11y-target]`));
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 10000, "workspace");
  await wd.click(await wd.find('[data-path="src"]'));
  await wd.waitFor(`return !!document.querySelector('[data-path="src/main.rs"]')`, [], 5000, "src");
  await wd.click(await wd.find('[data-path="README.md"]'));
  await wd.click(await wd.find('[data-path="src/main.rs"]'));
  await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.includes('hello')`, [], 20000, "editor");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.keys([..."// x"]);
  await wd.waitFor(`return document.querySelector('[role=tab][data-path="src/main.rs"]')?.dataset.dirty === 'true'`, [], 5000, "dirty");
  await wd.exec(`window.__axiom.tasks.run(arguments[0])`, [`node -e "console.log('out'); console.error('warning: careful'); process.exit(3)"`]);
  await wd.waitFor(`return [...document.querySelectorAll('.output__row')].some(r => r.dataset.kind === 'statusFailed')`, [], 15000, "failed run");

  for (const theme of ["dark", "light"]) {
    await setTheme(wd, theme);
    const t = (record.themes[theme] = {});
    await wd.exec(HELPERS);
    contrast[`S4 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);

    // ---- 1–3: traversal, focus, names --------------------------------------------------
    await wd.exec(`document.activeElement?.blur(); window.focus();`);
    const fwd = await traverse(wd, false);
    await wd.exec(`document.activeElement?.blur()`);
    const back = await traverse(wd, true);
    t.forward = regionOrder(fwd.seen);
    t.reverse = regionOrder(back.seen);
    const EXPECTED = ["title", "files", "tabs", "editor", "console", "status"];
    const inOrder = (got, want) => {
      let k = 0;
      for (const r of got) if (r === want[k]) k++;
      return k === want.length;
    };
    const fwdOk = inOrder(t.forward, EXPECTED);
    const backOk = inOrder(t.reverse, [...EXPECTED].reverse().filter((r) => r !== "editor").concat()) || inOrder(t.reverse, ["status", "console", "tabs", "files", "title"]);
    check(
      `1 tab traversal (${theme}): forward and reverse reach every region in order`,
      fwdOk && backOk,
      `forward ${t.forward.join(" → ")}; reverse ${t.reverse.join(" → ")}; the editor was left with Ctrl+M (Tab moves focus) — ${fwd.monacoStops + back.monacoStops} time(s)`,
      [],
    );
    // <body>: where focus rests between the last control and the wrap to the first (WKWebView,
    // WebView2): the webview's own chrome, not a control.
    const all = [...fwd.seen, ...back.seen].filter((d) => d.region !== "body").filter((d, i, a) => a.findIndex((x) => x.region === d.region && x.name === d.name && x.role === d.role) === i);
    const noFocus = all.filter((d) => !d.focus).map((d) => `${d.region} ${d.role} "${d.name}"`);
    const clippedEls = all.filter((d) => d.clipped).map((d) => `${d.region} ${d.role} "${d.name}": ${d.clipped}`);
    check(`2 focus visible (${theme}): every element reached shows focus and is not clipped`, noFocus.length === 0 && clippedEls.length === 0, `${all.length} elements`, [...noFocus.map((v) => "no focus indicator: " + v), ...clippedEls.map((v) => "clipped: " + v)]);
    const unnamed = all.filter((d) => !d.name || !d.role).map((d) => `${d.region} <${d.tag} class="${d.cls}"> role="${d.role}" name="${d.name}"`);
    check(`3 names and roles (${theme})`, unnamed.length === 0, `${all.length} elements`, unnamed);

    // ---- S7, S11, S12 (contrast) --------------------------------------------------------
    await wd.click(await wd.find(".monaco-editor .view-lines"));
    await wd.chord(MOD, "k");
    await wd.waitFor(`return !!document.querySelector('[data-testid=palette] [role=option]')`, [], 3000, "palette");
    await sleep(300);
    contrast[`S7 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return !document.querySelector('[data-testid=palette]')`, [], 3000, "palette closed");
    await wd.chord(MOD, ",");
    await wd.waitFor(`return !!document.querySelector('[data-testid=settings]')`, [], 3000, "settings");
    await sleep(300);
    contrast[`S11 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return !document.querySelector('[data-testid=settings]')`, [], 3000, "settings closed");
    await wd.exec(`window.__axiom.documents.close('src/main.rs')`);
    await wd.waitFor(`return !!document.querySelector('[data-testid=unsaved-dialog]')`, [], 3000, "unsaved dialog");
    await sleep(300);
    contrast[`S12 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return !document.querySelector('[data-testid=unsaved-dialog]')`, [], 3000, "dialog closed");

    // ---- S18: the Environments page and the install-plan dialog (M2-UI-001, M2-UI-002) ----
    await wd.click(await wd.find('.rail [data-destination="environments"]'));
    await wd.waitFor(`return window.__axiom.environments.getSnapshot().status === 'ready' && !!document.querySelector('.env-table')`, [], 15000, "environments page");
    await sleep(300);
    contrast[`S18 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    // By keyboard, as check 2 (programmatic focus after a click does not show :focus-visible).
    await wd.exec(`document.activeElement?.blur()`);
    const envTrav = await traverse(wd, false);
    const envNames = await wd.exec(`return [...document.querySelectorAll('.rail button, [data-region=files] button, .env-page button')].filter(e => e.getBoundingClientRect().width > 0 && e.tabIndex >= 0).map(e => window.__a11y.describe(e).name)`); // the rail is one Tab stop (roving)
    const envEls = envTrav.seen.filter((d) => envNames.includes(d.name));
    const missed = envNames.filter((n) => !envEls.some((d) => d.name === n));
    const badEnv = [
      ...envEls.filter((d) => !d.name || !d.role || !d.focus || d.clipped).map((d) => `${d.role} "${d.name}" focus=${d.focus} clipped=${d.clipped}`),
      ...missed.map((n) => `not reached by Tab: "${n}"`),
    ];
    const table = await wd.exec(`const t = document.querySelector('.env-table'); return { name: t?.getAttribute('aria-label'), headers: [...t.querySelectorAll('thead th')].map(h => h.getAttribute('aria-label') || h.textContent), leds: [...t.querySelectorAll('.led')].every(l => (l.getAttribute('aria-label') ?? '').includes(':')) }`);
    check(
      `S18 environments page (${theme}): every control named, with a role and visible focus; the table and its columns are named; LEDs say their state`,
      envEls.length > 0 && badEnv.length === 0 && table.name === "Toolchains" && table.headers.every(Boolean) && table.leds,
      `${envEls.length} controls; table ${JSON.stringify(table)}`,
      badEnv,
    );
    // The plan dialog (the daemon's plan for a fixture provider would need the network; the
    // store's own plan state is set, as the shots harness does).
    await wd.exec(`window.__axiom.environments.set({ plan: { providerName: "Rust", plan: { planId: "a11y", providerId: "rust", version: "1.84.0", scope: "system", downloadBytes: 1024, expiresMs: 0, effects: ["network", "systemChange", "elevation"],
      steps: [{ kind: "download", url: "https://example.org/tool.tar.gz", bytes: 1024, sha256: { kind: "digest", value: "${"ab".repeat(32)}" } }, { kind: "systemPackages", manager: "dnf", packages: ["tool"], argv: ["pkexec", "dnf", "install", "-y", "tool"] }, { kind: "verify", toolId: "tool" }] } } })`);
    await wd.waitFor(`return !!document.querySelector('[data-testid=install-plan] .plan-step')`, [], 5000, "plan dialog");
    await sleep(300);
    contrast[`S18d ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    const dlg = await wd.exec(`
      const d = document.querySelector('[data-testid=install-plan]');
      const list = d.querySelector('ol');
      return { role: d.getAttribute('role'), name: document.getElementById(d.getAttribute('aria-labelledby'))?.textContent,
               list: list?.tagName + ':' + list?.getAttribute('aria-label'), items: [...list.children].map(li => li.tagName + ':' + li.textContent.trim().slice(0, 30)),
               warning: d.querySelector('.plan__warning')?.textContent, focus: document.activeElement?.dataset.action }`);
    const trapped = [];
    for (let i = 0; i < 6; i++) {
      await wd.keys(["\uE004"]); // Tab
      trapped.push(await wd.exec(`return !!document.activeElement?.closest('[data-testid=install-plan]')`));
    }
    await wd.keys([Keys.ESC]);
    const closed = await wd.waitFor(`return !document.querySelector('[data-testid=install-plan]') && !window.__axiom.environments.getSnapshot().plan`, [], 3000, "plan closed").then(() => true, () => false);
    const installs = await wd.exec(`return Object.keys(window.__axiom.environments.getSnapshot().installs).length`);
    check(
      `S18d install plan dialog (${theme}): a named dialog, steps are a named list of named items, focus starts on Install and stays inside, Escape cancels without installing`,
      dlg.role === "dialog" && dlg.name === "Install Rust 1.84.0" && dlg.list === "OL:Steps" && dlg.items.length === 3 && dlg.items.every((x) => x.startsWith("LI:") && x.length > 4) &&
        /System install/.test(dlg.warning ?? "") && dlg.focus === "install" && trapped.every(Boolean) && closed && installs === 0,
      JSON.stringify({ ...dlg, trapped, closed, installs }),
    );
    await wd.click(await wd.find('.rail [data-destination="files"]'));
    await wd.waitFor(`return document.querySelector('.editor-column__files')?.hidden === false`, [], 3000, "back to files");

    // ---- S19: the console's Problems view (M2-UI-003) --------------------------------------
    await wd.exec(`window.__axiom.problems.begin("rust/build", "a11y", "Build: cargo build");
      window.__axiom.problems.set({ byTask: { a11y: { complete: true, diagnostics: [
        { severity: "error", message: "mismatched types", code: "E0308", source: "rustc", related: [], location: { file: "src/main.rs", line: 2, column: 18 } },
        { severity: "warning", message: "unused variable: x", code: "unused_variables", source: "rustc", related: [], location: { file: "src/main.rs", line: 3, column: 9 } } ] } } })`);
    await wd.click(await wd.find('[data-view=problems]'));
    await wd.waitFor(`return document.querySelectorAll('[data-testid=problems] .problem').length === 2`, [], 5000, "problems");
    await sleep(200);
    contrast[`S19 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    await wd.exec(`document.activeElement?.blur()`);
    const probTrav = await traverse(wd, false);
    const strip = await wd.exec(`const l = document.querySelector('[role=tablist][aria-label="Console views"]'); return { tabs: [...l.querySelectorAll('[role=tab]')].map(t => t.getAttribute('aria-label') + ':' + t.getAttribute('aria-selected')), panel: document.querySelector('[role=tabpanel]#console-problems')?.getAttribute('aria-label') }`);
    const probEls = probTrav.seen.filter((x) => x.cls.startsWith("problem") || x.cls.startsWith("console__view"));
    const badProb = probEls.filter((x) => !x.name || !x.role || !x.focus).map((x) => `${x.role} "${x.name}" focus=${x.focus}`);
    check(
      `S19 problems view (${theme}): named tabs and panel, rows reachable by Tab with names that carry severity and position, visible focus`,
      strip.tabs.join() === "Output:false,Problems, 2:true" && strip.panel === "Problems" && probEls.some((x) => /^Error: mismatched types \(E0308\) at src\/main\.rs:2:18$/.test(x.name)) && badProb.length === 0,
      `${JSON.stringify(strip)}; reached ${probEls.length}`,
      badProb,
    );
    await wd.click(await wd.find('[data-view=output]'));
    await wd.exec(`window.__axiom.problems.set({ runs: {}, byTask: {} })`);

    // ---- S20: Chat (M3-UI-001; workspace-spec.md §5.4) ------------------------------------
    await wd.click(await wd.find('.rail [data-destination="chat"]'));
    await wd.waitFor(`return window.__axiom.agent.getSnapshot().status === 'ready' && !!window.__axiom.agent.getSnapshot().model`, [], 10000, "chat ready");
    await wd.exec(`window.__axiom.agent.newChat("general"); window.__axiom.agent.send("Answer briefly")`);
    await wd.waitFor(`return !!document.querySelector('[data-testid=chat-log] .code-block') && window.__axiom.agent.getSnapshot().active.running === false`, [], 10000, "chat answer").catch(async (e) => {
      console.log("S20 state:", JSON.stringify(await wd.exec(`const s = window.__axiom.agent.getSnapshot(); return { scope: s.scope, active: s.activeId, err: s.sendError, error: s.error, model: s.model, n: s.active.events.map(e => e.event.type), page: !!document.querySelector('[data-testid=chat-page]') }`)));
      throw e;
    });
    await sleep(500);
    contrast[`S20 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    await wd.exec(`document.activeElement?.blur()`);
    const chatTrav = await traverse(wd, false);
    const chatEls = chatTrav.seen.filter((x) => x.region === "files" || x.region === "editor-area");
    const badChat = chatEls.filter((x) => !x.name || !x.role || !x.focus).map((x) => `${x.region} ${x.role} "${x.name}" focus=${x.focus}`);
    const sem = await wd.exec(`return {
      log: document.querySelector('[role=log]')?.getAttribute('aria-label'),
      articles: [...document.querySelectorAll('[data-testid=chat-log] article')].map(a => a.getAttribute('aria-label')),
      composer: !!document.querySelector('textarea[aria-label="Message"]'),
      list: document.querySelector('[role=list][aria-label=Conversations]') !== null,
      status: !!document.querySelector('.chat [role=status][aria-live=polite]'),
    }`);
    const names = chatEls.map((x) => x.name);
    check(
      `S20 chat (${theme}): log, named messages, conversation list, composer; every control reached by Tab is named with a role and visible focus`,
      sem.log === "Conversation" && sem.articles.join() === "You,Assistant" && sem.composer && sem.list && sem.status &&
        names.includes("Message") && names.includes("Copy code") && names.includes("Search conversations") && badChat.length === 0,
      `${JSON.stringify(sem)}; reached ${chatEls.length}`,
      badChat,
    );
    await wd.click(await wd.find('.rail [data-destination="files"]'));
    await wd.waitFor(`return document.querySelector('.editor-column__files')?.hidden === false`, [], 3000, "back to files");

    // ---- S21: the AI dock and the approval card (M3-UI-002; workspace-spec.md §5.2) -------
    if (!(await wd.exec(`return !!document.querySelector('[data-region=dock]')`))) await wd.exec(`document.querySelector('[aria-label="Toggle AI dock"]').click()`);
    await wd.waitFor(`return !!document.querySelector('[data-testid=chat-dock]') && window.__axiom.agent.getSnapshot().scope === 'workspace'`, [], 5000, "the dock");
    await wd.exec(`document.activeElement?.blur(); window.__axiom.agent.newChat("workspace"); window.__axiom.agent.send("Edit the readme")`);
    await wd.waitFor(`return !!document.querySelector('[data-testid=chat-dock] [data-testid=approval]')`, [], 10000, "approval card");
    await sleep(300);
    contrast[`S21 ${theme}`] = await wd.exec(`return window.__a11y.contrastScan()`);
    const cardA11y = await wd.exec(`const c = document.querySelector('[data-testid=approval]'); const a = document.activeElement; return {
      role: c.getAttribute('role'), name: c.getAttribute('aria-label'),
      focused: a?.dataset.answer ?? null, focus: window.__a11y.focusVisible(a),
      buttons: [...c.querySelectorAll('button')].map(b => window.__a11y.describe(b).name),
      badge: document.querySelector('.rail [data-destination=chat]').getAttribute('aria-label') }`);
    await wd.keys([Keys.ESC]);
    await wd.waitFor(`return !document.querySelector('[data-testid=approval]') && window.__axiom.agent.getSnapshot().active.running === false`, [], 10000, "denied");
    const denied = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .tool-row')].map(r => r.dataset.state)`);
    check(
      `S21 approval card (${theme}): an alert named for the tool and target, focused on Allow once (not typing), all actions named, the rail badge says it; Esc denies`,
      cardA11y.role === "alert" && cardA11y.name === "Approval needed: fs.applyPatch on README.md" && cardA11y.focused === "once" && !!cardA11y.focus &&
        cardA11y.buttons.join() === "Allow once,Allow for session,Deny" && /need attention/.test(cardA11y.badge) && denied.join() === "error",
      JSON.stringify({ ...cardA11y, denied }),
    );
    await wd.exec(`document.querySelector('[aria-label="Toggle AI dock"]').click()`);
    await wd.waitFor(`return !document.querySelector('[data-region=dock]')`, [], 5000, "dock closed");
    // Leave keyboard focus where S20 leaves it (the rail), so the next traversal starts alike.
    await wd.click(await wd.find('.rail [data-destination="files"]'));

    // ---- 5: not by colour alone ----------------------------------------------------------
    const marks = await wd.exec(`
      const dirty = document.querySelector('[role=tab][data-dirty=true] .tab__close')?.getAttribute('aria-label') ?? '';
      const stderr = [...document.querySelectorAll('.output__row[data-stream=stderr]')].every(r => r.querySelector('.visually-hidden')?.textContent.includes('stderr:'));
      const leds = [...document.querySelectorAll('.led')].filter(l => l.getBoundingClientRect().width > 0).map(l => ({ named: !!l.getAttribute('aria-label') || (l.getAttribute('aria-hidden') === 'true' && (l.parentElement?.textContent ?? '').trim().length > 0) }));
      const failed = [...document.querySelectorAll('.output__row[data-kind=statusFailed]')].map(r => r.textContent);
      return { dirty, stderr, leds: leds.length, unlabeled: leds.filter(l => !l.named).length, failed };`);
    check(
      `5 not by colour alone (${theme}): dirty, stderr, LEDs, failed status carry text`,
      /\(unsaved\)/.test(marks.dirty) && marks.stderr && marks.unlabeled === 0 && marks.failed.some((f) => f.includes("Failed")),
      JSON.stringify(marks),
    );

    // ---- hit targets (listed) -----------------------------------------------------------
    t.targets = await wd.exec(`return window.__a11y.targets()`);
  }

  // ---- 4: rendered contrast across S2, S4, S7, S11, S12, S18, S18d, S19, S20, S21, both themes -
  for (const [state, fails] of Object.entries(contrast)) {
    check(`4 rendered contrast ${state}`, fails.length === 0, `${fails.length} text node(s) below the minimum`, fails.map((f) => `${f.region} .${f.cls} "${f.text}" ${f.ratio}:1 < ${f.need} (${f.color})`));
  }

  // ---- 6: reduced motion -----------------------------------------------------------------
  await setTheme(wd, "dark");
  await wd.exec(`window.__axiom.settings.setValue("appearance.motion", "off")`);
  await wd.waitFor(`return document.documentElement.dataset.motion === 'off'`, [], 3000, "motion off");
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  await wd.chord(MOD, "k");
  await wd.waitFor(`return !!document.querySelector('[data-testid=palette]')`, [], 3000, "palette");
  await wd.keys([Keys.ESC]);
  await wd.chord(MOD, "b");
  await wd.chord(MOD, "b");
  await sleep(500);
  const running = await wd.exec(`return document.getAnimations().filter(a => a.playState === 'running' && !a.effect?.target?.closest?.('.monaco-editor')).map(a => (a.animationName || a.transitionProperty) + ' on ' + (a.effect?.target?.className || '?'))`);
  await wd.exec(`window.__axiom.settings.setValue("appearance.motion", "reduced")`);
  await wd.waitFor(`return document.documentElement.dataset.motion === 'reduced'`, [], 3000, "motion reduced");
  await wd.chord(MOD, "k");
  await wd.waitFor(`return !!document.querySelector('[data-testid=palette]')`, [], 3000, "palette");
  const durations = await wd.exec(`return document.getAnimations().filter(a => a.effect?.target?.closest?.('.palette, .overlay')).map(a => a.effect.getTiming().duration)`);
  await wd.keys([Keys.ESC]);
  await wd.exec(`window.__axiom.settings.setValue("appearance.motion", "standard")`);
  check(
    "6 reduced motion: Off leaves nothing running once settled; Reduced presence ≤ 80 ms",
    running.length === 0 && durations.length > 0 && durations.every((d) => d <= 80),
    `Off running ${JSON.stringify(running)}; Reduced palette/scrim durations ${JSON.stringify(durations)} (the setting; the OS media query cannot be emulated in WebKitGTK)`,
  );
  console.log("SKIP  7 forced colours — WebKitGTK has no forced-colours emulation; checked on Windows in QA-005");

  // ---- hit targets ---------------------------------------------------------------------
  const small = record.themes.dark?.targets ?? [];
  console.log(`INFO  hit targets under 24 × 24 px at 100% (spacing exception not evaluated): ${small.length}`);
  for (const s of small) console.log(`        · ${s.region} ${s.role} "${s.name}" ${s.w}×${s.h}`);
} catch (e) {
  check("unexpected failure", false, e instanceof Error ? e.message : String(e));
} finally {
  try {
    await wd?.quit();
  } catch {
    /* gone */
  }
  await removeDir(data);
}

// ---- 200%: screenshots of S4, S7 and S11 with GDK_SCALE=2 -----------------------------------
if (process.platform === "linux" && process.env.AXIOM_A11Y_SKIP_200 !== "1") {
  const data2 = mkdtempSync(join(tmpdir(), "axiom-a11y-data2-"));
  let w2;
  try {
    w2 = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data2], { GDK_SCALE: "2" }, log);
    await w2.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected (200%)");
    await w2.exec(`window.__axiom.documents.requestOpenFolder(arguments[0])`, [ws]);
    await w2.waitFor(`return !!document.querySelector('[data-path="README.md"]')`, [], 10000, "tree (200%)");
    await w2.click(await w2.find('[data-path="README.md"]'));
    await sleep(1500);
    writeFileSync(join(OUT, "s4-200.png"), await w2.screenshot());
    await w2.chord(MOD, "k");
    await sleep(500);
    writeFileSync(join(OUT, "s7-200.png"), await w2.screenshot());
    await w2.keys([Keys.ESC]);
    await w2.chord(MOD, ",");
    await sleep(500);
    writeFileSync(join(OUT, "s11-200.png"), await w2.screenshot());
    console.log(`INFO  200% screenshots in ${OUT} (s4-200, s7-200, s11-200) for the manual review`);
  } catch (e) {
    console.log(`INFO  200% pass not captured: ${e instanceof Error ? e.message : e}`);
  } finally {
    try {
      await w2?.quit();
    } catch {
      /* gone */
    }
    await removeDir(data2);
  }
}

await removeDir(ws);
writeFileSync(join(OUT, "a11y.json"), JSON.stringify({ results, record }, null, 2));
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} a11y checks passed; details in ${join(OUT, "a11y.json")}`);
process.exit(failed.length ? 1 : 0);
