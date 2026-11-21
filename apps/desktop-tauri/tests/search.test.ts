// The Search page's store (M4-UX-001): grouping, debounced runs, a new query cancelling the
// previous request, options on the wire, errors.
import { describe, expect, it } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import { groupMatches, SearchStore } from "../src/state/search";
import type { WorkspaceStore } from "../src/state/workspace";

const m = (path: string, line: number) => ({ path, line, column: 1, endColumn: 4, preview: "foo", previewColumn: 1 });

describe("search", () => {
  it("groups matches by file in order", () => {
    expect(groupMatches([m("a", 1), m("a", 3), m("b", 2)]).map((g) => [g.path, g.matches.length])).toEqual([["a", 2], ["b", 1]]);
  });

  it("runs once after typing stops, cancels the previous request, and sends the options", async () => {
    const calls: Record<string, unknown>[] = [];
    const cancelled: string[] = [];
    const resolvers: ((v: unknown) => void)[] = [];
    const client = {
      cancellable: (_m: string, params: Record<string, unknown>) => {
        calls.push(params);
        return { promise: new Promise((r) => resolvers.push(r)), cancel: () => cancelled.push(String(params.query)) };
      },
      onNotification: () => () => {},
    } as unknown as DaemonClient;
    const ws = { getSnapshot: () => ({ id: "w1" }), on: () => {} } as unknown as WorkspaceStore;
    const s = new SearchStore(client, ws);
    s.update({ query: "f" });
    s.update({ query: "fo" });
    s.update({ query: "foo", wholeWord: true, include: " src/** " });
    await new Promise((r) => setTimeout(r, 250));
    expect(calls).toEqual([{ workspaceId: "w1", query: "foo", regex: false, caseSensitive: false, wholeWord: true, include: "src/**", exclude: undefined }]);
    s.update({ query: "foob" });
    s.now();
    expect(cancelled).toEqual(["foo"]);
    resolvers[1]!({ matches: [m("a", 1), m("b", 2)], files: [], truncated: true, filesSearched: 2 });
    await new Promise((r) => setTimeout(r, 0));
    expect(s.getSnapshot()).toMatchObject({ running: false, total: 2, truncated: true });
    expect(s.getSnapshot().groups.map((g) => g.path)).toEqual(["a", "b"]);
  });
});
