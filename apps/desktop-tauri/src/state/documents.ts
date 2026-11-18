// Port of OpenDocumentsModel (apps/desktop/src/models/OpenDocumentsModel.cpp).
//
// The editor surface still owns unsaved text, the daemon owns the file on
// disk, and this store owns which documents are open and their saved
// `version` (data-flow.md §Open, edit and save). It talks to the editor only
// through the editor-bridge-1 message set (`EditorPort` = the bridge's
// shell→page signals; `EditorListener` = its page→shell slots), so the seam
// of ADR-0004 survives even though both now live in one webview.

import type {
  ConfigurePayload,
  CursorChangedPayload,
  DirtyChangedPayload,
  DocumentSavedPayload,
  EditorCommand,
  OpenDocumentPayload,
  PathPayload,
  ReadyPayload,
  RevealPositionPayload,
  SaveRequestedPayload,
  TextSnapshotPayload,
} from "@editor/bridge";
import type { DaemonClient } from "../daemon/client";
import { RpcError, type DidChangeParams, type ReadFileResult } from "../daemon/types";
import { Debounce, Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export const OPEN_PATHS_KEY = "editor.openPaths";
export const ACTIVE_PATH_KEY = "editor.activePath";
const TAB_SAVE_DEBOUNCE_MS = 500;
const BRIDGE_VERSION = 1;

/** Shell → editor (bridge-1 signals). */
export interface EditorPort {
  configure(p: ConfigurePayload): void;
  openDocument(p: OpenDocumentPayload): void;
  activateDocument(p: PathPayload): void;
  closeDocument(p: PathPayload): void;
  requestSave(p: PathPayload): void;
  documentSaved(p: DocumentSavedPayload): void;
  saveFailed(p: PathPayload): void;
  revealPosition(p: RevealPositionPayload): void;
  runCommand(p: { command: EditorCommand }): void;
}

/**
 * Where unsaved text is kept so it survives the webview process (T-08). The
 * page holds a copy for the editor surface; the host holds the durable one,
 * because a dead webview takes the page's copy with it. Defaults to a no-op so
 * the store runs headless in tests.
 */
export interface RecoveryPort {
  snapshotPut(path: string, text: string): void;
  snapshotClear(path: string): void;
  state(): Promise<{ recovered: boolean; snapshots: Record<string, string> }>;
}

/** Editor → shell (bridge-1 slots). */
export interface EditorListener {
  ready(p: ReadyPayload): void;
  saveRequested(p: SaveRequestedPayload): void;
  dirtyChanged(p: DirtyChangedPayload): void;
  cursorChanged(p: CursorChangedPayload): void;
  textSnapshot(p: TextSnapshotPayload): void;
  openLink(p: { url: string }): void;
  reportError(p: { message: string; detail: string }): void;
}

export interface Doc {
  path: string;
  title: string;
  dirty: boolean;
  savedVersion: string;
  changedOnDisk: boolean;
}

export interface Prompt {
  kind: "close" | "folder" | "quit";
  names: string[];
}

export interface DocumentsState {
  docs: Doc[];
  activePath: string;
  cursorText: string;
  editorReady: boolean;
  banner: string;
  conflictName: string | null;
  prompt: Prompt | null;
}

function titleOf(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

export class DocumentsStore extends Observable<DocumentsState> implements EditorListener {
  private port: EditorPort | null = null;
  private appearance: ConfigurePayload = { theme: "dark", fontSize: 13, fontFamily: "", wordWrap: false };
  private readonly snapshots = new Map<string, string>();
  private readonly opening = new Set<string>();
  private conflicts: { path: string; text: string }[] = [];
  private proceed: (() => void) | null = null;
  private promptPaths: string[] = [];
  private awaitingSave = new Set<string>();
  private afterSave: (() => void) | null = null;
  private generation = 0;
  private restoring = false;
  private readonly tabSave = new Debounce(TAB_SAVE_DEBOUNCE_MS, () => void this.saveTabState());
  /** Hook for opening external links (host service). */
  openExternal: (url: string) => void = () => {};
  /** Keeps unsaved text where it survives a webview crash (T-08). */
  recovery: RecoveryPort = {
    snapshotPut: () => {},
    snapshotClear: () => {},
    state: async () => ({ recovered: false, snapshots: {} }),
  };
  /** Called when a quit has been confirmed (no unsaved work, or the user chose). */
  onQuitConfirmed: () => void = () => {};

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super({
      docs: [], activePath: "", cursorText: "", editorReady: false,
      banner: "", conflictName: null, prompt: null,
    });
    client.onNotification<DidChangeParams>("fs/didChange", (p) => this.onDidChange(p));
    // The workspace id going away means "closed" *or* "switching to another
    // folder"; both must drop the old documents (the Qt model did this on
    // openChanged, not only on closed).
    let wasOpen = workspace.isOpen;
    workspace.subscribe(() => {
      const open = workspace.isOpen;
      if (wasOpen && !open) {
        ++this.generation;
        this.tabSave.stop();
        this.clearAll(true);
      }
      wasOpen = open;
    });
    workspace.on("opened", () => void this.restoreTabs());
  }

  // ---- editor attachment -----------------------------------------------------

  attachEditor(port: EditorPort): void {
    this.port = port;
  }

  /** The editor surface went away (unmounted, crashed, reloading). */
  detachEditor(): void {
    this.port = null;
    this.set({ editorReady: false });
  }

  private get editorUp(): boolean {
    return this.state.editorReady && this.port !== null;
  }

  // ---- helpers ---------------------------------------------------------------

  private get wsId(): string {
    return this.workspace.getSnapshot().id;
  }

  private index(path: string): number {
    return this.state.docs.findIndex((d) => d.path === path);
  }

  private updateDoc(path: string, patch: Partial<Doc>): void {
    const docs = this.state.docs.map((d) => (d.path === path ? { ...d, ...patch } : d));
    this.set({ docs });
  }

  get hasDirty(): boolean {
    return this.state.docs.some((d) => d.dirty);
  }

  private dirtyPaths(): string[] {
    return this.state.docs.filter((d) => d.dirty).map((d) => d.path);
  }

  dismissBanner(): void {
    this.set({ banner: "" });
  }

  showBanner(text: string): void {
    this.set({ banner: text });
  }

  setAppearance(c: ConfigurePayload): void {
    this.appearance = c;
    if (this.editorUp) this.port?.configure(c);
  }

  // ---- open / activate / close ---------------------------------------------

  open(path: string): void {
    if (this.index(path) >= 0) {
      this.activate(path);
      return;
    }
    void this.openInternal(path, false);
  }

  /** Opens (or activates) a document and puts the cursor at 1-based line and column, centred
   * (bridge-1 `revealPosition`, which also activates). Used by the Problems view. */
  openAt(path: string, line: number, column: number): void {
    const reveal = () => {
      if (this.index(path) < 0) return;
      this.activate(path);
      if (this.editorUp) this.port?.revealPosition({ path, line, column });
    };
    if (this.index(path) >= 0) return reveal();
    void this.openInternal(path, false).then(reveal);
  }

  private async openInternal(path: string, silent: boolean): Promise<void> {
    if (!this.workspace.isOpen || this.opening.has(path)) return;
    this.opening.add(path);
    const gen = this.generation;
    try {
      const r = await this.client.request<ReadFileResult>("fs/readFile", { workspaceId: this.wsId, path });
      if (gen !== this.generation) return;
      this.addDocument(path, r.text, r.version);
    } catch (e) {
      if (gen !== this.generation || silent) return;
      const err = e as RpcError;
      const reason =
        err.kind === "unsupported_encoding" ? "it is not a UTF-8 text file"
        : err.kind === "too_large" ? "it is too large (over 16 MiB)"
        : err.message;
      this.set({ banner: `Cannot open ${titleOf(path)}: ${reason}` });
    } finally {
      this.opening.delete(path);
    }
  }

  private addDocument(path: string, text: string, version: string): void {
    if (this.index(path) >= 0) return;
    const doc: Doc = { path, title: titleOf(path), dirty: false, savedVersion: version, changedOnDisk: false };
    const activate = this.state.activePath === "" || !this.restoring;
    this.set({ docs: [...this.state.docs, doc], banner: "" });
    if (this.editorUp) this.port?.openDocument({ path, text, version, activate, dirty: false });
    if (activate || !this.state.activePath) this.setActive(path);
    this.scheduleTabSave();
  }

  private setActive(path: string): void {
    if (this.state.activePath === path) return;
    this.set({ activePath: path, cursorText: "" });
    if (this.editorUp && path) this.port?.activateDocument({ path });
    this.scheduleTabSave();
  }

  activate(path: string): void {
    if (this.index(path) >= 0) this.setActive(path);
  }

  cycle(delta: number): void {
    const n = this.state.docs.length;
    if (n < 2) return;
    const i = Math.max(0, this.index(this.state.activePath));
    const next = this.state.docs[(((i + delta) % n) + n) % n];
    if (next) this.setActive(next.path);
  }

  /** Moves a tab to `toIndex` (clamped) in the tab order; saved with the tab state (D7, INTERACTION-004). */
  move(path: string, toIndex: number): void {
    const from = this.index(path);
    if (from < 0) return;
    const to = Math.max(0, Math.min(this.state.docs.length - 1, Math.round(toIndex)));
    if (to === from) return;
    const docs = [...this.state.docs];
    const [d] = docs.splice(from, 1);
    docs.splice(to, 0, d!);
    this.set({ docs });
    this.scheduleTabSave();
  }

  close(path: string): void {
    const d = this.state.docs.find((x) => x.path === path);
    if (!d) return;
    if (d.dirty) this.guarded("close", [path], () => this.closeNow(path));
    else this.closeNow(path);
  }

  private closeNow(path: string): void {
    const i = this.index(path);
    if (i < 0) return;
    const wasActive = this.state.activePath === path;
    const docs = this.state.docs.filter((d) => d.path !== path);
    this.awaitingSave.delete(path);
    this.snapshots.delete(path);
    this.recovery.snapshotClear(path);
    this.conflicts = this.conflicts.filter((c) => c.path !== path);
    this.set({ docs, conflictName: this.conflicts[0] ? titleOf(this.conflicts[0].path) : null });
    if (this.editorUp) this.port?.closeDocument({ path });
    if (wasActive) {
      this.set({ activePath: "", cursorText: "" });
      const next = docs[Math.min(i, docs.length - 1)];
      if (next) this.setActive(next.path);
    }
    this.scheduleTabSave();
  }

  private clearAll(notifyPage: boolean): void {
    for (const d of this.state.docs) this.recovery.snapshotClear(d.path);
    if (notifyPage && this.editorUp) for (const d of this.state.docs) this.port?.closeDocument({ path: d.path });
    this.awaitingSave.clear();
    this.conflicts = [];
    this.opening.clear();
    this.snapshots.clear();
    this.proceed = null;
    this.set({ docs: [], activePath: "", cursorText: "", conflictName: null, prompt: null });
  }

  // ---- editor commands -------------------------------------------------------

  runCommand(command: EditorCommand): void {
    if (this.editorUp && this.state.activePath) this.port?.runCommand({ command });
  }

  save(path: string): void {
    if (this.index(path) >= 0 && this.editorUp) this.port?.requestSave({ path });
  }

  saveAll(): void {
    for (const d of this.state.docs) if (d.dirty) this.save(d.path);
  }

  // ---- bridge slots (editor → shell) ------------------------------------------

  ready(p: ReadyPayload): void {
    if (p.bridgeVersion !== BRIDGE_VERSION) {
      this.set({ banner: `The editor speaks bridge version ${p.bridgeVersion} but this build needs ${BRIDGE_VERSION}.` });
      return;
    }
    this.set({ editorReady: true });
    const port = this.port;
    if (!port) return;
    port.configure(this.appearance);
    for (const d of this.state.docs) {
      const snap = this.snapshots.get(d.path);
      if (snap !== undefined) {
        // After a renderer reload: unsaved text comes back from the last snapshot, marked dirty.
        port.openDocument({ path: d.path, text: snap, version: d.savedVersion, activate: false, dirty: true });
      } else {
        void this.reloadFromDisk(d.path);
      }
    }
    if (this.state.activePath) port.activateDocument({ path: this.state.activePath });
  }

  saveRequested(p: SaveRequestedPayload): void {
    if (this.index(p.path) < 0) return; // not open: ignored (bridge §Failure modes)
    void this.writeFile(p.path, p.text, false);
  }

  dirtyChanged(p: DirtyChangedPayload): void {
    const d = this.state.docs.find((x) => x.path === p.path);
    if (!d || d.dirty === p.dirty) return;
    if (!p.dirty) {
      this.snapshots.delete(p.path);
      this.recovery.snapshotClear(p.path); // the saved state is on disk: nothing to recover
    }
    this.updateDoc(p.path, { dirty: p.dirty });
  }

  cursorChanged(p: CursorChangedPayload): void {
    if (p.path !== this.state.activePath) return;
    this.set({
      cursorText: p.selectionLength > 0
        ? `Ln ${p.line}, Col ${p.column} (${p.selectionLength} selected)`
        : `Ln ${p.line}, Col ${p.column}`,
    });
  }

  textSnapshot(p: TextSnapshotPayload): void {
    if (this.index(p.path) < 0) return;
    this.snapshots.set(p.path, p.text);
    // The host's copy is the one that survives the page (T-08). Memory only: it
    // is never written to disk and never used to overwrite the file by itself.
    this.recovery.snapshotPut(p.path, p.text);
  }

  openLink(p: { url: string }): void {
    const lower = p.url.toLowerCase();
    if (lower.startsWith("http://") || lower.startsWith("https://")) this.openExternal(p.url);
  }

  reportError(p: { message: string; detail: string }): void {
    console.warn("[editor] page error:", p.message, p.detail);
  }

  // ---- saving ----------------------------------------------------------------

  private async writeFile(path: string, text: string, overwrite: boolean): Promise<void> {
    const d = this.state.docs.find((x) => x.path === path);
    if (!d) return;
    const params: Record<string, unknown> = { workspaceId: this.wsId, path, text };
    if (overwrite) params.overwrite = true;
    else if (d.savedVersion) params.expectedVersion = d.savedVersion;
    const gen = this.generation;
    try {
      const r = await this.client.request<{ version: string }>("fs/writeFile", params);
      if (gen !== this.generation) return;
      this.saveSucceeded(path, r.version);
    } catch (e) {
      if (gen !== this.generation) return;
      const err = e as RpcError;
      if (err.kind === "conflict") {
        this.conflicts.push({ path, text });
        this.set({ conflictName: titleOf(this.conflicts[0]!.path) });
      } else {
        this.set({ banner: `Could not save ${titleOf(path)}: ${err.message}` });
        this.saveFailed(path);
      }
    }
  }

  private saveSucceeded(path: string, version: string): void {
    this.recovery.snapshotClear(path);
    if (this.index(path) >= 0) {
      this.updateDoc(path, { savedVersion: version, changedOnDisk: false });
      if (this.editorUp) this.port?.documentSaved({ path, version });
    }
    this.awaitingSave.delete(path);
    if (this.awaitingSave.size === 0 && this.afterSave) {
      const f = this.afterSave;
      this.afterSave = null;
      f();
    }
  }

  private saveFailed(path: string): void {
    if (this.editorUp && this.index(path) >= 0) this.port?.saveFailed({ path });
    if (this.awaitingSave.has(path)) {
      this.awaitingSave.clear();
      this.afterSave = null;
    }
  }

  resolveConflict(action: "overwrite" | "reload" | "cancel"): void {
    const c = this.conflicts.shift();
    this.set({ conflictName: this.conflicts[0] ? titleOf(this.conflicts[0].path) : null });
    if (!c) return;
    if (action === "overwrite") {
      void this.writeFile(c.path, c.text, true);
    } else if (action === "reload") {
      this.saveFailed(c.path);
      void this.reloadFromDisk(c.path);
    } else {
      this.saveFailed(c.path);
    }
  }

  private async reloadFromDisk(path: string): Promise<void> {
    if (!this.workspace.isOpen) return;
    const gen = this.generation;
    let r: ReadFileResult;
    try {
      r = await this.client.request<ReadFileResult>("fs/readFile", { workspaceId: this.wsId, path });
    } catch {
      return;
    }
    if (gen !== this.generation || this.index(path) < 0) return;
    this.snapshots.delete(path);
    this.updateDoc(path, { savedVersion: r.version, changedOnDisk: false, dirty: false });
    if (this.editorUp) {
      this.port?.openDocument({
        path, text: r.text, version: r.version, activate: path === this.state.activePath, dirty: false,
      });
    }
  }

  // ---- external changes ------------------------------------------------------

  private onDidChange(p: DidChangeParams): void {
    if (p.workspaceId !== this.wsId) return;
    const everything = p.changes.some((c) => c.path === "");
    const changed = new Set(p.changes.map((c) => c.path));
    for (const d of this.state.docs) if (everything || changed.has(d.path)) void this.recheck(d.path);
  }

  /** The version is the arbiter: our own saves and unchanged content cause no action. */
  private async recheck(path: string): Promise<void> {
    if (!this.workspace.isOpen) return;
    const gen = this.generation;
    let r: ReadFileResult | null = null;
    try {
      r = await this.client.request<ReadFileResult>("fs/readFile", { workspaceId: this.wsId, path });
    } catch {
      r = null;
    }
    const d = this.state.docs.find((x) => x.path === path);
    if (gen !== this.generation || !d) return;
    if (!r) {
      if (d.dirty && !d.changedOnDisk) this.updateDoc(path, { changedOnDisk: true });
      return;
    }
    if (r.version === d.savedVersion) return;
    if (!d.dirty) void this.reloadFromDisk(path);
    else if (!d.changedOnDisk) this.updateDoc(path, { changedOnDisk: true });
  }

  // ---- unsaved-work prompts ----------------------------------------------------

  private guarded(kind: Prompt["kind"], dirty: string[], proceed: () => void): void {
    if (dirty.length === 0) {
      proceed();
      return;
    }
    if (this.state.prompt) return; // one question at a time
    this.promptPaths = dirty;
    this.proceed = proceed;
    this.set({ prompt: { kind, names: dirty.map(titleOf) } });
  }

  resolvePrompt(action: "save" | "discard" | "cancel"): void {
    if (!this.state.prompt) return;
    const proceed = this.proceed;
    const paths = this.promptPaths;
    this.proceed = null;
    this.set({ prompt: null });
    if (!proceed) return;
    if (action === "save") {
      if (!this.editorUp) return; // cannot save without the editor: do not lose the request silently
      this.awaitingSave = new Set(paths);
      this.afterSave = proceed;
      for (const p of paths) this.save(p);
    } else if (action === "discard") {
      proceed();
    }
  }

  requestOpenFolder(path: string): void {
    this.guarded("folder", this.dirtyPaths(), () => void this.workspace.openFolder(path));
  }

  requestCloseFolder(): void {
    this.guarded("folder", this.dirtyPaths(), () => void this.workspace.close());
  }

  requestQuit(): void {
    this.guarded("quit", this.dirtyPaths(), () => this.onQuitConfirmed());
  }

  // ---- tab persistence ---------------------------------------------------------

  private scheduleTabSave(): void {
    if (!this.restoring && this.workspace.isOpen) this.tabSave.schedule();
  }

  async saveTabState(): Promise<void> {
    this.tabSave.stop();
    if (!this.workspace.isOpen) return;
    try {
      await this.client.request("workspace/setState", {
        workspaceId: this.wsId,
        values: {
          [OPEN_PATHS_KEY]: this.state.docs.map((d) => d.path),
          [ACTIVE_PATH_KEY]: this.state.activePath || null,
        },
      });
    } catch {
      /* best effort */
    }
  }

  /**
   * Re-opens a document whose unsaved text survived a webview crash. The
   * version on disk is read first so that saving still uses the correct
   * `expectedVersion`: if the file changed while the window was dead, the save
   * conflicts instead of silently overwriting (ipc.md §fs/writeFile).
   */
  private async openRecovered(path: string, text: string): Promise<void> {
    if (this.index(path) >= 0) return;
    let version = "";
    try {
      const r = await this.client.request<ReadFileResult>("fs/readFile", { workspaceId: this.wsId, path });
      version = r.version;
    } catch {
      // Deleted or unreadable: keep the user's text; saving creates the file.
    }
    if (this.index(path) >= 0) return;
    const doc: Doc = { path, title: titleOf(path), dirty: true, savedVersion: version, changedOnDisk: false };
    const activate = this.state.activePath === "" || !this.restoring;
    this.set({ docs: [...this.state.docs, doc] });
    this.recovery.snapshotPut(path, text);
    if (this.editorUp) this.port?.openDocument({ path, text, version, activate, dirty: true });
    if (activate) this.setActive(path);
    this.scheduleTabSave();
  }

  private async restoreTabs(): Promise<void> {
    if (!this.workspace.isOpen) return;
    const gen = this.generation;
    let values: Record<string, unknown>;
    try {
      values = (
        await this.client.request<{ values: Record<string, unknown> }>("workspace/getState", {
          workspaceId: this.wsId,
          keys: [OPEN_PATHS_KEY, ACTIVE_PATH_KEY],
        })
      ).values;
    } catch {
      return;
    }
    if (gen !== this.generation) return;
    const raw = values[OPEN_PATHS_KEY];
    const paths = Array.isArray(raw) ? raw.filter((x): x is string => typeof x === "string") : [];
    const active = typeof values[ACTIVE_PATH_KEY] === "string" ? (values[ACTIVE_PATH_KEY] as string) : "";
    if (paths.length === 0) return;
    // A page that is loading after a crash gets its unsaved text back (T-08):
    // the same outcome as the Qt shell's renderer-crash path.
    const empty = { recovered: false, snapshots: {} as Record<string, string> };
    const rec = await this.recovery.state().catch(() => empty);
    if (gen !== this.generation) return;
    if (rec.recovered) {
      this.set({
        banner:
          "The window was reloaded after the editor stopped responding. Unsaved changes were restored; check them before saving.",
      });
    }
    this.restoring = true;
    // Sequentially, so tabs keep their order; files that are gone are skipped silently.
    for (const p of paths) {
      if (gen !== this.generation) return;
      const snapshot = rec.snapshots[p];
      if (typeof snapshot === "string") await this.openRecovered(p, snapshot);
      else await this.openInternal(p, true);
    }
    this.restoring = false;
    if (active && this.index(active) >= 0) this.setActive(active);
  }
}
