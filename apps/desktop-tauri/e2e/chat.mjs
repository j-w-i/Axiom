#!/usr/bin/env node
// Chat through the real app (M3-UI-001; ADR-0017; workspace-spec.md §5.4) with no folder open and
// the scripted model (AXIOM_TEST_SCRIPTED_MODEL): open Chat from the rail, send by keyboard,
// streamed Markdown with a table and a code block, Copy, edit the first message, regenerate, an
// image (the script checks it arrived), rename (F2) and delete (Delete + dialog). Screens are
// captured in the default theme. Usage: scripts/e2e.sh chat.mjs

import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
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
const onLog = appLogger("chat");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();

const answer = [
  "Here is **Markdown**:",
  "",
  "| Name | Value |",
  "|------|-------|",
  "| one  | 1     |",
  "",
  "```rust",
  "fn main() {",
  '    println!("hello");',
  "}",
  "```",
  "",
  "- a list item",
].join("\n");
const dataDir = mkdtempSync(join(tmpdir(), "axiom-chat-data-"));
const script = join(dataDir, "script.json");
writeFileSync(
  script,
  JSON.stringify({
    turns: [
      { expect: { userContains: "markdown", noTools: true }, events: [{ type: "delay", ms: 30 }, { type: "text", text: answer }] },
      { expect: { userContains: "edited question" }, events: [{ type: "text", text: "An edited answer." }] },
      { expect: { userContains: "edited question" }, events: [{ type: "text", text: "A regenerated answer." }] },
      { expect: { userImages: 1, userContains: "picture" }, events: [{ type: "text", text: "I see one image." }] },
    ],
  }),
);
// A 1×1 PNG.
const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
const appExpr = "window.__axiom";
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], { AXIOM_TEST_SCRIPTED_MODEL: script }, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");

  // 1. Chat from the rail, no folder open.
  const rail = await wd.find('[data-destination="chat"]');
  await wd.cmd("POST", `/element/${rail}/click`, {});
  await wd.waitFor(`return !!document.querySelector('[data-testid=chat-page]')`, [], 10000, "the Chat page");
  await wd.waitFor(`return ${appExpr}.agent.getSnapshot().status === 'ready' && !!${appExpr}.agent.getSnapshot().model`, [], 10000, "a model");
  const model = await wd.exec(`return ${appExpr}.agent.getSnapshot().model`);
  check("1 Chat opens from the rail with no folder; a model is chosen", model?.providerId === "scripted", JSON.stringify(model));
  writeFileSync(join(SHOTS, "chat-empty.png"), Buffer.from(await wd.screenshot(), "base64"));

  // 2. Type and send with Enter.
  const field = await wd.find('[data-testid=composer] textarea[aria-label="Message"]');
  await wd.cmd("POST", `/element/${field}/click`, {});
  await wd.keys("Please show some markdown");
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return !!document.querySelector('[data-testid=chat-log] .code-block')`, [], 20000, "the answer's code block");
  await wd.waitFor(`return ${appExpr}.agent.getSnapshot().active.running === false`, [], 10000, "the turn's end");
  const shape = await wd.exec(`
    const log = document.querySelector('[data-testid=chat-log]');
    return {
      user: log.querySelectorAll('article[aria-label="You"]').length,
      assistant: log.querySelectorAll('article[aria-label="Assistant"]').length,
      table: !!log.querySelector('.md table td'),
      strong: log.querySelector('.md strong')?.textContent,
      lang: log.querySelector('.code-block__lang')?.textContent,
      copy: !!log.querySelector('[aria-label="Copy code"]'),
      title: document.querySelector('.chat__title')?.textContent,
    };`);
  check(
    "2 Enter sends; the answer is Markdown with a table and a rust code block with Copy",
    shape.user === 1 && shape.assistant === 1 && shape.table && shape.strong === "Markdown" && shape.lang === "rust" && shape.copy,
    JSON.stringify(shape),
  );
  check("2b the conversation is titled from the first message", shape.title === "Please show some markdown", shape.title);
  await new Promise((r) => setTimeout(r, 400)); // Monaco colouring is coalesced
  const coloured = await wd.exec(`return !!document.querySelector('.code-block__body span[class^="mtk"]')`);
  check("2c the code block is coloured by Monaco", coloured);
  writeFileSync(join(SHOTS, "chat-answer.png"), Buffer.from(await wd.screenshot(), "base64"));

  // 3. Edit the first message: ↑ in the empty composer opens the editor.
  await wd.cmd("POST", `/element/${field}/click`, {});
  await wd.keys(Keys.UP);
  const editor = await wd.find('textarea[aria-label="Edit message"]', 5000);
  // Select all and type over it (WebDriver's clear blurs the field).
  await wd.chord(MOD, "a");
  await wd.keys("An edited question");
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('An edited answer.'))`, [], 10000, "the edited answer");
  const afterEdit = await wd.exec(`
    const log = document.querySelector('[data-testid=chat-log]');
    return { users: [...log.querySelectorAll('article[aria-label="You"] .msg__text')].map(e => e.textContent), answers: log.querySelectorAll('article[aria-label="Assistant"]').length };`);
  check("3 editing replaces the branch: one user message, one answer", afterEdit.users.length === 1 && afterEdit.users[0] === "An edited question" && afterEdit.answers === 1, JSON.stringify(afterEdit));

  // 4. Regenerate.
  await wd.waitFor(`return ${appExpr}.agent.getSnapshot().active.running === false`, [], 10000, "idle");
  await wd.exec(`document.querySelector('[data-testid=chat-regenerate]').click()`);
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('A regenerated answer.'))`, [], 10000, "the regenerated answer");
  const answers = await wd.exec(`return document.querySelectorAll('[data-testid=chat-log] article[aria-label="Assistant"]').length`);
  check("4 Regenerate replaces the last answer", answers === 1, `${answers} answers`);

  // 5. An image through the composer's file input (the picker's path), then send.
  await wd.waitFor(`return ${appExpr}.agent.getSnapshot().active.running === false`, [], 10000, "idle");
  await wd.exec(`
    const bytes = Uint8Array.from(atob(arguments[0]), c => c.charCodeAt(0));
    const dt = new DataTransfer();
    dt.items.add(new File([bytes], 'dot.png', { type: 'image/png' }));
    const input = document.querySelector('[data-testid=composer] input[type=file]');
    input.files = dt.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));`, [PNG]);
  await wd.waitFor(`return document.querySelectorAll('.composer__chip').length === 1`, [], 5000, "the image chip");
  await wd.cmd("POST", `/element/${field}/click`, {});
  await wd.keys("What is in this picture?");
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('I see one image.'))`, [], 10000, "the image answer");
  await wd.waitFor(`return !!document.querySelector('[data-testid=chat-log] img.msg__thumb')`, [], 5000, "the thumbnail");
  check("5 an attached image reaches the model and shows as a thumbnail", true);
  writeFileSync(join(SHOTS, "chat-image.png"), Buffer.from(await wd.screenshot(), "base64"));

  // 6. Rename with F2, delete with Delete + the dialog.
  const row = await wd.find('.chat-row__open[aria-current="true"]');
  await wd.cmd("POST", `/element/${row}/click`, {});
  await wd.keys(""); // F2
  const rename = await wd.find('input[aria-label="Conversation title"]', 5000);
  await wd.chord(MOD, "a");
  await wd.keys("Markdown demo");
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return [...document.querySelectorAll('.chat-row__title')].some(e => e.textContent === 'Markdown demo')`, [], 5000, "the new title");
  check("6 F2 renames the conversation", true);
  const row2 = await wd.find('.chat-row__open[aria-current="true"]');
  await wd.cmd("POST", `/element/${row2}/click`, {});
  await wd.keys(Keys.DELETE);
  await wd.find('[data-testid=chat-delete]', 5000);
  await wd.exec(`document.querySelector('[data-testid=chat-delete] [data-action=delete]').click()`);
  await wd.waitFor(`return document.querySelectorAll('.chat-row').length === 0`, [], 5000, "the list to empty");
  const listed = await wd.execAsync(`const done = arguments[arguments.length - 1]; ${appExpr}.client.request('agent/sessionList', {}).then(r => done(r.sessions.length), e => done(String(e)))`);
  check("6b Delete (with the dialog) removes it from the daemon too", listed === 0, `${listed} left`);
} catch (e) {
  check("chat suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("chat", console.log);
} finally {
  if (wd) await wd.quit();
  removeDir(dataDir);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} chat checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
