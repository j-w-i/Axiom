// The tab strip's lit edge (INTERACTION-003): placed by transform; glides on activation and
// jumps on its first placement, after a close and on resize.

import { describe, expect, it } from "vitest";
import { glideMode, glideTransform } from "../src/components/EditorArea";

describe("tab glide", () => {
  it("glide transform from tab geometry", () => {
    expect(glideTransform(240, 132)).toBe("translateX(240px) scaleX(1.32)");
    expect(glideTransform(0, 100)).toBe("translateX(0px) scaleX(1)");
  });

  it("instant on first render", () => {
    expect(glideMode(null, { path: "a.rs", count: 4 })).toBe("instant");
    expect(glideMode({ path: "a.rs", count: 4 }, { path: "b.rs", count: 4 })).toBe("glide"); // activation
    expect(glideMode({ path: "a.rs", count: 4 }, { path: "b.rs", count: 3 })).toBe("instant"); // a close reflows the strip
    expect(glideMode({ path: "a.rs", count: 3 }, { path: "c.rs", count: 4 })).toBe("glide"); // opening a new tab
    expect(glideMode({ path: "a.rs", count: 4 }, { path: "a.rs", count: 4 })).toBe("instant"); // same tab: resize, re-render
  });
});
