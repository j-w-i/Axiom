// Minimal W3C WebDriver client (fetch only, no dependencies) for driving the
// real Tauri app through tauri-driver → WebKitWebDriver (Linux) / msedgedriver
// (Windows). macOS has no WKWebView WebDriver; see performance.md §Method.

export class WebDriver {
  constructor(base, sessionId) {
    this.base = base;
    this.id = sessionId;
  }

  static async start(base, application, args = [], env = {}) {
    const res = await fetch(`${base}/session`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        capabilities: {
          alwaysMatch: {
            "tauri:options": { application, args, env },
            browserName: "wry",
          },
        },
      }),
    });
    const body = await res.json();
    if (!res.ok || !body.value?.sessionId) throw new Error(`session failed: ${JSON.stringify(body)}`);
    return new WebDriver(base, body.value.sessionId);
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

  async find(css) {
    const v = await this.cmd("POST", "/element", { using: "css selector", value: css });
    return Object.values(v)[0];
  }

  click(el) {
    return this.cmd("POST", `/element/${el}/click`, {});
  }

  /** Sends real keyboard input to the focused element (W3C actions). */
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
      await new Promise((r) => setTimeout(r, 50));
    }
    throw new Error(`timed out (${ms} ms) waiting for: ${what}; last=${last}`);
  }

  async quit() {
    try {
      await fetch(`${this.base}/session/${this.id}`, { method: "DELETE" });
    } catch {
      /* already gone */
    }
  }
}

export const Keys = { CTRL: "\uE009", SHIFT: "\uE008", ENTER: "\uE007", ESC: "\uE00C", META: "\uE03D" };
