#!/usr/bin/env node
// Real-app screenshot harness and visual baseline (M1.6-QA-001, docs/design/visual-qa.md).
//
// Drives the production shell (Tauri + the platform webview, through the same
// WebDriver servers as the other suites) into every state of visual-qa.md §4
// that the *current* shell can show, in both themes, at a fixed 1440x900
// window, and saves
//
//   e2e/shots/<os>/<state>-<theme>[@2x].png   + manifest.json
//
// States the shell cannot show yet are listed as `pending` in the manifest,
// with the task that adds them. Compare two sets with e2e/shots-compare.mjs.
//
//   scripts/e2e.sh shots.mjs                       (under the runner's private Xvfb)
//   AXIOM_SHOTS_DIR=/some/dir scripts/e2e.sh shots.mjs     another output folder
//   AXIOM_SHOTS_EXTRA_CSS='...' scripts/e2e.sh shots.mjs   injected CSS, used only to
//                                                          prove the comparer notices a change
//
// Determinism: animations and transitions off, caret and Monaco cursor hidden,
// durations in finished-run texts replaced by a fixed string (they are the
// only wall-clock values on screen), pointer parked, fonts awaited, two
// animation frames awaited. The workspace lives at a fixed path (its path is
// on the Welcome screen) and is a fresh copy of tests/fixtures/visual/.

import { spawnSync } from "node:child_process";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, normalize, resolve } from "node:path";
import { Keys, PROVIDER, WebDriver, appLogger, removeDir } from "./webdriver.mjs";

const here = import.meta.dirname;
const repo = resolve(here, "../../..");
const EXE = process.platform === "win32" ? ".exe" : "";
const APP = process.env.AXIOM_TAURI_APP ?? join(repo, `target/release/axiom-tauri${EXE}`);
const DAEMON = process.env.AXIOMD_UNDER_TEST ?? join(repo, `target/debug/axiomd${EXE}`);
const OS = process.platform === "darwin" ? "macos" : process.platform === "win32" ? "windows" : "linux";
const OUT = resolve(process.env.AXIOM_SHOTS_DIR ?? join(here, "shots", OS));
const MOD = process.platform === "darwin" ? Keys.META : Keys.CTRL;
const DOWN = "";
const WINDOW = { width: 1440, height: 900 };
const MIN_WINDOW = { width: 720, height: 480 };
const log = appLogger("shots");
const started = Date.now();

if (process.platform === "linux" && !process.env.DISPLAY) {
  // The runner always sets a private DISPLAY; this guards a direct `node e2e/shots.mjs`.
  console.error("shots.mjs: refusing to run without a private display; use scripts/e2e.sh shots.mjs (it starts Xvfb)");
  process.exit(2);
}

// ---- the catalogue of visual-qa.md §4 --------------------------------------------

/** id → title, and for states the current shell cannot show, why and which task adds them. */
const CATALOGUE = [
  ["s1", "Welcome, backend connecting"],
  ["s1b", "Workspace opening (tree skeleton rows)", "SHELL-007: the shell has no skeleton rows yet"],
  ["s1c", "Workspace empty folder", "SHELL-007: no empty-folder state yet"],
  ["s1d", "Workspace listing error row", "SHELL-007: no restyled listing-error row yet"],
  ["s2", "Welcome, backend connected, 3 recent folders"],
  ["s3", "Backend failed banner"],
  ["s4", "Workspace, 4 tabs (one dirty), executor.rs active, console with a passing run"],
  ["s5", "S4 with a task running (stderr progress lines; no LED, transport or elapsed time exist yet)"],
  ["s6", "S4 with a failed run"],
  ["s7", "Command palette, empty query"],
  ["s8", "Palette, file mode with a query (no match highlighting exists yet)"],
  ["s9", "File menu open"],
  ["s10", "Output context menu"],
  ["s11", "Settings (one sheet: appearance and editor together)"],
  ["s11b", "Settings, Editor section", "SURFACES-002: settings has no sections yet; S11 shows all of it"],
  ["s12", "Unsaved-changes dialog"],
  ["s12b", "Conflict dialog"],
  ["s13", "Side bar hidden, console hidden (compact)"],
  ["s14", "Window at its 720x480 minimum"],
  ["s15", "Focus on the tree (keyboard focus ring)"],
  ["s16", "Hint line showing the Rerun hint", "SHELL-005: there is no hint line yet"],
  ["s17", "Gallery page, each section (dev-only gallery served locally, opened in the app's own webview, motion off)"],
  ["focused-region", "Focused-region rule", "SHELL-002 / EDITOR-002: no focused-region treatment yet"],
];
const states = new Map(
  CATALOGUE.map(([id, title, pending]) => [id, { id, title, status: pending ? "pending" : "not-run", note: pending ?? "", files: [] }]),
);
const failures = [];

function mark(id, patch) {
  Object.assign(states.get(id), patch);
}

// ---- the world the app sees -------------------------------------------------------

// A fixed path: the workspace root is printed on the Welcome screen.
const BASE = join(tmpdir(), "axiom-shots-ws");
const WS = join(BASE, "axiom");
const SIM = `// Stands in for a build tool: deterministic stdout and stderr, then an exit code.
const mode = process.argv[2] ?? "test";
// stdout and stderr are two pipes: lines written back to back may arrive in either
// order, so every line is queued and written 40 ms after the previous one.
const queue = [];
const out = (s) => queue.push([process.stdout, s]);
const err = (s) => queue.push([process.stderr, s]);
let code = 0;
const progress = () => {
  err("   Compiling libc v0.2.155");
  err("   Compiling tokio v1.38.0");
  err("   Compiling axiom-protocol v0.1.0 (/axiom/crates/protocol)");
  err("   Compiling axiom-exec v0.1.0 (/axiom/crates/exec)");
  err("    Finished \`test\` profile [unoptimized + debuginfo] target(s)");
  err("     Running unittests src/lib.rs (target/debug/deps/axiom_exec)");
};
const names = ["spawn_streams_output_in_order", "cancel_ends_the_process_tree", "utf8_split_across_reads", "exit_code_is_reported", "env_is_not_inherited_by_default", "cwd_is_the_workspace_root"];
if (mode === "test") {
  progress();
  out("running " + names.length + " tests");
  for (const n of names) out("test executor::" + n + " ... ok");
  out("");
  out("test result: ok. " + names.length + " passed; 0 failed; 0 ignored; 0 measured; 0 filtered out");
} else if (mode === "hold") {
  progress();
  out("running " + names.length + " tests");
  out("test executor::" + names[0] + " ... ok");
  out("test executor::" + names[1] + " ... ok");
} else {
  progress();
  out("running " + names.length + " tests");
  for (const n of names.slice(0, 3)) out("test executor::" + n + " ... ok");
  out("test executor::" + names[3] + " ... FAILED");
  out("");
  out("failures:");
  out("");
  out("---- executor::" + names[3] + " stdout ----");
  err("thread 'executor::" + names[3] + "' panicked at src/executor.rs:212:9:");
  err("assertion \`left == right\` failed: exit code");
  err("  left: Some(2)");
  err(" right: Some(0)");
  out("");
  out("test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 2 filtered out");
  code = 101;
}
(async () => {
  for (const [stream, line] of queue) {
    stream.write(line + "\\n");
    await new Promise((r) => setTimeout(r, 40));
  }
  if (mode === "hold") await new Promise((r) => setTimeout(r, 30000)); // "sleep 30": the suite stops it
  process.exitCode = code;
})();
`;

function prepareWorld() {
  rmSync(BASE, { recursive: true, force: true });
  mkdirSync(BASE, { recursive: true });
  cpSync(join(repo, "tests/fixtures/visual"), WS, { recursive: true });
  for (const name of ["notes", "scratch"]) mkdirSync(join(BASE, name), { recursive: true });
  writeFileSync(join(BASE, "sim.js"), SIM);
  // Backends that never answer / die at once, for S1 and S3 (POSIX only: they are scripts).
  writeFileSync(
    join(BASE, "daemon-stall"),
    "#!/usr/bin/env node\nprocess.stdin.resume();\nprocess.stdin.on('end', () => process.exit(0));\nsetTimeout(() => process.exit(0), 120000);\n",
  );
  writeFileSync(join(BASE, "daemon-fail"), "#!/bin/sh\nexit 3\n");
  if (process.platform !== "win32") for (const f of ["daemon-stall", "daemon-fail"]) chmodSync(join(BASE, f), 0o755);
}

// ---- page helpers -----------------------------------------------------------------

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const STORES = "window.__axiom";

const FREEZE_CSS =
  "*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}" +
  ".monaco-editor .cursors-layer{visibility:hidden!important}";

async function installFreeze(wd) {
  await wd.exec(
    `let s = document.getElementById('__shots'); if (!s) { s = document.createElement('style'); s.id = '__shots'; document.head.appendChild(s); }
     s.textContent = arguments[0]; return true`,
    [FREEZE_CSS + (process.env.AXIOM_SHOTS_EXTRA_CSS ?? "")],
  );
}

/** Durations are the only wall-clock text on screen; give them one fixed value. */
const FREEZE_TEXT = `
  const re = / in (\\d+(\\.\\d+)? (ms|s)|\\d+ min \\d+ s)/g;
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  let n; let replaced = 0;
  while ((n = walker.nextNode())) {
    if (re.test(n.nodeValue)) { n.nodeValue = n.nodeValue.replace(re, ' in 1.2 s'); replaced++; }
    re.lastIndex = 0;
  }
  // The task transport's elapsed time is the only other clock on screen (visual-qa.md §1: frozen).
  for (const t of document.querySelectorAll('.transport__time')) { t.textContent = '00:12'; replaced++; }
  return replaced;`;

async function parkPointer(wd) {
  try {
    await wd.cmd("POST", "/actions", {
      actions: [{ type: "pointer", id: "mouse", parameters: { pointerType: "mouse" }, actions: [{ type: "pointerMove", duration: 0, x: 1, y: 1, origin: "viewport" }] }],
    });
  } catch {
    /* a server without pointer actions: nothing was hovering anyway */
  }
}

async function settle(wd) {
  await wd.execAsync(
    `const cb = arguments[arguments.length - 1];
     document.fonts.ready.then(() => requestAnimationFrame(() => requestAnimationFrame(() => cb(true))));`,
  );
  await wd.exec(FREEZE_TEXT);
}

async function setTheme(wd, theme, viaDaemon) {
  if (viaDaemon) {
    await wd.exec(`${STORES}.settings.setValue("appearance.theme", arguments[0])`, [theme]);
  } else {
    // No daemon to store a setting (S1, S3): set what the app itself would set.
    await wd.exec(`document.documentElement.dataset.theme = arguments[0]`, [theme]);
  }
  await wd.waitFor(`return document.documentElement.dataset.theme === arguments[0]`, [theme], 5000, `theme ${theme}`);
  await sleep(250);
}

async function boot(wd, { daemon = true } = {}) {
  await wd.cmd("POST", "/window/rect", { x: 0, y: 0, ...WINDOW });
  await sleep(300);
  await installFreeze(wd);
  if (daemon) await wd.waitFor(`return document.querySelector('[data-testid=status]')?.dataset.state === 'ready'`, [], 20000, "connected");
}

async function viewport(wd) {
  return wd.exec(`return { w: window.innerWidth, h: window.innerHeight, dpr: window.devicePixelRatio }`);
}

/** Saves one PNG of the current page under `<state>-<theme>[@2x].png`. */
async function shot(wd, id, theme, scale) {
  await settle(wd);
  await parkPointer(wd);
  const name = `${id}-${theme}${scale === 2 ? "@2x" : ""}.png`;
  const bytes = await wd.screenshot();
  mkdirSync(OUT, { recursive: true });
  writeFileSync(join(OUT, name), bytes);
  const st = states.get(id);
  st.files.push(name);
  st.viewport = await viewport(wd);
  console.log(`SHOT  ${name} (${bytes.length} bytes, viewport ${st.viewport.w}x${st.viewport.h} @${st.viewport.dpr}x)`);
}

/** The current state in both themes; leaves the app in dark. */
async function bothThemes(wd, id, { scale = 1, viaDaemon = true } = {}) {
  for (const theme of ["dark", "light"]) {
    await setTheme(wd, theme, viaDaemon);
    await shot(wd, id, theme, scale);
  }
  await setTheme(wd, "dark", viaDaemon);
}

async function esc(wd, times = 2) {
  for (let i = 0; i < times; i++) {
    await wd.keys([Keys.ESC]);
    await sleep(120);
  }
}

/** Runs one state; a failure is recorded, the app is put back to a plain screen, the suite continues. */
async function state(wd, id, fn) {
  try {
    await fn();
    mark(id, { status: "captured" });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    failures.push(`${id}: ${msg}`);
    mark(id, { status: "failed", note: msg });
    console.log(`FAIL  ${id} — ${msg}`);
    await wd.diagnose(`shots ${id} failure`, log);
    await esc(wd).catch(() => {});
  }
}

const waitFor = (wd, body, what, ms = 15000) => wd.waitFor(body, [], ms, what);
const treeRow = (p) => `[data-path="${p}"]`;

/** Puts keyboard focus in Monaco: a real click, then (if the page says focus is elsewhere) its input element. */
async function focusEditor(wd) {
  const inEditor = `return !!document.activeElement?.closest('.monaco-editor')`;
  await wd.click(await wd.find(".monaco-editor .view-lines"));
  if (await wd.exec(inEditor)) return;
  await wd.exec(`document.querySelector('.monaco-editor textarea.inputarea')?.focus()`);
  await waitFor(wd, inEditor, "keyboard focus in the editor", 3000);
}

/** Moves the cursor to the document start (verified through the status line) and types "// ". */
async function commentOutFirstLine(wd) {
  await focusEditor(wd);
  for (let attempt = 0; ; attempt++) {
    await wd.chord(MOD, Keys.HOME);
    try {
      await waitFor(wd, `return ${STORES}.documents.getSnapshot().cursorText.startsWith('Ln 1, Col 1')`, "the cursor at the document start", 2500);
      break;
    } catch (e) {
      if (attempt >= 3) throw e;
      await focusEditor(wd);
    }
  }
  await wd.keys([..."// "]);
}


/**
 * Gives the app's X window input focus (Linux). A bare Xvfb has no window manager, so no
 * window is ever focused and :focus-within / :focus-visible never match; WebDriver delivers
 * keys regardless. Uses python-xlib from the bench's virtualenv (bench/run.sh); false when
 * that is not installed.
 */
function focusAppWindow(pid) {
  if (process.platform !== "linux") return false;
  const py = join(repo, ".toolchains/bench-venv/bin/python");
  if (!existsSync(py)) return false;
  const script = `
import sys
from Xlib import X, display
d = display.Display(); atom = d.intern_atom("_NET_WM_PID"); pid = int(sys.argv[1]); hit = []
def walk(w):
    try:
        p = w.get_full_property(atom, X.AnyPropertyType)
        if p and p.value and int(p.value[0]) == pid and w.get_attributes().map_state == X.IsViewable: hit.append(w)
        for c in w.query_tree().children: walk(c)
    except Exception: pass
walk(d.screen().root)
if not hit: sys.exit(1)
d.set_input_focus(hit[0], X.RevertToParent, X.CurrentTime); d.sync()
`;
  return spawnSync(py, ["-I", "-c", script, String(pid)], { stdio: "ignore" }).status === 0;
}

// ---- state builders ---------------------------------------------------------------

/** Opens the fixture, expands src/, opens four tabs (backend.rs dirty), executor.rs active. */
async function openWorkbench(wd) {
  await wd.exec(`${STORES}.documents.requestOpenFolder(arguments[0])`, [WS]);
  await waitFor(wd, `return document.querySelector('[data-path="src"]') !== null`, "root listing");
  await wd.click(await wd.find(treeRow("src")));
  await waitFor(wd, `return document.querySelector('[data-path="src/executor.rs"]') !== null`, "src expanded");
  for (const f of ["lib.rs", "tree.rs", "backend.rs"]) {
    await wd.click(await wd.find(treeRow(`src/${f}`)));
    await waitFor(wd, `return document.querySelector('[role=tab][data-path="src/${f}"][aria-selected=true]') !== null`, `${f} active`);
    await waitFor(wd, `return !!document.querySelector('.monaco-editor .view-lines')?.textContent`, `${f} in Monaco`, 20000);
  }
  // backend.rs is active: make it dirty with a deterministic edit at the top.
  await commentOutFirstLine(wd);
  await waitFor(wd, `return document.querySelector('[role=tab][data-path="src/backend.rs"]')?.dataset.dirty === 'true'`, "backend.rs dirty");
  await wd.click(await wd.find(treeRow("src/executor.rs")));
  await waitFor(wd, `return document.querySelector('[role=tab][data-path="src/executor.rs"][aria-selected=true]') !== null`, "executor.rs active");
  await waitFor(
    wd,
    `return new Set([...document.querySelectorAll('.monaco-editor .view-lines span span')].map(s => s.className)).size >= 2`,
    "executor.rs is syntax-coloured",
    20000,
  );
  await focusEditor(wd);
  await wd.chord(MOD, Keys.HOME);
  await sleep(300);
}

async function runCommand(wd, command, until) {
  const input = await wd.find("[data-testid=command-input]");
  await wd.click(input);
  await wd.chord(MOD, "a");
  await wd.keys([...command, Keys.ENTER]);
  await waitFor(wd, until, `after "${command}"`, 30000);
  await sleep(250);
}

const OUTCOME = (o) => `return ${STORES}.tasks.getSnapshot().lastOutcome === '${o}'`;
const RUNNING_WITH_OUTPUT = `const t = ${STORES}.tasks.getSnapshot(); return t.running && t.rows.some(r => r.text.includes('cancel_ends_the_process_tree'))`;

async function passingRun(wd) {
  await runCommand(wd, "node ../sim.js test", OUTCOME("succeeded"));
}


// ---- S17: the component gallery ---------------------------------------------------

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2", ".svg": "image/svg+xml", ".json": "application/json", ".png": "image/png" };

/** A static file server for one directory on 127.0.0.1 (the built gallery). */
function serveDir(dir) {
  const server = createServer((req, res) => {
    const path = normalize(decodeURIComponent((req.url ?? "/").split("?")[0])).replace(/^([/\\])+/, "");
    const file = join(dir, path || "gallery.html");
    if (!file.startsWith(dir) || !existsSync(file)) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
  });
  return new Promise((ok) => server.listen(0, "127.0.0.1", () => ok({ port: server.address().port, close: () => server.close() })));
}

/** Opens the gallery in the app's webview and saves one PNG per section and theme. */
async function captureGallery(scale) {
  const appDir = resolve(here, "..");
  const dist = join(appDir, "dist-gallery");
  const build = spawnSync("pnpm", ["-s", "run", "build:gallery"], { cwd: appDir, encoding: "utf8" });
  if (build.status !== 0) throw new Error(`build:gallery failed: ${build.stderr || build.stdout}`);
  const srv = await serveDir(dist);
  const env = scale === 2 ? { GDK_SCALE: "2", GDK_DPI_SCALE: "1" } : {};
  const { wd, data } = await launchPlain(["--daemon", DAEMON], env);
  try {
    await wd.cmd("POST", "/window/rect", { x: 0, y: 0, ...WINDOW });
    for (const theme of ["dark", "light"]) {
      await wd.cmd("POST", "/url", { url: `http://127.0.0.1:${srv.port}/gallery.html?theme=${theme}&motion=off` });
      await wd.waitFor(`return !!document.querySelector('[data-gallery]') && document.documentElement.dataset.theme === arguments[0]`, [theme], 20000, `gallery (${theme})`);
      await installFreeze(wd);
      const sections = await wd.exec(`return [...document.querySelectorAll('[data-gallery-section]')].map((s) => s.dataset.gallerySection)`);
      for (const name of sections) {
        const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
        const el = await wd.find(`[data-gallery-section="${name}"]`);
        await wd.exec(`document.querySelector('[data-gallery-section="' + arguments[0] + '"]').scrollIntoView({ block: "start" })`, [name]);
        await settle(wd);
        await parkPointer(wd);
        let bytes;
        try {
          bytes = Buffer.from(await wd.cmd("GET", `/element/${el}/screenshot`), "base64");
        } catch {
          bytes = await wd.screenshot(); // a server without element screenshots: the viewport, section at the top
        }
        const file = `s17-${slug}-${theme}${scale === 2 ? "@2x" : ""}.png`;
        writeFileSync(join(OUT, file), bytes);
        states.get("s17").files.push(file);
        console.log(`SHOT  ${file} (${bytes.length} bytes)`);
      }
    }
  } finally {
    await wd.quit().catch(() => {});
    await removeDir(data);
    srv.close();
  }
}

// ---- the run ----------------------------------------------------------------------

async function launchPlain(extraArgs, env = {}) {
  const data = mkdtempSync(join(tmpdir(), "axiom-shots-data-"));
  const wd = await WebDriver.start(APP, [...extraArgs, "--data-dir", data], env, log);
  return { wd, data };
}

async function main() {
  prepareWorld();
  rmSync(OUT, { recursive: true, force: true });
  mkdirSync(OUT, { recursive: true });
  let engine = "";

  // ---- S1 / S3: a backend that never answers, and one that dies at once ----------
  for (const [id, script, wantState] of [["s1", "daemon-stall", "starting"], ["s3", "daemon-fail", "failed"]]) {
    if (process.platform === "win32") {
      mark(id, { status: "skipped", note: "the stand-in backends are POSIX scripts" });
      continue;
    }
    const { wd, data } = await launchPlain(["--daemon", join(BASE, script)]);
    try {
      await state(wd, id, async () => {
        await boot(wd, { daemon: false });
        await waitFor(wd, `return document.querySelector('[data-testid=status]')?.dataset.state === '${wantState}'`, `backend ${wantState}`, 30000);
        // A failed backend may be restarted a few times by the host; settle on the final screen.
        await sleep(1500);
        await bothThemes(wd, id, { viaDaemon: false });
      });
    } finally {
      await wd.quit().catch(() => {});
      await removeDir(data);
    }
  }

  // ---- everything else: one real session ------------------------------------------
  const { wd, data } = await launchPlain(["--daemon", DAEMON]);
  try {
    await boot(wd);
    engine = await wd.exec(`return navigator.userAgent`);

    await state(wd, "s2", async () => {
      // Oldest first, so the Welcome list reads axiom, notes, scratch.
      for (const dir of [join(BASE, "scratch"), join(BASE, "notes"), WS]) {
        await wd.exec(`${STORES}.documents.requestOpenFolder(arguments[0])`, [dir]);
        await wd.waitFor(`return ${STORES}.workspace.getSnapshot().root === arguments[0]`, [dir], 15000, `workspace ${dir} opened`);
        await sleep(400);
      }
      await wd.exec(`${STORES}.documents.requestCloseFolder()`);
      await waitFor(wd, `return document.querySelector('[data-testid=welcome] .welcome__item') !== null`, "Welcome with recents");
      await waitFor(wd, `return document.querySelectorAll('[data-testid=welcome] .welcome__item').length === 3`, "three recents");
      await bothThemes(wd, "s2");
    });

    let workbench = false;
    await state(wd, "s4", async () => {
      await openWorkbench(wd);
      await passingRun(wd);
      workbench = true;
      await bothThemes(wd, "s4");
    });

    if (workbench) {
      await state(wd, "s5", async () => {
        await runCommand(wd, "node ../sim.js hold", RUNNING_WITH_OUTPUT);
        // The transport's rate meter shows the last 4 s of output: let it settle to a fixed (empty) state.
        await sleep(4500);
        await bothThemes(wd, "s5");
        await wd.exec(`${STORES}.tasks.stop()`);
        await waitFor(wd, OUTCOME("cancelled"), "the held run stops");
      });

      await state(wd, "s6", async () => {
        await runCommand(wd, "node ../sim.js fail", OUTCOME("failed"));
        await bothThemes(wd, "s6");
      });

      // Back to S4's passing console for everything that follows.
      await wd.exec(`${STORES}.tasks.clear()`);
      await passingRun(wd);

      await state(wd, "s7", async () => {
        await wd.chord(MOD, "k");
        await waitFor(wd, `return document.querySelector('[data-testid=palette] [role=option]') !== null`, "palette");
        await sleep(300);
        await bothThemes(wd, "s7");
        await esc(wd, 1);
        await waitFor(wd, `return document.querySelector('[data-testid=palette]') === null`, "palette closed");
      });

      await state(wd, "s8", async () => {
        await wd.chord(MOD, "p");
        await waitFor(wd, `return document.querySelector('[data-testid=palette] [role=option]') !== null`, "file palette");
        await wd.keys([..."exec"]);
        await waitFor(wd, `const o = [...document.querySelectorAll('[data-testid=palette] [role=option]')]; return o.length > 0 && o.length < 10 && o[0].textContent.includes('executor')`, "filtered files");
        await sleep(300);
        await bothThemes(wd, "s8");
        await esc(wd, 1);
        await waitFor(wd, `return document.querySelector('[data-testid=palette]') === null`, "palette closed");
      });

      await state(wd, "s9", async () => {
        await wd.click(await wd.find('button[aria-label="File menu"]'));
        await waitFor(wd, `return document.querySelectorAll('.menu__item').length > 0`, "File menu");
        await sleep(300);
        await bothThemes(wd, "s9");
        await esc(wd, 1);
        await waitFor(wd, `return document.querySelectorAll('.menu__item').length === 0`, "File menu closed");
      });

      await state(wd, "s10", async () => {
        await wd.exec(
          `const el = document.querySelector('[data-testid=output]'); const r = el.getBoundingClientRect();
           el.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: Math.round(r.left + 220), clientY: Math.round(r.top + 40) }));`,
        );
        await waitFor(wd, `return document.querySelector('[data-testid=output-menu]') !== null`, "Output menu");
        await bothThemes(wd, "s10");
        await esc(wd, 1);
        await waitFor(wd, `return document.querySelector('[data-testid=output-menu]') === null`, "Output menu closed");
      });

      await state(wd, "s11", async () => {
        await wd.click(await wd.find('button[aria-label="Settings"]'));
        await waitFor(wd, `return document.querySelector('[role=dialog] #font-size') !== null`, "Settings");
        await sleep(300);
        await bothThemes(wd, "s11");
        await esc(wd, 1);
        await waitFor(wd, `return document.querySelector('[role=dialog]') === null`, "Settings closed");
      });

      await state(wd, "s13", async () => {
        await wd.click(await wd.find('button[aria-label^="Toggle side bar"]'));
        await wd.click(await wd.find('button[aria-label^="Toggle panel"]'));
        await waitFor(wd, `return !document.querySelector('.sidebar') && !document.querySelector('[data-testid=output-panel]')`, "compact layout");
        await bothThemes(wd, "s13");
        await wd.click(await wd.find('button[aria-label^="Toggle side bar"]'));
        await wd.click(await wd.find('button[aria-label^="Toggle panel"]'));
        await waitFor(wd, `return !!document.querySelector('.sidebar') && !!document.querySelector('[data-testid=output-panel]')`, "layout restored");
      });

      await state(wd, "s14", async () => {
        await wd.cmd("POST", "/window/rect", { x: 0, y: 0, ...MIN_WINDOW });
        await sleep(500);
        await waitFor(wd, `return window.innerWidth <= ${MIN_WINDOW.width + 40}`, "the window shrank to its minimum");
        try {
          await bothThemes(wd, "s14");
        } finally {
          await wd.cmd("POST", "/window/rect", { x: 0, y: 0, ...WINDOW });
          await sleep(500);
        }
      });

      await state(wd, "s15", async () => {
        // Selected row: executor.rs (S13 remounted the explorer, which forgets its selection).
        await wd.click(await wd.find(treeRow("src/executor.rs")));
        // Keyboard focus, so :focus-visible matches (focus by script after a click does not):
        // focus the header's Refresh button, Tab into the tree, then move down one row.
        // The window must have input focus for :focus-* to match.
        const focusedWindow = focusAppWindow(wd.pid);
        await sleep(200);
        await wd.exec(`document.querySelector('button[aria-label="Refresh explorer"]').focus()`);
        await wd.keys(["\uE004"]); // Tab
        await wd.keys([DOWN]);
        if (!focusedWindow) mark("s15", { note: "the app window could not be given X input focus (no python-xlib): the focus ring may be missing" });
        await waitFor(wd, `return document.activeElement?.dataset.testid === 'explorer' && document.querySelector('[data-testid=explorer]').matches(':focus-visible') && document.querySelector('[data-testid=explorer] .tree-row[data-focused=true]') !== null`, "keyboard focus on a tree row");
        await bothThemes(wd, "s15");
      });

      await state(wd, "s12", async () => {
        await wd.exec(`${STORES}.documents.close("src/backend.rs")`);
        await waitFor(wd, `return document.querySelector('[data-testid=unsaved-dialog]') !== null`, "unsaved dialog");
        await sleep(300);
        await bothThemes(wd, "s12");
        await wd.click(await wd.find('[data-testid=unsaved-dialog] [data-action=cancel]'));
        await waitFor(wd, `return document.querySelector('[data-testid=unsaved-dialog]') === null`, "dialog closed");
      });

      await state(wd, "s12b", async () => {
        // executor.rs is active: edit it, change it on disk, then save.
        await commentOutFirstLine(wd);
        await waitFor(wd, `return document.querySelector('[role=tab][data-path="src/executor.rs"]')?.dataset.dirty === 'true'`, "executor.rs dirty");
        writeFileSync(join(WS, "src", "executor.rs"), "// rewritten elsewhere\n");
        await waitFor(wd, `return document.querySelector('[role=tab][data-path="src/executor.rs"]')?.textContent.includes('changed on disk')`, "changed-on-disk badge");
        await wd.chord(MOD, "s");
        await waitFor(wd, `return document.querySelector('[data-testid=conflict-dialog]') !== null`, "conflict dialog");
        await sleep(300);
        await bothThemes(wd, "s12b");
        await wd.click(await wd.find('[data-testid=conflict-dialog] [data-action=cancel]'));
      });
    }
  } finally {
    await wd.quit().catch(() => {});
    await removeDir(data);
  }

  // ---- S4 at 200% (GDK_SCALE=2) ----------------------------------------------------
  if (process.platform !== "linux") {
    states.get("s4").note = "the 200% pass runs on Linux only (GDK_SCALE); macOS and Windows are QA-005";
  } else {
    // S12b rewrote a file of the workspace: start the 200% pass from a clean copy.
    prepareWorld();
    const { wd, data } = await launchPlain(["--daemon", DAEMON], { GDK_SCALE: "2", GDK_DPI_SCALE: "1" });
    try {
      await boot(wd);
      const vp = await viewport(wd);
      if (vp.dpr !== 2) throw new Error(`GDK_SCALE=2 gave devicePixelRatio ${vp.dpr}, not 2`);
      await openWorkbench(wd);
      await passingRun(wd);
      await bothThemes(wd, "s4", { scale: 2 });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      failures.push(`s4@2x: ${msg}`);
      console.log(`FAIL  s4@2x — ${msg}`);
      await wd.diagnose("shots s4@2x failure", log);
    } finally {
      await wd.quit().catch(() => {});
      await removeDir(data);
    }
  }

  // ---- S17: gallery, 100% and (Linux) 200% -------------------------------------------
  for (const scale of process.platform === "linux" ? [1, 2] : [1]) {
    try {
      await captureGallery(scale);
      mark("s17", { status: "captured" });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      failures.push(`s17${scale === 2 ? "@2x" : ""}: ${msg}`);
      mark("s17", { status: "failed", note: msg });
      console.log(`FAIL  s17 — ${msg}`);
    }
  }

  return engine;
}

function git(...args) {
  const r = spawnSync("git", args, { cwd: repo, encoding: "utf8" });
  return r.status === 0 ? r.stdout.trim() : "";
}

let engine = "";
try {
  engine = await main();
} catch (e) {
  failures.push(`suite: ${e instanceof Error ? e.stack ?? e.message : String(e)}`);
  console.log(`FAIL  unexpected failure — ${e instanceof Error ? e.message : e}`);
}

for (const st of states.values()) if (st.status === "not-run") st.status = "failed";
const manifest = {
  commit: git("rev-parse", "HEAD"),
  dirty: git("status", "--porcelain", "--", "apps/desktop-tauri/src", "crates", "editor", "tests/fixtures/visual") !== "",
  os: OS,
  platform: `${process.platform} ${process.arch}`,
  engine,
  webdriver: PROVIDER,
  window: WINDOW,
  minimumWindow: MIN_WINDOW,
  scales: [1, 2],
  fixture: "tests/fixtures/visual",
  extraCss: process.env.AXIOM_SHOTS_EXTRA_CSS ?? null,
  seconds: Math.round((Date.now() - started) / 1000),
  states: [...states.values()],
};
mkdirSync(OUT, { recursive: true });
writeFileSync(join(OUT, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");

const all = [...states.values()];
const count = (s) => all.filter((x) => x.status === s).length;
console.log(
  `\n${count("captured")} states captured, ${count("pending")} pending, ${count("skipped")} skipped, ${count("failed")} failed ` +
    `(${manifest.seconds} s) → ${OUT}`,
);
for (const f of failures) console.log(`FAIL  ${f}`);
process.exit(failures.length || count("failed") ? 1 : 0);
