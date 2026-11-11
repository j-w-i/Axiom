// Document bookkeeping for the editor surface: dirty tracking (savedId /
// pendingId), serialised saves, snapshot and cursor throttling. Pure logic
// over a minimal model interface so it runs without Monaco.

import type { HostSlots, OpenDocumentPayload } from "./bridge";

/** The subset of Monaco's ITextModel the store needs. */
export interface ModelLike {
  getValue(): string;
  setValue(text: string): void;
  getAlternativeVersionId(): number;
  onDidChangeContent(listener: () => void): { dispose(): void };
  dispose(): void;
}

export type ModelFactory = (path: string, text: string) => ModelLike;

export const SNAPSHOT_INTERVAL_MS = 1000;
export const CURSOR_INTERVAL_MS = 50;

/** Leading + trailing throttle: the first call goes out at once, later calls
 * within the interval collapse into one trailing call with the latest value. */
export class Throttle<T> {
  private last = Number.NEGATIVE_INFINITY;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private pending: { value: T } | null = null;

  constructor(
    private readonly intervalMs: number,
    private readonly send: (value: T) => void,
  ) {}

  call(value: T): void {
    const wait = this.last + this.intervalMs - Date.now();
    if (this.timer === null && wait <= 0) {
      this.fire(value);
      return;
    }
    this.pending = { value };
    if (this.timer === null) {
      this.timer = setTimeout(() => {
        this.timer = null;
        const p = this.pending;
        this.pending = null;
        if (p) this.fire(p.value);
      }, Math.max(0, wait));
    }
  }

  cancel(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.pending = null;
  }

  private fire(value: T): void {
    this.last = Date.now();
    this.send(value);
  }
}

interface Doc {
  path: string;
  model: ModelLike;
  /** Alternative version id of the saved content; null = dirty until saved. */
  savedId: number | null;
  /** Alternative version id captured by the in-flight save, if any. */
  pendingId: number | null;
  /** Last dirty value reported to the shell. */
  reportedDirty: boolean | null;
  /** Suppresses change handling while the store itself replaces content. */
  muted: boolean;
  snapshots: Throttle<string>;
  subscription: { dispose(): void };
}

export class DocumentStore {
  private readonly docs = new Map<string, Doc>();
  private readonly cursor: Throttle<{
    path: string;
    line: number;
    column: number;
    selectionLength: number;
  }>;

  constructor(
    private readonly host: Pick<
      HostSlots,
      "saveRequested" | "dirtyChanged" | "cursorChanged" | "textSnapshot"
    >,
    private readonly createModel: ModelFactory,
  ) {
    this.cursor = new Throttle(CURSOR_INTERVAL_MS, (p) => this.host.cursorChanged(p));
  }

  has(path: string): boolean {
    return this.docs.has(path);
  }

  paths(): string[] {
    return [...this.docs.keys()];
  }

  model(path: string): ModelLike | undefined {
    return this.docs.get(path)?.model;
  }

  isDirty(path: string): boolean {
    const d = this.docs.get(path);
    return d ? this.computeDirty(d) : false;
  }

  /** `openDocument`: create the model, or replace the content of an existing one. */
  open(p: Pick<OpenDocumentPayload, "path" | "text" | "dirty">): ModelLike {
    let doc = this.docs.get(p.path);
    if (doc) {
      doc.snapshots.cancel();
      doc.muted = true;
      try {
        doc.model.setValue(p.text);
      } finally {
        doc.muted = false;
      }
    } else {
      const model = this.createModel(p.path, p.text);
      doc = {
        path: p.path,
        model,
        savedId: null,
        pendingId: null,
        reportedDirty: null,
        muted: false,
        snapshots: new Throttle(SNAPSHOT_INTERVAL_MS, (text) =>
          this.host.textSnapshot({ path: p.path, text }),
        ),
        subscription: { dispose() {} },
      };
      const created = doc;
      created.subscription = model.onDidChangeContent(() => this.onContentChanged(created));
      this.docs.set(p.path, created);
    }
    doc.pendingId = null;
    doc.savedId = p.dirty ? null : doc.model.getAlternativeVersionId();
    // Always report once after an open, even if the value did not change.
    doc.reportedDirty = null;
    this.report(doc);
    return doc.model;
  }

  /** `closeDocument`: dispose the model and forget the document. */
  close(path: string): void {
    const doc = this.docs.get(path);
    if (!doc) return;
    doc.snapshots.cancel();
    doc.subscription.dispose();
    doc.model.dispose();
    this.docs.delete(path);
  }

  /** Ctrl/Cmd+S in the editor, or the shell's `requestSave`. Returns whether a save was sent. */
  requestSave(path: string): boolean {
    const doc = this.docs.get(path);
    if (!doc || doc.pendingId !== null) return false;
    doc.pendingId = doc.model.getAlternativeVersionId();
    this.host.saveRequested({ path, text: doc.model.getValue() });
    return true;
  }

  /** `documentSaved`: the content sent in the pending save is on disk. */
  documentSaved(path: string): void {
    const doc = this.docs.get(path);
    if (!doc || doc.pendingId === null) return;
    doc.savedId = doc.pendingId;
    doc.pendingId = null;
    this.report(doc);
  }

  /** `saveFailed`: clear the pending save; the document stays as it was. */
  saveFailed(path: string): void {
    const doc = this.docs.get(path);
    if (doc) doc.pendingId = null;
  }

  /** Called by the editor on cursor/selection moves; throttled to 50 ms. */
  cursorMoved(path: string, line: number, column: number, selectionLength: number): void {
    if (!this.docs.has(path)) return;
    this.cursor.call({ path, line, column, selectionLength });
  }

  dispose(): void {
    this.cursor.cancel();
    for (const path of [...this.docs.keys()]) this.close(path);
  }

  private computeDirty(d: Doc): boolean {
    return d.savedId === null || d.model.getAlternativeVersionId() !== d.savedId;
  }

  private report(d: Doc): void {
    const dirty = this.computeDirty(d);
    if (d.reportedDirty !== dirty) {
      d.reportedDirty = dirty;
      this.host.dirtyChanged({ path: d.path, dirty });
    }
    if (!dirty) d.snapshots.cancel();
  }

  private onContentChanged(d: Doc): void {
    if (d.muted) return;
    this.report(d);
    if (this.computeDirty(d)) d.snapshots.call(d.model.getValue());
  }
}

// ---- Language inference -------------------------------------------------

export interface LanguageInfo {
  id: string;
  extensions?: readonly string[];
  filenames?: readonly string[];
}

/** Picks a language id from the file name: exact filename first, then the
 * longest matching extension; falls back to `plaintext`. */
export function inferLanguageId(path: string, languages: readonly LanguageInfo[]): string {
  const name = path.slice(path.lastIndexOf("/") + 1);
  const lower = name.toLowerCase();
  for (const l of languages) {
    if (l.filenames?.some((f) => f.toLowerCase() === lower)) return l.id;
  }
  let best: { id: string; len: number } | null = null;
  for (const l of languages) {
    for (const ext of l.extensions ?? []) {
      if (lower.endsWith(ext.toLowerCase()) && lower.length > ext.length) {
        if (!best || ext.length > best.len) best = { id: l.id, len: ext.length };
      }
    }
  }
  return best ? best.id : "plaintext";
}
