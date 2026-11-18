// Explorer glyph chips (SHELL-004; design-system.md §7).

import { describe, expect, it } from "vitest";
import { glyphFor, rowSlots } from "../src/components/Explorer";

describe("explorer", () => {
  it("glyph label per extension", () => {
    expect(glyphFor("executor.rs")).toEqual({ label: "rs", kind: "rs" });
    expect(glyphFor("README.md")).toEqual({ label: "md", kind: "md" });
    expect(glyphFor("Cargo.toml")).toEqual({ label: "tm", kind: "toml" });
    expect(glyphFor("Cargo.lock")).toEqual({ label: "lk", kind: "toml" });
    expect(glyphFor("package.json")).toEqual({ label: "js", kind: "other" });
    expect(glyphFor("main.TS")).toEqual({ label: "ts", kind: "other" });
    expect(glyphFor(".gitignore")).toEqual({ label: "", kind: "other" }); // a dotfile has no extension
    expect(glyphFor("Makefile")).toEqual({ label: "", kind: "other" });
  });

  it("an error row takes its own slot, so the rows after it move down (SHELL-007)", () => {
    expect(rowSlots([false, true, false, false])).toEqual({ slot: [0, 1, 3, 4], total: 5 });
    expect(rowSlots([])).toEqual({ slot: [], total: 0 });
  });
});
