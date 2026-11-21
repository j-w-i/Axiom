// Workspace search for the Search page (M4-UX-001): the query and its options, results grouped by
// file, through `workspace/search` in the daemon. A new query cancels the previous request.

import type { DaemonClient } from "../daemon/client";
import type { SearchMatch, SearchResult } from "../daemon/protocol.gen";
import { Debounce, Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export interface SearchOptions {
  regex: boolean;
  caseSensitive: boolean;
  wholeWord: boolean;
  include: string;
  exclude: string;
}

export interface SearchState extends SearchOptions {
  query: string;
  running: boolean;
  /** Results of the last finished search (for `query` as it was then). */
  groups: { path: string; matches: SearchMatch[] }[];
  total: number;
  truncated: boolean;
  error: string;
}

/** Matches grouped by file, in the order the daemon found them. Pure. */
export function groupMatches(matches: SearchMatch[]): { path: string; matches: SearchMatch[] }[] {
  const out: { path: string; matches: SearchMatch[] }[] = [];
  for (const m of matches) {
    const last = out[out.length - 1];
    if (last && last.path === m.path) last.matches.push(m);
    else out.push({ path: m.path, matches: [m] });
  }
  return out;
}

const EMPTY: SearchState = { query: "", regex: false, caseSensitive: false, wholeWord: false, include: "", exclude: "", running: false, groups: [], total: 0, truncated: false, error: "" };

export class SearchStore extends Observable<SearchState> {
  private cancel: (() => void) | null = null;
  private seq = 0;
  private readonly runner = new Debounce(200, () => void this.run());

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super(EMPTY);
    workspace.on("closed", () => {
      this.cancel?.();
      this.set({ ...EMPTY });
    });
  }

  /** Changes the query or options; the search runs 200 ms after the last change. */
  update(patch: Partial<SearchOptions & { query: string }>): void {
    this.set(patch);
    this.runner.schedule();
  }

  /** Runs now (Enter in the query field). */
  now(): void {
    this.runner.stop();
    void this.run();
  }

  private async run(): Promise<void> {
    this.cancel?.();
    this.cancel = null;
    const s = this.state;
    const ws = this.workspace.getSnapshot().id;
    if (!ws || !s.query) {
      this.set({ running: false, groups: [], total: 0, truncated: false, error: "" });
      return;
    }
    const seq = ++this.seq;
    const c = this.client.cancellable<SearchResult>("workspace/search", {
      workspaceId: ws,
      query: s.query,
      regex: s.regex,
      caseSensitive: s.caseSensitive,
      wholeWord: s.wholeWord,
      include: s.include.trim() || undefined,
      exclude: s.exclude.trim() || undefined,
    });
    this.cancel = c.cancel;
    this.set({ running: true, error: "" });
    try {
      const r = await c.promise;
      if (seq !== this.seq) return;
      this.set({ running: false, groups: groupMatches(r.matches ?? []), total: r.matches?.length ?? 0, truncated: r.truncated, error: "" });
    } catch (e) {
      if (seq !== this.seq) return;
      const msg = e instanceof Error ? e.message : String(e);
      this.set({ running: false, groups: [], total: 0, truncated: false, error: /cancel/i.test(msg) ? "" : msg });
    } finally {
      if (seq === this.seq) this.cancel = null;
    }
  }
}
