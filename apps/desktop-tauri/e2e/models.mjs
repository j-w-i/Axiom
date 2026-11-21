#!/usr/bin/env node
// First-run model setup and the Models pane through the real app (M3-UI-003; ADR-0017 §5),
// against a fake OpenAI-compatible server on loopback (the real adapter, no scripted model):
// nothing configured → Chat shows the setup → add the server with a key → the key leaves the
// webview → chat with it → Settings → Models lists it with "Key stored" → remove it → the setup
// returns. Usage: scripts/e2e.sh models.mjs

import { createServer } from "node:http";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { Keys, WebDriver, appLogger, removeDir } from "./webdriver.mjs";
import { HELPERS } from "./a11y-helpers.mjs";

/** Accessibility of a surface (the a11y suite's own helpers): rendered contrast, and every
 * control in `root` named, with a role, showing focus when focused. */
async function audit(wd, label, root) {
  await wd.exec(HELPERS);
  const contrast = await wd.exec(`return window.__a11y.contrastScan()`);
  // Keyboard modality first: focus moved by script after a mouse click is not :focus-visible.
  await wd.exec(`document.querySelector(arguments[0] + ' button, ' + arguments[0] + ' input')?.focus()`, [root]);
  await wd.keys(["\uE004"]); // Tab
  const controls = await wd.exec(`return [...document.querySelectorAll(arguments[0] + ' button, ' + arguments[0] + ' input, ' + arguments[0] + ' [role=radio]')]
    .filter(e => e.getBoundingClientRect().width > 0 && !e.disabled)
    .map(e => { e.focus(); const d = window.__a11y.describe(e); d.focus = window.__a11y.focusVisible(e); return d; })`, [root]);
  const bad = controls.filter((c) => !c.name || !c.role || !c.focus).map((c) => `${c.role} "${c.name}" focus=${c.focus}`);
  check(
    `${label}: contrast, and every control named with a role and visible focus`,
    contrast.length === 0 && bad.length === 0 && controls.length > 0,
    `${controls.length} controls; contrast ${contrast.map((f) => `.${f.cls} "${f.text}" ${f.ratio}:1 < ${f.need}`).join("; ") || "ok"}; ${bad.join("; ")}`,
  );
}

async function theme(wd, t) {
  await wd.exec(`window.__axiom.settings.setValue("appearance.theme", arguments[0])`, [t]);
  await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [t], 5000, `theme ${t}`);
  await new Promise((r) => setTimeout(r, 300));
}

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const SHOTS = process.env.AXIOM_SCREENSHOT_DIR ?? join(tmpdir(), "axiom-tauri-shots");
mkdirSync(SHOTS, { recursive: true });
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const onLog = appLogger("models");
const results = [];
function check(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? " — " + detail : ""}`);
}
const t0 = Date.now();
const appExpr = "window.__axiom";
const SECRET = "sk-e2e-not-a-real-key-4242";

// ---- a fake OpenAI-compatible server -------------------------------------------------
const seen = [];
const server = createServer((req, res) => {
  let body = "";
  req.on("data", (c) => (body += c));
  req.on("end", () => {
    seen.push({ path: req.url, auth: req.headers.authorization ?? "" });
    if (req.method === "GET" && req.url === "/v1/models") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ object: "list", data: [{ id: "fake-gpt", object: "model" }] }));
      return;
    }
    if (req.method === "POST" && req.url === "/v1/chat/completions") {
      res.writeHead(200, { "content-type": "text/event-stream" });
      const chunk = (delta, finish = null) => `data: ${JSON.stringify({ id: "c1", object: "chat.completion.chunk", choices: [{ index: 0, delta, finish_reason: finish }] })}\n\n`;
      res.write(chunk({ role: "assistant", content: "" }));
      for (const w of ["Hello ", "from ", "the ", "fake ", "server."]) res.write(chunk({ content: w }));
      res.write(chunk({}, "stop"));
      res.write(`data: ${JSON.stringify({ id: "c1", choices: [], usage: { prompt_tokens: 12, completion_tokens: 5 } })}\n\n`);
      res.end("data: [DONE]\n\n");
      return;
    }
    res.writeHead(404).end();
  });
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const BASE = `http://127.0.0.1:${server.address().port}/v1`;

const dataDir = mkdtempSync(join(tmpdir(), "axiom-models-data-"));
const creds = join(dataDir, "credentials.json");
let wd;
try {
  // The test credential store (a file): never the developer's keychain.
  wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", dataDir], { AXIOM_TEST_CREDENTIALS_FILE: creds }, onLog);
  await wd.waitFor(`return ${appExpr}.client.ready`, [], 20000, "the backend");

  // 1. Nothing configured: Chat shows the first-run setup.
  await wd.click(await wd.find('.rail [data-destination="chat"]'));
  await wd.find("[data-testid=model-setup]", 10000);
  check("1 with no model, Chat shows the first-run setup", true);
  writeFileSync(join(SHOTS, "models-setup.png"), Buffer.from(await wd.screenshot(), "base64"));
  for (const t of ["dark", "light"]) {
    await theme(wd, t);
    await audit(wd, `S22 first-run setup (${t})`, "[data-testid=model-setup]");
    writeFileSync(join(SHOTS, `s22-${t}.png`), Buffer.from(await wd.screenshot(), "base64"));
  }
  await theme(wd, "dark");

  // 2. Add the server: OpenAI-compatible, its address, a key.
  await wd.click(await wd.find('[data-testid=model-setup] [role=radio][aria-checked=false]'));
  await wd.click(await wd.find('[data-testid=model-setup] input[aria-label="Server address"]'));
  await wd.keys(BASE);
  await wd.click(await wd.find('[data-testid=model-setup] input[aria-label="API key"]'));
  await wd.keys(SECRET);
  await wd.click(await wd.find("[data-testid=model-setup] [data-testid=models-save]"));
  await wd.waitFor(`return !document.querySelector('[data-testid=model-setup]') && ${appExpr}.agent.getSnapshot().model?.model === 'fake-gpt'`, [], 15000, "the setup to finish");
  const leak = await wd.exec(`return document.documentElement.outerHTML.includes(arguments[0]) || [...document.querySelectorAll('input')].some(i => i.value.includes(arguments[0])) || JSON.stringify(${appExpr}.agent.getSnapshot()).includes(arguments[0])`, [SECRET]);
  check("2 the provider is added and chosen; the key is nowhere in the webview", !leak);

  // 3. Chat with it.
  const field = await wd.find('[data-testid=composer] textarea[aria-label="Message"]');
  await wd.click(field);
  await wd.keys("Say hello");
  await wd.keys(Keys.ENTER);
  await wd.waitFor(`return [...document.querySelectorAll('[data-testid=chat-log] .md')].some(e => e.textContent.includes('Hello from the fake server.'))`, [], 15000, "the answer");
  const asked = seen.find((r) => r.path === "/v1/chat/completions");
  check("3 a conversation with the added server streams its answer; the key went to the server as a bearer", asked?.auth === `Bearer ${SECRET}`, asked?.auth.replace(SECRET, "<key>"));

  // 4. Settings → Models lists it, with "Key stored".
  await wd.chord(MOD, ",");
  await wd.find("[data-testid=settings]", 5000);
  await wd.click(await wd.find('[data-testid=settings] [role=tab]#settings-tab-models'));
  await wd.find("[data-testid=models-pane] .models-row", 5000);
  const row = await wd.exec(`const r = document.querySelector('[data-testid=models-pane] .models-row'); return { name: r.querySelector('.models-row__name').textContent, key: r.querySelector('[data-testid=key-state]').textContent, models: r.querySelector('.models-row__state').textContent }`);
  check("4 Settings → Models lists the provider with its key stored and its models", row.key === "Key stored" && /1 model/.test(row.models), JSON.stringify(row));
  writeFileSync(join(SHOTS, "models-pane.png"), Buffer.from(await wd.screenshot(), "base64"));
  for (const t of ["dark", "light"]) {
    await theme(wd, t);
    await audit(wd, `S23 Settings → Models (${t})`, "[data-testid=models-pane]");
    writeFileSync(join(SHOTS, `s23-${t}.png`), Buffer.from(await wd.screenshot(), "base64"));
  }
  await theme(wd, "dark");

  // 5. Remove it: the setup is back in Chat.
  await wd.click(await wd.find('[data-testid=models-pane] [aria-label^="Remove "]'));
  await wd.waitFor(`return !document.querySelector('[data-testid=models-pane] .models-row')`, [], 10000, "the row to go");
  await wd.keys([Keys.ESC]);
  await wd.exec(`${appExpr}.agent.newChat("general")`);
  await wd.find("[data-testid=model-setup]", 10000);
  check("5 removing the provider brings the setup back", true);
} catch (e) {
  check("models suite", false, String(e?.stack ?? e));
  if (wd) await wd.diagnose("models", console.log);
} finally {
  if (wd) await wd.quit();
  server.close();
  removeDir(dataDir);
}
const failed = results.filter((r) => !r.ok);
console.log(`\n${results.length - failed.length}/${results.length} models checks passed in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
process.exit(failed.length ? 1 : 0);
