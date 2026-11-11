// E3: closed-loop web inspection experiment.
// select by coordinate -> resolve source -> patch -> hot reload -> verify geometry + pixels.
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import CDP from "chrome-remote-interface";
import { PNG } from "pngjs";
import pixelmatch from "pixelmatch";

const ROOT = path.dirname(new URL(import.meta.url).pathname);
const PORT = 5199, DBG = 9339;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = { steps: [] };
const step = (name, data) => { results.steps.push({ name, ...data }); console.log(`[${name}]`, JSON.stringify(data)); };

async function waitHttp(url, tries = 80) {
  for (let i = 0; i < tries; i++) { try { const r = await fetch(url); if (r.ok) return; } catch {} await sleep(150); }
  throw new Error("timeout waiting for " + url);
}

const vite = spawn("node_modules/.bin/vite", [], { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] });
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "axiom-e3-"));
const chromeBin = process.env.CHROME || "google-chrome";
const chrome = spawn(chromeBin, ["--headless=new", `--remote-debugging-port=${DBG}`, `--user-data-dir=${profile}`,
  "--no-first-run", "--disable-gpu", "--window-size=800,600", "--force-device-scale-factor=2", "about:blank"], { stdio: "ignore" });

const target = path.join(ROOT, "src/components/BalanceCard.jsx");
const original = fs.readFileSync(target, "utf8");
let exitCode = 1;
try {
  await waitHttp(`http://localhost:${PORT}/`);
  await waitHttp(`http://localhost:${DBG}/json/version`);
  const client = await CDP({ port: DBG });
  const { Page, DOM, Runtime, Emulation } = client;
  await Promise.all([Page.enable(), DOM.enable(), Runtime.enable()]);
  await Page.navigate({ url: `http://localhost:${PORT}/` });
  await Page.loadEventFired();
  await sleep(500);

  const shot = async (file) => {
    const { data } = await Page.captureScreenshot({ format: "png" });
    fs.writeFileSync(path.join(ROOT, file), Buffer.from(data, "base64"));
    return PNG.sync.read(Buffer.from(data, "base64"));
  };
  const metrics = await Page.getLayoutMetrics();
  const dpr = (await Runtime.evaluate({ expression: "devicePixelRatio", returnByValue: true })).result.value;
  const before = await shot("before.png");
  step("capture", { cssViewport: [metrics.cssLayoutViewport.clientWidth, metrics.cssLayoutViewport.clientHeight], devicePixelRatio: dpr, screenshotPx: [before.width, before.height] });

  // Test oracle only: where is the 2nd "Transfer" button? (A user would click it.)
  const rectOf = async () => (await Runtime.evaluate({ returnByValue: true,
    expression: `(()=>{const b=[...document.querySelectorAll('button')][1].getBoundingClientRect();return {x:b.x,y:b.y,width:b.width,height:b.height}})()` })).result.value;
  const r0 = await rectOf();
  const click = { x: Math.round(r0.x + r0.width / 2), y: Math.round(r0.y + r0.height / 2) };

  // Level 2: coordinate -> node, via the protocol (no page cooperation needed).
  const t0 = performance.now();
  const { backendNodeId } = await DOM.getNodeForLocation({ x: click.x, y: click.y, includeUserAgentShadowDOM: false });
  const { node } = await DOM.describeNode({ backendNodeId });
  const { model } = await DOM.getBoxModel({ backendNodeId });
  step("select", { click, backendNodeId, nodeName: node.nodeName, borderQuad: model.border, ms: +(performance.now() - t0).toFixed(1) });

  // Level 3: node -> source chain, from compile-time stamps; plus React owner names from the fiber.
  const { object } = await DOM.resolveNode({ backendNodeId });
  const chain = (await Runtime.callFunctionOn({ objectId: object.objectId, returnByValue: true, functionDeclaration: `function(){
    const out=[]; for(let el=this; el && el.nodeType===1; el=el.parentElement){
      const src=el.getAttribute('data-axiom-src'); const k=Object.keys(el).find(k=>k.startsWith('__reactFiber$'));
      let owner=null, hasDebugSource=null; if(k){ const f=el[k]; hasDebugSource = f._debugSource!==undefined; const o=f._debugOwner; owner=o&&(o.type&&(o.type.displayName||o.type.name)||o.name)||null; }
      const sibs=el.parentElement?[...el.parentElement.children].filter(c=>c.getAttribute('data-axiom-src')===src):[el];
      out.push({tag:el.tagName.toLowerCase(), src, owner, hasDebugSource, instanceIndex:sibs.indexOf(el), instanceCount:sibs.length});
    } return out; }` })).result.value;
  step("resolveSource", { chain });
  const hit = chain[0];
  if (!hit.src) throw new Error("no source stamp on selected element");
  const [file, line, col] = hit.src.split(":");
  const srcLine = original.split("\n")[+line - 1];
  const ok = file === "src/components/BalanceCard.jsx" && srcLine.slice(+col - 1).startsWith("<button");
  step("verifySourceMapping", { file, line: +line, column: +col, textAtLocation: srcLine.trim().slice(0, 40), exact: ok });
  if (!ok) throw new Error("source mapping incorrect");
  // Repeated component: both cards share one source location.
  const same = (await Runtime.evaluate({ returnByValue: true, expression: `document.querySelectorAll('[data-axiom-src="${hit.src}"]').length` })).result.value;
  step("repeatedInstances", { elementsSharingSource: same });

  // Controlled modification (what an agent patch would do).
  const t1 = performance.now();
  fs.writeFileSync(target, original.replace("marginLeft: 0", "marginLeft: 24"));
  let r1 = r0;
  for (let i = 0; i < 60 && r1.x === r0.x; i++) { await sleep(100); r1 = await rectOf(); }
  step("hotReload", { beforeX: r0.x, afterX: r1.x, deltaX: r1.x - r0.x, ms: Math.round(performance.now() - t1) });
  const after = await shot("after.png");
  const diff = new PNG({ width: before.width, height: before.height });
  const changed = pixelmatch(before.data, after.data, diff.data, before.width, before.height, { threshold: 0.1 });
  fs.writeFileSync(path.join(ROOT, "diff.png"), PNG.sync.write(diff));
  // Bounding box of changed pixels, in CSS px.
  let minX = 1e9, minY = 1e9, maxX = -1, maxY = -1;
  for (let y = 0; y < diff.height; y++) for (let x = 0; x < diff.width; x++) { const i = (y * diff.width + x) * 4;
    if (diff.data[i] === 255 && diff.data[i + 1] === 0) { minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y); } }
  step("visualDiff", { changedPixels: changed, totalPixels: before.width * before.height, changedRegionCss: maxX < 0 ? null : { x: minX / dpr, y: minY / dpr, width: (maxX - minX + 1) / dpr, height: (maxY - minY + 1) / dpr } });
  const verified = r1.x - r0.x === 24 && changed > 0;
  step("verdict", { geometryMatchesIntent: r1.x - r0.x === 24, pixelsChanged: changed > 0, verified });
  exitCode = verified ? 0 : 2;
  await client.close();
} catch (e) { console.error("FAILED:", e.message); results.error = e.message; }
finally {
  fs.writeFileSync(target, original);
  fs.writeFileSync(path.join(ROOT, "results.json"), JSON.stringify(results, null, 2));
  vite.kill(); chrome.kill(); await sleep(300);
  fs.rmSync(profile, { recursive: true, force: true });
  process.exit(exitCode);
}
