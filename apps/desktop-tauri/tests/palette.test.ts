// Command palette (SURFACES-001): recent-first ordering, match highlighting, empty states and
// the filter's cost on a large listing.

import { describe, expect, it } from "vitest";
import { emptyState, filterCommands, loadMru, matchRanges, MRU_KEY, MRU_MAX, orderByMru, pushMru, type PaletteCommand } from "../src/components/CommandPalette";

const cmd = (id: string, label = id): PaletteCommand => ({ id, group: "View", label, run: () => {} });

describe("palette", () => {
  it("mru puts recent commands first", () => {
    const all = ["open", "save", "theme", "about"].map((id) => cmd(id));
    expect(orderByMru(all, ["about", "gone", "save"]).map((c) => c.id)).toEqual(["about", "save", "open", "theme"]);
    let mru: string[] = [];
    for (const id of ["a", "b", "c", "a"]) mru = pushMru(mru, id);
    expect(mru).toEqual(["a", "c", "b"]);
    for (let i = 0; i < 20; i++) mru = pushMru(mru, `x${i}`);
    expect(mru).toHaveLength(MRU_MAX);
    // Persisted list read back (shape-checked: junk is ignored).
    const store = (v: string | null) => ({ getItem: (k: string) => (k === MRU_KEY ? v : null) });
    expect(loadMru(store(JSON.stringify(["a", 3, "b"])))).toEqual(["a", "b"]);
    expect(loadMru(store("not json"))).toEqual([]);
    expect(loadMru(store(null))).toEqual([]);
  });

  it("file match highlight span", () => {
    expect(matchRanges("exec", "crates/exec/src/executor.rs")).toEqual([[7, 11]]); // first substring
    expect(matchRanges("eor", "src/executor.rs")).toEqual([[4, 5], [10, 12]]); // subsequence; adjacent hits merged
    expect(matchRanges("", "a")).toEqual([]);
    expect(matchRanges("zz", "abc")).toEqual([]);
  });

  it("empty state without workspace offers open folder", () => {
    expect(emptyState("files", false)).toEqual({ title: "Open a folder to search files", hint: "", openFolder: true });
    expect(emptyState("commands", true)).toEqual({ title: "No matching commands", hint: "Try fewer letters", openFolder: false });
    expect(emptyState("files", true).openFolder).toBe(false);
  });

  it("filter 5000 entries under 8 ms", () => {
    const files = Array.from({ length: 5000 }, (_, i) => cmd(`f${i}`, `crates/mod${i % 50}/src/file_${i}.rs`));
    filterCommands(files, "warm"); // JIT warm-up
    const times: number[] = [];
    for (const q of ["f", "fi", "fil", "file_4", "mod3/src", "zzz", "crs"]) {
      const t0 = performance.now();
      filterCommands(files, q);
      times.push(performance.now() - t0);
    }
    times.sort((a, b) => a - b);
    expect(times[Math.floor(times.length / 2)]).toBeLessThan(8); // median per keystroke
    expect(filterCommands(files, "file_4999").map((c) => c.id)[0]).toBe("f4999");
  });
});
