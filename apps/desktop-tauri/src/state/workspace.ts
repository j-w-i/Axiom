// Port of WorkspaceController + FileTreeModel (apps/desktop/src/models/).
// Same protocol sequence (data-flow.md §Open a workspace), same persisted key
// (`explorer.expanded`), same lazy listing and change handling: only expanded,
// loaded directories that are parents of changed paths are re-listed.

import type { DaemonClient } from "../daemon/client";
import { RpcError, type DidChangeParams, type DirEntry, type OpenResult, type RecentWorkspace } from "../daemon/types";
import { Debounce, Observable } from "./observable";

export const EXPANDED_KEY = "explorer.expanded";
const STATE_SAVE_DEBOUNCE_MS = 500;

export interface TreeNode {
  path: string; // workspace-relative, "/"-separated; "" = root
  name: string;
  kind: "file" | "directory" | "symlink";
  ignored: boolean;
  expanded: boolean;
  loaded: boolean;
  loading: boolean;
  children: string[];
  error: string | null; // listing failed: shown inline under the directory
}

export interface VisibleRow {
  node: TreeNode;
  depth: number;
}

export interface WorkspaceState {
  id: string;
  root: string;
  name: string;
  opening: boolean;
  error: string;
  recent: RecentWorkspace[];
  rootLoaded: boolean;
  /** Bumped on every tree change (the node map is mutated in place). */
  treeVersion: number;
}

type Events = { opened: () => void; closed: () => void };

function parentPath(p: string): string {
  const i = p.lastIndexOf("/");
  return i < 0 ? "" : p.slice(0, i);
}

export class WorkspaceStore extends Observable<WorkspaceState> {
  private nodes = new Map<string, TreeNode>();
  private restore = new Set<string>();
  private refetch = new Set<string>();
  private generation = 0;
  private restoring = false;
  private pendingClose = 0;
  private deferredOpen: string | null = null;
  private readonly saveTimer = new Debounce(STATE_SAVE_DEBOUNCE_MS, () => void this.flushState());
  private readonly handlers: { [K in keyof Events]: Set<Events[K]> } = { opened: new Set(), closed: new Set() };

  constructor(private readonly client: DaemonClient) {
    super({ id: "", root: "", name: "", opening: false, error: "", recent: [], rootLoaded: false, treeVersion: 0 });
    client.onNotification<DidChangeParams>("fs/didChange", (p) => {
      if (p.workspaceId !== this.state.id) return;
      this.applyChanges(p.changes.map((c) => c.path));
    });
    client.onState((s) => {
      if (s.state === "ready") {
        void this.refreshRecent();
      } else {
        // A new daemon process knows no workspaces.
        this.pendingClose = 0;
        this.deferredOpen = null;
        if (this.isOpen || this.state.opening) this.clearOpenState();
      }
    });
  }

  on<K extends keyof Events>(event: K, fn: Events[K]): () => void {
    this.handlers[event].add(fn);
    return () => this.handlers[event].delete(fn);
  }

  get isOpen(): boolean {
    return this.state.id !== "";
  }

  node(path: string): TreeNode | undefined {
    return this.nodes.get(path);
  }

  /** Depth-first list of rows to render (children of expanded directories). */
  visibleRows(): VisibleRow[] {
    const out: VisibleRow[] = [];
    const walk = (dir: TreeNode, depth: number) => {
      for (const p of dir.children) {
        const n = this.nodes.get(p);
        if (!n) continue;
        out.push({ node: n, depth });
        if (n.kind === "directory" && n.expanded) walk(n, depth + 1);
      }
    };
    const root = this.nodes.get("");
    if (root) walk(root, 0);
    return out;
  }

  expandedPaths(): string[] {
    return [...this.nodes.values()]
      .filter((n) => n.path !== "" && n.kind === "directory" && n.expanded)
      .map((n) => n.path)
      .sort();
  }

  dismissError(): void {
    this.set({ error: "" });
  }

  // ---- open / close ---------------------------------------------------------

  async openFolder(path: string): Promise<void> {
    if (!path) return;
    const gen = ++this.generation;
    this.set({ opening: true, error: "" });
    if (this.pendingClose > 0) {
      this.deferredOpen = path;
      return;
    }
    if (this.isOpen) {
      const oldId = this.state.id;
      await this.flushState();
      this.resetTree("");
      this.set({ id: "" });
      try {
        await this.client.request("workspace/close", { workspaceId: oldId });
      } catch {
        /* the old one is gone either way */
      }
    }
    if (gen !== this.generation) return;
    let r: OpenResult;
    try {
      r = await this.client.request<OpenResult>("workspace/open", { path });
    } catch (e) {
      if (gen !== this.generation) return;
      const err = e as RpcError;
      this.set({
        opening: false,
        error: err.kind === "not_found" ? "The folder does not exist." : `Could not open the folder: ${err.message}`,
      });
      return;
    }
    if (gen !== this.generation) return;
    this.resetTree(r.workspaceId);
    this.restoring = true;
    this.set({ id: r.workspaceId, root: r.root, name: r.name });
    try {
      await this.client.request("fs/watch", { workspaceId: r.workspaceId, enabled: true });
    } catch {
      /* watching is best effort; listing still works */
    }
    if (gen !== this.generation) return;
    let expanded: string[] = [];
    try {
      const s = await this.client.request<{ values: Record<string, unknown> }>("workspace/getState", {
        workspaceId: r.workspaceId,
        keys: [EXPANDED_KEY],
      });
      const v = s.values[EXPANDED_KEY];
      if (Array.isArray(v)) expanded = v.filter((x): x is string => typeof x === "string");
    } catch {
      /* no saved state */
    }
    if (gen !== this.generation) return;
    this.restore = new Set(expanded);
    await this.fetch("");
    if (gen !== this.generation) return;
    this.restoring = false;
    this.set({ opening: false });
    for (const fn of [...this.handlers.opened]) fn();
    void this.refreshRecent();
  }

  async close(): Promise<void> {
    if (!this.isOpen) return;
    const id = this.state.id;
    this.pendingClose++;
    const flushed = this.flushState();
    this.clearOpenState();
    await flushed;
    try {
      await this.client.request("workspace/close", { workspaceId: id });
    } catch {
      /* ignore */
    }
    if (this.pendingClose > 0) this.pendingClose--;
    if (this.pendingClose === 0 && this.deferredOpen) {
      const p = this.deferredOpen;
      this.deferredOpen = null;
      await this.openFolder(p);
    }
  }

  private clearOpenState(): void {
    this.saveTimer.stop();
    ++this.generation;
    this.resetTree("");
    this.set({ id: "", root: "", name: "", opening: false });
    for (const fn of [...this.handlers.closed]) fn();
  }

  private resetTree(workspaceId: string): void {
    this.nodes = new Map();
    this.refetch.clear();
    if (workspaceId) {
      this.nodes.set("", {
        path: "", name: "", kind: "directory", ignored: false,
        expanded: true, loaded: false, loading: false, children: [], error: null,
      });
    }
    this.set({ rootLoaded: false, treeVersion: this.state.treeVersion + 1 });
  }

  async refreshRecent(): Promise<void> {
    try {
      const r = await this.client.request<{ workspaces: RecentWorkspace[] }>("workspace/recent", {});
      this.set({ recent: r.workspaces });
    } catch {
      /* keep the old list */
    }
  }

  /** Opens the last workspace when `workspace.reopenLast` is true. */
  async reopenLast(): Promise<void> {
    try {
      const r = await this.client.request<{ values: Record<string, unknown> }>("settings/get", {
        keys: ["workspace.reopenLast", "workspace.lastRoot"],
      });
      if (this.isOpen || this.state.opening) return;
      const last = r.values["workspace.lastRoot"];
      const reopen = r.values["workspace.reopenLast"] !== false;
      if (reopen && typeof last === "string" && last) await this.openFolder(last);
    } catch {
      /* nothing to reopen */
    }
  }

  // ---- explorer state persistence ------------------------------------------

  async flushState(): Promise<void> {
    this.saveTimer.stop();
    if (!this.isOpen || !this.state.rootLoaded) return;
    try {
      await this.client.request("workspace/setState", {
        workspaceId: this.state.id,
        values: { [EXPANDED_KEY]: this.expandedPaths() },
      });
    } catch {
      /* best effort, like the Qt shell */
    }
  }

  // ---- tree ------------------------------------------------------------------

  setExpanded(path: string, expanded: boolean): void {
    const n = this.nodes.get(path);
    if (!n || n.kind !== "directory" || n.expanded === expanded) return;
    n.expanded = expanded;
    this.bump();
    // Re-list on expand even when loaded: it may have changed while collapsed.
    if (expanded) void this.fetch(path);
    if (!this.restoring && this.isOpen) this.saveTimer.schedule();
  }

  toggle(path: string): void {
    const n = this.nodes.get(path);
    if (n) this.setExpanded(path, !n.expanded);
  }

  private bump(): void {
    this.set({ treeVersion: this.state.treeVersion + 1 });
  }

  private async fetch(path: string): Promise<void> {
    const dir = this.nodes.get(path);
    const workspaceId = this.state.id;
    if (!dir || !workspaceId) return;
    if (dir.loading) {
      this.refetch.add(path); // a change arrived mid-listing: list again afterwards
      return;
    }
    dir.loading = true;
    if (!dir.loaded && path !== "") this.bump();
    const gen = this.generation;
    let entries: DirEntry[] | null = null;
    let error: string | null = null;
    try {
      entries = (await this.client.request<{ entries: DirEntry[] }>("fs/list", { workspaceId, path })).entries;
    } catch (e) {
      error = (e as RpcError).message;
    }
    if (gen !== this.generation) return;
    const current = this.nodes.get(path);
    if (!current) return; // disappeared while listing
    current.loading = false;
    if (error !== null) {
      if (path === "") {
        this.set({ error: `Could not list the folder: ${error}` });
      } else {
        this.removeSubtree(current, false);
        current.children = [];
        current.error = error;
      }
      this.bump();
      return;
    }
    const toFetch = this.applyListing(current, entries ?? []);
    current.loaded = true;
    current.error = null;
    if (path === "" && !this.state.rootLoaded) this.set({ rootLoaded: true });
    this.bump();
    const again = this.refetch.delete(path);
    await Promise.all([...toFetch.map((p) => this.fetch(p)), ...(again ? [this.fetch(path)] : [])]);
  }

  /** Replaces a directory's children, keeping nodes (and their expansion) that still exist. */
  private applyListing(dir: TreeNode, entries: DirEntry[]): string[] {
    const keep = new Map<string, TreeNode>();
    for (const p of dir.children) {
      const n = this.nodes.get(p);
      if (n) keep.set(n.name, n);
    }
    const next: string[] = [];
    const toFetch: string[] = [];
    for (const e of entries) {
      const path = dir.path === "" ? e.name : `${dir.path}/${e.name}`;
      const old = keep.get(e.name);
      if (old && old.kind === e.kind) {
        old.ignored = e.ignored ?? false;
        keep.delete(e.name);
        next.push(path);
        continue;
      }
      const restore = e.kind === "directory" && this.restore.delete(path);
      this.nodes.set(path, {
        path, name: e.name, kind: e.kind, ignored: e.ignored ?? false,
        expanded: restore, loaded: false, loading: false, children: [], error: null,
      });
      next.push(path);
      if (restore) toFetch.push(path);
    }
    for (const gone of keep.values()) this.removeSubtree(gone, true);
    dir.children = next;
    return toFetch;
  }

  private removeSubtree(n: TreeNode, self: boolean): void {
    for (const c of n.children) {
      const child = this.nodes.get(c);
      if (child) this.removeSubtree(child, true);
    }
    if (self) this.nodes.delete(n.path);
  }

  /** `paths` from fs/didChange; "" means "re-list everything shown". */
  applyChanges(paths: string[]): void {
    if (!this.isOpen || !this.state.rootLoaded) return;
    const all = paths.includes("");
    const parents = new Set(paths.filter((p) => p !== "").map(parentPath));
    const visit = (n: TreeNode) => {
      const shown = n.path === "" || (n.expanded && n.loaded);
      if (!shown) return;
      if (all || parents.has(n.path)) void this.fetch(n.path);
      for (const c of n.children) {
        const child = this.nodes.get(c);
        if (child?.kind === "directory") visit(child);
      }
    };
    const root = this.nodes.get("");
    if (root) visit(root);
  }
}
