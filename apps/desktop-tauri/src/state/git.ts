// The workspace's Git state as the UI shows it (M4-GIT-002; ADR-0018 §5): status from the daemon
// (`git/status`, refreshed on `git/didChange`), a mark per changed file and per folder that holds
// changes (explorer, tabs, Source Control), the branch (status bar), and HEAD text for the
// gutter and the diff view (cached until the repository changes).

import type { DaemonClient } from "../daemon/client";
import type { GitFile, GitFileState, GitStatus, ShowResult } from "../daemon/protocol.gen";
import { Debounce, Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export type GitKind = "modified" | "added" | "deleted" | "renamed" | "untracked" | "conflicted";

export interface GitMark {
  kind: GitKind;
  /** One letter, shown as text (never colour alone). */
  letter: "M" | "A" | "D" | "R" | "U" | "C";
  /** In words, for names: "modified". */
  word: string;
  /** Staged (index side) rather than only in the working tree. */
  staged: boolean;
}

export interface GitState {
  status: GitStatus | null;
  marks: Record<string, GitMark>;
  /** Folders that contain changes → the most severe kind inside. */
  folders: Record<string, GitKind>;
  /** The file whose diff (HEAD ↔ working tree) the editor area shows, if any. */
  diff: string | null;
}

const LETTER: Record<GitKind, GitMark["letter"]> = { modified: "M", added: "A", deleted: "D", renamed: "R", untracked: "U", conflicted: "C" };
const SEVERITY: Record<GitKind, number> = { conflicted: 5, deleted: 4, modified: 3, renamed: 2, added: 1, untracked: 0 };

function kindOf(s: GitFileState): GitKind | null {
  switch (s) {
    case "modified":
    case "typeChanged":
      return "modified";
    case "added":
    case "copied":
      return "added";
    case "deleted":
      return "deleted";
    case "renamed":
      return "renamed";
    case "untracked":
      return "untracked";
    case "conflicted":
      return "conflicted";
    default:
      return null;
  }
}

/** The mark of one file: conflicts first, then the working-tree change, then the staged one. Pure. */
export function markOf(f: GitFile): GitMark | null {
  if (f.index === "conflicted" || f.worktree === "conflicted") return mark("conflicted", false);
  const w = kindOf(f.worktree);
  const i = kindOf(f.index);
  if (w && w !== "untracked") return mark(w, false);
  if (i) return mark(i, true);
  if (w) return mark(w, false);
  return null;
}

function mark(kind: GitKind, staged: boolean): GitMark {
  return { kind, letter: LETTER[kind], word: kind, staged };
}

/** Marks per path and the folders that hold changes. Pure. */
export function marksOf(status: GitStatus | null): Pick<GitState, "marks" | "folders"> {
  const marks: Record<string, GitMark> = {};
  const folders: Record<string, GitKind> = {};
  for (const f of status?.files ?? []) {
    const m = markOf(f);
    if (!m) continue;
    marks[f.path] = m;
    const parts = f.path.split("/");
    for (let i = 1; i < parts.length; i++) {
      const dir = parts.slice(0, i).join("/");
      const had = folders[dir];
      if (!had || SEVERITY[m.kind] > SEVERITY[had]) folders[dir] = m.kind;
    }
  }
  return { marks, folders };
}

/** Changes grouped as Source Control lists them. Pure. */
export function groups(status: GitStatus | null): { conflicts: GitFile[]; staged: GitFile[]; changes: GitFile[]; untracked: GitFile[] } {
  const out = { conflicts: [] as GitFile[], staged: [] as GitFile[], changes: [] as GitFile[], untracked: [] as GitFile[] };
  for (const f of status?.files ?? []) {
    if (f.index === "conflicted" || f.worktree === "conflicted") out.conflicts.push(f);
    else if (f.index === "untracked") out.untracked.push(f);
    else {
      if (kindOf(f.index)) out.staged.push(f);
      if (kindOf(f.worktree)) out.changes.push(f);
    }
  }
  for (const list of Object.values(out)) list.sort((a, b) => a.path.localeCompare(b.path));
  return out;
}

export class GitStore extends Observable<GitState> {
  private readonly heads = new Map<string, Promise<string | null>>();
  private readonly refresher = new Debounce(150, () => void this.refresh());
  private seq = 0;

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super({ status: null, marks: {}, folders: {}, diff: null });
    workspace.on("opened", () => void this.refresh());
    workspace.on("closed", () => {
      this.heads.clear();
      this.seq++;
      this.set({ status: null, marks: {}, folders: {}, diff: null });
    });
    client.onNotification<{ workspaceId: string }>("git/didChange", (p) => {
      if (p.workspaceId !== this.wsId) return;
      this.heads.clear();
      this.refresher.schedule();
    });
  }

  private get wsId(): string {
    return this.workspace.getSnapshot().id;
  }

  async refresh(): Promise<void> {
    if (!this.wsId) return;
    const seq = ++this.seq;
    try {
      const status = await this.client.request<GitStatus>("git/status", { workspaceId: this.wsId });
      if (seq !== this.seq) return;
      this.set({ status, ...marksOf(status) });
    } catch {
      /* an older daemon, or git failed: nothing to show */
    }
  }

  /** The file's text at HEAD (null: not in HEAD, binary, or no repository). Cached until the
   * repository changes. */
  headText(path: string): Promise<string | null> {
    let p = this.heads.get(path);
    if (!p) {
      p = this.state.status?.repo
        ? this.client
            .request<ShowResult>("git/show", { workspaceId: this.wsId, path, rev: "head" })
            .then((r) => (r.binary ? null : (r.text ?? null)))
            .catch(() => null)
        : Promise.resolve(null);
      this.heads.set(path, p);
    }
    return p;
  }

  /** Shows the diff of a file (HEAD ↔ working tree) in the editor area. */
  openDiff(path: string): void {
    this.set({ diff: path });
  }

  closeDiff(): void {
    if (this.state.diff !== null) this.set({ diff: null });
  }

  /** Number of changed files. */
  count(): number {
    return this.state.status?.files?.length ?? 0;
  }
}
