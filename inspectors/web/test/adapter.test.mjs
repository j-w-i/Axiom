// The in-page adapter's logic (M5-WEB-001) over jsdom, with layout supplied by the test (jsdom
// has none): snapshots parents-first with bounds and stamps, hit testing to the deepest element
// with its ancestors, details with namespaced css.* properties, the highlight overlay; and the
// protocol: hello, requests, notifications, inspect mode swallowing the click.
import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { createAdapter, start } from "../adapter.js";

/** A page with layout boxes from `data-box="x,y,w,h"`, and elementsFromPoint over them. */
function page(html) {
  const dom = new JSDOM(`<!doctype html><html><head><title>Fixture</title></head><body>${html}</body></html>`, {
    pretendToBeVisual: true,
  });
  const win = dom.window;
  const box = (el) => {
    const b = el.getAttribute && el.getAttribute("data-box");
    if (el === win.document.body) return [0, 0, 800, 600];
    return b ? b.split(",").map(Number) : [0, 0, 0, 0];
  };
  win.HTMLElement.prototype.getBoundingClientRect = function () {
    const [x, y, w, h] = box(this);
    return { x, y, width: w, height: h, left: x, top: y, right: x + w, bottom: y + h };
  };
  win.document.elementsFromPoint = (x, y) => {
    const hits = [];
    for (const el of win.document.querySelectorAll("body *")) {
      const [bx, by, w, h] = box(el);
      if (x >= bx && x < bx + w && y >= by && y < by + h) hits.push(el);
    }
    // Deepest (last in document order among nested) first; nothing outside the viewport.
    if (x < 0 || y < 0 || x >= 800 || y >= 600) return [];
    return hits.reverse().concat([win.document.body, win.document.documentElement]);
  };
  win.innerWidth = 800;
  win.innerHeight = 600;
  return win;
}

const HTML = `
<main data-box="0,0,800,600" data-axiom-src="src/App.jsx:5:5">
  <section id="card" data-box="10,10,300,100" data-axiom-src="src/Card.jsx:3:5">
    <button data-box="20,20,100,30" data-axiom-src="src/Card.jsx:6:7" aria-label="Pay now">Pay</button>
    <span data-box="140,20,60,30">library text</span>
  </section>
  <ul data-box="10,200,300,100">
    <li data-box="10,200,300,20" data-axiom-src="src/List.jsx:4:9">one</li>
    <li data-box="10,220,300,20" data-axiom-src="src/List.jsx:4:9">two</li>
  </ul>
  <div style="display:none" data-box="0,0,10,10">hidden</div>
  <script>1</script>
</main>`;

test("snapshot: parents first, bounds, stamps, stable keys, instances, invisible omitted", () => {
  const win = page(HTML);
  const a = createAdapter(win);
  const s = a.snapshot({});
  assert.equal(s.surfaces[0].id, "page");
  assert.equal(s.surfaces[0].title, "Fixture");
  const seen = new Set();
  for (const n of s.nodes) {
    if (n.parent) assert.ok(seen.has(n.parent), `parent of ${n.typeName} first`);
    seen.add(n.id);
  }
  const btn = s.nodes.find((n) => n.typeName === "button");
  assert.deepEqual(btn.bounds, { x: 20, y: 20, width: 100, height: 30 });
  assert.deepEqual(btn.sources[0], {
    location: { file: "src/Card.jsx", line: 6, column: 7 },
    relation: "instantiation",
    method: "buildInstrumentation",
    confidence: 1,
  });
  assert.equal(btn.stableKey, "src/Card.jsx:6:7#0");
  assert.equal(btn.text, "Pay");
  assert.deepEqual(btn.accessibility, { role: "button", name: "Pay now" });
  const lis = s.nodes.filter((n) => n.typeName === "li");
  assert.deepEqual(lis.map((l) => l.instance), [{ index: 0, count: 2 }, { index: 1, count: 2 }]);
  assert.deepEqual(lis.map((l) => l.stableKey), ["src/List.jsx:4:9#0", "src/List.jsx:4:9#1"]);
  assert.equal(s.nodes.find((n) => n.name === "card").stableKey, "src/Card.jsx:3:5#0");
  assert.ok(!s.nodes.some((n) => n.text === "hidden"), "invisible omitted");
  assert.ok(!s.nodes.some((n) => n.typeName === "script"));
  const all = a.snapshot({ includeInvisible: true });
  assert.ok(all.nodes.some((n) => n.text === "hidden"));
  const capped = a.snapshot({ maxNodes: 3 });
  assert.equal(capped.nodes.length, 3);
  assert.equal(capped.truncated, true);
  // Children lists agree with parents.
  const main = s.nodes.find((n) => n.typeName === "main");
  assert.equal(main.children.length, 2);
});

test("hit test: the deepest element and its ancestors; empty space is not_found", () => {
  const win = page(HTML);
  const a = createAdapter(win);
  const r = a.hitTest({ surface: "page", x: 25, y: 25 });
  assert.deepEqual(r.chain.map((n) => n.typeName), ["button", "section", "main", "body"]);
  const span = a.hitTest({ surface: "page", x: 150, y: 25 }).chain[0];
  assert.equal(span.text, "library text");
  assert.deepEqual(span.sources, []);
  const bodyOnly = createAdapter(page(""));
  assert.throws(() => bodyOnly.hitTest({ x: 5000, y: 5000 }), (e) => e.data.kind === "not_found");
});

test("details: namespaced css.* and dom.* properties; a gone node is not_found", () => {
  const win = page(HTML);
  const a = createAdapter(win);
  const btn = a.hitTest({ x: 25, y: 25 }).chain[0];
  const d = a.nodeDetails({ node: btn.id });
  assert.equal(d.properties["dom.tag"], "button");
  assert.equal(d.properties["css.display"], "inline-block");
  assert.ok(Object.keys(d.properties).every((k) => k.startsWith("css.") || k.startsWith("dom.")));
  win.document.querySelector("button").remove();
  assert.throws(() => a.nodeDetails({ node: btn.id }), (e) => e.data.kind === "not_found");
});

test("highlight: an overlay over the node that hit testing ignores", () => {
  const win = page(HTML);
  const a = createAdapter(win);
  const btn = a.hitTest({ x: 25, y: 25 }).chain[0];
  a.highlight({ node: btn.id });
  const o = win.document.querySelector("[data-axiom-overlay]");
  assert.equal(o.style.left, "20px");
  assert.equal(o.style.width, "100px");
  assert.equal(o.style.pointerEvents, "none");
  assert.ok(!a.snapshot({}).nodes.some((n) => n.typeName === "div" && n.bounds.width === 100));
  a.highlight({});
  assert.equal(o.style.display, "none");
});

/** A fake WebSocket: the test plays the daemon. */
function fakeSocket() {
  const sockets = [];
  class WS {
    constructor(url) {
      this.url = url;
      this.readyState = 1;
      this.sent = [];
      sockets.push(this);
      queueMicrotask(() => this.onopen && this.onopen());
    }
    send(t) {
      this.sent.push(JSON.parse(t));
    }
    close() {
      this.readyState = 3;
    }
    daemon(m) {
      this.onmessage({ data: JSON.stringify(m) });
    }
  }
  return { WS, sockets };
}

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

test("protocol: the token in the URL, hello, requests, errors, and notifications", async () => {
  const win = page(HTML);
  const { WS, sockets } = fakeSocket();
  let afterUpdate;
  const hot = { on: (e, f) => e === "vite:afterUpdate" && (afterUpdate = f) };
  const s = start({ endpoint: "ws://127.0.0.1:9/", token: "t 0", hot, win, WebSocketImpl: WS });
  const ws = sockets[0];
  assert.equal(ws.url, "ws://127.0.0.1:9/?token=t%200");
  ws.daemon({ jsonrpc: "2.0", id: 1, method: "inspect/hello", params: { protocolVersion: "0.5", token: "t 0" } });
  assert.equal(ws.sent[0].id, 1);
  assert.equal(ws.sent[0].result.screenshot, false);
  assert.equal(ws.sent[0].result.source, "lineColumn");
  ws.daemon({ jsonrpc: "2.0", id: 2, method: "inspect/snapshot", params: {} });
  assert.ok(ws.sent[1].result.nodes.length > 3);
  ws.daemon({ jsonrpc: "2.0", id: 3, method: "inspect/nodeDetails", params: { node: "nope" } });
  assert.equal(ws.sent[2].error.data.kind, "not_found");
  ws.daemon({ jsonrpc: "2.0", id: 4, method: "inspect/nope", params: {} });
  assert.equal(ws.sent[3].error.code, -32601);

  // A change in the page → didMutate (coalesced); a hot reload → didMutate hotReload.
  ws.sent.length = 0;
  win.document.querySelector("button").textContent = "Pay later";
  win.document.querySelector("span").textContent = "x";
  await tick(80);
  assert.deepEqual(ws.sent.map((m) => [m.method, m.params.reason]), [["inspect/didMutate", "tree"]]);
  afterUpdate();
  assert.deepEqual(ws.sent.at(-1), { jsonrpc: "2.0", method: "inspect/didMutate", params: { reason: "hotReload" } });

  // A console error → runtimeError (and the console still gets it).
  win.console.error("boom", new Error("bad"));
  const re = ws.sent.find((m) => m.method === "inspect/runtimeError");
  assert.match(re.params.message, /boom bad/);
  s.stop();
});

test("inspect mode: hover highlights and notifies; a click selects and does not reach the app", async () => {
  const win = page(HTML);
  const { WS, sockets } = fakeSocket();
  const s = start({ endpoint: "ws://x/", token: "t", win, WebSocketImpl: WS });
  const ws = sockets[0];
  let appClicks = 0;
  win.document.querySelector("button").addEventListener("click", () => appClicks++);
  ws.daemon({ jsonrpc: "2.0", id: 1, method: "inspect/setMode", params: { on: true } });
  assert.deepEqual(ws.sent[0], { jsonrpc: "2.0", id: 1, result: {} });

  win.dispatchEvent(new win.MouseEvent("pointermove", { clientX: 25, clientY: 25, bubbles: true }));
  await tick(40);
  const hovered = ws.sent.find((m) => m.method === "inspect/hovered");
  assert.equal(hovered.params.node.typeName, "button");
  assert.equal(win.document.querySelector("[data-axiom-overlay]").style.left, "20px");

  win.document.querySelector("button").dispatchEvent(new win.MouseEvent("click", { clientX: 25, clientY: 25, bubbles: true, cancelable: true }));
  const selected = ws.sent.find((m) => m.method === "inspect/selected");
  assert.deepEqual(selected.params.chain.map((n) => n.typeName), ["button", "section", "main", "body"]);
  assert.equal(appClicks, 0, "the click did not reach the app");

  // Off: clicks reach the app again, the overlay is hidden.
  s.setMode(false);
  win.document.querySelector("button").dispatchEvent(new win.MouseEvent("click", { bubbles: true }));
  assert.equal(appClicks, 1);
  assert.equal(win.document.querySelector("[data-axiom-overlay]").style.display, "none");
  s.stop();
});
