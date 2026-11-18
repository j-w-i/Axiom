// Editor tab strip (EDITOR-001): the lit edge is placed by transform from the active tab's box.

import { describe, expect, it } from "vitest";
import { glideTransform } from "../src/components/EditorArea";

describe("tabs", () => {
  it("glide geometry from tab rect", () => {
    expect(glideTransform(0, 100)).toBe("translateX(0px) scaleX(1)");
    expect(glideTransform(132, 150)).toBe("translateX(132px) scaleX(1.5)");
    expect(glideTransform(48.5, 87)).toBe("translateX(48.5px) scaleX(0.87)");
  });
});
