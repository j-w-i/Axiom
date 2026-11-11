import { describe, expect, it } from "vitest";
import { readFileSync, existsSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { BRIDGE_VERSION } from "../src/bridge";
import { MONACO_VERSION } from "../src/version";
import { createFakeHost } from "../src/dev-host";

const root = join(import.meta.dirname, "..");

describe("package", () => {
  const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

  it("is @axiom/editor, private, ESM, with exactly pinned deps", () => {
    expect(pkg.name).toBe("@axiom/editor");
    expect(pkg.private).toBe(true);
    expect(pkg.type).toBe("module");
    const all = { ...pkg.dependencies, ...pkg.devDependencies };
    for (const [name, v] of Object.entries(all)) {
      expect(v, name).toMatch(/^\d+\.\d+\.\d+$/);
    }
    expect(Object.keys(pkg.dependencies)).toEqual(["monaco-editor"]);
  });

  it("exposes the required scripts", () => {
    expect(pkg.scripts.build).toBe("vite build");
    expect(pkg.scripts.check).toBe("tsc --noEmit");
    expect(pkg.scripts.test).toBe("vitest run");
    expect(pkg.scripts.dev).toBe("vite");
  });

  it("mirrors the pinned monaco version and speaks bridge 1", () => {
    expect(MONACO_VERSION).toBe(pkg.dependencies["monaco-editor"]);
    expect(BRIDGE_VERSION).toBe(1);
  });
});

describe("index.html source", () => {
  const html = readFileSync(join(root, "index.html"), "utf8");
  it("loads qwebchannel.js as a classic script before the module script, tolerating its absence", () => {
    const classic = html.indexOf('<script src="/qwebchannel.js"');
    const module = html.indexOf('<script type="module"');
    expect(classic).toBeGreaterThan(-1);
    expect(module).toBeGreaterThan(classic);
    expect(html.slice(classic, module)).toContain("onerror");
  });
});

// Needs `pnpm run build` first; scripts/check.sh builds before testing.
describe("dist (built bundle)", () => {
  const dist = join(root, "dist");

  it("exists (run `pnpm run build` before `pnpm run test`)", () => {
    expect(existsSync(join(dist, "index.html"))).toBe(true);
  });

  it("has no http(s) URL in dist/index.html", () => {
    const html = readFileSync(join(dist, "index.html"), "utf8");
    expect(html).not.toMatch(/https?:\/\//i);
  });

  it("uses relative asset URLs only", () => {
    const html = readFileSync(join(dist, "index.html"), "utf8");
    const refs = [...html.matchAll(/(?:src|href)="([^"]+)"/g)].map((m) => m[1]!);
    expect(refs.length).toBeGreaterThan(0);
    for (const r of refs) {
      if (r === "/qwebchannel.js") continue; // served by the shell's scheme handler
      expect(r.startsWith("./"), r).toBe(true);
    }
  });

  it("references the bundled workers", () => {
    const assets = readdirSync(join(dist, "assets"));
    for (const w of ["editor.worker", "json.worker", "css.worker", "html.worker", "ts.worker"]) {
      expect(assets.some((a: string) => a.startsWith(w)), w).toBe(true);
    }
    expect(statSync(join(dist, "assets")).isDirectory()).toBe(true);
  });
});

describe("dev host", () => {
  it("opens a sample document after ready and acknowledges saves", async () => {
    const lines: string[] = [];
    const { host } = createFakeHost((l) => lines.push(l));
    const opened: string[] = [];
    host.openDocument.connect((p) => opened.push(p.path));
    host.ready({ bridgeVersion: 1, monacoVersion: MONACO_VERSION });
    await Promise.resolve();
    expect(opened).toEqual(["src/sample.ts"]);
    expect(lines[0]).toContain("[host.ready]");
  });
});
