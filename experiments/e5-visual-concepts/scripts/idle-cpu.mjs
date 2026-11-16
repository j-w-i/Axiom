// Whole-browser CPU (all Chrome processes incl. GPU/compositor/raster) while idle,
// with the "task running" animations on vs reduced motion. Linux only (/proc).
import { chromium } from "playwright-core";
import { readFileSync, readdirSync } from "node:fs";
const base = process.argv[2] ?? "http://localhost:5190/";
const only = (process.argv[3] ?? "r,a,b,c").split(",");
const HZ = 100;
function chromeTicks(rootPid) {
  let total = 0;
  for (const p of readdirSync("/proc").filter((d) => /^\d+$/.test(d))) {
    try {
      const st = readFileSync(`/proc/${p}/stat`, "utf8");
      const f = st.slice(st.lastIndexOf(")") + 2).split(" ");
      const cmd = readFileSync(`/proc/${p}/cmdline`, "utf8");
      if (p === String(rootPid) || cmd.includes("playwright_chromiumdev_profile") ) total += Number(f[11]) + Number(f[12]);
    } catch {}
  }
  return total;
}
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome" });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const sample = async (ms) => { const a = chromeTicks(); const t = Date.now(); await page.waitForTimeout(ms); return +(((chromeTicks() - a) / HZ) / ((Date.now() - t) / 1000) * 100).toFixed(1); };
for (const c of only) {
  await page.goto(`${base}?c=${c}&theme=dark&still`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);
  await page.emulateMedia({ reducedMotion: "no-preference" });
  const on = []; for (let i = 0; i < 3; i++) on.push(await sample(5000));
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.waitForTimeout(500);
  const off = []; for (let i = 0; i < 3; i++) off.push(await sample(5000));
  console.log(JSON.stringify({ concept: c, runningAnimationsPctCore: on, reducedMotionPctCore: off }));
}
await browser.close();
