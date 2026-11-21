// Go to File's ranking (M4-UX-001).
import { describe, expect, it } from "vitest";
import { fuzzyMatch, rankFuzzy } from "../src/ui/fuzzy";

const PATHS = [
  "apps/desktop-tauri/src/components/EditorArea.tsx",
  "apps/desktop-tauri/src/components/Explorer.tsx",
  "apps/desktop-tauri/src/editor/surface.ts",
  "crates/axiomd/src/main.rs",
  "crates/lsp/src/lib.rs",
  "crates/term/src/lib.rs",
  "docs/roadmap/milestones.md",
  "README.md",
  "src/main.rs",
  "tests/helpers/fake-lsp/src/main.rs",
  "beads.txt",
];
const top = (q: string, n = 1) => rankFuzzy(PATHS, q, (p) => p).slice(0, n).map((r) => r.item);

describe("fuzzy ranking of paths", () => {
  it("a subsequence matches; anything else does not", () => {
    expect(fuzzyMatch("mnrs", "src/main.rs")).not.toBeNull();
    expect(fuzzyMatch("zzz", "src/main.rs")).toBeNull();
  });

  it("the file name and segment starts win", () => {
    expect(top("mainrs")).toEqual(["src/main.rs"]);
    expect(top("lsplib")).toEqual(["crates/lsp/src/lib.rs"]);
    expect(top("termlib")).toEqual(["crates/term/src/lib.rs"]);
    expect(top("readme")).toEqual(["README.md"]);
    expect(top("miles")).toEqual(["docs/roadmap/milestones.md"]);
  });

  it("camelCase humps", () => {
    expect(top("EA")).toEqual(["apps/desktop-tauri/src/components/EditorArea.tsx"]);
    expect(top("edar")).toEqual(["apps/desktop-tauri/src/components/EditorArea.tsx"]);
  });

  it("highlight ranges follow the chosen alignment", () => {
    expect(fuzzyMatch("main", "src/main.rs")!.ranges).toEqual([[4, 8]]);
    expect(fuzzyMatch("EA", "components/EditorArea.tsx")!.ranges).toEqual([[11, 12], [17, 18]]);
  });

  it("20 000 paths rank within a keystroke's budget", () => {
    const many = Array.from({ length: 20_000 }, (_, i) => `crates/c${i % 50}/src/module_${i}/file_${i}.rs`);
    const t = performance.now();
    const r = rankFuzzy(many, "mod12fil", (p) => p);
    const ms = performance.now() - t;
    expect(r.length).toBeGreaterThan(0);
    expect(ms).toBeLessThan(500);
  });
});
