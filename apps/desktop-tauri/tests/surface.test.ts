// Monaco themes from tokens (EDITOR-002): the rule table and the hex conversion.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { hexFromCanvasStyle, lineHeightFor, THEME_TOKENS, themeData, type ThemeToken } from "../src/editor/theme";

/** A colour per token that encodes the token's index, so each use can be traced back. */
const fake = (t: ThemeToken) => `#${(THEME_TOKENS.indexOf(t) + 1).toString(16).padStart(6, "0")}`;

describe("surface theme", () => {
  it("theme rules cover every --syn token", () => {
    const tokens = readFileSync(join(import.meta.dirname, "../src/styles/tokens.css"), "utf8");
    const syn = [...new Set([...tokens.matchAll(/(--syn-[a-z-]+)\s*:/g)].map((m) => m[1]!))].filter((n) => n !== "--syn-keyword-style");
    expect(syn.length).toBeGreaterThanOrEqual(8);
    const data = themeData("vs-dark", fake, "");
    const used = new Set(data.rules.map((r) => r.foreground).filter(Boolean));
    for (const name of syn) expect(used, name).toContain(fake(name as ThemeToken).slice(1));
    expect(data.rules.find((r) => r.token === "comment")?.fontStyle).toBe("italic");
    expect(data.rules.find((r) => r.token === "keyword")?.fontStyle).toBeUndefined(); // --syn-keyword-style empty
    expect(themeData("vs", fake, "bold").rules.find((r) => r.token === "keyword")?.fontStyle).toBe("bold");
    expect(data.colors["editor.selectionBackground"]).toBe(fake("--editor-selection"));
    expect(data.colors["editorCursor.foreground"]).toBe(fake("--editor-cursor"));
    expect(data.colors["focusBorder"]).toBe("#00000000");
  });

  it("hex keeps alpha", () => {
    expect(hexFromCanvasStyle("#C9A35B")).toBe("#c9a35b");
    expect(hexFromCanvasStyle("rgba(201, 163, 91, 0.2)")).toBe("#c9a35b33");
    expect(hexFromCanvasStyle("rgb(255 255 255 / 0.045)")).toBe("#ffffff0b");
    expect(hexFromCanvasStyle("rgba(0, 0, 0, 1)")).toBe("#000000");
    expect(hexFromCanvasStyle("rgb(0 0 0 / 50%)")).toBe("#00000080");
  });

  it("line height keeps the 13/20 ratio", () => {
    expect(lineHeightFor(13)).toBe(20);
    expect(lineHeightFor(15)).toBe(23);
  });
});
