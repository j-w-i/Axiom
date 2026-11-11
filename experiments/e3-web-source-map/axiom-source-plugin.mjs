// Prototype of the Axiom web source-instrumentation plugin (dev builds only).
// Stamps every JSX element with data-axiom-src="<relative file>:<line>:<column>"
// before any other JSX transform runs, so it is independent of the React
// version (no reliance on fiber._debugSource, removed in React 19) and of the
// JSX compiler in use (Babel, esbuild, SWC, oxc).
import { transformAsync } from "@babel/core";
import path from "node:path";

function babelStamp({ types: t }, { root }) {
  return {
    visitor: {
      JSXOpeningElement(p, state) {
        const loc = p.node.loc;
        if (!loc) return;
        const name = p.node.name;
        if (t.isJSXMemberExpression(name)) return;
        // Fragments have no attributes; skip them.
        if (t.isJSXIdentifier(name) && name.name === "Fragment") return;
        // Only host elements (lowercase) can carry DOM attributes safely.
        if (!t.isJSXIdentifier(name) || !/^[a-z]/.test(name.name)) return;
        if (p.node.attributes.some((a) => t.isJSXAttribute(a) && a.name.name === "data-axiom-src")) return;
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

export default function axiomSource() {
  let root = process.cwd();
  return {
    name: "axiom-source",
    enforce: "pre",
    apply: "serve",
    configResolved(c) { root = c.root; },
    async transform(code, id) {
      const file = id.split("?")[0];
      if (!/\.[jt]sx$/.test(file) || file.includes("/node_modules/")) return null;
      const out = await transformAsync(code, {
        filename: file,
        babelrc: false,
        configFile: false,
        sourceMaps: true,
        plugins: [["@babel/plugin-syntax-jsx"], [babelStamp, { root }]],
        parserOpts: { plugins: file.endsWith(".tsx") ? ["jsx", "typescript"] : ["jsx"] },
      });
      return out ? { code: out.code, map: out.map } : null;
    },
  };
}
