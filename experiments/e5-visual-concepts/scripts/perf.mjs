// Frame-time and idle-CPU probe for the concept prototypes (headless Chrome,
// software rendering). Indicative only: production runs in WebKitGTK /
// WebView2 / WKWebView and is measured by apps/desktop-tauri/bench (M1.6-QA).
//
// For each concept it records every requestAnimationFrame interval while a
// motion plays, and the main-thread task time per second while idle with the
// "task running" animations (LED, activity sweep, caret) on and off.
//
// Usage: node scripts/perf.mjs [--url http://localhost:5190/] [--only r,a,b,c] [--reps 3]
import { chromium } from "playwright-core";
import { mkdirSync, writeFileSync } from "node:fs";
import os from "node:os";

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const base = arg("--url", "http://localhost:5190/");
const only = arg("--only", "r,a,b,c").split(",");
const reps = Number(arg("--reps", "3"));

const stats = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))] ?? 0;
  return { n: s.length, p50: +q(0.5).toFixed(2), p95: +q(0.95).toFixed(2), max: +(s.at(-1) ?? 0).toFixed(2), over20: s.filter((x) => x > 20).length };
};

const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome", args: ["--disable-background-timer-throttling", "--disable-renderer-backgrounding"] });
const results = { date: new Date().toISOString(), host: os.hostname(), cpu: os.cpus()[0]?.model, engine: `Chrome ${browser.version()} headless (software raster)`, concepts: {} };

async function frames(page, act, ms) {
  await page.evaluate(() => {
    window.__f = [];
    let last = performance.now();
    const loop = (t) => { window.__f.push(t - last); last = t; if (window.__rec) requestAnimationFrame(loop); };
    window.__rec = true;
    requestAnimationFrame((t) => { last = t; requestAnimationFrame(loop); });
  });
  await page.waitForTimeout(50);
  await act();
  await page.waitForTimeout(ms);
  return page.evaluate(() => { window.__rec = false; return window.__f.slice(1); });
}

async function cpu(page, cdp, ms) {
  const get = async () => Object.fromEntries((await cdp.send("Performance.getMetrics")).metrics.map((m) => [m.name, m.value]));
  const a = await get();
  await page.waitForTimeout(ms);
  const b = await get();
  return +(((b.TaskDuration - a.TaskDuration) / (ms / 1000)) * 100).toFixed(2); // % of one core
}

for (const c of only) {
  const r = { palette: [], sidebar: [], tabs: [], resize: [], typing: [], idleRunningPct: [], idleStillPct: [] };
  for (let rep = 0; rep < reps; rep++) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send("Performance.enable");
    await page.goto(`${base}?c=${c}&theme=dark&still`, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(800);

    r.idleRunningPct.push(await cpu(page, cdp, 4000));
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.waitForTimeout(200);
    r.idleStillPct.push(await cpu(page, cdp, 4000));
    await page.emulateMedia({ reducedMotion: "no-preference" });

    r.palette.push(...await frames(page, async () => { await page.keyboard.press("Control+k"); await page.waitForTimeout(300); await page.keyboard.press("ArrowDown"); await page.keyboard.press("ArrowDown"); await page.keyboard.press("Escape"); }, 400));
    r.sidebar.push(...await frames(page, async () => { await page.keyboard.press("Control+b"); await page.waitForTimeout(350); await page.keyboard.press("Control+b"); }, 400));
    r.tabs.push(...await frames(page, async () => { for (let i = 0; i < 4; i++) { await page.keyboard.press("Control+Tab"); await page.waitForTimeout(90); } }, 300));
    const split = await page.$(".split--v");
    if (split) {
      const box = await split.boundingBox();
      r.resize.push(...await frames(page, async () => {
        await page.mouse.move(box.x + 1, box.y + 200);
        await page.mouse.down();
        for (let i = 0; i < 40; i++) await page.mouse.move(box.x + 1 + i * 3, box.y + 200);
        await page.mouse.up();
      }, 150));
    }
    // Typing: keydown -> start of the next frame, inside Monaco.
    await page.evaluate(() => { const el = document.querySelector(".monaco-editor textarea, .monaco-editor .native-edit-context"); el?.focus(); });
    await page.waitForTimeout(100);
    await page.evaluate(() => {
      window.__lat = [];
      document.addEventListener("keydown", (e) => { const t0 = e.timeStamp; requestAnimationFrame(() => requestAnimationFrame((t) => window.__lat.push(t - t0))); }, true);
    });
    for (const ch of "let grace = KILL_GRACE.saturating_sub(elapsed);".split("")) { await page.keyboard.type(ch); await page.waitForTimeout(25); }
    await page.waitForTimeout(100);
    r.typing.push(...await page.evaluate(() => window.__lat));
    await page.close();
  }
  results.concepts[c] = {
    idleCpuPctRunning: stats(r.idleRunningPct), idleCpuPctReducedMotion: stats(r.idleStillPct),
    paletteFrameMs: stats(r.palette), sidebarFrameMs: stats(r.sidebar), tabFrameMs: stats(r.tabs), resizeFrameMs: stats(r.resize),
    typingKeyToSecondFrameMs: stats(r.typing),
  };
  console.log(c, JSON.stringify(results.concepts[c]));
}
await browser.close();
mkdirSync("results", { recursive: true });
const file = `results/perf-${results.date.slice(0, 19).replace(/[:T]/g, "")}.json`;
writeFileSync(file, JSON.stringify(results, null, 2) + "\n");
console.log(file);
