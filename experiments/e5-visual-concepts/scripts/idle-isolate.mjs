// Which looping animation costs idle CPU? Concept r, one variant at a time.
import { chromium } from "playwright-core";
import { readFileSync, readdirSync } from "node:fs";
const base = "http://localhost:5190/";
function ticks() { let t = 0; for (const p of readdirSync("/proc").filter((d) => /^\d+$/.test(d))) { try { const st = readFileSync(`/proc/${p}/stat`, "utf8"); if (!readFileSync(`/proc/${p}/cmdline`, "utf8").includes("playwright_chromiumdev_profile")) continue; const f = st.slice(st.lastIndexOf(")") + 2).split(" "); t += +f[11] + +f[12]; } catch {} } return t; }
const OFF = { led: ".led[data-state=run]{animation:none!important}", sweep: ".activity>span{animation:none!important}", spin: ".spin{animation:none!important}", caret: ".out__caret{animation:none!important}" };
const variants = {
  "POLICY: stepped led+spin, no sweep": OFF.sweep + ".led[data-state=run]{animation-timing-function:steps(6)!important}.spin{animation-timing-function:steps(8)!important}",
  "all on": "",
  "led off": OFF.led, "sweep off": OFF.sweep, "spin off": OFF.spin, "caret off": OFF.caret,
  "only led (opacity)": OFF.sweep + OFF.spin + OFF.caret,
  "only led, steps(6) at 1.6s": OFF.sweep + OFF.spin + OFF.caret + ".led[data-state=run]{animation-timing-function:steps(6)!important}",
  "only spin": OFF.led + OFF.sweep + OFF.caret,
  "only sweep": OFF.led + OFF.spin + OFF.caret,
  "all off": Object.values(OFF).join(""),
};
const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome" });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
for (const [name, css] of Object.entries(variants).filter(([n]) => !process.argv[2] || n.startsWith(process.argv[2]))) {
  await page.goto(`${base}?c=r&theme=dark&still`, { waitUntil: "networkidle" });
  if (css) await page.addStyleTag({ content: css });
  await page.waitForTimeout(1500);
  const out = [];
  for (let i = 0; i < 2; i++) { const a = ticks(), t = Date.now(); await page.waitForTimeout(5000); out.push(+(((ticks() - a) / 100) / ((Date.now() - t) / 1000) * 100).toFixed(1)); }
  console.log(name.padEnd(30), out.join("  "));
}
await browser.close();
