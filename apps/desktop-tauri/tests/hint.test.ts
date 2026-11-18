// The hint line's text rules (SHELL-005).

import { describe, expect, it } from "vitest";
import { parseHint, substituteMod } from "../src/ui/hint";

describe("hint", () => {
  it("parseHint splits key chords", () => {
    expect(parseHint("Rerun the last command — F5", false)).toEqual({ what: "Rerun the last command", keys: ["F5"] });
    expect(parseHint("Command palette — Mod+K", false)).toEqual({ what: "Command palette", keys: ["Ctrl", "K"] });
    expect(parseHint("Stop the task — Shift+F5", false)).toEqual({ what: "Stop the task", keys: ["Shift", "F5"] });
    expect(parseHint("Focus the command field — Mod Shift R", false)).toEqual({ what: "Focus the command field", keys: ["Ctrl", "Shift", "R"] });
  });

  it("MOD substitution per platform", () => {
    expect(parseHint("Save — Mod+S", true).keys).toEqual(["⌘", "S"]);
    expect(parseHint("Save — Mod+S", false).keys).toEqual(["Ctrl", "S"]);
    expect(substituteMod("Alt", true)).toBe("⌥");
    expect(substituteMod("Alt", false)).toBe("Alt");
  });

  it("non-key suffix stays text", () => {
    expect(parseHint("Drag to resize — double-click resets", false)).toEqual({ what: "Drag to resize — double-click resets", keys: [] });
    expect(parseHint("Open the backend log", false)).toEqual({ what: "Open the backend log", keys: [] });
    expect(parseHint("Workspace — ~/src/axiom", false).keys).toEqual([]);
  });
});
