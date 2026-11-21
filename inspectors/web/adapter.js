// Axiom's in-page inspection adapter (ADR-0019 §1; docs/protocols/inspection.md). Plain DOM:
// it works in any browser. It dials the daemon's loopback WebSocket with the preview's token and
// answers `inspect/*` (the daemon is the JSON-RPC client), and sends notifications:
// `inspect/didMutate` (MutationObserver, resizes, Vite hot reloads), `inspect/runtimeError`
// (uncaught errors, console.error), and in inspect mode `inspect/hovered` and
// `inspect/selected` (the click is not passed to the app). No pixel capture (`screenshot:
// false`). The plugin inlines this file into a virtual module and calls `start`.

const NAME = "axiom-web-adapter";
const VERSION = "0.1.0";
const MAX_TEXT = 200;
const OVERLAY = "data-axiom-overlay";
const SKIP = new Set(["SCRIPT", "STYLE", "LINK", "META", "NOSCRIPT", "TEMPLATE", "HEAD", "TITLE"]);
const CSS = [
  "display", "position", "box-sizing",
  "width", "height", "min-width", "min-height", "max-width", "max-height",
  "margin-top", "margin-right", "margin-bottom", "margin-left",
  "padding-top", "padding-right", "padding-bottom", "padding-left",
  "border-top-width", "border-right-width", "border-bottom-width", "border-left-width", "border-radius",
  "gap", "row-gap", "column-gap", "flex-direction", "flex-wrap", "justify-content", "align-items", "align-self",
  "grid-template-columns", "grid-template-rows",
  "top", "right", "bottom", "left", "z-index",
  "color", "background-color", "opacity", "visibility", "overflow",
  "font-family", "font-size", "font-weight", "line-height", "letter-spacing", "text-align",
];

/** The adapter's logic over a window (tests pass a jsdom window). */
export function createAdapter(win) {
  const doc = win.document;
  const ids = new WeakMap();
  const nodes = new Map(); // id → WeakRef(element) (or the element where WeakRef is missing)
  let next = 1;

  function idOf(el) {
    let id = ids.get(el);
    if (!id) {
      id = "n" + next++;
      ids.set(el, id);
      nodes.set(id, typeof WeakRef === "function" ? new WeakRef(el) : el);
    }
    return id;
  }

  function byId(id) {
    const r = nodes.get(id);
    const el = r && (typeof r.deref === "function" ? r.deref() : r);
    return el && el.isConnected ? el : null;
  }

  function stampOf(el) {
    const s = el.getAttribute && el.getAttribute("data-axiom-src");
    if (!s) return null;
    const m = /^(.*):(\d+):(\d+)$/.exec(s);
    return m ? { file: m[1], line: Number(m[2]), column: Number(m[3]), raw: s } : null;
  }

  function ownText(el) {
    let t = "";
    for (const c of el.childNodes) if (c.nodeType === 3) t += c.nodeValue;
    t = t.replace(/\s+/g, " ").trim();
    return t ? t.slice(0, MAX_TEXT) : undefined;
  }

  function visible(el, rect) {
    const cs = win.getComputedStyle(el);
    const opacity = parseFloat(cs.opacity);
    if (cs.display === "none" || cs.visibility === "hidden" || opacity === 0) return false;
    return !rect || rect.width > 0 || rect.height > 0 || el === doc.body;
  }

  function instances(stamp) {
    const all = doc.querySelectorAll(`[data-axiom-src="${stamp.raw.replace(/["\\]/g, "\\$&")}"]`);
    return Array.from(all);
  }

  function role(el) {
    const r = el.getAttribute("role");
    if (r) return r;
    const tag = el.tagName.toLowerCase();
    return { button: "button", a: "link", input: "textbox", h1: "heading", h2: "heading", h3: "heading", nav: "navigation", main: "main", img: "img" }[tag];
  }

  /** One element as a `UiNode` (without properties). */
  function describe(el) {
    const rect = el.getBoundingClientRect();
    const vw = win.innerWidth, vh = win.innerHeight;
    const n = {
      id: idOf(el),
      surface: "page",
      children: [],
      typeName: el.tagName.toLowerCase(),
      kind: "element",
      bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      visibility: {
        visible: visible(el, rect),
        clipped: rect.x < 0 || rect.y < 0 || rect.right > vw || rect.bottom > vh,
      },
      sources: [],
    };
    const parent = el.parentElement;
    if (parent && parent !== doc.documentElement) n.parent = idOf(parent);
    const name = el.id || el.getAttribute("data-testid");
    if (name) n.name = name;
    const text = ownText(el);
    if (text) n.text = text;
    const s = stampOf(el);
    if (s) {
      n.sources.push({
        location: { file: s.file, line: s.line, column: s.column },
        relation: "instantiation",
        method: "buildInstrumentation",
        confidence: 1,
      });
      const all = instances(s);
      const index = Math.max(0, all.indexOf(el));
      n.stableKey = `${s.raw}#${index}`;
      if (all.length > 1) n.instance = { index, count: all.length };
    } else if (name) {
      n.stableKey = `#${name}`;
    }
    const r = role(el);
    const label = el.getAttribute("aria-label");
    if (r || label) n.accessibility = { ...(r ? { role: r } : {}), ...(label ? { name: label } : {}) };
    return n;
  }

  function surface() {
    return {
      id: "page",
      kind: "page",
      title: doc.title || undefined,
      width: win.innerWidth,
      height: win.innerHeight,
      devicePixelRatio: win.devicePixelRatio || 1,
      scrollX: win.scrollX || 0,
      scrollY: win.scrollY || 0,
    };
  }

  function isOverlay(el) {
    return !!(el && el.closest && el.closest(`[${OVERLAY}]`));
  }

  /** `inspect/snapshot`: parents first, bounded. */
  function snapshot({ root, maxNodes = 5000, includeInvisible = false } = {}) {
    const start = root ? byId(root) : doc.body;
    if (!start) throw { code: -32010, message: "That node no longer exists.", data: { kind: "not_found" } };
    const out = [];
    let truncated = false;
    const queue = [start];
    while (queue.length) {
      const el = queue.shift();
      if (SKIP.has(el.tagName) || isOverlay(el)) continue;
      const n = describe(el);
      if (!n.visibility.visible && !includeInvisible) continue;
      if (out.length >= maxNodes) {
        truncated = true;
        break;
      }
      out.push(n);
      for (const c of el.children) queue.push(c);
    }
    const ids = new Set(out.map((n) => n.id));
    const byIdMap = new Map(out.map((n) => [n.id, n]));
    for (const n of out) {
      if (n.parent && !ids.has(n.parent)) delete n.parent;
      if (n.parent) byIdMap.get(n.parent).children.push(n.id);
    }
    return { capturedMs: Date.now(), surfaces: [surface()], nodes: out, truncated };
  }

  /** The element under a point, ignoring the overlay. */
  function at(x, y) {
    const list = typeof doc.elementsFromPoint === "function" ? doc.elementsFromPoint(x, y) : [];
    return list.find((e) => !isOverlay(e) && e !== doc.documentElement) || null;
  }

  function chainOf(el) {
    const chain = [];
    for (let e = el; e && e !== doc.documentElement; e = e.parentElement) chain.push(describe(e));
    return { chain };
  }

  /** `inspect/hitTest`. */
  function hitTest({ x, y }) {
    const el = at(x, y);
    if (!el) throw { code: -32010, message: "Nothing is at that point.", data: { kind: "not_found" } };
    return chainOf(el);
  }

  /** `inspect/nodeDetails`: the node with its computed style (`css.*`) and DOM facts (`dom.*`). */
  function nodeDetails({ node }) {
    const el = byId(node);
    if (!el) throw { code: -32010, message: "That node no longer exists.", data: { kind: "not_found" } };
    const n = describe(el);
    const cs = win.getComputedStyle(el);
    const properties = {};
    for (const p of CSS) {
      const v = cs.getPropertyValue(p);
      if (v !== "") properties["css." + p] = v;
    }
    properties["dom.tag"] = el.tagName.toLowerCase();
    if (el.className && typeof el.className === "string") properties["dom.class"] = el.className;
    n.properties = properties;
    return n;
  }

  let overlay = null;
  /** `inspect/highlight`: an overlay over the node, or none. */
  function highlight({ node } = {}) {
    if (!node) {
      if (overlay) overlay.style.display = "none";
      return {};
    }
    const el = byId(node);
    if (!el) throw { code: -32010, message: "That node no longer exists.", data: { kind: "not_found" } };
    show(el);
    return {};
  }

  function show(el) {
    if (!overlay) {
      overlay = doc.createElement("div");
      overlay.setAttribute(OVERLAY, "");
      overlay.setAttribute("aria-hidden", "true");
      Object.assign(overlay.style, {
        position: "fixed", pointerEvents: "none", zIndex: "2147483647", boxSizing: "border-box",
        border: "2px solid #c9a35b", background: "rgba(201, 163, 91, 0.14)", borderRadius: "2px",
      });
      doc.documentElement.appendChild(overlay);
    }
    const r = el.getBoundingClientRect();
    Object.assign(overlay.style, {
      display: "block", left: r.x + "px", top: r.y + "px", width: r.width + "px", height: r.height + "px",
    });
  }

  return { snapshot, hitTest, nodeDetails, highlight, show, at, chainOf, describe, hideOverlay: () => highlight({}) };
}

/** Connects to the daemon and serves it (the plugin's virtual module calls this). */
export function start({ endpoint, token, hot, win = globalThis.window, WebSocketImpl } = {}) {
  if (!win || !endpoint || !token) return null;
  const WS = WebSocketImpl || win.WebSocket;
  const a = createAdapter(win);
  const doc = win.document;
  let ws = null;
  let mode = false;
  let closed = false;
  let retry = 500;
  const send = (m) => ws && ws.readyState === 1 && ws.send(JSON.stringify(m));
  const notify = (method, params) => send({ jsonrpc: "2.0", method, params });

  const methods = {
    "inspect/hello": () => ({
      framework: "dom",
      adapter: { name: NAME, version: VERSION },
      tree: true,
      hitTest: true,
      properties: true,
      accessibility: true,
      source: "lineColumn",
      highlight: true,
      screenshot: false,
      mutations: true,
    }),
    "inspect/snapshot": (p) => a.snapshot(p || {}),
    "inspect/hitTest": (p) => a.hitTest(p),
    "inspect/nodeDetails": (p) => a.nodeDetails(p),
    "inspect/highlight": (p) => a.highlight(p || {}),
    "inspect/setMode": (p) => {
      setMode(!!(p && p.on));
      return {};
    },
  };

  function connect() {
    if (closed) return;
    const sep = endpoint.includes("?") ? "&" : "?";
    ws = new WS(`${endpoint}${sep}token=${encodeURIComponent(token)}`);
    ws.onopen = () => (retry = 500);
    ws.onmessage = (ev) => {
      let m;
      try {
        m = JSON.parse(ev.data);
      } catch {
        return;
      }
      if (m.id === undefined || !m.method) return;
      const f = methods[m.method];
      if (!f) {
        send({ jsonrpc: "2.0", id: m.id, error: { code: -32601, message: `Unknown method ${m.method}` } });
        return;
      }
      try {
        send({ jsonrpc: "2.0", id: m.id, result: f(m.params) });
      } catch (e) {
        const err = e && e.code ? e : { code: -32603, message: String((e && e.message) || e) };
        send({ jsonrpc: "2.0", id: m.id, error: err });
      }
    };
    ws.onclose = () => {
      ws = null;
      if (!closed) setTimeout(connect, (retry = Math.min(retry * 2, 8000)));
    };
  }

  // Mutations, coalesced.
  let pending = null;
  const changed = (reason) => {
    if (pending) return;
    pending = setTimeout(() => {
      pending = null;
      notify("inspect/didMutate", { reason });
    }, 50);
  };
  const observer = new win.MutationObserver((records) => {
    if (records.every((r) => r.target && r.target.closest && r.target.closest(`[${OVERLAY}]`))) return;
    changed("tree");
  });
  observer.observe(doc.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
  win.addEventListener("resize", () => changed("layout"));
  if (hot && typeof hot.on === "function") {
    hot.on("vite:afterUpdate", () => notify("inspect/didMutate", { reason: "hotReload" }));
  }

  // Runtime errors.
  const error = (message) => notify("inspect/runtimeError", { message: String(message).slice(0, 2000), atMs: Date.now() });
  win.addEventListener("error", (e) => error(e.message || "Error"));
  win.addEventListener("unhandledrejection", (e) => error((e.reason && e.reason.message) || e.reason || "Unhandled rejection"));
  const origError = win.console && win.console.error;
  if (origError) {
    win.console.error = function (...args) {
      error(args.map((x) => (x && x.message) || String(x)).join(" "));
      return origError.apply(this, args);
    };
  }

  // Inspect mode: the pointer is the inspector's.
  let frame = 0;
  let lastHover = null;
  const onMove = (e) => {
    if (frame) return;
    frame = win.requestAnimationFrame(() => {
      frame = 0;
      const el = a.at(e.clientX, e.clientY);
      if (el === lastHover) return;
      lastHover = el;
      if (el) a.show(el);
      notify("inspect/hovered", el ? { node: a.describe(el) } : {});
    });
  };
  const swallow = (e) => {
    e.preventDefault();
    e.stopImmediatePropagation();
  };
  const onClick = (e) => {
    swallow(e);
    const el = a.at(e.clientX, e.clientY);
    if (el) notify("inspect/selected", a.chainOf(el));
  };
  function setMode(on) {
    if (on === mode) return;
    mode = on;
    const opts = { capture: true };
    const f = on ? "addEventListener" : "removeEventListener";
    win[f]("pointermove", onMove, opts);
    for (const t of ["pointerdown", "pointerup", "mousedown", "mouseup", "dblclick", "contextmenu"]) {
      win[f](t, swallow, opts);
    }
    win[f]("click", onClick, opts);
    if (!on) {
      lastHover = null;
      a.hideOverlay();
    }
    doc.documentElement.style.cursor = on ? "crosshair" : "";
  }

  connect();
  return {
    adapter: a,
    setMode,
    stop() {
      closed = true;
      observer.disconnect();
      setMode(false);
      if (ws) ws.close();
    },
  };
}
