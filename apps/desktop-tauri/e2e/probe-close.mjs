// Probe: does closing the window (a) reach the host, (b) make the UI ask about
// unsaved work, (c) let Cancel keep the app alive and Save exit cleanly?
import { mkdtempSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { WebDriver } from "./webdriver.mjs";

const APP = process.env.AXIOM_TAURI_APP ?? join(import.meta.dirname, "../../../target/release/axiom-tauri");
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(import.meta.dirname, "../../../target/debug/axiomd");
const data = mkdtempSync(join(tmpdir(), "axprobe-data-"));
const ws = mkdtempSync(join(tmpdir(), "axprobe-ws-"));
mkdirSync(join(ws, "src"), { recursive: true });
writeFileSync(join(ws, "README.md"), "# demo\n");

const wd = await WebDriver.start(process.env.TAURI_DRIVER_URL, APP, ["--daemon", DAEMON, "--data-dir", data]);
await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 15000, "ready");
await wd.exec(`window.__axiom.documents.requestOpenFolder(arguments[0])`, [ws]);
await wd.waitFor(`return document.querySelector('[data-path="README.md"]') !== null`, [], 10000, "tree");
await wd.click(await wd.find('[data-path="README.md"]'));
await wd.waitFor(`return document.querySelector('.monaco-editor .view-lines')?.textContent.includes('demo')`, [], 15000, "editor");
await wd.click(await wd.find(".monaco-editor .view-lines"));
await wd.keys([..."x"]);
await wd.waitFor(`return document.querySelector('[data-path="README.md"][role=tab]')?.dataset.dirty === 'true'`, [], 5000, "dirty");

// 1. does window.close() reach the host's CloseRequested handler?
const closed = await wd.exec(`try { window.close(); return "called" } catch (e) { return String(e) }`);
console.log("window.close():", closed);
const dialog = await wd
  .waitFor(`return document.querySelector('[data-testid=unsaved-dialog]')?.textContent ?? false`, [], 6000, "unsaved dialog")
  .catch(() => false);
console.log("unsaved dialog after window.close():", JSON.stringify(dialog).slice(0, 120));

if (dialog) {
  await wd.click(await wd.find('[data-testid=unsaved-dialog] [data-action=cancel]'));
  const stillThere = await wd.exec(`return !!document.querySelector('[data-testid=status]')`);
  console.log("after Cancel, window alive:", stillThere);
}
await wd.quit();
console.log("probe done");
