// Axiom's Vite plugin (ADR-0019 §2; from experiment E3). Development server only
// (`apply: "serve"`): production builds never contain any of it.
//
// 1. Source stamps. A `pre` transform gives every *host* JSX element (lower-case tag) a
//    `data-axiom-src="<file relative to the root>:<line>:<column>"` attribute, before any other
//    JSX transform, so it works with any React version and JSX compiler. Components, fragments
//    and member expressions are left alone (they may not forward unknown props); the daemon
//    recovers their location as the owner of the nearest stamped ancestor.
// 2. The adapter. `transformIndexHtml` adds a module script that loads `virtual:axiom-adapter`:
//    the in-page adapter (adapter.js, inlined) started with the daemon's endpoint and the
//    preview's token, and told about hot reloads through `import.meta.hot`.
//
// Babel comes from the project (a Vite React project has `@babel/core` through
// `@vitejs/plugin-react`), else from this package. Without it nothing is stamped: inspection
// still works, with owners and text search only.

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const VIRTUAL = "virtual:axiom-adapter";
const RESOLVED = "\0" + VIRTUAL;

/** `@babel/core`, from the project first, then from beside this file; null if neither has it. */
export async function loadBabel(root) {
  for (const from of [path.join(root, "package.json"), path.join(here, "package.json")]) {
    try {
      const p = createRequire(from).resolve("@babel/core");
      const m = await import(pathToFileURL(p).href);
      return m.default ?? m;
    } catch {
      // Not there: the next place.
    }
  }
  return null;
}

/** The Babel plugin that stamps host elements. */
export function stampPlugin({ types: t }, { root }) {
  return {
    visitor: {
      JSXOpeningElement(p, state) {
        const loc = p.node.loc;
        if (!loc) return;
        const name = p.node.name;
        // Only host elements (a lower-case identifier) take DOM attributes safely.
        if (!t.isJSXIdentifier(name) || !/^[a-z]/.test(name.name)) return;
        const has = p.node.attributes.some(
          (a) => t.isJSXAttribute(a) && t.isJSXIdentifier(a.name) && a.name.name === "data-axiom-src",
        );
        if (has) return;
        const rel = path.relative(root, state.filename).split(path.sep).join("/");
        p.node.attributes.push(
          t.jsxAttribute(
            t.jsxIdentifier("data-axiom-src"),
            t.stringLiteral(`${rel}:${loc.start.line}:${loc.start.column + 1}`),
          ),
        );
      },
    },
  };
}

/** Stamps [code] of [file]; null when there is nothing to do. */
export async function stamp(babel, code, file, root) {
  if (!/\.[jt]sx$/.test(file) || file.includes("/node_modules/")) return null;
  const out = await babel.transformAsync(code, {
    filename: file,
    babelrc: false,
    configFile: false,
    sourceMaps: true,
    plugins: [[stampPlugin, { root }]],
    parserOpts: { plugins: file.endsWith(".tsx") ? ["jsx", "typescript"] : ["jsx"] },
  });
  return out ? { code: out.code, map: out.map } : null;
}

/**
 * The plugin. `endpoint` and `token` are the daemon's (the generated config passes them);
 * `adapterSource` overrides adapter.js (tests).
 */
export default function axiom({ endpoint, token, adapterSource } = {}) {
  let root = process.cwd();
  let babel;
  let warned = false;
  return {
    name: "axiom",
    enforce: "pre",
    apply: "serve",
    configResolved(c) {
      root = c.root;
    },
    async buildStart() {
      babel = await loadBabel(root);
    },
    async transform(code, id) {
      const file = id.split("?")[0];
      if (!babel) {
        if (!warned && /\.[jt]sx$/.test(file)) {
          warned = true;
          this.warn?.("Axiom: @babel/core was not found; elements are not source-stamped.");
        }
        return null;
      }
      return stamp(babel, code, file, root);
    },
    resolveId(id) {
      return id === VIRTUAL ? RESOLVED : null;
    },
    load(id) {
      if (id !== RESOLVED) return null;
      const source = adapterSource ?? readFileSync(path.join(here, "adapter.js"), "utf8");
      return `${source}
start({ endpoint: ${JSON.stringify(endpoint ?? "")}, token: ${JSON.stringify(token ?? "")}, hot: import.meta.hot });
`;
    },
    transformIndexHtml() {
      return [
        { tag: "script", attrs: { type: "module", src: "/@id/__x00__" + VIRTUAL }, injectTo: "head-prepend" },
      ];
    },
  };
}
