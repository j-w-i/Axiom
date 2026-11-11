// Design-system guards (design-system.md §Rules), the web equivalent of
// scripts/check_qml.py: no literal colours outside tokens.css; every theme
// defines every semantic token; text/background pairs meet WCAG AA.

import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const src = join(import.meta.dirname, "../src");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

const tokens = readFileSync(join(src, "styles/tokens.css"), "utf8");

function block(selector: string): string {
  const i = tokens.indexOf(selector);
  const open = tokens.indexOf("{", i);
  return tokens.slice(open + 1, tokens.indexOf("}", open));
}

function vars(css: string): Map<string, string> {
  const m = new Map<string, string>();
  for (const [, k, v] of css.matchAll(/(--[\w-]+):\s*([^;]+);/g)) m.set(k!, v!.trim());
  return m;
}

describe("design tokens", () => {
  it("no literal colours outside tokens.css", () => {
    const offenders: string[] = [];
    for (const f of files(src)) {
      if (f.endsWith("tokens.css") || f.endsWith("icons.tsx")) continue;
      if (!/\.(css|tsx?)$/.test(f)) continue;
      const text = readFileSync(f, "utf8");
      for (const [i, line] of text.split("\n").entries()) {
        // Monaco theme keys need #rrggbb strings built at runtime from tokens; "#00000000" is "transparent".
        if (/#[0-9a-fA-F]{6}\b|#[0-9a-fA-F]{3}\b|rgba?\(/.test(line) && !line.includes('"#00000000"') && !line.includes('"#000000"')) {
          offenders.push(`${f}:${i + 1}: ${line.trim()}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("light and dark define the same semantic tokens", () => {
    const dark = [...vars(block(':root,\n[data-theme="dark"]')).keys()].sort();
    const light = [...vars(block('[data-theme="light"]')).keys()].sort();
    expect(light).toEqual(dark);
    expect(dark.length).toBeGreaterThan(20);
  });

  it("text on backgrounds meets WCAG AA (4.5:1) in both themes", () => {
    const prim = vars(block(":root {"));
    const resolve = (theme: Map<string, string>, v: string): string => {
      let cur = v;
      for (let n = 0; n < 5; n++) {
        const m = /^var\((--[\w-]+)\)$/.exec(cur);
        if (!m) break;
        cur = theme.get(m[1]!) ?? prim.get(m[1]!) ?? cur;
      }
      return cur;
    };
    const lum = (hex: string) => {
      const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((x) => (x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4));
      return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
    };
    const ratio = (a: string, b: string) => {
      const [l1, l2] = [lum(a), lum(b)].sort((x, y) => y - x);
      return (l1! + 0.05) / (l2! + 0.05);
    };
    for (const sel of [':root,\n[data-theme="dark"]', '[data-theme="light"]']) {
      const t = vars(block(sel));
      for (const [fg, bg] of [["--text", "--bg"], ["--text", "--bg-subtle"], ["--text-muted", "--bg"], ["--text-muted", "--bg-subtle"], ["--danger", "--bg"], ["--text-on-accent", "--accent"]] as const) {
        const r = ratio(resolve(t, t.get(fg)!), resolve(t, t.get(bg)!));
        expect(r, `${sel} ${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });
});
