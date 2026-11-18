#!/usr/bin/env node
// Contract check for the two script channels of e2e/webdriver.mjs: W3C Execute
// Script, and the plugin's /wdio/eval (used on macOS, where WKWebView can
// reclaim Execute Script's completion). Both must give the same result for
// every shape the suites use, including errors. Run it both ways:
//   AXIOM_WEBDRIVER=embedded node e2e/runner.mjs e2e/eval-contract.mjs
//   AXIOM_DIRECT_EVAL=1 AXIOM_WEBDRIVER=embedded node e2e/runner.mjs e2e/eval-contract.mjs
// Exits non-zero when a case differs from the expected value.

import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { WebDriver, removeDir } from "./webdriver.mjs";

const repo = resolve(import.meta.dirname, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const data = mkdtempSync(join(tmpdir(), "axiom-eval-contract-"));

const wd = await WebDriver.start(APP, ["--daemon", DAEMON, "--data-dir", data]);
const failures = [];
async function expect(name, fn, check) {
  let got;
  try {
    got = { value: await fn() };
  } catch (e) {
    got = { error: String(e instanceof Error ? e.message : e) };
  }
  const ok = check(got);
  if (!ok) failures.push(name);
  console.log(`${ok ? "PASS" : "FAIL"}  ${name} — ${JSON.stringify(got).slice(0, 110)}`);
}
const is = (v) => (g) => JSON.stringify(g.value) === JSON.stringify(v);
const throwsWith = (text) => (g) => typeof g.error === "string" && g.error.includes(text);

try {
  console.log(`channel: ${wd.usesDirectEval ? "/wdio/eval" : "W3C Execute Script"}`);
  await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "ready");
  await expect("a number", () => wd.exec(`return 1 + 2`), is(3));
  await expect("arguments", () => wd.exec(`return arguments[0] + arguments[1]`, [2, 5]), is(7));
  await expect("an object", () => wd.exec(`return { a: [1, "x"], b: null }`), is({ a: [1, "x"], b: null }));
  await expect("undefined is null", () => wd.exec(`void 0`), is(null));
  await expect("no return statement is null", () => wd.exec(`window.__x = 7`), is(null));
  await expect("DOM text", () => wd.exec(`return document.querySelector('[data-testid=status]').textContent`), (g) => /Connected/.test(g.value));
  await expect("a sync throw is an error", () => wd.exec(`throw new Error("boom-sync")`), throwsWith("boom-sync"));
  await expect("an async callback", () => wd.execAsync(`const cb = arguments[arguments.length-1]; setTimeout(() => cb(arguments[0] * 2), 50)`, [21]), is(42));
  await expect("an async host call", () => wd.execAsync(`const cb = arguments[arguments.length-1]; window.__axiomHost.appInfo().then(i => cb(typeof i.pid))`), is("number"));
  await expect("an async throw is an error", () => wd.execAsync(`throw new Error("boom-async")`), throwsWith("boom-async"));
  await expect("waitFor polls through it", () => wd.waitFor(`return window.__x === 7`, [], 3000, "x"), is(true));
} finally {
  await wd.quit();
  await removeDir(data);
}
console.log(`\n${failures.length ? `FAILED: ${failures.join(", ")}` : "contract holds"}`);
process.exit(failures.length ? 1 : 0);
