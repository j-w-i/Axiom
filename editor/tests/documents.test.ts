import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  CURSOR_INTERVAL_MS,
  DocumentStore,
  inferLanguageId,
  SNAPSHOT_INTERVAL_MS,
  type LanguageInfo,
  type ModelLike,
} from "../src/documents";
import type { HostSlots } from "../src/bridge";

/** A text model with undo/redo and Monaco's alternative-version-id semantics:
 * undoing back to an earlier state restores that state's alternative id. */
class FakeModel implements ModelLike {
  disposed = false;
  private text: string;
  private nextVersion = 1;
  private altId: number;
  private undoStack: { text: string; altId: number }[] = [];
  private redoStack: { text: string; altId: number }[] = [];
  private listeners = new Set<() => void>();

  constructor(text: string) {
    this.text = text;
    this.altId = this.nextVersion++;
  }
  getValue() {
    return this.text;
  }
  /** `setValue` is not undoable in Monaco: it resets the undo stack. */
  setValue(text: string) {
    this.text = text;
    this.undoStack = [];
    this.redoStack = [];
    this.altId = this.nextVersion++;
    this.fire();
  }
  getAlternativeVersionId() {
    return this.altId;
  }
  onDidChangeContent(l: () => void) {
    this.listeners.add(l);
    return { dispose: () => this.listeners.delete(l) };
  }
  dispose() {
    this.disposed = true;
  }
  // test helpers
  type(s: string) {
    this.undoStack.push({ text: this.text, altId: this.altId });
    this.redoStack = [];
    this.text += s;
    this.altId = this.nextVersion++;
    this.fire();
  }
  undo() {
    const prev = this.undoStack.pop();
    if (!prev) return;
    this.redoStack.push({ text: this.text, altId: this.altId });
    this.text = prev.text;
    this.altId = prev.altId;
    this.fire();
  }
  redo() {
    const next = this.redoStack.pop();
    if (!next) return;
    this.undoStack.push({ text: this.text, altId: this.altId });
    this.text = next.text;
    this.altId = next.altId;
    this.fire();
  }
  private fire() {
    for (const l of [...this.listeners]) l();
  }
}

function setup() {
  const host = {
    saveRequested: vi.fn<HostSlots["saveRequested"]>(),
    dirtyChanged: vi.fn<HostSlots["dirtyChanged"]>(),
    cursorChanged: vi.fn<HostSlots["cursorChanged"]>(),
    textSnapshot: vi.fn<HostSlots["textSnapshot"]>(),
  };
  const models = new Map<string, FakeModel>();
  const store = new DocumentStore(host, (path, text) => {
    const m = new FakeModel(text);
    models.set(path, m);
    return m;
  });
  return { host, store, models };
}

const P = "src/a.rs";

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
});
afterEach(() => {
  vi.useRealTimers();
});

describe("dirty tracking", () => {
  it("open → clean, reported once", () => {
    const { host, store } = setup();
    store.open({ path: P, text: "hello", dirty: false });
    expect(store.isDirty(P)).toBe(false);
    expect(host.dirtyChanged).toHaveBeenCalledTimes(1);
    expect(host.dirtyChanged).toHaveBeenLastCalledWith({ path: P, dirty: false });
  });

  it("edit → dirtyChanged(true) once; further edits do not repeat it", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "hello", dirty: false });
    host.dirtyChanged.mockClear();
    models.get(P)!.type("!");
    models.get(P)!.type("!");
    expect(host.dirtyChanged).toHaveBeenCalledTimes(1);
    expect(host.dirtyChanged).toHaveBeenCalledWith({ path: P, dirty: true });
    expect(store.isDirty(P)).toBe(true);
  });

  it("undo back to the saved state → dirtyChanged(false); redo → true again", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "hello", dirty: false });
    const m = models.get(P)!;
    m.type("!");
    host.dirtyChanged.mockClear();
    m.undo();
    expect(host.dirtyChanged).toHaveBeenCalledWith({ path: P, dirty: false });
    expect(store.isDirty(P)).toBe(false);
    m.redo();
    expect(host.dirtyChanged).toHaveBeenLastCalledWith({ path: P, dirty: true });
  });

  it("saveRequested carries the current text and the path", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "hello", dirty: false });
    models.get(P)!.type(" world");
    expect(store.requestSave(P)).toBe(true);
    expect(host.saveRequested).toHaveBeenCalledTimes(1);
    expect(host.saveRequested).toHaveBeenCalledWith({ path: P, text: "hello world" });
  });

  it("documentSaved makes the saved content clean", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    models.get(P)!.type("b");
    store.requestSave(P);
    host.dirtyChanged.mockClear();
    store.documentSaved(P);
    expect(store.isDirty(P)).toBe(false);
    expect(host.dirtyChanged).toHaveBeenCalledWith({ path: P, dirty: false });
  });

  it("typing during a save keeps the document dirty after documentSaved (pendingId rule)", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    const m = models.get(P)!;
    m.type("b");
    store.requestSave(P);
    m.type("c"); // typed while the save is in flight
    host.dirtyChanged.mockClear();
    store.documentSaved(P);
    expect(store.isDirty(P)).toBe(true);
    expect(host.dirtyChanged).not.toHaveBeenCalled();
    // Undoing the extra typing lands exactly on the saved content: clean.
    m.undo();
    expect(store.isDirty(P)).toBe(false);
    expect(host.dirtyChanged).toHaveBeenLastCalledWith({ path: P, dirty: false });
  });

  it("a second save trigger while one is pending is ignored", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    models.get(P)!.type("b");
    expect(store.requestSave(P)).toBe(true);
    expect(store.requestSave(P)).toBe(false);
    expect(store.requestSave(P)).toBe(false);
    expect(host.saveRequested).toHaveBeenCalledTimes(1);
    store.documentSaved(P);
    models.get(P)!.type("c");
    expect(store.requestSave(P)).toBe(true);
    expect(host.saveRequested).toHaveBeenCalledTimes(2);
  });

  it("saveFailed clears the pending save and keeps the document dirty", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    models.get(P)!.type("b");
    store.requestSave(P);
    host.dirtyChanged.mockClear();
    store.saveFailed(P);
    expect(store.isDirty(P)).toBe(true);
    expect(host.dirtyChanged).not.toHaveBeenCalled();
    expect(store.requestSave(P)).toBe(true); // saving works again
    expect(host.saveRequested).toHaveBeenCalledTimes(2);
  });

  it("documentSaved / saveFailed without a pending save are ignored", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    models.get(P)!.type("b");
    host.dirtyChanged.mockClear();
    store.documentSaved(P);
    store.saveFailed(P);
    store.documentSaved("unknown/path");
    expect(store.isDirty(P)).toBe(true);
    expect(host.dirtyChanged).not.toHaveBeenCalled();
  });

  it("open with dirty:true stays dirty across undo and becomes clean after a successful save", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "recovered", dirty: true });
    expect(store.isDirty(P)).toBe(true);
    expect(host.dirtyChanged).toHaveBeenLastCalledWith({ path: P, dirty: true });
    const m = models.get(P)!;
    m.type("x");
    m.undo(); // back to the opened text, but the disk text is unknown
    expect(store.isDirty(P)).toBe(true);
    store.requestSave(P);
    store.documentSaved(P);
    expect(store.isDirty(P)).toBe(false);
    expect(host.dirtyChanged).toHaveBeenLastCalledWith({ path: P, dirty: false });
  });

  it("re-openDocument of an existing path replaces content and marks clean", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "v1", dirty: false });
    const m = models.get(P)!;
    m.type("-edited");
    expect(store.isDirty(P)).toBe(true);
    host.dirtyChanged.mockClear();
    store.open({ path: P, text: "v2 from disk", dirty: false });
    expect(models.size).toBe(1); // same model, not a new one
    expect(m.getValue()).toBe("v2 from disk");
    expect(store.isDirty(P)).toBe(false);
    expect(host.dirtyChanged).toHaveBeenCalledTimes(1);
    expect(host.dirtyChanged).toHaveBeenCalledWith({ path: P, dirty: false });
  });

  it("re-open reports once even when the state does not change, and clears a pending save", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "v1", dirty: false });
    models.get(P)!.type("x");
    store.requestSave(P);
    host.dirtyChanged.mockClear();
    store.open({ path: P, text: "v1", dirty: false });
    expect(host.dirtyChanged).toHaveBeenCalledTimes(1);
    models.get(P)!.type("y");
    expect(store.requestSave(P)).toBe(true); // pendingId was cleared by open
  });

  it("closeDocument disposes the model and drops pending state", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    const m = models.get(P)!;
    m.type("b");
    store.requestSave(P);
    store.close(P);
    expect(m.disposed).toBe(true);
    expect(store.has(P)).toBe(false);
    expect(store.requestSave(P)).toBe(false);
    store.documentSaved(P); // late answer for a closed doc: ignored
    host.dirtyChanged.mockClear();
    m.type("zzz"); // no longer observed
    expect(host.dirtyChanged).not.toHaveBeenCalled();
    store.close(P); // idempotent
    // Reopening starts fresh with no pending save.
    store.open({ path: P, text: "again", dirty: false });
    expect(store.isDirty(P)).toBe(false);
  });

  it("documents are independent", () => {
    const { host, store, models } = setup();
    store.open({ path: "a", text: "1", dirty: false });
    store.open({ path: "b", text: "2", dirty: false });
    models.get("a")!.type("x");
    expect(store.isDirty("a")).toBe(true);
    expect(store.isDirty("b")).toBe(false);
    store.requestSave("a");
    expect(store.requestSave("b")).toBe(true);
    expect(host.saveRequested).toHaveBeenCalledTimes(2);
  });
});

describe("snapshot throttling", () => {
  it("sends at most one textSnapshot per second per dirty document, latest text last", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    const m = models.get(P)!;
    m.type("1"); // leading edge: sent immediately
    expect(host.textSnapshot).toHaveBeenCalledTimes(1);
    expect(host.textSnapshot).toHaveBeenLastCalledWith({ path: P, text: "a1" });
    m.type("2");
    m.type("3");
    vi.advanceTimersByTime(SNAPSHOT_INTERVAL_MS - 1);
    expect(host.textSnapshot).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(1);
    expect(host.textSnapshot).toHaveBeenCalledTimes(2);
    expect(host.textSnapshot).toHaveBeenLastCalledWith({ path: P, text: "a123" });
    // Nothing more without further edits.
    vi.advanceTimersByTime(10_000);
    expect(host.textSnapshot).toHaveBeenCalledTimes(2);
  });

  it("is per document", () => {
    const { host, store, models } = setup();
    store.open({ path: "a", text: "", dirty: false });
    store.open({ path: "b", text: "", dirty: false });
    models.get("a")!.type("x");
    models.get("b")!.type("y");
    expect(host.textSnapshot).toHaveBeenCalledTimes(2);
  });

  it("does not send snapshots for clean documents, and cancels a pending one when the doc becomes clean", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    const m = models.get(P)!;
    m.type("1");
    m.type("2"); // trailing snapshot is now queued
    host.textSnapshot.mockClear();
    store.requestSave(P);
    store.documentSaved(P); // clean
    vi.advanceTimersByTime(5000);
    expect(host.textSnapshot).not.toHaveBeenCalled();
  });

  it("an open with replaced content does not itself produce a snapshot", () => {
    const { host, store } = setup();
    store.open({ path: P, text: "a", dirty: false });
    store.open({ path: P, text: "b", dirty: false });
    vi.advanceTimersByTime(5000);
    expect(host.textSnapshot).not.toHaveBeenCalled();
  });

  it("closing cancels a queued snapshot", () => {
    const { host, store, models } = setup();
    store.open({ path: P, text: "a", dirty: false });
    models.get(P)!.type("1");
    models.get(P)!.type("2");
    host.textSnapshot.mockClear();
    store.close(P);
    vi.advanceTimersByTime(5000);
    expect(host.textSnapshot).not.toHaveBeenCalled();
  });
});

describe("cursor throttling", () => {
  it("sends at most one cursorChanged per 50 ms, latest position last", () => {
    const { host, store } = setup();
    store.open({ path: P, text: "abc", dirty: false });
    store.cursorMoved(P, 1, 1, 0);
    expect(host.cursorChanged).toHaveBeenCalledTimes(1);
    store.cursorMoved(P, 1, 2, 0);
    store.cursorMoved(P, 1, 3, 2);
    expect(host.cursorChanged).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(CURSOR_INTERVAL_MS);
    expect(host.cursorChanged).toHaveBeenCalledTimes(2);
    expect(host.cursorChanged).toHaveBeenLastCalledWith({
      path: P,
      line: 1,
      column: 3,
      selectionLength: 2,
    });
    // After a quiet period the next move goes out immediately again.
    vi.advanceTimersByTime(500);
    store.cursorMoved(P, 2, 1, 0);
    expect(host.cursorChanged).toHaveBeenCalledTimes(3);
  });

  it("ignores cursor moves for unknown documents", () => {
    const { host, store } = setup();
    store.cursorMoved("nope", 1, 1, 0);
    expect(host.cursorChanged).not.toHaveBeenCalled();
  });
});

describe("language inference", () => {
  const langs: LanguageInfo[] = [
    { id: "rust", extensions: [".rs", ".rlib"] },
    { id: "typescript", extensions: [".ts", ".tsx", ".mts", ".cts"] },
    { id: "json", extensions: [".json", ".bowerrc"] },
    { id: "makefile", extensions: [".mk"], filenames: ["Makefile", "makefile", "GNUmakefile"] },
    { id: "ini", extensions: [".ini"], filenames: ["config", ".gitconfig"] },
    { id: "dockerfile", extensions: [".dockerfile"], filenames: ["Dockerfile"] },
  ];

  it.each([
    ["src/main.rs", "rust"],
    ["a/b/c.ts", "typescript"],
    ["x.tsx", "typescript"],
    ["package.json", "json"],
    ["Makefile", "makefile"],
    ["sub/dir/Makefile", "makefile"],
    ["sub/GNUmakefile", "makefile"],
    ["README", "plaintext"],
    ["notes.unknownext", "plaintext"],
    ["", "plaintext"],
    [".rs", "plaintext"], // an extension alone is a hidden file name, not a Rust file
    ["UPPER.RS", "rust"],
    ["Dockerfile", "dockerfile"],
  ])("%s → %s", (path, id) => {
    expect(inferLanguageId(path, langs)).toBe(id);
  });
});
