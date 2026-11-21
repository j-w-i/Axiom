#!/usr/bin/env node
// The AI dock beside the editor and the approval flow through the real app (M3-UI-002;
// workspace-spec.md §5.2), with the scripted model and the default policy (writes ask):
// Mod+L opens the dock → an edit asks → the card shows the diff → Allow once → the file changes;
// the next edit asks → Esc denies → the file does not change; the rail shows the waiting
// approval; Restore puts the file back; a file the user changed after the agent gives the
// conflict dialog and stays as the user left it. Usage: scripts/e2e.sh dock.mjs

import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const onLog = appLogger("dock");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";

const dataDir = mkdtempSync(join(tmpdir(), "axiom-dock-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-dock-ws-"));
mkdirSync(join(ws, "src"));
const LIB = join(ws, "src", "lib.rs");
writeFileSync(LIB, "fn a() {}\n");
const edit = (id, search, replace) => ({ type: "toolCall", id, name: "fs.applyPatch", input: { path: "src/lib.rs", edits: [{ search, replace }] } });
const script = join(dataDir, "script.json");
writeFileSync(
  script,
  JSON.stringify({
    turns: [
      { expect: { userContains: "public" }, events: [{ type: "text", text: "Making it public." }, edit("c1", "fn a() {}", "pub fn a() {}")] },
      { expect: { toolResult: { callId: "c1", isError: false } }, events: [edit("c2", "pub fn a() {}", "pub fn a2() {}")] },
      { expect: { toolResult: { callId: "c2", isError: true, contains: "denied" } }, events: [{ type: "text", text: "Understood, I left it." }] },
      { expect: { userContains: "rename" }, events: [edit("c3", "fn a() {}", "fn z() {}")] },
      { expect: { toolResult: { callId: "c3", isError: false } }, events: [{ type: "text", text: "Renamed." }] },
    ],
  }),
);

/** Waits for an approval card, returns its tool and targets. */
async function card(wd) {
  await wd.waitFor(`return !!document.querySelector('[data-testid=chat-dock] [data-testid=approval]')`, [], 10000, "an approval card");
  return wd.exec(`const c = document.querySelector('[data-testid=chat-dock] [data-testid=approval]'); return { label: c.getAttribute('aria-label'), role: c.getAttribute('role'), diff: !!c.querySelector('.diff'), rail: document.querySelector('.rail [data-destination=chat]').getAttribute('aria-label') }`);
}

async function send(wd, text) {
  const field = await wd.find('[data-testid=chat-dock] textarea[aria-label="Message"]');
  await wd.click(field);
  await wd.keys(text);
  await wd.keys(Keys.ENTER);
}

const idle = (wd) => wd.waitFor(`return ${appExpr}.agent.getSnapshot().active.running === false`, [], 15000, "the turn's end");

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], { AXIOM_TEST_SCRIPTED_MODEL: script }, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 10000, "the workspace");

  // 1. Mod+L opens the dock (the folder scope).
  await wd.click(await wd.find('[data-path="src"]'));
  await wd.chord(MOD, "l");
  await wd.waitFor(`return !!document.querySelector('[data-region=dock] [data-testid=chat-dock]')`, [], 5000, "the dock");
  await wd.waitFor(`return ${appExpr}.agent.getSnapshot().scope === 'workspace' && !!${appExpr}.agent.getSnapshot().model`, [], 10000, "the folder scope");
  check("1 Mod+L opens the AI dock beside the editor, in the folder scope", true);

  // 2. An edit asks; the card names it and shows the diff; Allow once applies it.
  await send(wd, "Make a public");
  const c1 = await card(wd);
  check(
    "2 the edit asks: a named alert card with the diff, and the rail shows the waiting approval",
    c1.role === "alert" && c1.label === "Approval needed: fs.applyPatch on src/lib.rs" && c1.diff && /need attention/.test(c1.rail),
    JSON.stringify(c1),
  );
  writeFileSync(join(SHOTS, "dock-approval.png"), Buffer.from(await wd.screenshot(), "base64"));
  const first = await wd.exec(`return ${appExpr}.agent.getSnapshot().approvals[0].requestId`);
  await wd.click(await wd.find('[data-testid=chat-dock] [data-testid=approval] [data-answer=once]'));

  // 3. The next edit asks too; Esc on the card denies it.
  await wd.waitFor(`const a = ${appExpr}.agent.getSnapshot().approvals; return a.length === 1 && a[0].requestId !== arguments[0] && !!document.querySelector('[data-testid=chat-dock] [data-testid=approval]')`, [first], 10000, "the second card");
  await wd.exec(`document.querySelector('[data-testid=chat-dock] [data-testid=approval] [data-answer=once]').focus()`);
  await wd.keys(Keys.ESC);
  await idle(wd);
  const lib1 = readFileSync(LIB, "utf8");
  check("3 Allow once applied the first edit; Esc denied the second", lib1 === "pub fn a() {}\n", JSON.stringify(lib1));
  const view = await wd.exec(`const d = document.querySelector('[data-testid=chat-dock]'); return {
    diff: !!d.querySelector('.tool-row .diff'),
    failed: [...d.querySelectorAll('.tool-row[data-state=error]')].length,
    checkpoint: !!d.querySelector('.chat__checkpoint [data-testid=restore-1]'),
    cards: d.querySelectorAll('[data-testid=approval]').length,
    rail: document.querySelector('.rail [data-destination=chat]').getAttribute('aria-label') }`);
  check(
    "3b the applied edit shows as a diff block, the denied one as failed; a checkpoint marker with Restore; no card left; the badge cleared",
    view.diff && view.failed === 1 && view.checkpoint && view.cards === 0 && view.rail === "Chat",
    JSON.stringify(view),
  );
  writeFileSync(join(SHOTS, "dock-diff.png"), Buffer.from(await wd.screenshot(), "base64"));

  // 4. Restore checkpoint 1: the file is as before the agent.
  await wd.click(await wd.find('[data-testid=chat-dock] [data-testid=restore-1]'));
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-dock] .chat__notice')].some(e => e.textContent.startsWith('Restored checkpoint 1'))`, [], 10000, "the restore note");
  const lib2 = readFileSync(LIB, "utf8");
  check("4 Restore puts the file back and says how to undo it", lib2 === "fn a() {}\n", JSON.stringify(lib2));

  // 5. The agent changes it again, then the user does: Restore reports the conflict and leaves
  //    the user's version.
  await send(wd, "Please rename it");
  await card(wd);
  await wd.click(await wd.find('[data-testid=chat-dock] [data-testid=approval] [data-answer=once]'));
  await idle(wd);
  writeFileSync(LIB, "fn mine() {}\n");
  const cp = await wd.exec(`return Math.max(...[...document.querySelectorAll('[data-testid=chat-dock] .chat__checkpoint')].map(e => Number(e.dataset.checkpoint)))`);
  await wd.exec(`document.querySelector('[data-testid=chat-dock] [data-testid=restore-' + arguments[0] + ']').click()`, [cp]);
  await wd.find("[data-testid=restore-conflict]", 5000);
  const conflict = await wd.exec(`return document.querySelector('[data-testid=restore-conflict]').textContent`);
  await wd.click(await wd.find("[data-testid=restore-conflict] [data-action=cancel]"));
  const lib3 = readFileSync(LIB, "utf8");
  check(
    "5 a file changed after the agent is a conflict: listed, nothing restored, the user's text kept",
    /src\/lib\.rs/.test(conflict) && /edited after the agent/.test(conflict) && lib3 === "fn mine() {}\n",
    JSON.stringify({ conflict: conflict.slice(0, 120), lib3 }),
  );

  // 6. Mod+L closes the dock.
  await wd.chord(MOD, "l");
  await wd.waitFor(`return !document.querySelector('[data-region=dock]')`, [], 5000, "the dock to close");
  check("6 Mod+L closes the dock", true);
} catch (e) {
  check("dock suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("dock", console.log);
} finally {
  if (wd) await wd.quit();
  removeDir(dataDir);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} dock checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
