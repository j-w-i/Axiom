// Git in the UI (M4-GIT-002): marks, groups and the gutter's line diff.
import { describe, expect, it } from "vitest";
import type { GitFile } from "../src/daemon/protocol.gen";
import { lineChanges } from "../src/editor/git/linediff";
import { groups, markOf, marksOf } from "../src/state/git";

const f = (path: string, index: GitFile["index"], worktree: GitFile["worktree"]): GitFile => ({ path, index, worktree });

describe("git marks", () => {
  it("a file's mark: conflicts first, then the working tree, then what is staged", () => {
    expect(markOf(f("a", "unmodified", "modified"))).toMatchObject({ letter: "M", staged: false });
    expect(markOf(f("a", "added", "unmodified"))).toMatchObject({ letter: "A", staged: true });
    expect(markOf(f("a", "added", "modified"))).toMatchObject({ letter: "M", staged: false });
    expect(markOf(f("a", "untracked", "untracked"))).toMatchObject({ letter: "U", word: "untracked" });
    expect(markOf(f("a", "conflicted", "conflicted"))).toMatchObject({ letter: "C" });
    expect(markOf(f("a", "renamed", "unmodified"))).toMatchObject({ letter: "R" });
    expect(markOf(f("a", "unmodified", "deleted"))).toMatchObject({ letter: "D" });
    expect(markOf(f("a", "unmodified", "unmodified"))).toBeNull();
  });

  it("folders carry the most severe change inside", () => {
    const { marks, folders } = marksOf({ repo: true, ahead: 0, behind: 0, files: [f("src/a/x.rs", "unmodified", "modified"), f("src/b.rs", "untracked", "untracked"), f("src/a/y.rs", "conflicted", "conflicted")] });
    expect(Object.keys(marks).sort()).toEqual(["src/a/x.rs", "src/a/y.rs", "src/b.rs"]);
    expect(folders).toEqual({ src: "conflicted", "src/a": "conflicted" });
  });

  it("Source Control groups", () => {
    const g = groups({ repo: true, ahead: 0, behind: 0, files: [f("z", "added", "modified"), f("m", "unmodified", "modified"), f("u", "untracked", "untracked"), f("c", "conflicted", "conflicted")] });
    expect(g.conflicts.map((x) => x.path)).toEqual(["c"]);
    expect(g.staged.map((x) => x.path)).toEqual(["z"]);
    expect(g.changes.map((x) => x.path)).toEqual(["m", "z"]);
    expect(g.untracked.map((x) => x.path)).toEqual(["u"]);
  });
});

describe("the gutter's line diff", () => {
  const base = "a\nb\nc\nd\ne\n";
  it("nothing changed", () => expect(lineChanges(base, base)).toEqual([]));
  it("added lines", () => expect(lineChanges(base, "a\nb\nX\nY\nc\nd\ne\n")).toEqual([{ kind: "added", start: 3, end: 4 }]));
  it("a modified line", () => expect(lineChanges(base, "a\nB\nc\nd\ne\n")).toEqual([{ kind: "modified", start: 2, end: 2 }]));
  it("deleted lines mark the line before", () => expect(lineChanges(base, "a\nb\ne\n")).toEqual([{ kind: "deleted", start: 2, end: 2 }]));
  it("deleted at the top", () => expect(lineChanges(base, "c\nd\ne\n")).toEqual([{ kind: "deleted", start: 1, end: 1 }]));
  it("several changes", () =>
    expect(lineChanges(base, "a\nB\nc\ne\nf\n")).toEqual([
      { kind: "modified", start: 2, end: 2 },
      { kind: "deleted", start: 3, end: 3 },
      { kind: "added", start: 5, end: 5 },
    ]));
  it("a new file is all added; an emptied one deleted", () => {
    expect(lineChanges("", "x\ny\n")).toEqual([{ kind: "added", start: 1, end: 2 }]);
    expect(lineChanges("x\n", "")).toEqual([{ kind: "deleted", start: 1, end: 1 }]);
  });
  it("a large rewrite stays bounded", () => {
    const big = Array.from({ length: 5000 }, (_, i) => `line ${i}`).join("\n");
    const other = Array.from({ length: 5000 }, (_, i) => `other ${i}`).join("\n");
    const t = performance.now();
    const r = lineChanges(big, other);
    expect(performance.now() - t).toBeLessThan(2000);
    expect(r.length).toBeGreaterThan(0);
  });
});
