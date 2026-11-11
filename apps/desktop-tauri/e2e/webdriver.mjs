// Minimal W3C WebDriver client (fetch only, no dependencies) for driving the
// real Tauri app on Linux, Windows and macOS, through one of two servers:
//
//   "embedded"  The app is built with the `e2e` Cargo feature, which embeds a
//               W3C WebDriver server (`tauri-plugin-wdio-webdriver`, the
//               provider @wdio/tauri-service uses by default) on 127.0.0.1.
//               Works on all three OSes. Keyboard input is dispatched by
//               script: correct text and shortcuts, but `isTrusted === false`.
//   "external"  tauri-driver → WebKitWebDriver (Linux only). WebKit's own
//               automation delivers *trusted* key events, the closest thing to
//               a user typing, so Linux keeps proving that.
//
// AXIOM_WEBDRIVER selects it; the default is "external" on Linux and
// "embedded" elsewhere. Both launch the app with the same arguments and both
// expose the same commands to the suites.

import { spawn } from "node:child_process";
import { appendFileSync, readFileSync } from "node:fs";
import { createServer } from "node:net";

/** Which WebDriver server drives the app in this run. */
export const PROVIDER =
  process.env.AXIOM_WEBDRIVER ?? (process.platform === "linux" ? "external" : "embedded");
if (PROVIDER !== "embedded" && PROVIDER !== "external") {
  throw new Error(`AXIOM_WEBDRIVER must be "embedded" or "external", not ${JSON.stringify(PROVIDER)}`);
}
/** True when key events reach the page as trusted (OS-level) input. */
export const TRUSTED_INPUT = PROVIDER === "external";

/**
 * A log sink for the app's stderr: every line goes to AXIOM_E2E_LOG (if set),
 * tagged with the suite, and to `extra` (assertions on the host's output).
 */
export function appLogger(suite, extra = () => {}) {
  const file = process.env.AXIOM_E2E_LOG;
  return (line) => {
    extra(line);
    if (!file) return;
    try {
      appendFileSync(file, `[${suite}] ${line}\n`);
    } catch {
      /* logging must never fail a check */
    }
  };
}

export async function freePort() {
  return new Promise((resolve, reject) => {
    const srv = createServer();
    srv.once("error", reject);
    srv.listen(0, "127.0.0.1", () => {
      const { port } = srv.address();
      srv.close(() => resolve(port));
    });
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitForStatus(base, ms, gone = () => null) {
  const deadline = Date.now() + ms;
  let last = "no answer";
  while (Date.now() < deadline) {
    const exited = gone();
    if (exited) throw new Error(exited);
    try {
      const res = await fetch(`${base}/status`);
      const body = await res.json();
      if (body?.value?.ready !== false && res.ok) return;
      last = JSON.stringify(body);
    } catch (e) {
      last = e instanceof Error ? e.message : String(e);
    }
    await sleep(100);
  }
  throw new Error(`the WebDriver server at ${base} did not become ready in ${ms} ms (${last})`);
}

/** Forwards a child's output, line by line. */
function pipeLines(child, onLog) {
  const forward = (chunk) => {
    for (const line of chunk.toString().split(/\r?\n/)) if (line) onLog(line);
  };
  child.stdout?.on("data", forward);
  child.stderr?.on("data", forward);
}

async function spawned(child) {
  await new Promise((resolve, reject) => {
    child.once("spawn", resolve);
    child.once("error", reject);
  });
}

const isGone = (child) => child.exitCode !== null || child.signalCode !== null;

async function waitForExit(child, ms) {
  if (isGone(child)) return true;
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(false), ms);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

/**
 * The app process in the "external" mode is started by WebKitWebDriver, not by
 * us: we know it only by pid. This stands in for a ChildProcess there, with
 * the same fields the suites read.
 */
class ForeignProcess {
  constructor(pid) {
    this.pid = pid;
    this.exitCode = null;
    this.signalCode = null;
  }
  refresh() {
    if (this.exitCode !== null) return;
    try {
      process.kill(this.pid, 0);
      // A zombie is still signalable; /proc tells it apart on Linux.
      const stat = readProcStat(this.pid);
      if (stat === "Z") this.exitCode = 0;
    } catch {
      this.exitCode = 0;
    }
  }
}

/** A Linux process's state letter ("R", "S", "Z", ...), or null if unknown. */
function readProcStat(pid) {
  try {
    // "pid (comm) S ..." - the state is the first field after the last ")".
    const text = readFileSync(`/proc/${pid}/stat`, "utf8");
    const end = text.lastIndexOf(")");
    return text.slice(end + 2, end + 3);
  } catch {
    return null;
  }
}

export class WebDriver {
  constructor(base, sessionId, app, extras = {}) {
    this.base = base;
    this.id = sessionId;
    /** The app process (a ChildProcess, or a ForeignProcess in "external" mode). */
    this.child = app;
    /** Processes we own besides the app (tauri-driver). */
    this.helpers = extras.helpers ?? [];
  }

  /** The app's process id. */
  get pid() {
    return this.child.pid;
  }

  /**
   * Launches `application` with `args` (and extra `env`) and opens a session.
   * The app's output (and in "external" mode the driver's) goes to
   * `onLog(line)`, so suites can assert on the host's own log lines.
   */
  static async start(application, args = [], env = {}, onLog = () => {}) {
    return PROVIDER === "embedded"
      ? WebDriver.#startEmbedded(application, args, env, onLog)
      : WebDriver.#startExternal(application, args, env, onLog);
  }

  static async #startEmbedded(application, args, env, onLog) {
    const port = await freePort();
    const child = spawn(application, args, {
      env: { ...process.env, ...env, TAURI_WEBDRIVER_PORT: String(port) },
      stdio: ["ignore", "pipe", "pipe"],
    });
    pipeLines(child, onLog);
    await spawned(child);
    const base = `http://127.0.0.1:${port}`;
    try {
      await waitForStatus(base, 30000, () =>
        isGone(child) ? `the app exited (${child.exitCode ?? child.signalCode}) before its WebDriver server came up` : null,
      );
      const id = await createSession(base, { alwaysMatch: {} });
      return new WebDriver(base, id, child);
    } catch (e) {
      child.kill();
      throw e;
    }
  }

  static async #startExternal(application, args, env, onLog) {
    if (process.platform !== "linux") {
      throw new Error("AXIOM_WEBDRIVER=external is Linux-only (tauri-driver has no macOS path; Windows uses embedded)");
    }
    const port = await freePort();
    const nativePort = await freePort();
    // tauri-driver passes its environment to WebKitWebDriver, which starts the
    // app: `env` must reach the app through it.
    const driver = spawn("tauri-driver", ["--port", String(port), "--native-port", String(nativePort)], {
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
      detached: true,
    });
    pipeLines(driver, onLog);
    await spawned(driver);
    const base = `http://127.0.0.1:${port}`;
    try {
      await waitForStatus(base, 15000, () =>
        isGone(driver) ? `tauri-driver exited (${driver.exitCode ?? driver.signalCode}) before answering` : null,
      );
      const id = await createSession(base, {
        alwaysMatch: { "tauri:options": { application, args }, browserName: "wry" },
      });
      const wd = new WebDriver(base, id, null, { helpers: [driver] });
      // WebKitWebDriver returns the session as soon as the webview exists,
      // before the page has run: wait for the host bridge it installs.
      await wd.waitFor(`return !!window.__axiomHost`, [], 30000, "the page to boot");
      // The app's pid, as the host reports it: WebKitWebDriver launched it.
      const pid = await wd.execAsync(
        `const cb = arguments[arguments.length-1]; window.__axiomHost.appInfo().then(i => cb(i.pid))`,
      );
      wd.child = new ForeignProcess(pid);
      return wd;
    } catch (e) {
      killGroup(driver);
      throw e;
    }
  }

  async cmd(method, path, payload) {
    const res = await fetch(`${this.base}/session/${this.id}${path}`, {
      method,
      headers: { "content-type": "application/json" },
      body: payload === undefined ? undefined : JSON.stringify(payload),
    });
    const body = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(`${method} ${path}: ${JSON.stringify(body.value ?? body)}`);
    return body.value;
  }

  exec(script, args = []) {
    return this.cmd("POST", "/execute/sync", { script, args });
  }

  execAsync(script, args = []) {
    return this.cmd("POST", "/execute/async", { script, args });
  }

  /**
   * Finds an element, waiting up to `ms` for it to exist (the UI renders
   * asynchronously: Monaco is lazy-loaded, dialogs animate in). Polls rather
   * than relying on a driver's implicit wait, so the behaviour is the same on
   * both servers. Fails with the selector in the message.
   */
  async find(css, ms = 10000) {
    const deadline = Date.now() + ms;
    let last;
    for (;;) {
      try {
        const v = await this.cmd("POST", "/element", { using: "css selector", value: css });
        return Object.values(v)[0];
      } catch (e) {
        last = e;
        if (!String(e).includes("no such element") || Date.now() >= deadline) break;
      }
      await sleep(50);
    }
    throw new Error(`element not found within ${ms} ms: ${css} (${last instanceof Error ? last.message : last})`);
  }

  click(el) {
    return this.cmd("POST", `/element/${el}/click`, {});
  }

  /** Sends keyboard input to the focused element (W3C actions). */
  async keys(sequence) {
    const actions = [];
    for (const k of sequence) {
      actions.push({ type: "keyDown", value: k }, { type: "keyUp", value: k });
    }
    await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "kbd", actions }] });
  }

  /** Chord, e.g. chord("\uE009", "s") for Ctrl+S. */
  async chord(...keys) {
    const actions = [
      ...keys.map((k) => ({ type: "keyDown", value: k })),
      ...[...keys].reverse().map((k) => ({ type: "keyUp", value: k })),
    ];
    await this.cmd("POST", "/actions", { actions: [{ type: "key", id: "kbd", actions }] });
  }

  async screenshot() {
    return Buffer.from(await this.cmd("GET", "/screenshot"), "base64");
  }

  async waitFor(fnBody, args = [], ms = 10000, what = fnBody) {
    const deadline = Date.now() + ms;
    let last;
    while (Date.now() < deadline) {
      try {
        last = await this.exec(fnBody, args);
        if (last) return last;
      } catch (e) {
        last = e;
      }
      await sleep(50);
    }
    throw new Error(`timed out (${ms} ms) waiting for: ${what}; last=${last}`);
  }

  /** True while the app process is running. */
  alive() {
    if (this.child instanceof ForeignProcess) this.child.refresh();
    return !isGone(this.child);
  }

  /** Resolves true when the app has exited, false after `ms`. */
  async waitForExit(ms) {
    if (!(this.child instanceof ForeignProcess)) return waitForExit(this.child, ms);
    const deadline = Date.now() + ms;
    while (Date.now() < deadline) {
      if (!this.alive()) return true;
      await sleep(100);
    }
    return false;
  }

  /**
   * The app's exit code when this harness can know it: always in "embedded"
   * mode (we are its parent); never in "external" mode (WebKitWebDriver is).
   */
  get exitCode() {
    return this.child instanceof ForeignProcess ? undefined : this.child.exitCode;
  }

  /** Ends the session, the app and any helper process. Safe to call twice. */
  async quit() {
    try {
      await fetch(`${this.base}/session/${this.id}`, { method: "DELETE" });
    } catch {
      /* the server is already gone */
    }
    if (!(this.child instanceof ForeignProcess) && !isGone(this.child)) {
      this.child.kill();
      if (!(await waitForExit(this.child, 5000))) this.child.kill("SIGKILL");
    }
    if (this.child instanceof ForeignProcess && this.alive()) {
      try {
        process.kill(this.child.pid, "SIGTERM");
      } catch {
        /* gone */
      }
    }
    for (const h of this.helpers) killGroup(h);
  }
}

async function createSession(base, capabilities) {
  const res = await fetch(`${base}/session`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ capabilities }),
  });
  const body = await res.json();
  if (!res.ok || !body.value?.sessionId) throw new Error(`session failed: ${JSON.stringify(body)}`);
  return body.value.sessionId;
}

/** Ends a detached helper and its process group (tauri-driver + WebKitWebDriver). */
function killGroup(child) {
  if (isGone(child)) return;
  try {
    process.kill(-child.pid, "SIGTERM");
  } catch {
    child.kill("SIGTERM");
  }
}

export const Keys = { CTRL: "\uE009", SHIFT: "\uE008", ENTER: "\uE007", ESC: "\uE00C", META: "\uE03D" };
