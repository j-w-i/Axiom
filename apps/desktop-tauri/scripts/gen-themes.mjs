#!/usr/bin/env node
// Generates the colour blocks of src/styles/tokens.css from src/styles/themes.json
// (docs/design/design-system.md §2–§3, M1.6-SYSTEM-001). No dependencies.
//
//   node scripts/gen-themes.mjs           rewrite the generated blocks
//   node scripts/gen-themes.mjs --check   exit 1 if tokens.css is stale
//
// Model (inputs per theme: base hue/chroma, accent hue/chroma/lightness, contrast):
//   - neutral tiers (`--graphite-*`) are fixed OKLCH lightnesses, with chroma as a
//     multiple of the base chroma and hue as an offset from the base hue;
//   - brass steps are offsets from the accent input (lightness, chroma factor, hue);
//   - text tiers are solved: the lightness at which the tier reaches its target
//     contrast (x `contrast`, never below the §2.3 minimum) against the worst
//     surface it appears on;
//   - state, syntax, glyph and editor hues are fixed per theme, and the state
//     hues used as text are lightness-clamped until they reach 4.5:1 on the
//     editor and panel surfaces;
//   - translucent tokens are a source colour with a fixed alpha.
// The calibration tables below reproduce the approved prototype
// (experiments/e5-visual-concepts, variant r) within ΔE2000 < 2;
// tests/tokens.test.ts checks that against design-system.md §2.2.

import { readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

// ---- colour math -------------------------------------------------------------

const toLin = (x) => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4);
const fromLin = (x) => (x <= 0.0031308 ? 12.92 * x : 1.055 * Math.max(0, x) ** (1 / 2.4) - 0.055);
const clamp01 = (x) => Math.min(1, Math.max(0, x));

/** "#rrggbb" → [r, g, b] in 0..1 */
export function hexToRgb(hex) {
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!m) throw new Error(`not a #rrggbb colour: ${hex}`);
  return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16) / 255);
}

export function rgbToHex(rgb) {
  return "#" + rgb.map((v) => Math.round(clamp01(v) * 255).toString(16).padStart(2, "0")).join("");
}

export function rgbToOklch([r, g, b]) {
  const [R, G, B] = [r, g, b].map(toLin);
  const l = Math.cbrt(0.4122214708 * R + 0.5363325363 * G + 0.0514459929 * B);
  const m = Math.cbrt(0.2119034982 * R + 0.6806995451 * G + 0.1073969566 * B);
  const s = Math.cbrt(0.0883024619 * R + 0.2817188376 * G + 0.6299787005 * B);
  const L = 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s;
  const a = 1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s;
  const bb = 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s;
  return [L, Math.hypot(a, bb), ((Math.atan2(bb, a) * 180) / Math.PI + 360) % 360];
}

/** OKLCH → sRGB (0..1), clipped to the gamut. */
export function oklchToRgb(L, C, H) {
  const a = C * Math.cos((H * Math.PI) / 180);
  const b = C * Math.sin((H * Math.PI) / 180);
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ].map((v) => clamp01(fromLin(v)));
}

export function relativeLuminance(hex) {
  const [r, g, b] = hexToRgb(hex).map(toLin);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** WCAG 2 contrast ratio of two opaque colours. */
export function contrast(a, b) {
  const [x, y] = [relativeLuminance(a), relativeLuminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

function rgbToLab(rgb) {
  const [R, G, B] = rgb.map(toLin);
  const X = (0.4124564 * R + 0.3575761 * G + 0.1804375 * B) / 0.95047;
  const Y = 0.2126729 * R + 0.7151522 * G + 0.072175 * B;
  const Z = (0.0193339 * R + 0.119192 * G + 0.9503041 * B) / 1.08883;
  const f = (t) => (t > 216 / 24389 ? Math.cbrt(t) : ((24389 / 27) * t + 16) / 116);
  return [116 * f(Y) - 16, 500 * (f(X) - f(Y)), 200 * (f(Y) - f(Z))];
}

/** CIEDE2000 colour difference of two "#rrggbb" colours. */
export function deltaE2000(hexA, hexB) {
  const [L1, a1, b1] = rgbToLab(hexToRgb(hexA));
  const [L2, a2, b2] = rgbToLab(hexToRgb(hexB));
  const rad = Math.PI / 180;
  const Cb = (Math.hypot(a1, b1) + Math.hypot(a2, b2)) / 2;
  const G = 0.5 * (1 - Math.sqrt(Cb ** 7 / (Cb ** 7 + 25 ** 7)));
  const a1p = (1 + G) * a1;
  const a2p = (1 + G) * a2;
  const C1p = Math.hypot(a1p, b1);
  const C2p = Math.hypot(a2p, b2);
  const hue = (a, b) => (a === 0 && b === 0 ? 0 : (Math.atan2(b, a) / rad + 360) % 360);
  const h1p = hue(a1p, b1);
  const h2p = hue(a2p, b2);
  let dhp = 0;
  if (C1p * C2p !== 0) {
    dhp = h2p - h1p;
    if (dhp > 180) dhp -= 360;
    else if (dhp < -180) dhp += 360;
  }
  const dLp = L2 - L1;
  const dCp = C2p - C1p;
  const dHp = 2 * Math.sqrt(C1p * C2p) * Math.sin((dhp / 2) * rad);
  const Lbp = (L1 + L2) / 2;
  const Cbp = (C1p + C2p) / 2;
  let hbp = h1p + h2p;
  if (C1p * C2p !== 0) {
    if (Math.abs(h1p - h2p) > 180) hbp += hbp < 360 ? 360 : -360;
    hbp /= 2;
  }
  const T = 1 - 0.17 * Math.cos((hbp - 30) * rad) + 0.24 * Math.cos(2 * hbp * rad) + 0.32 * Math.cos((3 * hbp + 6) * rad) - 0.2 * Math.cos((4 * hbp - 63) * rad);
  const dTh = 30 * Math.exp(-(((hbp - 275) / 25) ** 2));
  const Rc = 2 * Math.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7));
  const Sl = 1 + (0.015 * (Lbp - 50) ** 2) / Math.sqrt(20 + (Lbp - 50) ** 2);
  const Sc = 1 + 0.045 * Cbp;
  const Sh = 1 + 0.015 * Cbp * T;
  const Rt = -Math.sin(2 * dTh * rad) * Rc;
  return Math.sqrt((dLp / Sl) ** 2 + (dCp / Sc) ** 2 + (dHp / Sh) ** 2 + Rt * (dCp / Sc) * (dHp / Sh));
}

// ---- calibration ---------------------------------------------------------------

// Neutral tiers: [OKLCH lightness, chroma / base.chroma, hue - base.hue].
// Steps are roles shared by both themes ("dark use → light use", §2.1).
// (The approved dark neutrals are slightly cool — OKLCH hue 250-285 — while the text
// tiers are warm; the hue offsets keep both.)
const NEUTRAL = {
  dark: {
    0: [0.1452, 0.3553, -143.9], // seam / control outline
    50: [0.1536, 0.4525, 178], // canvas
    100: [0.1815, 0.434, 178], // well
    150: [0.1955, 0.7178, -165.5], // chrome
    175: [0.2088, 0.7058, -165.5], // editor
    200: [0.2395, 0.9831, -158.8], // panel
    250: [0.2729, 0.9506, -158.8], // overlay
    275: [0.2811, 0.9434, -158.8], // raised, bottom
    300: [0.2937, 1.2263, -155.3], // hairline
    350: [0.3137, 1.2057, -155.3], // raised, top
    400: [0.3528, 1.1698, -155.2], // strong line
  },
  light: {
    0: [0.8085, 1.1221, 9.6],
    50: [0.8116, 1.1211, 9.6],
    100: [1, 0, 0],
    150: [0.919, 0.8827, 13.6],
    175: [0.9883, 0.3567, 9.6],
    200: [0.9465, 0.7204, 9.6],
    250: [0.9883, 0.3567, 9.6],
    275: [0.9526, 0.7192, 9.6],
    300: [0.886, 1.0972, 9.6],
    350: [1, 0, 0],
    400: [0.787, 1.3458, 6.8],
  },
};

// Text tiers (steps 600/700/900) and the input boundary (500): solved. [target contrast against the worst of
// `on`, §2.3 minimum, chroma / base.chroma, hue - base.hue].
const TEXT = {
  dark: {
    900: { token: "--text", ratio: 11.65, min: 7, on: ["175", "200", "150", "250"], c: 1.659, dh: 17.5 },
    700: { token: "--text-muted", ratio: 6.3, min: 4.5, on: ["175", "200", "150"], c: 2.2193, dh: 21.6 },
    600: { token: "--text-faint", ratio: 3.43, min: 3, on: ["175", "200"], c: 1.8757, dh: 17.5 },
    // Input boundary (CR-M1.6-SYSTEM-001-line-contrast, option A): WCAG 1.4.11 3:1
    // against every surface a field sits on; --line stays the seam.
    500: { token: "--line-input", ratio: 3, min: 3, on: ["175", "200", "150", "100"], c: 1.8757, dh: 17.5 },
  },
  light: {
    900: { token: "--text", ratio: 13.26, min: 7, on: ["175", "200", "150", "250"], c: 0.5104, dh: 9.6 },
    700: { token: "--text-muted", ratio: 5.58, min: 4.5, on: ["175", "200", "150"], c: 1.2859, dh: 9.6 },
    600: { token: "--text-faint", ratio: 3.36, min: 3, on: ["175", "200"], c: 1.2724, dh: 3.3 },
    500: { token: "--line-input", ratio: 3, min: 3, on: ["175", "200", "150", "100"], c: 1.2724, dh: 3.3 },
  },
};

// Brass: [lightness - accent.lightness, chroma / accent.chroma, hue - accent.hue].
const BRASS = {
  dark: { 300: [0.0615, 1.017, 4.2], 400: [0.0521, 0.9545, 3.1], 500: [-0.0051, 1.0115, 2.1], 600: [-0.0772, 1.1136, -1.6], 700: [-0.1611, 0.9963, -1.7], 800: [-0.2247, 0.9483, -1.6] },
  light: { 500: [0.1147, 1.0289, 0.8], 600: [0.0628, 1.0123, 0.4], 700: [-0.0211, 0.9057, 0.3], 800: [-0.0847, 0.8621, 0.4] },
};
// Accent shades that are not brass steps.
const ACCENT_EXTRA = {
  dark: { "--accent-lo": [-0.1042, 0.9801, 2.6], "--focus": [0.0231, 1.0191, 2.5] },
  light: {},
};

// Fixed hues per theme (state, syntax, glyphs, editor chrome). `clamp` = used as
// text: lightness is moved until it reaches 4.5:1 on the editor and panel surfaces.
const FIXED = {
  dark: {
    sage: "#8fbf7f", "sage-text": ["#9fca90", "clamp"], ember: ["#e8904f", "clamp"], brick: ["#e5645d", "clamp"], steel: "#9fb8d8",
    "on-accent": "#1a1307", "icon-folder": "#8e8a82", "editor-lnum": "#4f4d49",
    "syn-text": "#dcd8d0", "syn-comment": "#77736c", "syn-keyword": "#d18e6e", "syn-type": "#8ec2b2", "syn-string": "#a9c47f",
    "syn-number": "#c4a7de", "syn-punct": "#9d998f", "syn-macro": "#9fb8d8",
    "glyph-rs": "#dd9e80", "glyph-md": "#a9c1df", "glyph-toml": "#95c6b6", "glyph-other": "#a3a097",
  },
  light: {
    sage: "#4f8f45", "sage-text": ["#3f7a36", "clamp"], ember: ["#a8520f", "clamp"], brick: ["#c43c35", "clamp"], steel: "#3b6a9e",
    "on-accent": "#1d1505", "icon-folder": "#7a766e", "editor-lnum": "#b5b1a9",
    "syn-text": "#2b2925", "syn-comment": "#8d887f", "syn-keyword": "#a8502e", "syn-type": "#2f7a68", "syn-string": "#557a25",
    "syn-number": "#7a4ea8", "syn-punct": "#6d6961", "syn-macro": "#3b6a9e",
    "glyph-rs": "#a8502e", "glyph-md": "#3b6a9e", "glyph-toml": "#2f7a68", "glyph-other": "#5b5852",
  },
};

// Translucent tokens: [source, alpha]. Sources: white, black, ink (warm black of
// the light theme), or a generated token.
const INK = [40, 30, 10];
const ALPHA = {
  dark: {
    "--edge": ["white", 0.055], "--edge-strong": ["white", 0.085], "--hover": ["white", 0.045], "--selected": ["white", 0.075], "--press": ["black", 0.28],
    "--accent-soft": ["--accent", 0.13], "--accent-line": ["--accent", 0.55], "--danger-soft": ["--danger", 0.12],
    "--scrim": [[6, 6, 7], 0.5], "--scroll-thumb": ["white", 0.11], "--scroll-thumb-hover": ["white", 0.2], "--tree-guide": ["white", 0.05],
    "--editor-line": ["white", 0.028], "--editor-selection": ["--accent", 0.2], "--editor-guide": ["white", 0.045], "--editor-guide-active": ["white", 0.14],
    "--glyph-rs-bg": ["--syn-keyword", 0.17], "--glyph-md-bg": ["--steel", 0.16], "--glyph-toml-bg": ["--syn-type", 0.15], "--glyph-other-bg": ["white", 0.07],
  },
  light: {
    "--edge": ["white", 0.9], "--edge-strong": ["white", 1], "--hover": ["ink", 0.05], "--selected": ["ink", 0.08], "--press": ["ink", 0.1],
    "--accent-soft": ["--accent", 0.14], "--accent-line": [[150, 110, 40], 0.55], "--danger-soft": ["--danger", 0.1],
    "--scrim": [[60, 55, 45], 0.18], "--scroll-thumb": ["black", 0.14], "--scroll-thumb-hover": ["black", 0.26], "--tree-guide": ["black", 0.07],
    "--editor-line": [[60, 45, 20], 0.035], "--editor-selection": ["--accent", 0.22], "--editor-guide": ["black", 0.06], "--editor-guide-active": ["black", 0.18],
    "--glyph-rs-bg": ["--syn-keyword", 0.12], "--glyph-md-bg": ["--steel", 0.12], "--glyph-toml-bg": ["--syn-type", 0.12], "--glyph-other-bg": ["black", 0.06],
  },
};

// Semantic tokens that are a primitive (dark, light). Others are listed in build().
const ROLES = [
  ["--surface-canvas", "graphite-50"], ["--surface-chrome", "graphite-150"], ["--surface-panel", "graphite-200"],
  ["--surface-editor", "graphite-175"], ["--surface-well", "graphite-100"], ["--surface-raised-1", "graphite-350"],
  ["--surface-raised-2", "graphite-275"], ["--surface-overlay", "graphite-250"],
  ["--line", "graphite-0"], ["--line-mid", "graphite-300"], ["--line-strong", "graphite-400"], ["--line-input", "graphite-500"],
  ["--text", "graphite-900"], ["--text-muted", "graphite-700"], ["--text-faint", "graphite-600"],
];
const ACCENT_ROLES = {
  dark: { "--accent": "brass-500", "--accent-hi": "brass-300", "--accent-text": "brass-400", "--editor-cursor": "brass-300" },
  light: { "--accent": "brass-600", "--accent-hi": "brass-500", "--accent-lo": "brass-700", "--accent-text": "brass-800", "--focus": "brass-700", "--editor-cursor": "brass-700" },
};

// ---- generation ----------------------------------------------------------------

const hexOf = (L, C, H) => rgbToHex(oklchToRgb(L, C, ((H % 360) + 360) % 360));

/** Moves the lightness of `hex` (keeping chroma and hue) until it reaches `min` against every surface. */
function clampContrast(hex, surfaces, min, lighter) {
  const [L0, C, H] = rgbToOklch(hexToRgb(hex));
  let out = hex;
  for (let L = L0; L >= 0 && L <= 1; L += lighter ? 0.001 : -0.001) {
    out = L === L0 ? hex : hexOf(L, C, H);
    if (surfaces.every((s) => contrast(out, s) >= min)) return out;
  }
  throw new Error(`cannot reach ${min}:1 for ${hex}`);
}

/** Lightness at which a tier of the given chroma/hue reaches `ratio` against the worst surface. */
function solveText(ratio, surfaces, C, H, lighter) {
  let lo = 0;
  let hi = 1;
  const worst = (hex) => Math.min(...surfaces.map((s) => contrast(hex, s)));
  for (let i = 0; i < 40; i++) {
    const mid = (lo + hi) / 2;
    const ok = worst(hexOf(mid, C, H)) >= ratio;
    // dark theme: text is lighter than its surfaces, so contrast grows with L
    if (lighter ? ok : !ok) hi = mid;
    else lo = mid;
  }
  return hexOf(lighter ? hi : lo, C, H);
}

function rgba(rgb, a) {
  return a >= 1 ? rgbToHex(rgb.map((v) => v / 255)) : `rgb(${rgb.join(" ")} / ${a})`;
}

/** Builds one theme: { primitives: [[name, value]], semantic: [[name, value]] }. */
export function build(theme, input) {
  const dark = theme === "dark";
  const { hue: bh, chroma: bc } = input.base;
  const { hue: ah, chroma: ac, lightness: al } = input.accent;
  const k = input.contrast ?? 1;
  const prim = new Map();
  for (const [step, [L, c, dh]] of Object.entries(NEUTRAL[theme])) prim.set(`graphite-${step}`, hexOf(L, c * bc, bh + dh));
  for (const [step, t] of Object.entries(TEXT[theme])) {
    const surfaces = t.on.map((s) => prim.get(`graphite-${s}`));
    prim.set(`graphite-${step}`, solveText(Math.max(t.min, t.ratio * k), surfaces, t.c * bc, bh + t.dh, dark));
  }
  for (const [step, [dL, c, dh]] of Object.entries(BRASS[theme])) prim.set(`brass-${step}`, hexOf(al + dL, c * ac, ah + dh));
  const textSurfaces = [prim.get("graphite-175"), prim.get("graphite-200")];
  for (const [name, v] of Object.entries(FIXED[theme])) {
    if (!["sage", "sage-text", "ember", "brick", "steel"].includes(name)) continue;
    const [hex, mode] = Array.isArray(v) ? v : [v];
    prim.set(name, mode === "clamp" ? clampContrast(hex, textSurfaces, 4.5, dark) : hex);
  }

  const sem = new Map();
  const ref = (p) => `var(--${p})`;
  sem.set("color-scheme", theme);
  for (const [token, p] of ROLES) sem.set(token, ref(p));
  for (const [token, p] of Object.entries(ACCENT_ROLES[theme])) sem.set(token, ref(p));
  for (const [token, [dL, c, dh]] of Object.entries(ACCENT_EXTRA[theme])) sem.set(token, hexOf(al + dL, c * ac, ah + dh));
  sem.set("--on-accent", FIXED[theme]["on-accent"]);
  sem.set("--ok", ref("sage"));
  sem.set("--ok-text", ref("sage-text"));
  sem.set("--run", "var(--accent)");
  sem.set("--warn", ref("ember"));
  sem.set("--danger", ref("brick"));
  sem.set("--attention", "var(--warn)");
  sem.set("--icon-folder", FIXED[theme]["icon-folder"]);
  sem.set("--editor-bg", "var(--surface-editor)");
  sem.set("--editor-gutter", "var(--surface-editor)");
  sem.set("--editor-lnum", FIXED[theme]["editor-lnum"]);
  sem.set("--editor-lnum-active", "var(--text-muted)");
  for (const n of ["text", "comment", "keyword", "type", "string", "number", "punct", "macro"]) sem.set(`--syn-${n}`, FIXED[theme][`syn-${n}`]);
  for (const n of ["rs", "md", "toml", "other"]) sem.set(`--glyph-${n}`, FIXED[theme][`glyph-${n}`]);

  // Resolve a token or primitive to #rrggbb (for translucent sources).
  const resolve = (name) => {
    let v = sem.get(name) ?? prim.get(name.replace(/^--/, ""));
    for (let i = 0; i < 6 && v && v.startsWith("var("); i++) {
      const inner = v.slice(4, -1);
      v = sem.get(inner) ?? prim.get(inner.replace(/^--/, ""));
    }
    if (!v || !v.startsWith("#")) throw new Error(`${theme}: cannot resolve ${name}`);
    return v;
  };
  for (const [token, [src, a]] of Object.entries(ALPHA[theme])) {
    const rgb = Array.isArray(src)
      ? src
      : src === "white" ? [255, 255, 255]
      : src === "black" ? [0, 0, 0]
      : src === "ink" ? INK
      : hexToRgb(resolve(src)).map((v) => Math.round(v * 255));
    sem.set(token, rgba(rgb, a));
  }
  return { primitives: [...prim.entries()].map(([n, v]) => [`--${n}`, v]), semantic: [...sem.entries()] };
}

/** The CSS text of one generated block (between its markers, markers excluded). */
export function renderBlock(theme, { primitives, semantic }) {
  const selector = theme === "dark" ? ':root,\n[data-theme="dark"]' : '[data-theme="light"]';
  const line = ([n, v]) => `  ${n}: ${v};`;
  return [
    `${selector} {`,
    "  /* primitives: only tokens.css may use these */",
    ...primitives.map(line),
    "  /* semantic */",
    ...semantic.map(line),
    "}",
  ].join("\n");
}

const MARK = (theme) => [`/* <generated:${theme}> written by scripts/gen-themes.mjs from themes.json: do not edit */`, `/* </generated:${theme}> */`];

/** Returns `css` with both generated blocks replaced. */
export function render(css, themes) {
  let out = css;
  for (const theme of ["dark", "light"]) {
    const [open, close] = MARK(theme);
    const i = out.indexOf(open);
    const j = out.indexOf(close);
    if (i < 0 || j < i) throw new Error(`tokens.css: missing ${open} … ${close}`);
    out = out.slice(0, i + open.length) + "\n" + renderBlock(theme, build(theme, themes[theme])) + "\n" + out.slice(j);
  }
  return out;
}

function main(argv) {
  const here = import.meta.dirname;
  const tokensPath = join(here, "../src/styles/tokens.css");
  const themes = JSON.parse(readFileSync(join(here, "../src/styles/themes.json"), "utf8"));
  const css = readFileSync(tokensPath, "utf8");
  const next = render(css, themes);
  if (argv.includes("--check")) {
    if (next !== css) {
      console.error("tokens.css is stale: run node apps/desktop-tauri/scripts/gen-themes.mjs");
      return 1;
    }
    console.log("tokens.css is current");
    return 0;
  }
  if (next !== css) writeFileSync(tokensPath, next);
  console.log(next === css ? "tokens.css unchanged" : "tokens.css written");
  return 0;
}

if (process.argv[1] && basename(process.argv[1]) === "gen-themes.mjs") process.exit(main(process.argv.slice(2)));
