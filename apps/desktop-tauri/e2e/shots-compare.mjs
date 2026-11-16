#!/usr/bin/env node
// Compares two sets of screenshots (M1.6-QA-001): per-pixel, no dependencies
// (PNG through node:zlib), with an anti-aliasing tolerance of one channel step.
//
//   node e2e/shots-compare.mjs <dirA> <dirB> [--threshold 0.001] [--out <dir>]
//
// A = before, B = after. Every `*.png` in either directory is a state. The exit
// code is 1 when any state changed by more than the threshold (a fraction of
// pixels, default 0.001 = 0.1%), differs in size, or exists in only one set.
// `<out>/diff/<state>.png` marks changed pixels in red over a faded copy of A;
// `<out>/report.html` shows before / after / diff side by side. The default
// <out> is e2e/shots/compare (git-ignored).

import { deflateSync, inflateSync } from "node:zlib";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";

// ---- PNG ---------------------------------------------------------------------

const SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const head = Buffer.alloc(8);
  head.writeUInt32BE(data.length, 0);
  head.write(type, 4, "latin1");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([head.subarray(4), data])), 0);
  return Buffer.concat([head, data, crc]);
}

/** Encodes RGBA pixels (width*height*4 bytes) as an 8-bit RGBA PNG. */
export function encodePng(width, height, rgba) {
  if (rgba.length !== width * height * 4) throw new Error("encodePng: the pixel buffer does not match width x height");
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height); // filter 0 on every row
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0;
    Buffer.from(rgba.buffer, rgba.byteOffset + y * stride, stride).copy(raw, y * (stride + 1) + 1);
  }
  return Buffer.concat([SIGNATURE, chunk("IHDR", ihdr), chunk("IDAT", deflateSync(raw)), chunk("IEND", Buffer.alloc(0))]);
}

const paeth = (a, b, c) => {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
};

/**
 * Decodes a non-interlaced 8-bit PNG (grey, grey+alpha, RGB, RGBA or palette,
 * which is what WebDriver screenshots and this file's encoder produce) to
 * `{ width, height, data }` with `data` as RGBA bytes.
 */
export function decodePng(buf) {
  if (buf.length < 8 || !buf.subarray(0, 8).equals(SIGNATURE)) throw new Error("not a PNG file");
  let pos = 8;
  let ihdr = null;
  let palette = null;
  let trns = null;
  const idat = [];
  while (pos + 8 <= buf.length) {
    const len = buf.readUInt32BE(pos);
    const type = buf.toString("latin1", pos + 4, pos + 8);
    const data = buf.subarray(pos + 8, pos + 8 + len);
    pos += 12 + len;
    if (type === "IHDR") ihdr = data;
    else if (type === "PLTE") palette = data;
    else if (type === "tRNS") trns = data;
    else if (type === "IDAT") idat.push(data);
    else if (type === "IEND") break;
  }
  if (!ihdr) throw new Error("PNG without IHDR");
  const width = ihdr.readUInt32BE(0);
  const height = ihdr.readUInt32BE(4);
  const depth = ihdr[8];
  const colour = ihdr[9];
  if (depth !== 8) throw new Error(`unsupported PNG bit depth ${depth}`);
  if (ihdr[12] !== 0) throw new Error("interlaced PNGs are not supported");
  const channels = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[colour];
  if (!channels) throw new Error(`unsupported PNG colour type ${colour}`);
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  if (raw.length < (stride + 1) * height) throw new Error("PNG pixel data is truncated");
  const cur = Buffer.alloc(stride);
  let prev = Buffer.alloc(stride);
  const out = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? cur[x - channels] : 0;
      const b = prev[x];
      const c = x >= channels ? prev[x - channels] : 0;
      const v = line[x];
      cur[x] =
        filter === 0 ? v
        : filter === 1 ? v + a
        : filter === 2 ? v + b
        : filter === 3 ? v + ((a + b) >> 1)
        : filter === 4 ? v + paeth(a, b, c)
        : (() => { throw new Error(`bad PNG filter ${filter}`); })();
    }
    for (let x = 0; x < width; x++) {
      const o = (y * width + x) * 4;
      const i = x * channels;
      if (colour === 6) { out[o] = cur[i]; out[o + 1] = cur[i + 1]; out[o + 2] = cur[i + 2]; out[o + 3] = cur[i + 3]; }
      else if (colour === 2) { out[o] = cur[i]; out[o + 1] = cur[i + 1]; out[o + 2] = cur[i + 2]; out[o + 3] = 255; }
      else if (colour === 0) { out[o] = out[o + 1] = out[o + 2] = cur[i]; out[o + 3] = 255; }
      else if (colour === 4) { out[o] = out[o + 1] = out[o + 2] = cur[i]; out[o + 3] = cur[i + 1]; }
      else {
        const k = cur[i];
        if (!palette) throw new Error("palette PNG without PLTE");
        out[o] = palette[k * 3]; out[o + 1] = palette[k * 3 + 1]; out[o + 2] = palette[k * 3 + 2];
        out[o + 3] = trns && k < trns.length ? trns[k] : 255;
      }
    }
    cur.copy(prev);
    prev = Buffer.from(cur);
  }
  return { width, height, data: out };
}

// ---- diff --------------------------------------------------------------------

/**
 * Compares two decoded images. A pixel counts as changed when any channel
 * differs by more than `tolerance` (default 1: anti-aliasing and colour
 * rounding move a channel by one step). Differently sized images are fully
 * changed. Returns `{ changed, total, ratio, sameSize, diff }`; `diff` is RGBA
 * of A faded to grey with changed pixels in red (size of A; null if sizes differ).
 */
export function diffImages(a, b, tolerance = 1) {
  const total = Math.max(a.width * a.height, b.width * b.height);
  if (a.width !== b.width || a.height !== b.height) {
    return { changed: total, total, ratio: 1, sameSize: false, diff: null };
  }
  const diff = new Uint8Array(a.data.length);
  let changed = 0;
  for (let i = 0; i < a.data.length; i += 4) {
    const d = Math.max(
      Math.abs(a.data[i] - b.data[i]),
      Math.abs(a.data[i + 1] - b.data[i + 1]),
      Math.abs(a.data[i + 2] - b.data[i + 2]),
      Math.abs(a.data[i + 3] - b.data[i + 3]),
    );
    if (d > tolerance) {
      changed++;
      diff[i] = 255; diff[i + 1] = 0; diff[i + 2] = 0; diff[i + 3] = 255;
    } else {
      const g = Math.round(0.299 * a.data[i] + 0.587 * a.data[i + 1] + 0.114 * a.data[i + 2]);
      const faded = 255 - Math.round((255 - g) * 0.35); // pale grey ghost of A
      diff[i] = diff[i + 1] = diff[i + 2] = faded;
      diff[i + 3] = 255;
    }
  }
  return { changed, total, ratio: total === 0 ? 0 : changed / total, sameSize: true, diff };
}

/** `*.png` names (without extension) directly inside `dir`. */
function statesIn(dir) {
  return readdirSync(dir).filter((f) => f.endsWith(".png")).map((f) => f.slice(0, -4));
}

/**
 * Compares every state of two directories. Returns one entry per state:
 * `{ name, status: "same" | "changed" | "missing-before" | "missing-after" | "size", ratio, changed, total, diff }`.
 */
export function compareDirs(dirA, dirB, { threshold = 0.001, tolerance = 1 } = {}) {
  const names = [...new Set([...statesIn(dirA), ...statesIn(dirB)])].sort();
  return names.map((name) => {
    const fa = join(dirA, `${name}.png`);
    const fb = join(dirB, `${name}.png`);
    if (!existsSync(fa)) return { name, status: "missing-before", ratio: 1, changed: 0, total: 0, diff: null };
    if (!existsSync(fb)) return { name, status: "missing-after", ratio: 1, changed: 0, total: 0, diff: null };
    const a = decodePng(readFileSync(fa));
    const b = decodePng(readFileSync(fb));
    const r = diffImages(a, b, tolerance);
    const status = !r.sameSize ? "size" : r.ratio > threshold ? "changed" : "same";
    return { name, status, ratio: r.ratio, changed: r.changed, total: r.total, diff: r.diff && { width: a.width, height: a.height, data: r.diff } };
  });
}

const escapeHtml = (s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

function reportHtml(results, dirA, dirB, threshold) {
  const bad = results.filter((r) => r.status !== "same");
  const rows = results
    .map((r) => {
      const pct = `${(r.ratio * 100).toFixed(3)}%`;
      const body =
        r.status === "same"
          ? ""
          : `<div class="imgs">${["before", "diff", "after"]
              .map((k) => `<figure><figcaption>${k}</figcaption><img loading="lazy" src="${k}/${escapeHtml(r.name)}.png" alt="${k} ${escapeHtml(r.name)}"></figure>`)
              .join("")}</div>`;
      return `<section class="${r.status === "same" ? "ok" : "bad"}"><h2>${escapeHtml(r.name)} <span>${r.status} · ${pct} (${r.changed} px)</span></h2>${body}</section>`;
    })
    .join("\n");
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><title>Screenshot comparison</title>
<style>
body{font:14px system-ui,sans-serif;margin:24px;background:#f4f4f5;color:#18181b}
h1{font-size:18px} h2{font-size:14px;margin:0 0 8px} h2 span{font-weight:400;color:#52525b}
section{background:#fff;border:1px solid #d4d4d8;border-radius:6px;padding:12px;margin:12px 0}
section.bad{border-color:#dc2626} section.ok h2{margin:0}
.imgs{display:grid;grid-template-columns:repeat(3,1fr);gap:8px}
figure{margin:0} figcaption{font-size:12px;color:#52525b;margin-bottom:4px}
img{width:100%;height:auto;border:1px solid #d4d4d8;image-rendering:pixelated}
</style></head><body>
<h1>Screenshot comparison: ${bad.length} of ${results.length} states differ (threshold ${(threshold * 100).toFixed(3)}%)</h1>
<p>before: <code>${escapeHtml(dirA)}</code><br>after: <code>${escapeHtml(dirB)}</code></p>
${rows}
</body></html>
`;
}

/** Writes diff images, copies of changed before/after images, and report.html. */
export function writeReport(results, dirA, dirB, outDir, threshold) {
  rmSync(outDir, { recursive: true, force: true });
  for (const sub of ["diff", "before", "after"]) mkdirSync(join(outDir, sub), { recursive: true });
  for (const r of results) {
    if (r.status === "same") continue;
    if (r.diff) writeFileSync(join(outDir, "diff", `${r.name}.png`), encodePng(r.diff.width, r.diff.height, r.diff.data));
    if (existsSync(join(dirA, `${r.name}.png`))) copyFileSync(join(dirA, `${r.name}.png`), join(outDir, "before", `${r.name}.png`));
    if (existsSync(join(dirB, `${r.name}.png`))) copyFileSync(join(dirB, `${r.name}.png`), join(outDir, "after", `${r.name}.png`));
  }
  writeFileSync(join(outDir, "report.html"), reportHtml(results, dirA, dirB, threshold));
}

// ---- CLI ---------------------------------------------------------------------

function main(argv) {
  const dirs = [];
  let threshold = 0.001;
  let out = join(import.meta.dirname, "shots", "compare");
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--threshold") threshold = Number(argv[++i]);
    else if (argv[i] === "--out") out = resolve(argv[++i] ?? "");
    else dirs.push(resolve(argv[i]));
  }
  if (dirs.length !== 2 || !Number.isFinite(threshold) || threshold < 0) {
    console.error("usage: node e2e/shots-compare.mjs <dirA> <dirB> [--threshold 0.001] [--out <dir>]");
    return 2;
  }
  const [dirA, dirB] = dirs;
  for (const d of dirs) if (!existsSync(d)) { console.error(`no such directory: ${d}`); return 2; }
  const results = compareDirs(dirA, dirB, { threshold });
  if (results.length === 0) { console.error("no PNG files in either directory"); return 2; }
  writeReport(results, dirA, dirB, out, threshold);
  for (const r of results) {
    console.log(`${r.status === "same" ? "same   " : "CHANGED"} ${r.name} — ${r.status}, ${(r.ratio * 100).toFixed(3)}% (${r.changed} px)`);
  }
  const bad = results.filter((r) => r.status !== "same");
  console.log(`\n${results.length - bad.length}/${results.length} states within ${(threshold * 100).toFixed(3)}%; report: ${join(out, "report.html")}`);
  if (bad.length) console.log(`over threshold: ${bad.map((r) => r.name).join(", ")}`);
  return bad.length ? 1 : 0;
}

if (process.argv[1] && basename(process.argv[1]) === "shots-compare.mjs") process.exit(main(process.argv.slice(2)));
