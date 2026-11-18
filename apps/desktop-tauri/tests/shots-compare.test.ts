// The screenshot comparer (e2e/shots-compare.mjs, M1.6-QA-001) is the instrument
// every M1.6 visual claim rests on, so its counting is tested here.

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { compareDirs, decodePng, diffImages, encodePng, type Image } from "../e2e/shots-compare.mjs";

function solid(width: number, height: number, rgba: [number, number, number, number]): Image {
  const data = new Uint8Array(width * height * 4);
  for (let i = 0; i < data.length; i += 4) data.set(rgba, i);
  return { width, height, data };
}

describe("shots-compare", () => {
  it("identical images report 0 changed pixels", () => {
    const a = solid(8, 6, [30, 40, 50, 255]);
    const r = diffImages(a, solid(8, 6, [30, 40, 50, 255]));
    expect(r.changed).toBe(0);
    expect(r.total).toBe(48);
    expect(r.ratio).toBe(0);
  });

  it("one changed pixel is counted", () => {
    const a = solid(8, 6, [30, 40, 50, 255]);
    const b = solid(8, 6, [30, 40, 50, 255]);
    b.data.set([200, 40, 50, 255], (3 * 8 + 5) * 4);
    const r = diffImages(a, b);
    expect(r.changed).toBe(1);
    expect(r.ratio).toBeCloseTo(1 / 48);
    // the diff image marks exactly that pixel in red
    expect([...r.diff!.slice((3 * 8 + 5) * 4, (3 * 8 + 5) * 4 + 4)]).toEqual([255, 0, 0, 255]);
  });

  it("anti-aliasing tolerance ignores ±1", () => {
    const a = solid(4, 4, [100, 100, 100, 255]);
    const off1 = solid(4, 4, [101, 99, 100, 255]);
    const off2 = solid(4, 4, [102, 100, 100, 255]);
    expect(diffImages(a, off1).changed).toBe(0);
    expect(diffImages(a, off2).changed).toBe(16);
    expect(diffImages(a, off2, 2).changed).toBe(0);
  });

  it("images of different size are fully changed", () => {
    const r = diffImages(solid(4, 4, [0, 0, 0, 255]), solid(5, 4, [0, 0, 0, 255]));
    expect(r.sameSize).toBe(false);
    expect(r.ratio).toBe(1);
  });

  it("PNG encode then decode returns the same pixels", () => {
    const img = solid(7, 5, [1, 2, 3, 255]);
    img.data.set([250, 128, 0, 77], 4 * 9);
    const back = decodePng(encodePng(img.width, img.height, img.data));
    expect(back.width).toBe(7);
    expect(back.height).toBe(5);
    expect([...back.data]).toEqual([...img.data]);
  });

  it("compares two directories: same, over threshold, and a state missing from one side", () => {
    const root = mkdtempSync(join(tmpdir(), "axiom-shots-compare-"));
    try {
      const a = join(root, "a");
      const b = join(root, "b");
      mkdirSync(a);
      mkdirSync(b);
      const base = solid(10, 10, [10, 10, 10, 255]);
      const moved = solid(10, 10, [10, 10, 10, 255]);
      for (let i = 0; i < 5; i++) moved.data.set([255, 255, 255, 255], i * 4); // 5% of 100 px
      const put = (dir: string, name: string, img: Image) =>
        writeFileSync(join(dir, `${name}.png`), encodePng(img.width, img.height, img.data));
      put(a, "s4-dark", base);
      put(b, "s4-dark", base);
      put(a, "s4-light", base);
      put(b, "s4-light", moved);
      put(a, "only-before", base);
      const status = Object.fromEntries(compareDirs(a, b, { threshold: 0.001 }).map((x) => [x.name, x.status]));
      expect(status).toEqual({ "only-before": "missing-after", "s4-dark": "same", "s4-light": "changed" });
      // a threshold above the change lets it pass
      expect(compareDirs(a, b, { threshold: 0.1 }).find((x) => x.name === "s4-light")!.status).toBe("same");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
