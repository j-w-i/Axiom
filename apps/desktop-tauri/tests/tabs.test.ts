// Editor tab strip (EDITOR-001): the lit edge is placed by transform from the active tab's box.

import { describe, expect, it } from "vitest";
import { dropSlot, glideTransform, siblingShift } from "../src/components/EditorArea";

describe("tabs", () => {
  it("glide geometry from tab rect", () => {
    expect(glideTransform(0, 100)).toBe("translateX(0px) scaleX(1)");
    expect(glideTransform(132, 150)).toBe("translateX(132px) scaleX(1.5)");
    expect(glideTransform(48.5, 87)).toBe("translateX(48.5px) scaleX(0.87)");
  });

  it("drag slot and sibling shift (D7)", () => {
    // Four tabs, 100 px wide, at 0/100/200/300; dragging the second (index 1).
    const lefts = [0, 100, 200, 300];
    const widths = [100, 100, 100, 100];
    expect(dropSlot(lefts, widths, 1, 0)).toBe(1); // not moved
    expect(dropSlot(lefts, widths, 1, 49)).toBe(1); // right edge 249: short of the next midpoint (250)
    expect(dropSlot(lefts, widths, 1, 51)).toBe(2); // crossed it
    expect(dropSlot(lefts, widths, 1, 151)).toBe(3);
    expect(dropSlot(lefts, widths, 1, -51)).toBe(0); // left edge 49: past the first midpoint (50)
    // A wide tab clamped at the strip's start still passes a narrow first tab (its left edge leads).
    expect(dropSlot([0, 60], [60, 140], 1, -60)).toBe(0);
    expect(siblingShift(widths, 1, 3)).toEqual([0, 0, -100, -100]); // the ones it passes move left
    expect(siblingShift(widths, 2, 0)).toEqual([100, 100, 0, 0]);
    expect(siblingShift(widths, 2, 2)).toEqual([0, 0, 0, 0]);
  });
});
