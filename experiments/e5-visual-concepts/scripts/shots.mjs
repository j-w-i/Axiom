// Screenshots of every concept state in headless Chrome at fixed size.
// Usage: node scripts/shots.mjs [--url http://localhost:5190/] [--dpr 1] [--only a,b] [--out screenshots]
import { chromium } from "playwright-core";
import { mkdirSync } from "node:fs";

const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const base = arg("--url", "http://localhost:5190/");
const dpr = Number(arg("--dpr", "1"));
const only = arg("--only", "r,a,b,c").split(",");
const out = arg("--out", "screenshots");
const which = arg("--states", "");
mkdirSync(out, { recursive: true });

const STATES = [
  ["workspace-dark", "theme=dark"],
  ["workspace-light", "theme=light"],
  ["palette-dark", "theme=dark&palette=commands"],
  ["files-dark", "theme=dark&palette=files"],
  ["components-dark", "theme=dark&view=components"],
  ["components-light", "theme=light&view=components"],
  ["environments-dark", "theme=dark&view=environments"],
  ["settings-dark", "theme=dark&view=settings"],
  ["compact-dark", "theme=dark&dock=0&panel=0"],
  ["hint-dark", "theme=dark"],
];

const browser = await chromium.launch({ executablePath: "/usr/bin/google-chrome", args: ["--font-render-hinting=none", "--disable-lcd-text"] });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: dpr });
page.on("pageerror", (e) => console.error("pageerror", e.message));
for (const c of only) {
  for (const [name, q] of STATES) {
    if (which && !which.split(",").includes(name)) continue;
    await page.goto(`${base}?c=${c}&still&${q}`, { waitUntil: "networkidle" });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(700);
    await page.mouse.move(1, 450);
    if (name === "hint-dark") { await page.hover(".a-transport .ibtn, .b-key--play, .c-task__stop"); await page.waitForTimeout(200); }
    const file = `${out}/${c}-${name}${dpr !== 1 ? `@${dpr}x` : ""}.png`;
    await page.screenshot({ path: file, animations: "disabled", caret: "hide" });
    console.log(file);
  }
}
// 2x detail crops: chrome + tabs, and the agent approval card (craftsmanship review)
if (!which) {
  const hi = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 });
  for (const c of only) {
    for (const t of ["dark", "light"]) {
      await hi.goto(`${base}?c=${c}&still&theme=${t}`, { waitUntil: "networkidle" });
      await hi.evaluate(() => document.fonts.ready);
      await hi.waitForTimeout(700);
      await hi.mouse.move(1, 450);
      await hi.screenshot({ path: `${out}/${c}-detail-chrome-${t}@2x.png`, clip: { x: 0, y: 0, width: 760, height: 130 } });
      await hi.screenshot({ path: `${out}/${c}-detail-agent-${t}@2x.png`, clip: { x: 1080, y: 360, width: 360, height: 330 } });
      console.log(`${out}/${c}-detail-*-${t}@2x.png`);
    }
  }
}
await browser.close();
