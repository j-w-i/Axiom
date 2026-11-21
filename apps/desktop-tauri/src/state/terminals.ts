// The workspace's integrated terminals (M4-TERM-002; ADR-0018 §4): the daemon runs them
// (`terminal/*`); this store lists them, keeps which one is shown, and hands their output to
// whoever draws them (the xterm instances live outside React, so switching console views keeps
// their screens). After a webview reload, `terminal/list` brings them back with their backlog.

import type { DaemonClient } from "../daemon/client";
import type { ExitedParams2 as ExitedParams, ListResult2 as ListResult, OutputParams2 as OutputParams, TerminalInfo } from "../daemon/protocol.gen";
import { Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export interface TerminalsState {
  terminals: TerminalInfo[];
  active: string | null;
  /** A create that failed, for the view to say. */
  error: string;
}

type OutputFn = (data: string, seq: number) => void;

export class TerminalsStore extends Observable<TerminalsState> {
  private readonly outputs = new Map<string, Set<OutputFn>>();
  /** Output that arrived before anyone drew the terminal (a create's first prompt). */
  private readonly early = new Map<string, { data: string; seq: number }[]>();
  /** The backlog from `terminal/list` (a reload), drawn first. */
  private readonly backlogs = new Map<string, { data: string; seq: number }>();
  private readonly lastSeq = new Map<string, number>();

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super({ terminals: [], active: null, error: "" });
    workspace.on("opened", () => void this.reattach());
    workspace.on("closed", () => {
      this.outputs.clear();
      this.early.clear();
      this.backlogs.clear();
      this.lastSeq.clear();
      this.set({ terminals: [], active: null, error: "" });
    });
    client.onNotification<OutputParams>("terminal/output", (p) => this.onOutput(p));
    client.onNotification<ExitedParams>("terminal/exited", (p) => {
      this.set({ terminals: this.state.terminals.map((t) => (t.terminalId === p.terminalId ? { ...t, running: false, exitCode: p.code ?? null } : t)) });
    });
  }

  private get wsId(): string {
    return this.workspace.getSnapshot().id;
  }

  private onOutput(p: OutputParams): void {
    if (!this.state.terminals.some((t) => t.terminalId === p.terminalId) && !this.early.has(p.terminalId)) {
      // A create still on its way: keep its first output.
      this.early.set(p.terminalId, []);
    }
    const last = this.lastSeq.get(p.terminalId) ?? 0;
    if (p.seq <= last) return; // already in the backlog
    this.lastSeq.set(p.terminalId, p.seq);
    const fns = this.outputs.get(p.terminalId);
    if (fns?.size) for (const fn of [...fns]) fn(p.data, p.seq);
    else (this.early.get(p.terminalId) ?? this.early.set(p.terminalId, []).get(p.terminalId)!).push({ data: p.data, seq: p.seq });
  }

  /** Subscribes a terminal's output; what arrived before (backlog, early output) is replayed. */
  onTerminalOutput(id: string, fn: OutputFn): () => void {
    let set = this.outputs.get(id);
    if (!set) this.outputs.set(id, (set = new Set()));
    set.add(fn);
    const b = this.backlogs.get(id);
    if (b) {
      fn(b.data, b.seq);
      this.backlogs.delete(id);
    }
    for (const e of this.early.get(id) ?? []) fn(e.data, e.seq);
    this.early.delete(id);
    return () => set.delete(fn);
  }

  /** After a reload: the daemon's terminals of this workspace, with their screens. */
  async reattach(): Promise<void> {
    if (!this.wsId) return;
    try {
      const r = await this.client.request<ListResult>("terminal/list", { workspaceId: this.wsId });
      for (const t of r.terminals) {
        if (t.backlog) this.backlogs.set(t.terminalId, { data: t.backlog, seq: t.seq });
        this.lastSeq.set(t.terminalId, t.seq);
      }
      const terminals = r.terminals.map(({ backlog: _b, seq: _s, ...info }) => info as TerminalInfo);
      this.set({ terminals, active: this.state.active ?? terminals.at(-1)?.terminalId ?? null });
    } catch {
      /* an older daemon: no terminals */
    }
  }

  async create(cols: number, rows: number): Promise<string | null> {
    if (!this.wsId) return null;
    try {
      const t = await this.client.request<TerminalInfo>("terminal/create", { workspaceId: this.wsId, cols: Math.max(1, cols), rows: Math.max(1, rows) });
      this.set({ terminals: [...this.state.terminals, t], active: t.terminalId, error: "" });
      return t.terminalId;
    } catch (e) {
      this.set({ error: e instanceof Error ? e.message : String(e) });
      return null;
    }
  }

  write(id: string, data: string): void {
    void this.client.request("terminal/write", { terminalId: id, data }).catch(() => {});
  }

  resize(id: string, cols: number, rows: number): void {
    if (cols < 1 || rows < 1) return;
    void this.client.request("terminal/resize", { terminalId: id, cols, rows }).catch(() => {});
  }

  activate(id: string): void {
    if (this.state.terminals.some((t) => t.terminalId === id)) this.set({ active: id });
  }

  async close(id: string): Promise<void> {
    await this.client.request("terminal/close", { terminalId: id }).catch(() => {});
    const i = this.state.terminals.findIndex((t) => t.terminalId === id);
    const terminals = this.state.terminals.filter((t) => t.terminalId !== id);
    const active = this.state.active === id ? (terminals[Math.min(i, terminals.length - 1)]?.terminalId ?? null) : this.state.active;
    this.outputs.delete(id);
    this.early.delete(id);
    this.set({ terminals, active });
  }

  /** The tab's name: the program, numbered ("zsh 2"), and "(ended)" after exit. */
  label(t: TerminalInfo): string {
    const same = this.state.terminals.filter((x) => x.title === t.title);
    const n = same.length > 1 ? ` ${same.indexOf(t) + 1}` : "";
    return `${t.title}${n}${t.running ? "" : " (ended)"}`;
  }
}
