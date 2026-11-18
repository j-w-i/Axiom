// Task transport numbers (SHELL-003).

import { describe, expect, it } from "vitest";
import { formatElapsed, rateLevels, shortenHome } from "../src/shell/transport";

describe("transport", () => {
  it("meter normalises samples", () => {
    // fewer than 16 samples: padded with zeros on the left (oldest side)
    expect(rateLevels([4])).toEqual([...Array(15).fill(0), 0.5]); // min scale 8
    const l = rateLevels([0, 10, 20, 40]);
    expect(l).toHaveLength(16);
    expect(l.slice(-4)).toEqual([0, 0.25, 0.5, 1]); // normalised to the window's max
    expect(rateLevels(Array.from({ length: 30 }, (_, i) => i)).slice(-1)).toEqual([1]); // only the last 16 count
    expect(rateLevels([])).toEqual(Array(16).fill(0));
  });

  it("elapsed formats mm:ss (1 s steps)", () => {
    expect(formatElapsed(0)).toBe("00:00");
    expect(formatElapsed(12_400)).toBe("00:12");
    expect(formatElapsed(61_999)).toBe("01:01");
    expect(formatElapsed(75 * 60_000 + 2_000)).toBe("75:02");
  });

  it("home is shortened to ~", () => {
    expect(shortenHome("/home/alex/src/axiom")).toBe("~/src/axiom");
    expect(shortenHome("/Users/alex/src")).toBe("~/src");
    expect(shortenHome("C:\\Users\\alex\\src")).toBe("~\\src");
    expect(shortenHome("/tmp/axiom-shots-ws/axiom")).toBe("/tmp/axiom-shots-ws/axiom");
    expect(shortenHome("/home/alex")).toBe("~");
  });
});
