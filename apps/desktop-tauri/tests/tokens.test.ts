// Design-system guards (docs/design/design-system.md §2, frontend-architecture.md §3):
// no literal colours outside tokens.css; both themes define the same semantic tokens;
// the §2.3 contrast pairs; the generated values match §2.2; every referenced token is
// defined; no animated layout properties or filters; durations use tokens.
//
// Each rule is a function over CSS text, run on the real files and on a deliberately
// broken fixture, so a guard that silently stops detecting anything fails too.

import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { contrast, deltaE2000 } from "../scripts/gen-themes.mjs";

const src = join(import.meta.dirname, "../src");

function files(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    return statSync(p).isDirectory() ? files(p) : [p];
  });
}

const tokens = readFileSync(join(src, "styles/tokens.css"), "utf8");
type Theme = "dark" | "light";
const THEMES: Theme[] = ["dark", "light"];

// ---- a small CSS reader: top-level rules (comments removed, @media skipped) -------

interface Rule {
  selectors: string[];
  body: string;
}

function stripComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Top-level rules; at-rules (@media, @keyframes) are returned with their selector as "@…". */
function rules(css: string): Rule[] {
  const text = stripComments(css);
  const out: Rule[] = [];
  let i = 0;
  while (i < text.length) {
    const open = text.indexOf("{", i);
    if (open < 0) break;
    let depth = 1;
    let j = open + 1;
    for (; j < text.length && depth > 0; j++) {
      if (text[j] === "{") depth++;
      else if (text[j] === "}") depth--;
    }
    const prelude = text.slice(i, open).trim();
    out.push({ selectors: prelude.split(",").map((s) => s.trim()), body: text.slice(open + 1, j - 1) });
    i = j;
  }
  return out;
}

function vars(body: string): Map<string, string> {
  const m = new Map<string, string>();
  for (const [, k, v] of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) m.set(k!, v!.trim());
  return m;
}

/** What an element with data-theme=`theme` on <html> sees: every applicable rule in document order. */
function themeVars(theme: Theme, css = tokens): Map<string, string> {
  const m = new Map<string, string>();
  for (const r of rules(css)) {
    const applies = r.selectors.some((s) => s === ":root" || s === "[data-theme]" || s === `[data-theme="${theme}"]`);
    if (applies) for (const [k, v] of vars(r.body)) m.set(k, v);
  }
  return m;
}

function resolve(map: Map<string, string>, name: string): string {
  let v = map.get(name);
  for (let n = 0; n < 10 && v; n++) {
    const ref = /^var\((--[\w-]+)\)$/.exec(v);
    if (!ref) return v;
    v = map.get(ref[1]!);
  }
  if (!v) throw new Error(`${name} is not defined`);
  return v;
}

/** The generated block of one theme, as its CSS body. */
function generated(theme: Theme): string {
  const open = tokens.indexOf(`<generated:${theme}>`);
  const close = tokens.indexOf(`</generated:${theme}>`);
  expect(open, `marker <generated:${theme}>`).toBeGreaterThan(0);
  return tokens.slice(open, close);
}

const PRIMITIVE = /^--(graphite|brass)-\d+$|^--(sage|sage-text|ember|brick|steel)$/;

// ---- rules (pure functions) -----------------------------------------------------

/** Every `var(--x)` in `texts` that `defined` lacks. */
export function undefinedRefs(texts: Record<string, string>, defined: Set<string>): string[] {
  const missing: string[] = [];
  for (const [file, text] of Object.entries(texts)) {
    for (const [, name] of text.matchAll(/var\(\s*(--[\w-]+)/g)) if (!defined.has(name!)) missing.push(`${file}: ${name}`);
  }
  return missing;
}

const LAYOUT = /^(width|height|top|left|right|bottom|inset|min-width|max-width|min-height|max-height|grid-template[\w-]*)$/;

/** Transitions of layout properties, keyframes that change them, and filters. */
export function motionAndFilterViolations(css: string): string[] {
  const bad: string[] = [];
  const text = stripComments(css);
  for (const [, prop, value] of text.matchAll(/(?:^|[;{\s])(transition(?:-property)?)\s*:\s*([^;}]+)/g)) {
    for (const part of value!.split(",")) {
      const first = part.trim().split(/\s+/)[0] ?? "";
      if (LAYOUT.test(first)) bad.push(`${prop}: ${value!.trim()}`);
    }
  }
  for (const [, name, body] of text.matchAll(/@keyframes\s+([\w-]+)\s*\{((?:[^{}]*\{[^{}]*\})*[^{}]*)\}/g)) {
    const props = new Set([...body!.matchAll(/([\w-]+)\s*:/g)].map((m) => m[1]!));
    for (const prop of props) if (LAYOUT.test(prop)) bad.push(`@keyframes ${name} animates ${prop}`);
  }
  for (const [m] of text.matchAll(/backdrop-filter\s*:[^;}]*/g)) bad.push(m);
  for (const [m] of text.matchAll(/(?:^|[;{\s])filter\s*:[^;}]*/g)) bad.push(m.trim());
  return bad;
}

/**
 * Durations in transitions and finite animations that are not a `var(--dur-*)`.
 * Loops (`infinite`) are exempt: their period is set by the looping policy
 * (motion-system.md §5, e.g. the spinner's `.8s steps(8)`), not by the duration scale.
 */
export function literalDurations(css: string): string[] {
  const bad: string[] = [];
  const text = stripComments(css);
  for (const [, prop, value] of text.matchAll(/(?:^|[;{\s])((?:transition|animation)(?:-duration|-delay)?)\s*:\s*([^;}]+)/g)) {
    for (const part of value!.split(",")) {
      if (prop!.startsWith("animation") && /\binfinite\b/.test(part)) continue;
      const literal = part.match(/(?<![\w-])\d*\.?\d+m?s\b/g)?.filter((t) => !/^0m?s$/.test(t) && !/^\.?0+m?s$/.test(t));
      if (literal?.length) bad.push(`${prop}: ${part.trim()}`);
    }
  }
  return bad;
}

const STYLE_FILES = Object.fromEntries(
  files(join(src, "styles"))
    .filter((f) => f.endsWith(".css"))
    .map((f) => [f, readFileSync(f, "utf8")]),
);

// ---- §2.2 and §2.3 ---------------------------------------------------------------

/** design-system.md §2.2, the documented values (hex tokens only). */
const DOCUMENTED: Record<Theme, Record<string, string>> = {
  dark: {
    "--surface-canvas": "#0b0c0d", "--surface-chrome": "#141517", "--surface-panel": "#1e1f22", "--surface-editor": "#17181a",
    "--surface-well": "#111213", "--surface-raised-1": "#303135", "--surface-raised-2": "#28292c", "--surface-overlay": "#26272a",
    "--line": "#0a0a0b", "--line-mid": "#2b2c30", "--line-strong": "#3a3b3f",
    "--text": "#e6e3dc", "--text-muted": "#a3a097", "--text-faint": "#75726b",
    "--accent": "#c9a35b", "--accent-text": "#d9b672", "--on-accent": "#1a1307", "--focus": "#d2ac63",
    "--ok": "#8fbf7f", "--ok-text": "#9fca90", "--warn": "#e8904f", "--danger": "#e5645d",
    "--glyph-rs": "#dd9e80", "--glyph-md": "#a9c1df", "--glyph-toml": "#95c6b6", "--glyph-other": "#a3a097", "--icon-folder": "#8e8a82",
  },
  light: {
    "--surface-canvas": "#c4c1bb", "--surface-chrome": "#e6e4df", "--surface-panel": "#efede9", "--surface-editor": "#fcfbf9",
    "--surface-well": "#ffffff", "--surface-raised-1": "#ffffff", "--surface-raised-2": "#f1efeb", "--surface-overlay": "#fcfbf9",
    "--line": "#c3c0ba", "--line-mid": "#dcd9d3", "--line-strong": "#bdb9b2",
    "--text": "#1e1d1b", "--text-muted": "#5b5852", "--text-faint": "#84807a",
    "--accent": "#b88a3a", "--accent-text": "#85601c", "--on-accent": "#1d1505", "--focus": "#9a722c",
    "--ok": "#4f8f45", "--ok-text": "#3f7a36", "--warn": "#a8520f", "--danger": "#c43c35",
    "--glyph-rs": "#a8502e", "--glyph-md": "#3b6a9e", "--glyph-toml": "#2f7a68", "--glyph-other": "#5b5852", "--icon-folder": "#7a766e",
  },
};

/** Every §2.2 token name, including the ones documented by role rather than value. */
const SECTION_2_2 = [
  ...Object.keys(DOCUMENTED.dark),
  "--edge", "--edge-strong", "--hover", "--selected", "--press",
  "--accent-hi", "--accent-lo", "--accent-soft", "--accent-line", "--run", "--attention",
  ...["text", "comment", "keyword", "type", "string", "number", "punct", "macro"].map((n) => `--syn-${n}`),
  ...["bg", "gutter", "line", "lnum", "lnum-active", "selection", "cursor", "guide", "guide-active"].map((n) => `--editor-${n}`),
  ...["rs", "md", "toml", "other"].map((n) => `--glyph-${n}-bg`),
  "--tree-guide", "--line-input",
];

/**
 * design-system.md §2.3. The input-boundary row checks `--line-input`, not `--line`
 * (the seam, decorative): CR-M1.6-SYSTEM-001-line-contrast, option A, owner 2026-11-17.
 */
const PAIRS: [string, string[], number][] = [
  ["--text", ["--surface-editor", "--surface-panel", "--surface-chrome", "--surface-overlay"], 7],
  ["--text-muted", ["--surface-editor", "--surface-panel", "--surface-chrome"], 4.5],
  ["--text-faint", ["--surface-editor", "--surface-panel"], 3],
  ["--accent-text", ["--surface-editor", "--surface-panel"], 4.5],
  ["--ok-text", ["--surface-editor", "--surface-panel"], 4.5],
  ["--warn", ["--surface-editor", "--surface-panel"], 4.5],
  ["--danger", ["--surface-editor", "--surface-panel"], 4.5],
  ["--on-accent", ["--accent"], 4.5],
  ["--line-input", ["--surface-editor", "--surface-panel", "--surface-chrome", "--surface-well"], 3],
];

function contrastFailures(map: Map<string, string>, pairs: [string, string[], number][]): string[] {
  const out: string[] = [];
  for (const [fg, bgs, min] of pairs) {
    for (const bg of bgs) {
      const r = contrast(resolve(map, fg), resolve(map, bg));
      if (r < min) out.push(`${fg} on ${bg}: ${r.toFixed(2)} < ${min}`);
    }
  }
  return out;
}

// ---- tests ------------------------------------------------------------------------

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
    const semantic = (t: Theme) => [...vars(generated(t)).keys()].filter((k) => !PRIMITIVE.test(k)).sort();
    expect(semantic("light")).toEqual(semantic("dark"));
    expect(semantic("dark").length).toBeGreaterThan(60);
  });

  it("text on backgrounds meets WCAG AA (4.5:1) in both themes", () => {
    for (const theme of THEMES) {
      const t = themeVars(theme);
      for (const [fg, bg] of [["--text", "--bg"], ["--text", "--bg-subtle"], ["--text-muted", "--bg"], ["--text-muted", "--bg-subtle"], ["--danger", "--bg"], ["--text-on-accent", "--accent"]] as const) {
        const r = contrast(resolve(t, fg), resolve(t, bg));
        expect(r, `${theme} ${fg} on ${bg}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it.each(THEMES)("§2.3 contrast pairs (%s)", (theme) => {
    expect(contrastFailures(themeVars(theme), PAIRS)).toEqual([]);
  });

  it("§2.3 contrast pairs detect a failing pair (fixture)", () => {
    const broken = new Map([["--text-muted", "#555555"], ["--surface-editor", "#444444"]]);
    expect(contrastFailures(broken, [["--text-muted", ["--surface-editor"], 4.5]])).toHaveLength(1);
  });

  it.each(THEMES)("focus ring ≥ 3:1 (%s)", (theme) => {
    const t = themeVars(theme);
    expect(contrastFailures(t, [["--focus", ["--surface-editor", "--surface-panel", "--surface-chrome"], 3]])).toEqual([]);
  });

  it.each(THEMES)("§2.2 tokens exist and match the documented values within ΔE2000 < 2 (%s)", (theme) => {
    const t = themeVars(theme);
    for (const name of SECTION_2_2) expect(t.has(name), `${theme} defines ${name}`).toBe(true);
    const far = Object.entries(DOCUMENTED[theme])
      .map(([name, want]) => [name, want, resolve(t, name)] as const)
      .map(([name, want, got]) => [name, want, got, deltaE2000(got, want)] as const)
      .filter(([, , , de]) => de >= 2);
    expect(far).toEqual([]);
  });

  it("every referenced token is defined", () => {
    const defined = new Set<string>();
    for (const r of rules(tokens)) for (const k of vars(r.body).keys()) defined.add(k);
    // @media bodies hold nested rules: read their declarations too.
    for (const k of vars(stripComments(tokens)).keys()) defined.add(k);
    const texts = Object.fromEntries(files(src).filter((f) => /\.(css|tsx?)$/.test(f)).map((f) => [f, readFileSync(f, "utf8")]));
    // Component-local properties are defined where a component sets them inline
    // (e.g. Segmented's `"--seg-n": options.length`), not in tokens.css.
    for (const [f, text] of Object.entries(texts)) {
      if (f.endsWith(".tsx")) for (const [, name] of text.matchAll(/["'](--[\w-]+)["']\s*:/g)) defined.add(name!);
    }
    expect(undefinedRefs(texts, defined)).toEqual([]);
    expect(undefinedRefs({ "fixture.css": ".x { color: var(--no-such-token); }" }, defined)).toEqual(["fixture.css: --no-such-token"]);
  });

  it("no animated layout properties or filters", () => {
    expect(Object.entries(STYLE_FILES).flatMap(([f, css]) => motionAndFilterViolations(css).map((v) => `${f}: ${v}`))).toEqual([]);
    const fixture = `
      .a { transition: width var(--dur-1) var(--ease-out), opacity var(--dur-1); }
      .b { transition-property: height; }
      @keyframes grow { from { top: 0; } to { top: 10px; } }
      .c { backdrop-filter: blur(4px); }
      .d { filter: blur(2px); }
      .ok { transition: transform var(--dur-2), opacity var(--dur-2); }`;
    expect(motionAndFilterViolations(fixture)).toHaveLength(5);
  });

  it("durations use tokens", () => {
    expect(Object.entries(STYLE_FILES).flatMap(([f, css]) => literalDurations(css).map((v) => `${f}: ${v}`))).toEqual([]);
    const fixture = `
      .a { transition: opacity 120ms ease; }
      .b { animation: fade-in .2s var(--ease-out); }
      .c { transition-duration: 1s; }
      .ok1 { transition: opacity var(--dur-1) var(--ease-out), transform 0s; }
      .ok2 { animation: spin .8s steps(8) infinite; }`;
    expect(literalDurations(fixture)).toHaveLength(3);
  });
});
