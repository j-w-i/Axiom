#!/usr/bin/env node
// The M3 exit scenario through the UI (M3-QA-001; docs/roadmap/milestones.md §M3), with the
// scripted model and real cargo:
//   "add a function and fix the build" completes through tools only (read → build fails →
//   patch, approved in the UI → build passes); every write was preceded by a checkpoint and can
//   be restored; a denied operation does not execute; the session resumes after the app quits
//   and starts again. Then, in the restarted app: a configured MCP server's tools in a turn
//   (each call approved, an error reported as one), and an ACP agent (tests/helpers/fake-acp)
//   whose write asks through Axiom's gate and happens only after Allow.
// First, the Chat scenario of ADR-0017, with no folder open and by keyboard only, against a fake
// OpenAI-compatible server (the real adapter): open Chat from the palette, configure the model in
// the first-run setup, converse (Markdown with a table and a code block), edit the message (↑),
// regenerate (Tab to it, Enter), paste an image.
// Usage: scripts/e2e.sh agent.mjs

import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
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
const onLog = appLogger("agent");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";

// The fake MCP server and ACP agent (tests/helpers), built next to the daemon.
const built = spawnSync("cargo", ["build", "-q", "-p", "fake-mcp", "-p", "fake-acp"], { cwd: repo, stdio: "inherit" });
if (built.status !== 0) throw new Error("building the fake MCP server and ACP agent failed");
const FAKE_MCP = join(repo, `target/debug/fake-mcp${EXE}`);
const FAKE_ACP = join(repo, `target/debug/fake-acp${EXE}`);

const dataDir = mkdtempSync(join(tmpdir(), "axiom-agent-data-"));
const ws = mkdtempSync(join(tmpdir(), "axiom-agent-ws-"));
writeFileSync(join(ws, "Cargo.toml"), '[package]\nname = "exit"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n');
writeFileSync(join(ws, ".gitignore"), "target/\n");
mkdirSync(join(ws, "src"));
const MAIN = join(ws, "src", "main.rs");
const ORIGINAL = 'fn main() {\n    println!("{}", add(2, 3));\n}\n';
writeFileSync(MAIN, ORIGINAL);

const script1 = join(dataDir, "script-1.json");
writeFileSync(
  script1,
  JSON.stringify({
    turns: [
      { expect: { userContains: "fix the build", toolsInclude: ["fs.applyPatch", "build.run"] }, events: [{ type: "text", text: "I'll look first." }, { type: "toolCall", id: "r", name: "fs.read", input: { path: "src/main.rs" } }] },
      { expect: { toolResult: { callId: "r", contains: "add(2, 3)" } }, events: [{ type: "toolCall", id: "b1", name: "build.run", input: { task: "build" } }] },
      { expect: { toolResult: { callId: "b1", contains: "E0425" } }, events: [{ type: "text", text: "`add` is missing; adding it." }, { type: "toolCall", id: "p", name: "fs.applyPatch", input: { path: "src/main.rs", edits: [{ search: "fn main() {", replace: "fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\nfn main() {" }] } }] },
      { expect: { toolResult: { callId: "p", isError: false } }, events: [{ type: "toolCall", id: "b2", name: "build.run", input: { task: "build" } }] },
      { expect: { toolResult: { callId: "b2", contains: "succeeded" } }, events: [{ type: "text", text: "Done: the build passes." }] },
      // A destructive command: the person denies it.
      { expect: { userContains: "clean up" }, events: [{ type: "toolCall", id: "x", name: "task.run", input: { argv: ["rm", "-rf", "src"] } }] },
      { expect: { toolResult: { callId: "x", isError: true, contains: "denied" } }, events: [{ type: "text", text: "Understood, I won't delete anything." }] },
    ],
  }),
);
const script2 = join(dataDir, "script-2.json");
writeFileSync(
  script2,
  JSON.stringify({
    turns: [
      { expect: { historyContains: "the build passes", userContains: "still there" }, events: [{ type: "text", text: "Yes: this conversation resumed after the restart." }] },
      { expect: { userContains: "use the server", toolsInclude: ["mcp.fake.echo", "mcp.fake.fail"] }, events: [{ type: "toolCall", id: "m1", name: "mcp.fake.echo", input: { text: "hi" } }] },
      { expect: { toolResult: { callId: "m1", contains: "echo: hi", isError: false } }, events: [{ type: "toolCall", id: "m2", name: "mcp.fake.fail", input: {} }] },
      { expect: { toolResult: { callId: "m2", contains: "it failed", isError: true } }, events: [{ type: "text", text: "The server echoed; its failing tool failed." }] },
    ],
  }),
);

const idle = (wd) => wd.waitFor(`return ${appExpr}.agent.getSnapshot().active.running === false && ${appExpr}.agent.getSnapshot().approvals.length === 0`, [], 120000, "the turn's end");
async function send(wd, text) {
  const field = await wd.find('[data-testid=chat-dock] textarea[aria-label="Message"]');
  await wd.click(field);
  await wd.keys(text);
  await wd.keys(Keys.ENTER);
}
/** Answers approvals in the dock until the turn ends; returns the tools asked about. */
async function answer(wd, decide) {
  const asked = [];
  // The turn must have started (a send's response can come before the store marks it running):
  // otherwise "not running" would read as "already ended" and an approval would go unanswered.
  await wd.waitFor(`const s = ${appExpr}.agent.getSnapshot(); return s.active.running || s.approvals.length > 0`, [], 15000, "the turn's start");
  const end = Date.now() + 180000;
  while (Date.now() < end) {
    const s = await wd.exec(`const s = ${appExpr}.agent.getSnapshot(); return { running: s.active.running, card: !!document.querySelector('[data-testid=chat-dock] [data-testid=approval]'), tool: s.approvals[0]?.tool, argv: s.approvals[0]?.argv }`);
    if (s.card) {
      asked.push(s.tool + (s.argv ? " " + s.argv.join(" ") : ""));
      await wd.click(await wd.find(`[data-testid=chat-dock] [data-testid=approval] [data-answer=${decide(s)}]`));
      await new Promise((r) => setTimeout(r, 200));
      continue;
    }
    if (!s.running) return asked;
    await new Promise((r) => setTimeout(r, 150));
  }
  throw new Error("the turn did not end");
}

// ---- 0. Chat, no folder, by keyboard ---------------------------------------------------
const ANSWER = ["Here is **Markdown**:", "", "| Name | Value |", "|------|-------|", "| one  | 1     |", "", "```rust", "fn main() {", '    println!("hello");', "}", "```"].join("\n");
const requests = [];
const fake = createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    if (req.method === "GET" && req.url === "/v1/models") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ object: "list", data: [{ id: "fake-gpt", object: "model" }] }));
      return;
    }
    if (req.method !== "POST" || req.url !== "/v1/chat/completions") return void res.writeHead(404).end();
    const msgs = JSON.parse(body).messages ?? [];
    const last = msgs.filter((m) => m.role === "user").at(-1);
    const parts = typeof last?.content === "string" ? [{ type: "text", text: last.content }] : (last?.content ?? []);
    const text = parts.filter((c) => c.type === "text").map((c) => c.text).join(" ");
    const images = parts.filter((c) => c.type === "image_url").length;
    requests.push({ text, images, turns: msgs.length });
    const edited = requests.filter((r) => r.text.includes("edited")).length;
    const reply = images ? `I see ${images} image.` : text.includes("edited") ? (edited === 1 ? "An edited answer." : "A regenerated answer.") : ANSWER;
    res.writeHead(200, { "content-type": "text/event-stream" });
    const chunk = (delta, finish = null) => `data: ${JSON.stringify({ id: "c", object: "chat.completion.chunk", choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`;
    res.write(chunk({ role: "assistant", content: "" }));
    for (const piece of reply.match(/[\s\S]{1,12}/g)) res.write(chunk({ content: piece }));
    res.write(chunk({}, "stop"));
    res.end("data: [DONE]\n\n");
  });
});
await new Promise((r) => fake.listen(0, "127.0.0.1", r));
const BASE = `http://127.0.0.1:${fake.address().port}/v1`;
const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

/** Presses Tab until the focused element matches `css` (keyboard only). */
async function tabTo(wd, css, max = 60) {
  for (let i = 0; i < max; i++) {
    if (await wd.exec(`return !!document.activeElement?.matches(arguments[0])`, [css])) return;
    await wd.keys(Keys.TAB);
  }
  throw new Error(`Tab never reached ${css}`);
}
const focused = (wd, css) => wd.exec(`return !!document.activeElement?.matches(arguments[0])`, [css]);
const chatIdle = (wd) => wd.waitFor(`return ${appExpr}.agent.getSnapshot().active.running === false`, [], 15000, "the turn's end");

async function chatByKeyboard() {
  const data = mkdtempSync(join(tmpdir(), "axiom-agent-chat-"));
  let wd;
  try {
    wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data], { AXIOM_TEST_CREDENTIALS_FILE: join(data, "credentials.json") }, onLog);
    await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
    // Open Chat from the command palette.
    await wd.chord(MOD, "k");
    await wd.find('[role=dialog] input', 5000);
    await wd.keys("Open Chat");
    await wd.keys(Keys.ENTER);
    await wd.find("[data-testid=model-setup]", 10000);
    // The setup: OpenAI-compatible (arrow keys in the kind control), the address, Enter.
    await tabTo(wd, '[data-testid=model-setup] [role=radio][aria-checked=true]');
    await wd.keys(Keys.RIGHT);
    await tabTo(wd, '[data-testid=model-setup] input[aria-label="Server address"]');
    await wd.keys(BASE);
    await wd.keys(Keys.ENTER);
    await wd.waitFor(`return !document.querySelector('[data-testid=model-setup]') && ${appExpr}.agent.getSnapshot().model?.model === 'fake-gpt'`, [], 15000, "the setup to finish");
    const ws0 = await wd.exec(`return ${appExpr}.workspace.getSnapshot().id`);
    check("0a with no folder open, Chat opens from the palette and the model is configured in the first-run setup by keyboard", ws0 === "", JSON.stringify(ws0));
    // Converse: the composer has focus.
    await wd.waitFor(`return document.activeElement?.matches('[data-testid=composer] textarea')`, [], 5000, "the composer focused");
    await wd.keys("Please show some markdown");
    await wd.keys(Keys.ENTER);
    await wd.waitFor(`return !!document.querySelector('[data-testid=chat-log] .code-block')`, [], 15000, "the code block");
    await chatIdle(wd);
    const shape = await wd.exec(`const l = document.querySelector('[data-testid=chat-log]'); return { table: !!l.querySelector('.md table td'), strong: l.querySelector('.md strong')?.textContent, lang: l.querySelector('.code-block__lang')?.textContent }`);
    check("0b the answer streams as Markdown with a table and a rust code block", shape.table && shape.strong === "Markdown" && shape.lang === "rust", JSON.stringify(shape));
    // Edit with ↑ in the empty composer.
    await tabTo(wd, '[data-testid=composer] textarea');
    await wd.keys(Keys.UP);
    await wd.find('textarea[aria-label="Edit message"]', 5000);
    await wd.chord(MOD, "a");
    await wd.keys("An edited question");
    await wd.keys(Keys.ENTER);
    await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('An edited answer.'))`, [], 15000, "the edited answer");
    await chatIdle(wd);
    const users = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-log] article[aria-label="You"] .msg__text')].map(e => e.textContent)`);
    check("0c ↑ edits the message and the branch is replaced", users.join() === "An edited question", JSON.stringify(users));
    // Regenerate: Tab to the button, Enter.
    await tabTo(wd, "[data-testid=chat-regenerate]");
    await wd.keys(Keys.ENTER);
    await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('A regenerated answer.'))`, [], 15000, "the regenerated answer");
    await chatIdle(wd);
    const answers = await wd.exec(`return document.querySelectorAll('[data-testid=chat-log] article[aria-label="Assistant"]').length`);
    check("0d Regenerate by keyboard replaces the answer", answers === 1, `${answers} answers`);
    // An image pasted into the composer (the paste path; the clipboard itself is the OS's).
    await tabTo(wd, '[data-testid=composer] textarea');
    await wd.exec(`
      const bytes = Uint8Array.from(atob(arguments[0]), c => c.charCodeAt(0));
      const dt = new DataTransfer();
      dt.items.add(new File([bytes], 'dot.png', { type: 'image/png' }));
      document.activeElement.dispatchEvent(new ClipboardEvent('paste', { clipboardData: dt, bubbles: true, cancelable: true }));`, [PNG]);
    await wd.waitFor(`return document.querySelectorAll('.composer__chip').length === 1`, [], 5000, "the image chip");
    const still = await focused(wd, "[data-testid=composer] textarea");
    await wd.keys("What is in this picture?");
    await wd.keys(Keys.ENTER);
    await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('I see 1 image.'))`, [], 15000, "the image answer");
    await wd.waitFor(`return !!document.querySelector('[data-testid=chat-log] img.msg__thumb')`, [], 5000, "the thumbnail");
    check("0e a pasted image reaches the model as an image part and shows as a thumbnail", still && requests.at(-1)?.images === 1, JSON.stringify(requests.at(-1)));
    writeFileSync(join(SHOTS, "agent-chat.png"), Buffer.from(await wd.screenshot(), "base64"));
  } catch (e) {
    check("chat by keyboard", false, String(e?.stack ?? e));
    if (wd) await wd.diagnose("agent-chat", console.log);
  } finally {
    if (wd) await wd.quit().catch(() => {});
    fake.close();
    removeDir(data);
  }
}
await chatByKeyboard();

/** Clicks an element, finding it again if a re-render replaced it. */
async function clickFresh(wd, css) {
  for (let i = 0; ; i++) {
    try {
      return await wd.click(await wd.find(css));
    } catch (e) {
      if (i >= 5 || !/stale element/.test(String(e))) throw e;
      await new Promise((r) => setTimeout(r, 200));
    }
  }
}

let wd;
try {
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], { AXIOM_TEST_SCRIPTED_MODEL: script1 }, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");
  await wd.exec(`${appExpr}.documents.requestOpenFolder(arguments[0])`, [ws]);
  await wd.waitFor(`return !!document.querySelector('[data-path="src"]')`, [], 10000, "the workspace");
  // The default policy (reads allowed, the rest asks) plus one user rule: `cargo build` runs
  // without asking, as a user would set it.
  await wd.exec(`${appExpr}.client.request('agent/setPolicy', { default: 'ask', rules: [ { effect: 'read', decision: 'allow' }, { effect: 'execute', argvPrefix: ['cargo', 'build'], decision: 'allow' } ] })`);
  await wd.click(await wd.find('[data-path="src"]'));
  await wd.chord(MOD, "l");
  await wd.waitFor(`return !!document.querySelector('[data-testid=chat-dock]') && ${appExpr}.agent.getSnapshot().scope === 'workspace' && !!${appExpr}.agent.getSnapshot().model`, [], 10000, "the dock");

  // 1. Add the function and fix the build, through tools only; the write is approved in the UI.
  await send(wd, "Please fix the build");
  const asked1 = await answer(wd, () => "once");
  await idle(wd);
  const fixed = readFileSync(MAIN, "utf8");
  const tools = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .tool-row')].map(r => r.dataset.tool + ':' + r.dataset.state)`);
  check(
    "1 add a function and fix the build through tools only: read → build fails → patch (approved in the UI) → build passes",
    fixed.startsWith("fn add(a: i32, b: i32) -> i32 {") && tools.join() === "fs.read:ok,build.run:ok,fs.applyPatch:ok,build.run:ok" && asked1.join() === "fs.applyPatch",
    JSON.stringify({ tools, asked1 }),
  );
  const buildRows = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .tool-row[data-tool="build.run"] .tool-row__summary')].map(e => e.textContent)`);
  check("1b the first build failed with an error, the second succeeded", /^failed · 1 errors/.test(buildRows[0] ?? "") && /^succeeded · 0 errors/.test(buildRows[1] ?? ""), JSON.stringify(buildRows));

  // 2. Every write was preceded by a checkpoint: a tool's row appears when the model calls it,
  //    the turn's checkpoint is taken after the gate and before the first non-read tool runs
  //    (the first build), so it sits right after that row and before the patch.
  const order = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .chat__checkpoint, [data-testid=chat-dock] .tool-row')].map(e => e.classList.contains('tool-row') ? e.dataset.tool : 'checkpoint:' + e.textContent.split(' · ')[1])`);
  check(
    "2 the turn's checkpoint is taken before its first non-read tool runs, so before every write",
    order.join() === "fs.read,build.run,checkpoint:before build.run,fs.applyPatch,build.run",
    order.join(" → "),
  );
  writeFileSync(join(SHOTS, "agent-fixed.png"), Buffer.from(await wd.screenshot(), "base64"));

  // 3. A denied operation does not execute.
  await send(wd, "Now clean up");
  const asked2 = await answer(wd, () => "deny");
  await idle(wd);
  const checkpoints = await wd.exec(`return document.querySelectorAll('[data-testid=chat-dock] .chat__checkpoint').length`);
  check("3 a denied `rm -rf src` does not run: src/ is still there, and no checkpoint was needed", existsSync(MAIN) && asked2.join() === "task.run rm -rf src" && checkpoints === 1, JSON.stringify({ asked2, checkpoints }));

  // 4. Restore the checkpoint: the original file is back.
  await wd.click(await wd.find("[data-testid=chat-dock] [data-testid=restore-1]"));
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-dock] .chat__notice')].some(e => e.textContent.startsWith('Restored checkpoint 1'))`, [], 10000, "the restore");
  check("4 Restore puts the original file back", readFileSync(MAIN, "utf8") === ORIGINAL);
  const sid = await wd.exec(`return ${appExpr}.agent.getSnapshot().activeId`);

  // 5. Quit and start again: the session resumes with its history.
  await wd.exec(`${appExpr}.documents.requestQuit()`);
  await wd.waitForExit(15000).catch(() => {});
  await wd.quit().catch(() => {});
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], { AXIOM_TEST_SCRIPTED_MODEL: script2 }, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready && !!document.querySelector('[data-path="src"]')`, [], 20000, "the reopened workspace");
  await clickFresh(wd, '[data-path="src"]');
  // The layout (with the dock open) is restored from the last run: the dock is already there.
  await wd.waitFor(`return !!document.querySelector('[data-region=dock] [data-testid=chat-dock]')`, [], 5000, "the restored dock");
  await wd.waitFor(`return !!document.querySelector('[data-testid=chat-dock]') && ${appExpr}.agent.getSnapshot().sessions.some(s => s.sessionId === arguments[0])`, [sid], 10000, "the session listed");
  await wd.exec(`${appExpr}.agent.open(arguments[0])`, [sid]);
  await wd.waitFor(`return document.querySelectorAll("[data-testid=chat-dock] .tool-row").length >= 5`, [], 10000, "the replayed history").catch(async (e) => { console.log(await wd.exec(`const s = ${appExpr}.agent.getSnapshot(); return JSON.stringify({ activeId: s.activeId, scope: s.scope, rows: document.querySelectorAll("[data-testid=chat-dock] .tool-row").length, text: document.querySelector("[data-testid=chat-dock]")?.textContent.slice(0, 600) })`)); throw e; });
  await send(wd, "Is the fix still there in your memory?");
  await idle(wd);
  const resumed = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .md')].some(e => e.textContent.includes('resumed after the restart'))`);
  check("5 after quitting and starting again, the session replays and continues with its context", resumed);

  // 6. An MCP server configured in settings: its tools are in the turn, each call asks.
  await wd.exec(`return ${appExpr}.client.request('settings/set', { values: { 'agent.mcpServers': [ { name: 'fake', argv: [arguments[0]] } ] } })`, [FAKE_MCP]);
  await wd.exec(`${appExpr}.agent.newChat('workspace')`);
  await send(wd, "Please use the server");
  const asked3 = await answer(wd, () => "once");
  await idle(wd);
  const mcpRows = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .tool-row')].map(r => r.dataset.tool + ':' + r.dataset.state)`);
  check(
    "6 an MCP server's tools run in a turn after approval; its error is reported as a failed call",
    mcpRows.join() === "mcp.fake.echo:ok,mcp.fake.fail:error" && asked3.map((a) => a.split(" ")[0]).join() === "mcp.fake.echo,mcp.fake.fail",
    JSON.stringify({ mcpRows, asked3 }),
  );

  // 7. An ACP agent: chosen like a model; its write asks through Axiom's gate.
  const err = await wd.exec(`return ${appExpr}.agent.configure({ id: 'fake-agent', kind: 'acp', name: 'Fake Agent', command: [arguments[0]], env: {}, models: [] })`, [FAKE_ACP]);
  await wd.waitFor(`return ${appExpr}.agent.getSnapshot().providers.some(p => p.providerId === 'fake-agent' && p.models.length > 0)`, [], 10000, "the agent listed");
  await wd.exec(`${appExpr}.agent.newChat('workspace'); ${appExpr}.agent.setModel({ providerId: 'fake-agent', model: 'default' })`);
  await send(wd, "write the notes");
  const asked4 = await answer(wd, () => "once");
  await idle(wd);
  const notes = existsSync(join(ws, "notes.md")) ? readFileSync(join(ws, "notes.md"), "utf8") : null;
  const said = await wd.exec(`return [...document.querySelectorAll('[data-testid=chat-dock] .md')].some(e => e.textContent.includes('wrote notes.md'))`);
  check("7 an ACP agent's write asks in the UI and happens after Allow once", !err && notes === "hello\n" && said && asked4.length === 1, JSON.stringify({ err, notes, said, asked4 }));
  writeFileSync(join(SHOTS, "agent-acp.png"), Buffer.from(await wd.screenshot(), "base64"));
} catch (e) {
  check("agent suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("agent", console.log);
} finally {
  if (wd) await wd.quit().catch(() => {});
  removeDir(dataDir);
  removeDir(ws);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} agent checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
