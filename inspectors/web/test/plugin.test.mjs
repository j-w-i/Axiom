// The Vite plugin (M5-WEB-001): stamps host JSX elements only, keeps a source map, is
// serve-only, and injects the adapter as a virtual module started with the endpoint and token.
import { test } from "node:test";
import assert from "node:assert/strict";
import path from "node:path";
import axiom, { loadBabel, stamp } from "../plugin.mjs";

const root = "/proj";
const file = path.join(root, "src/App.jsx");
const source = `import { Card } from "./Card";
import * as UI from "./ui";
export function App() {
  return (
    <main className="app">
      <h1>Hello</h1>
      <Card title="x" />
      <UI.Button />
      <>
        <span>frag child</span>
      </>
      <button data-axiom-src="kept:1:1">Pay</button>
    </main>
  );
}
`;

test("stamps host elements only, with file, line and column", async () => {
  const babel = await loadBabel(path.resolve("."));
  assert.ok(babel, "@babel/core resolves");
  const out = await stamp(babel, source, file, root);
  assert.match(out.code, /<main className="app" data-axiom-src="src\/App\.jsx:5:5">/);
  assert.match(out.code, /<h1 data-axiom-src="src\/App\.jsx:6:7">/);
  assert.match(out.code, /<span data-axiom-src="src\/App\.jsx:10:9">/);
  // Components, member expressions and fragments are not stamped.
  assert.match(out.code, /<Card title="x" \/>/);
  assert.match(out.code, /<UI\.Button \/>/);
  assert.match(out.code, /<>\n/);
  // An existing stamp is kept, not doubled.
  assert.equal((out.code.match(/data-axiom-src="kept:1:1"/g) || []).length, 1);
  assert.equal((out.code.match(/<button[^>]*data-axiom-src/g) || []).length, 1);
  // A source map that points back at the original.
  assert.ok(out.map && out.map.mappings.length > 0);
  assert.deepEqual(out.map.sources.map((s) => path.basename(s)), ["App.jsx"]);
});

test("TypeScript JSX is parsed; other files and node_modules are left alone", async () => {
  const babel = await loadBabel(path.resolve("."));
  const tsx = await stamp(babel, "const f = (x: number) => <div>{x}</div>;\n", path.join(root, "src/A.tsx"), root);
  assert.match(tsx.code, /<div data-axiom-src="src\/A\.tsx:1:26">/);
  assert.equal(await stamp(babel, "export const x = 1;", path.join(root, "src/a.js"), root), null);
  assert.equal(await stamp(babel, "<div/>", path.join(root, "node_modules/lib/x.jsx"), root), null);
});

test("serve only; the adapter is a virtual module started with the endpoint and token", async () => {
  const p = axiom({ endpoint: "ws://127.0.0.1:4242/", token: "t0k", adapterSource: "export function start(c) {}" });
  assert.equal(p.apply, "serve");
  assert.equal(p.enforce, "pre");
  const tags = p.transformIndexHtml();
  assert.deepEqual(tags[0], {
    tag: "script",
    attrs: { type: "module", src: "/@id/__x00__virtual:axiom-adapter" },
    injectTo: "head-prepend",
  });
  const id = p.resolveId("virtual:axiom-adapter");
  assert.equal(id, "\0virtual:axiom-adapter");
  assert.equal(p.resolveId("./other"), null);
  const code = p.load(id);
  assert.match(code, /export function start/);
  assert.match(code, /start\(\{ endpoint: "ws:\/\/127\.0\.0\.1:4242\/", token: "t0k", hot: import\.meta\.hot \}\);/);
  assert.equal(p.load("other"), null);
});

test("the real adapter is what the virtual module inlines", async () => {
  const p = axiom({ endpoint: "ws://x/", token: "t" });
  const code = p.load("\0virtual:axiom-adapter");
  assert.match(code, /export function createAdapter/);
  assert.match(code, /inspect\/hello/);
});

test("with Babel the transform stamps; it warns once and passes through without it", async () => {
  const p = axiom({ endpoint: "ws://x/", token: "t" });
  p.configResolved({ root: path.resolve(".") });
  await p.buildStart();
  const r = await p.transform("export const A = () => <p>x</p>;", path.resolve("src/A.jsx"));
  assert.match(r.code, /data-axiom-src="src\/A\.jsx:1:24"/);
  const q = axiom({});
  const warnings = [];
  const ctx = { warn: (m) => warnings.push(m) };
  // buildStart never ran: no Babel.
  assert.equal(await q.transform.call(ctx, "<p/>", "/x/A.jsx"), null);
  assert.equal(await q.transform.call(ctx, "<p/>", "/x/B.jsx"), null);
  assert.equal(warnings.length, 1);
});
