// The workspace's language servers as the UI sees them (M4-LSP-002; ADR-0018 §2): their state
// (status bar), their diagnostics (Problems and the editor's markers), and requests through the
// daemon. No Monaco here: the editor side is editor/lsp/client.ts.

import type { DaemonClient } from "../daemon/client";
import type { Diagnostic, LspNotificationParams, ServerInfo, ServerStateParams, ServersResult } from "../daemon/protocol.gen";
import { fileUri, relativeTo, toProblem, type LspDiagnostic } from "../editor/lsp/convert";
import { Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export interface LspState {
  servers: ServerInfo[];
  /** serverId → workspace path → its diagnostics as Problems rows. */
  problems: Record<string, Record<string, Diagnostic[]>>;
  /** serverId → the latest `$/progress` title ("Indexing…"), while one runs. */
  progress: Record<string, string>;
  /** Workspace path → the LSP language id of its open document (set by the editor). */
  languages: Record<string, string>;
}

export interface PublishedDiagnostics {
  serverId: string;
  uri: string;
  /** Workspace path, or null outside the workspace. */
  path: string | null;
  diagnostics: LspDiagnostic[];
}

const EMPTY: LspState = { servers: [], problems: {}, progress: {}, languages: {} };

export class LspStore extends Observable<LspState> {
  private readonly diagnosticListeners = new Set<(p: PublishedDiagnostics) => void>();
  private loadSeq = 0;

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super(EMPTY);
    workspace.on("opened", () => void this.load());
    workspace.on("closed", () => this.set(EMPTY));
    client.onNotification<ServerStateParams>("lsp/serverState", (p) => {
      if (p.workspaceId !== this.wsId) return;
      const i = this.state.servers.findIndex((s) => s.serverId === p.server.serverId);
      const servers = i < 0 ? [...this.state.servers, p.server] : this.state.servers.map((s, j) => (j === i ? p.server : s));
      // A server that is not running has no diagnostics or progress.
      const running = p.server.state === "ready" || p.server.state === "starting";
      if (running) this.set({ servers });
      else {
        const { [p.server.serverId]: _p, ...problems } = this.state.problems;
        const { [p.server.serverId]: _q, ...progress } = this.state.progress;
        this.set({ servers, problems, progress });
      }
    });
    client.onNotification<LspNotificationParams>("lsp/notification", (n) => {
      if (n.workspaceId !== this.wsId) return;
      if (n.method === "textDocument/publishDiagnostics") this.onDiagnostics(n.serverId, n.params as { uri: string; diagnostics: LspDiagnostic[] });
      else if (n.method === "$/progress") this.onProgress(n.serverId, n.params as { value?: { kind?: string; title?: string } });
    });
  }

  private get wsId(): string {
    return this.workspace.getSnapshot().id;
  }

  /** The workspace root as a `file://` URI. */
  get rootUri(): string {
    return fileUri(this.workspace.getSnapshot().root);
  }

  /** `file://` URI of a workspace path. */
  uriOf(path: string): string {
    return `${this.rootUri}/${path.split("/").map(encodeURIComponent).join("/")}`;
  }

  /** Workspace path of a URI, or null outside the workspace. */
  pathOf(uri: string): string | null {
    return relativeTo(uri, this.rootUri);
  }

  async load(): Promise<void> {
    const seq = ++this.loadSeq;
    if (!this.wsId) return;
    try {
      const r = await this.client.request<ServersResult>("lsp/servers", { workspaceId: this.wsId });
      if (seq === this.loadSeq) this.set({ servers: r.servers });
    } catch {
      /* an older daemon (no lsp/servers): no language servers */
    }
  }

  /** The server for a language (LSP language id), if any. */
  serverFor(languageId: string): ServerInfo | undefined {
    const base = languageId === "typescriptreact" ? "typescript" : languageId === "javascriptreact" ? "javascript" : languageId;
    return this.state.servers.find((s) => s.languages.includes(languageId) || s.languages.includes(base));
  }

  /** The editor says which language an open document is (for the status bar). */
  noteLanguage(path: string, languageId: string | null): void {
    if ((this.state.languages[path] ?? null) === languageId) return;
    const { [path]: _, ...rest } = this.state.languages;
    this.set({ languages: languageId ? { ...rest, [path]: languageId } : rest });
  }

  /** The server for an open document, if one handles its language. */
  serverForPath(path: string): ServerInfo | undefined {
    const l = this.state.languages[path];
    return l ? this.serverFor(l) : undefined;
  }

  /** One request; `onCancel` receives the function that cancels it (Monaco's token). */
  request<T>(serverId: string, method: string, params: unknown, onCancel?: (cancel: () => void) => void): Promise<T> {
    const c = this.client.cancellable<{ result: T }>("lsp/request", { workspaceId: this.wsId, serverId, method, params });
    onCancel?.(c.cancel);
    return c.promise.then((r) => r.result);
  }

  notify(serverId: string, method: string, params: unknown): Promise<void> {
    return this.client.request<void>("lsp/notify", { workspaceId: this.wsId, serverId, method, params });
  }

  /** A file outside the workspace that a server pointed to (read-only; ADR-0018 §2). */
  async readExternal(uri: string): Promise<string> {
    const r = await this.client.request<{ text: string }>("fs/readFile", { workspaceId: this.wsId, path: uri });
    return r.text;
  }

  restart(serverId: string): Promise<void> {
    return this.client.request<void>("lsp/restart", { workspaceId: this.wsId, serverId });
  }

  /** Published diagnostics as they arrive (the editor sets markers from them). */
  onPublish(fn: (p: PublishedDiagnostics) => void): () => void {
    this.diagnosticListeners.add(fn);
    return () => this.diagnosticListeners.delete(fn);
  }

  /** Every server's diagnostics as Problems rows. */
  all(): Diagnostic[] {
    return Object.values(this.state.problems).flatMap((byFile) => Object.values(byFile).flat());
  }

  private onDiagnostics(serverId: string, p: { uri: string; diagnostics: LspDiagnostic[] }): void {
    const path = this.pathOf(p.uri);
    for (const fn of [...this.diagnosticListeners]) fn({ serverId, uri: p.uri, path, diagnostics: p.diagnostics });
    if (path === null) return;
    const mine = { ...(this.state.problems[serverId] ?? {}) };
    if (p.diagnostics.length) mine[path] = p.diagnostics.map((d) => toProblem(d, path, serverId));
    else delete mine[path];
    this.set({ problems: { ...this.state.problems, [serverId]: mine } });
  }

  private onProgress(serverId: string, p: { value?: { kind?: string; title?: string } }): void {
    const v = p.value;
    if (!v?.kind) return;
    if (v.kind === "begin" && v.title) this.set({ progress: { ...this.state.progress, [serverId]: v.title } });
    else if (v.kind === "end" && serverId in this.state.progress) {
      const { [serverId]: _, ...progress } = this.state.progress;
      this.set({ progress });
    }
  }
}
