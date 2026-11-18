#!/usr/bin/env node
// E4: can a preview of a running UI be rendered into the app while being
// inspected over the Chrome DevTools Protocol?
//
// Option (a): an external Chromium, driven over CDP, captured frame by frame.
// Option (b), WebView2's CDP, is Windows-only and is not measured here.
//
// Nothing outside experiments/ is touched. Headless: no window is ever put on
// a developer's screen, and --user-data-dir points at a temp directory so the
// real browser profile is never read or written.
//
//   node experiments/e4-preview-cdp/probe.mjs [--frames 60] [--chrome <path>]
//
// Writes results.json next to this file and prints a short summary.

import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:http";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

const here = import.meta.dirname;
const args = process.argv.slice(2);
const num = (flag, dflt) => {
  const i = args.indexOf(flag);
  return i >= 0 ? Number(args[i + 1]) : dflt;
};
const FRAMES = num("--frames", 60);
const chromePath =
  (args.includes("--chrome") ? args[args.indexOf("--chrome") + 1] : null) ??
  process.env.CHROME_PATH ??
  ["/opt/google/chrome/chrome", "/usr/bin/google-chrome", "/usr/bin/chromium"].find((p) => existsSync(p));
if (!chromePath) {
  console.error("no Chromium found; pass --chrome <path>");
  process.exit(2);
}

// ---- a fixture page with a stamped element (ADR-0008's element → source) -----
const FIXTURE = `<!doctype html><html><head><meta charset="utf-8"><title>E4 fixture</title>
<style>
  body { margin: 0; font: 14px system-ui; background: #16171c; color: #e8e8ea; }
  .card { margin: 40px; padding: 24px; border: 1px solid #333; border-radius: 8px; }
  button { padding: 8px 14px; font: inherit; }
</style></head>
<body>
  <div class="card" data-source="src/App.tsx:42:7" id="stamped">
    <h1 id="title">Preview fixture</h1>
    <p>An element carrying its build-time source location.</p>
    <button id="go" data-source="src/Button.tsx:11:3">Run</button>
  </div>
  <script>
    // Something that paints continuously, so captures are never of a static page.
    let n = 0;
    setInterval(() => { document.getElementById('title').textContent = 'Preview fixture ' + (++n); }, 100);
  </script>
</body></html>`;

// ---- CDP plumbing -----------------------------------------------------------
class Cdp {
  constructor(url, sessionId = null) {
    this.ws = new WebSocket(url);
    this.next = 1;
    this.pending = new Map();
    this.events = new Map();
    /** When set, every command is scoped to this attached target. */
    this.sessionId = sessionId;
  }
  async open() {
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
    this.ws.addEventListener("message", (m) => {
      const msg = JSON.parse(m.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        msg.error ? reject(new Error(`${msg.error.message} (${JSON.stringify(msg.error)})`)) : resolve(msg.result);
      } else if (msg.method && this.events.has(msg.method)) {
        for (const fn of this.events.get(msg.method)) fn(msg.params);
      }
    });
  }
  send(method, params = {}) {
    const id = this.next++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const msg = { id, method, params };
      if (this.sessionId) msg.sessionId = this.sessionId;
      this.ws.send(JSON.stringify(msg));
    });
  }
  on(method, fn) {
    if (!this.events.has(method)) this.events.set(method, []);
    this.events.get(method).push(fn);
  }
  once(method) {
    return new Promise((resolve) => {
      const fn = (params) => {
        this.events.set(method, (this.events.get(method) ?? []).filter((f) => f !== fn));
        resolve(params);
      };
      this.on(method, fn);
    });
  }
  close() {
    try {
      this.ws.close();
    } catch {
      /* already closed */
    }
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
/** PSS of the group (shared pages counted proportionally), like the shell benchmarks. */
const pssMiB = (pids) =>
  pids.reduce((sum, pid) => {
    try {
      const rollup = readFileSync(`/proc/${pid}/smaps_rollup`, "utf8");
      const m = /^Pss:\s+(\d+) kB/m.exec(rollup);
      return sum + (m ? Number(m[1]) / 1024 : 0);
    } catch {
      return sum;
    }
  }, 0);

const rssMiB = (pids) =>
  pids.reduce((sum, pid) => {
    try {
      const status = readFileSync(`/proc/${pid}/status`, "utf8");
      const m = /VmRSS:\s+(\d+) kB/.exec(status);
      return sum + (m ? Number(m[1]) / 1024 : 0);
    } catch {
      return sum;
    }
  }, 0);
/** Chrome processes started with our user-data-dir: the browser process group. */
const ourChromePids = (marker) => {
  const out = spawnSync("pgrep", ["-f", marker], { encoding: "utf8" }).stdout ?? "";
  return out
    .split("\n")
    .map((s) => Number(s.trim()))
    .filter((n) => Number.isInteger(n) && n > 0);
};

// ---- the run -----------------------------------------------------------------
const userDataDir = mkdtempSync(join(tmpdir(), "e4-chrome-profile-"));
const results = {
  experiment: "E4 — preview rendering + CDP inspection",
  ran_at: new Date().toISOString(),
  env: {
    os: `${process.platform} ${process.arch}`,
    chrome: chromePath,
    node: process.version,
    user_data_dir: userDataDir,
    display: process.env.DISPLAY ?? null,
    headless: true,
  },
  pass_criterion: {
    stated_before_running:
      "recommend option (a) only if first frame < 1000 ms, p95 frame-capture latency < 100 ms, and a stamped element's source is readable over CDP",
  },
};

let chrome;
let cdp;
let server;

// The fixture is served over loopback HTTP: a top-level data: URL is blocked
// by Chrome (the target then detaches, "not attached to an active page"), and a
// preview would be served anyway.
const fixtureServer = createServer((req, res) => {
  res.writeHead(200, { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" });
  res.end(FIXTURE);
});
await new Promise((resolve) => fixtureServer.listen(0, "127.0.0.1", resolve));
const fixtureUrl = `http://127.0.0.1:${fixtureServer.address().port}/`;
server = fixtureServer;
try {
  results.stage = "launch";
  const version = spawnSync(chromePath, ["--version"], { encoding: "utf8" }).stdout.trim();
  results.env.chrome_version = version;

  chrome = spawn(
    chromePath,
    [
      "--headless=new",
      "--remote-debugging-port=0",
      `--user-data-dir=${userDataDir}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      "--disable-background-networking",
      "--disable-sync",
      "--metrics-recording-only",
      "--window-size=1180,760",
      ...(process.env.E4_DISABLE_GPU === "1" ? ["--disable-gpu"] : []),
      "about:blank",
    ],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  const chromeLog = [];
  chrome.stderr.on("data", (d) => chromeLog.push(d.toString()));

  // Port and browser target come from the DevToolsActivePort file Chrome writes.
  const portFile = join(userDataDir, "DevToolsActivePort");
  const deadline = Date.now() + 20000;
  while (!existsSync(portFile) && Date.now() < deadline) await sleep(50);
  if (!existsSync(portFile)) throw new Error(`Chrome wrote no DevToolsActivePort; log: ${chromeLog.join("").slice(-400)}`);
  const [port] = readFileSync(portFile, "utf8").split("\n");
  const portNumber = Number(port);
  results.env.remote_debugging_port = portNumber;
  if (!portNumber) throw new Error("DevToolsActivePort has no port");

  // Drive a target we create and attach to explicitly: connecting to a
  // /json/list page target is not reliable in this Chrome build ("not attached
  // to an active page"), and the browser endpoint is the documented way.
  const meta = await (await fetch(`http://127.0.0.1:${portNumber}/json/version`)).json();
  cdp = new Cdp(meta.webSocketDebuggerUrl);
  await cdp.open();
  // Create the target at the fixture URL and attach to that exact id. Picking
  // from Target.getTargets is ambiguous here (Chrome opens more than one
  // about:blank page) and attaching to the wrong one fails later with "not
  // attached to an active page".
  const { targetId } = await cdp.send("Target.createTarget", { url: fixtureUrl });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  cdp.sessionId = sessionId;
  results.env.target = targetId;
  results.env.fixture_url = fixtureUrl;

  await cdp.send("Page.enable");
  await cdp.send("Runtime.enable");
  await cdp.send("DOM.enable");
  results.stage = "metrics";
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1180, height: 760, deviceScaleFactor: 1, mobile: false });

  results.stage = "metrics";
  await cdp.send("Emulation.setDeviceMetricsOverride", { width: 1180, height: 760, deviceScaleFactor: 1, mobile: false });

  // ---- time to first frame: navigate, then capture until the page is not blank
  results.stage = "blank-capture";
  const blank = await cdp.send("Page.captureScreenshot", { format: "png" });
  const t0 = performance.now();
  const loaded = cdp.once("Page.loadEventFired");
  results.stage = "navigate";
  await cdp.send("Page.navigate", { url: `${fixtureUrl}?frame=1` });
  let firstFrameMs = null;
  let firstFrameBytes = null;
  results.stage = "first-frame";
  let transient = 0;
  for (let i = 0; i < 400 && firstFrameMs === null; i++) {
    let shot;
    try {
      shot = await cdp.send("Page.captureScreenshot", { format: "png" });
    } catch (e) {
      // A capture during the navigation's commit window fails with "not
      // attached to an active page" (the renderer is being replaced). That is
      // a "not painted yet", which is exactly what this loop polls for.
      if (!/not attached to an active page/i.test(String(e))) throw e;
      transient++;
      await sleep(10);
      continue;
    }
    if (shot.data !== blank.data && shot.data.length > 1000) {
      firstFrameMs = performance.now() - t0;
      firstFrameBytes = Math.round((shot.data.length * 3) / 4);
    } else {
      await sleep(5);
    }
  }
  await loaded;
  results.transient_capture_errors_before_first_frame = transient;
  results.navigation_to_first_frame_ms = firstFrameMs === null ? null : Math.round(firstFrameMs * 10) / 10;
  results.first_frame_png_bytes = firstFrameBytes;

  // ---- 60 frame captures: what a preview stream would cost per frame
  const captures = [];
  for (let i = 0; i < FRAMES; i++) {
  results.stage = "frames";
    const s = performance.now();
    await cdp.send("Page.captureScreenshot", { format: "png" });
    captures.push(performance.now() - s);
  }
  captures.sort((a, b) => a - b);
  const at = (p) => Math.round(captures[Math.min(captures.length - 1, Math.floor((p / 100) * captures.length))] * 10) / 10;
  results.frame_capture_ms = {
    frames: captures.length,
    p50: at(50),
    p95: at(95),
    max: Math.round(captures[captures.length - 1] * 10) / 10,
  };

  // ---- jpeg variant, which is what a stream would carry
  const jpeg = [];
  for (let i = 0; i < 20; i++) {
    const s = performance.now();
    await cdp.send("Page.captureScreenshot", { format: "jpeg", quality: 80 });
    jpeg.push(performance.now() - s);
  }
  jpeg.sort((a, b) => a - b);
  results.frame_capture_jpeg_ms = { frames: jpeg.length, p50: Math.round(jpeg[Math.floor(jpeg.length / 2)] * 10) / 10 };

  // ---- element → source over CDP (ADR-0008's model)
  results.stage = "done";
  const doc = await cdp.send("DOM.getDocument", { depth: -1 });
  const node = await cdp.send("DOM.querySelector", { nodeId: doc.root.nodeId, selector: "#stamped" });
  const box = await cdp.send("DOM.getBoxModel", { nodeId: node.nodeId });
  const source = await cdp.send("Runtime.evaluate", {
    expression: `document.querySelector('#stamped').dataset.source`,
    returnByValue: true,
  });
  results.element_to_source = {
    selector: "#stamped",
    box_model: box.model ? { width: Math.round(box.model.width), height: Math.round(box.model.height) } : null,
    stamped_source: source.result?.value ?? null,
    reachable_over_cdp: typeof source.result?.value === "string" && box.model !== undefined,
  };

  // Same again for a nested element, through Runtime.evaluate's DOM API.
  const nested = await cdp.send("Runtime.evaluate", {
    expression: `(() => { const b = document.querySelector('#go'); const r = b.getBoundingClientRect(); return { source: b.dataset.source, w: Math.round(r.width), h: Math.round(r.height) }; })()`,
    returnByValue: true,
  });
  results.nested_element = nested.result?.value ?? null;

  // ---- memory of the browser process group
  await sleep(500);
  const pids = ourChromePids(userDataDir);
  results.browser_rss_mib = Math.round(rssMiB(pids));
  results.browser_pss_mib = Math.round(pssMiB(pids));
  results.browser_process_count = pids.length;
} catch (e) {
  results.error = e instanceof Error ? e.message : String(e);
} finally {
  cdp?.close();
  server?.close();
  if (chrome && chrome.exitCode === null) {
    chrome.kill("SIGTERM");
    await sleep(300);
    if (chrome.exitCode === null) chrome.kill("SIGKILL");
  }
  // Whatever Chrome left behind for this profile.
  for (const pid of ourChromePids(userDataDir)) {
    try {
      process.kill(pid, "SIGKILL");
    } catch {
      /* gone */
    }
  }
  rmSync(userDataDir, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
}

// ---- verdict, against the criterion stated before running
const c = results.pass_criterion.stated_before_running;
const firstOk = typeof results.navigation_to_first_frame_ms === "number" && results.navigation_to_first_frame_ms < 1000;
const p95Ok = results.frame_capture_ms?.p95 < 100;
const cdpOk = results.element_to_source?.reachable_over_cdp === true;
results.verdict = {
  first_frame_under_1s: firstOk,
  p95_under_100ms: Boolean(p95Ok),
  element_source_reachable: cdpOk,
  recommend_option_a: Boolean(firstOk && p95Ok && cdpOk && !results.error),
  criterion: c,
  option_b: "not run: no Windows host for WebView2's CDP",
};
results.option_b = "not run: no Windows host in this environment";

writeFileSync(join(here, "results.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log(JSON.stringify({ ...results, env: { chrome_version: results.env.chrome_version } }, null, 2));
process.exit(results.error ? 1 : 0);
