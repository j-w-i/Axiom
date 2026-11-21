// Change markers in the editor's gutter (M4-GIT-002): each workspace file's buffer against its
// HEAD text (a line diff, off the keystroke path: recomputed 300 ms after typing stops), as
// added / modified / deleted marks beside the line numbers. A file not in HEAD (new, untracked)
// is all added; a file without a repository has none.

import * as monaco from "monaco-editor";
import type { GitStore } from "../../state/git";
import { lineChanges } from "./linediff";

type Model = monaco.editor.ITextModel;

function modelPath(m: Model): string | null {
  if (m.uri.scheme !== "file" || m.uri.authority) return null;
  const p = m.uri.path.replace(/^\//, "");
  return p && !p.startsWith("/") ? p : null;
}

/** The overview ruler's mark colour, from the tokens (--ok, --warn, --danger), at 60%. */
function rulerColor(kind: "added" | "modified" | "deleted"): string {
  const token = kind === "added" ? "--ok" : kind === "deleted" ? "--danger" : "--warn";
  const v = getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  return /^#[0-9a-f]{6}$/i.test(v) ? `${v}99` : v;
}

export class EditorGit {
  private readonly tracked = new Map<Model, { path: string; ids: string[]; timer: ReturnType<typeof setTimeout> | null; sub: monaco.IDisposable }>();
  private readonly disposables: monaco.IDisposable[] = [];
  private readonly unsub: () => void;
  private lastStatus: unknown = null;

  constructor(private readonly git: GitStore) {
    this.disposables.push(
      monaco.editor.onDidCreateModel((m) => this.track(m)),
      monaco.editor.onWillDisposeModel((m) => this.untrack(m)),
    );
    for (const m of monaco.editor.getModels()) this.track(m);
    // The repository changed (a commit, a checkout): every file's base may have.
    this.unsub = git.subscribe(() => {
      const st = git.getSnapshot().status;
      if (st === this.lastStatus) return;
      this.lastStatus = st;
      for (const m of this.tracked.keys()) this.schedule(m, 0);
    });
  }

  dispose(): void {
    this.unsub();
    for (const d of this.disposables) d.dispose();
    for (const m of [...this.tracked.keys()]) this.untrack(m);
  }

  private track(m: Model): void {
    const path = modelPath(m);
    if (!path || this.tracked.has(m)) return;
    const sub = m.onDidChangeContent(() => this.schedule(m, 300));
    this.tracked.set(m, { path, ids: [], timer: null, sub });
    this.schedule(m, 0);
  }

  private untrack(m: Model): void {
    const t = this.tracked.get(m);
    if (!t) return;
    if (t.timer) clearTimeout(t.timer);
    t.sub.dispose();
    this.tracked.delete(m);
  }

  private schedule(m: Model, ms: number): void {
    const t = this.tracked.get(m);
    if (!t) return;
    if (t.timer) clearTimeout(t.timer);
    t.timer = setTimeout(() => void this.update(m), ms);
  }

  private async update(m: Model): Promise<void> {
    const t = this.tracked.get(m);
    if (!t || m.isDisposed()) return;
    t.timer = null;
    const st = this.git.getSnapshot();
    if (!st.status?.repo) {
      t.ids = m.deltaDecorations(t.ids, []);
      return;
    }
    const mark = st.marks[t.path];
    const head = mark?.kind === "untracked" || mark?.kind === "added" ? "" : await this.git.headText(t.path);
    if (m.isDisposed() || !this.tracked.has(m)) return;
    if (head === null && mark === undefined) {
      // Not in HEAD and not changed: an ignored file (no marks).
      t.ids = m.deltaDecorations(t.ids, []);
      return;
    }
    const changes = lineChanges(head ?? "", m.getValue());
    t.ids = m.deltaDecorations(
      t.ids,
      changes.map((c) => ({
        range: new monaco.Range(c.start, 1, c.end, 1),
        options: {
          isWholeLine: true,
          linesDecorationsClassName: `git-gutter git-gutter--${c.kind}`,
          overviewRuler: { color: rulerColor(c.kind), position: monaco.editor.OverviewRulerLane.Left },
        },
      })),
    );
  }
}
