// Explorer glyph chips (SHELL-004; design-system.md §7).

import { describe, expect, it } from "vitest";
import { glyphFor } from "../src/components/Explorer";

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
});
