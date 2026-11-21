// Port of TaskController + TaskOutputModel (apps/desktop/src/models/).
// One command at a time per workspace, verbatim `command: { shell }`, output
// applied in `seq` order, split into lines per stream, ANSI stripped, capped
// at 10 000 lines, published in batches (one per animation frame here, 16 ms
// in Qt) so a chatty task cannot stall the UI. History (20) is persisted as
// `run.history` in workspace state.

import type { DaemonClient } from "../daemon/client";
import { RpcError, type ExitedParams, type OutputParams } from "../daemon/types";
import { Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export const HISTORY_KEY = "run.history";
export const MAX_HISTORY = 20;
export const MAX_LINES = 10_000;
const MAX_ROWS_PER_FLUSH = 2_000;

export type RowKind = "line" | "status" | "statusFailed" | "error";
export interface OutputRow {
  id: number;
  text: string;
  stream: "stdout" | "stderr" | "";
  kind: RowKind;
}

export interface TasksState {
  running: boolean;
  label: string;
  statusText: string;
  lastOutcome: ExitedParams["outcome"] | null;
  history: string[];
  rows: OutputRow[];
  dropped: number;
  /** Epoch ms the running task started (TaskInfo.startedMs via task/list); null when idle. */
  startedAt: number | null;
  /** Monotonic count of output rows appended (dropped ones included); the transport's rate meter samples it. */
  outputSeq: number;
  /** The open workspace's provider tasks (`build/tasks`, M2-UI-003). */
  providerTasks: ProviderTask[];
  /** Set when the last run was a provider task: Rerun repeats it (cleared by a command run). */
  lastProvider: ProviderTask | null;
}

export type ProviderTask = import("../daemon/protocol.gen").TaskInfo2;

/** The provider task's palette and transport name: "Build: cargo build". */
export function providerTaskTitle(t: ProviderTask): string {
  const names: Record<string, string> = { build: "Build", run: "Run", test: "Test", clean: "Clean", other: "Task" };
  const kind = names[t.kind] ?? "Task";
  return `${kind}: ${t.label}`;
}

const ANSI = /\x1B\[[0-?]*[ -/]*[@-~]|\x1B\][^\x07\x1B]*(?:\x07|\x1B\\)|\x1B[@-Z\\-_]/g;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\x00-\x08\x0B\x0C\x0E-\x1F]/g;

export function stripAnsi(s: string): string {
  return s.replace(ANSI, "").replace(CONTROL, "");
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  return `${Math.floor(ms / 60000)} min ${Math.floor((ms % 60000) / 1000)} s`;
}

/** Line assembly for one task's output; pure, so it is unit-tested directly. */
export class OutputBuffer {
  private nextSeq = 0;
  private readonly early = new Map<number, { stream: "stdout" | "stderr"; data: string }>();
  private readonly partial = { stdout: "", stderr: "" };
  private pending: Omit<OutputRow, "id">[] = [];

  append(seq: number, stream: "stdout" | "stderr", data: string): void {
    if (seq < this.nextSeq) return; // duplicate
    if (seq !== this.nextSeq) {
      this.early.set(seq, { stream, data });
      return;
    }
    this.apply(stream, data);
    this.nextSeq++;
    for (let e = this.early.get(this.nextSeq); e; e = this.early.get(this.nextSeq)) {
      this.early.delete(this.nextSeq);
      this.apply(e.stream, e.data);
      this.nextSeq++;
    }
  }

  private apply(stream: "stdout" | "stderr", data: string): void {
    const text = this.partial[stream] + stripAnsi(data);
    const parts = text.split("\n");
    this.partial[stream] = parts.pop() ?? "";
    for (let line of parts) {
      if (line.endsWith("\r")) line = line.slice(0, -1);
      const cr = line.lastIndexOf("\r");
      if (cr >= 0) line = line.slice(cr + 1);
      this.pending.push({ text: line, stream, kind: "line" });
    }
  }

  /** The task ended: trailing partial lines become real lines. */
  finish(): void {
    for (const stream of ["stdout", "stderr"] as const) {
      let line = this.partial[stream];
      if (!line) continue;
      const cr = line.lastIndexOf("\r");
      if (cr >= 0) line = line.slice(cr + 1);
      this.pending.push({ text: line, stream, kind: "line" });
      this.partial[stream] = "";
    }
  }

  push(row: Omit<OutputRow, "id">): void {
    this.pending.push(row);
  }

  take(max: number): Omit<OutputRow, "id">[] {
    return this.pending.splice(0, max);
  }

  get hasPending(): boolean {
    return this.pending.length > 0;
  }
}

export class TasksStore extends Observable<TasksState> {
  private taskId = "";
  private starting = false;
  /** Notifications that arrived while `task/start` was in flight: a fast task's output can be
   * dispatched before the start's response is handled (the response resolves a promise, whose
   * continuation runs after the lines read with it). Replayed once the id is known. */
  private early: ({ kind: "output"; p: OutputParams } | { kind: "exited"; p: ExitedParams })[] = [];
  private stopRequested = false;
  private generation = 0;
  private buffer = new OutputBuffer();
  private nextRowId = 0;
  private frame: number | null = null;

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super({ running: false, label: "", statusText: "", lastOutcome: null, history: [], rows: [], dropped: 0, startedAt: null, outputSeq: 0, providerTasks: [], lastProvider: null });
    client.onNotification<OutputParams>("task/output", (p) => {
      if (!this.taskId && this.starting) return void this.early.push({ kind: "output", p });
      if (!this.taskId || p.taskId !== this.taskId) return;
      this.buffer.append(p.seq, p.stream, p.data);
      this.scheduleFlush();
    });
    client.onNotification<ExitedParams>("task/exited", (p) => {
      if (!this.taskId && this.starting) return void this.early.push({ kind: "exited", p });
      if (!this.taskId || p.taskId !== this.taskId) return;
      this.onExited(p);
    });
    workspace.on("opened", () => {
      void this.loadHistory();
      void this.loadProviderTasks();
    });
    workspace.on("closed", () => {
      this.resetState();
      this.set({ history: [], providerTasks: [], lastProvider: null });
    });
    client.onState((s) => {
      if (s.state !== "ready" && this.state.running) this.resetState();
    });
  }

  get canRun(): boolean {
    return !this.state.running && this.workspace.isOpen && this.client.ready;
  }

  // ---- output publishing -----------------------------------------------------

  private scheduleFlush(): void {
    if (this.frame !== null) return;
    const raf = typeof requestAnimationFrame === "function" ? requestAnimationFrame : (f: () => void) => setTimeout(f, 16) as unknown as number;
    this.frame = raf(() => {
      this.frame = null;
      this.flush();
      if (this.buffer.hasPending) this.scheduleFlush();
    });
  }

  flushNow(): void {
    while (this.buffer.hasPending) this.flush();
  }

  private flush(): void {
    const batch = this.buffer.take(MAX_ROWS_PER_FLUSH).map((r) => ({ ...r, id: this.nextRowId++ }));
    if (batch.length === 0) return;
    let rows = this.state.rows.concat(batch);
    let dropped = this.state.dropped;
    if (rows.length > MAX_LINES) {
      dropped += rows.length - MAX_LINES;
      rows = rows.slice(rows.length - MAX_LINES);
    }
    this.set({ rows, dropped, outputSeq: this.state.outputSeq + batch.length });
  }

  private resetOutput(): void {
    this.buffer = new OutputBuffer();
    this.set({ rows: [], dropped: 0 });
  }

  allText(): string {
    return this.state.rows.map((r) => r.text).join("\n");
  }

  // ---- history ---------------------------------------------------------------

  private pushHistory(cmd: string): void {
    const history = [cmd, ...this.state.history.filter((h) => h !== cmd)].slice(0, MAX_HISTORY);
    this.set({ history });
    if (!this.workspace.isOpen) return;
    void this.client
      .request("workspace/setState", { workspaceId: this.workspace.getSnapshot().id, values: { [HISTORY_KEY]: history } })
      .catch(() => {});
  }

  private async loadHistory(): Promise<void> {
    if (!this.workspace.isOpen) return;
    const gen = this.generation;
    try {
      const r = await this.client.request<{ values: Record<string, unknown> }>("workspace/getState", {
        workspaceId: this.workspace.getSnapshot().id,
        keys: [HISTORY_KEY],
      });
      if (gen !== this.generation) return;
      const raw = r.values[HISTORY_KEY];
      let loaded = Array.isArray(raw) ? raw.filter((x): x is string => typeof x === "string") : [];
      loaded = [...new Set(loaded)].slice(0, MAX_HISTORY);
      // Anything run before the saved list arrived stays on top.
      const current = this.state.history;
      this.set({ history: [...current, ...loaded.filter((h) => !current.includes(h))] });
    } catch {
      /* no history */
    }
  }

  /** The workspace's provider tasks, for the palette. */
  async loadProviderTasks(): Promise<void> {
    if (!this.workspace.isOpen) return;
    const gen = this.generation;
    try {
      const r = await this.client.request<{ tasks: ProviderTask[] }>("build/tasks", { workspaceId: this.workspace.getSnapshot().id });
      if (gen === this.generation) this.set({ providerTasks: r.tasks });
    } catch {
      /* no provider tasks (an older daemon, or none apply) */
    }
  }

  // ---- running ---------------------------------------------------------------

  /** Called with each provider run's task id as soon as it is known (the Problems store). */
  onProviderStart: ((key: string, taskId: string, label: string) => void) | null = null;

  run(commandLine: string): void {
    const cmd = commandLine.trim();
    if (!cmd || !this.canRun) return;
    this.set({ lastProvider: null });
    void this.start(() => this.startCommand({ shell: commandLine }, cmd), cmd, true);
  }

  /** The last command, or the last provider task when that ran last. */
  get lastLabel(): string {
    return this.state.lastProvider ? providerTaskTitle(this.state.lastProvider) : (this.state.history[0] ?? "");
  }

  rerunLast(): void {
    if (this.state.lastProvider) return this.runProvider(this.state.lastProvider);
    const last = this.state.history[0];
    if (last) this.run(last);
  }

  /** Runs an explicit argv (no shell); used to exercise `spawn_failed`. */
  runArgv(argv: string[]): void {
    if (!this.canRun) return;
    void this.start(() => this.startCommand({ argv }, argv.join(" ")), argv.join(" "), false);
  }

  /** A provider task (`build/run`): same output, transport and Stop as a command; its
   * diagnostics go to the Problems view. Not recorded in the command history. */
  runProvider(t: ProviderTask): void {
    if (!this.canRun) return;
    this.set({ lastProvider: t });
    const key = `${t.providerId}/${t.id}`;
    void this.start(
      async () => {
        const r = await this.client.request<{ taskId: string }>("build/run", {
          workspaceId: this.workspace.getSnapshot().id,
          providerId: t.providerId,
          task: t.id,
        });
        this.onProviderStart?.(key, r.taskId, providerTaskTitle(t));
        return r;
      },
      t.label,
      false,
    );
  }

  private startCommand(command: unknown, label: string): Promise<{ taskId: string }> {
    return this.client.request<{ taskId: string }>("task/start", {
      workspaceId: this.workspace.getSnapshot().id,
      command,
      label,
    });
  }

  private async start(launch: () => Promise<{ taskId: string }>, label: string, record: boolean): Promise<void> {
    this.resetOutput();
    this.early = [];
    this.starting = true;
    this.stopRequested = false;
    this.set({ running: true, label, statusText: `Running: ${label}`, lastOutcome: null, startedAt: null });
    if (record) this.pushHistory(label);
    const gen = this.generation;
    try {
      const r = await launch();
      if (gen !== this.generation) return;
      this.starting = false;
      this.taskId = r.taskId;
      const early = this.early;
      this.early = [];
      let exited = false;
      for (const e of early) {
        if (e.p.taskId !== r.taskId) continue;
        if (e.kind === "output") {
          this.buffer.append(e.p.seq, e.p.stream, e.p.data);
          this.scheduleFlush();
        } else {
          this.onExited(e.p);
          exited = true;
        }
      }
      if (exited) return;
      if (this.stopRequested) this.stop();
      void this.loadStartedAt(r.taskId, gen);
    } catch (e) {
      if (gen !== this.generation) return;
      this.starting = false;
      const err = e as RpcError;
      const text =
        err.kind === "spawn_failed"
          ? `Could not start ${String(err.details.program ?? label)}: ${String(err.details.osError ?? err.message)}`
          : `Could not run the command: ${err.message}`;
      this.buffer.push({ text, stream: "stderr", kind: "error" });
      this.flushNow();
      this.set({ running: false, label: "", statusText: "Failed to start", startedAt: null });
    }
  }

  /** task/start answers with the id only (IPC 0.1); the start time is in task/list's TaskInfo. */
  private async loadStartedAt(taskId: string, gen: number): Promise<void> {
    try {
      const r = await this.client.request<{ tasks: { taskId: string; startedMs: number }[] }>("task/list", {
        workspaceId: this.workspace.getSnapshot().id,
      });
      const info = r.tasks.find((t) => t.taskId === taskId);
      if (info && gen === this.generation && this.taskId === taskId && this.state.running) this.set({ startedAt: info.startedMs });
    } catch {
      /* no elapsed time for this run */
    }
  }

  stop(): void {
    if (!this.state.running) return;
    this.stopRequested = true;
    if (!this.taskId) return; // still starting: cancelled as soon as the id is known
    void this.client.request("task/cancel", { taskId: this.taskId }).catch(() => {});
  }

  clear(): void {
    this.resetOutput();
    if (!this.state.running) this.set({ statusText: "", lastOutcome: null });
  }

  private onExited(p: ExitedParams): void {
    this.buffer.finish();
    let text: string;
    const ok = p.outcome === "succeeded";
    if (ok) text = `✔ Succeeded in ${formatDuration(p.durationMs)}`;
    else if (p.outcome === "cancelled") text = "■ Cancelled";
    else if (typeof p.exitCode === "number") text = `✖ Failed with exit code ${p.exitCode} in ${formatDuration(p.durationMs)}`;
    else if (typeof p.signal === "number") text = `✖ Failed (signal ${p.signal}) in ${formatDuration(p.durationMs)}`;
    else text = `✖ Failed in ${formatDuration(p.durationMs)}`;
    this.buffer.push({ text, stream: "", kind: ok ? "status" : "statusFailed" });
    this.flushNow();
    const label = this.state.label;
    this.taskId = "";
    this.stopRequested = false;
    this.set({
      running: false,
      label: "",
      startedAt: null,
      lastOutcome: p.outcome,
      statusText:
        p.outcome === "succeeded" ? `${label}: succeeded`
        : p.outcome === "cancelled" ? `${label}: cancelled`
        : typeof p.exitCode === "number" ? `${label}: failed (${p.exitCode})`
        : `${label}: failed`,
    });
  }

  private resetState(): void {
    ++this.generation;
    this.taskId = "";
    this.starting = false;
    this.early = [];
    this.stopRequested = false;
    this.resetOutput();
    this.set({ running: false, label: "", statusText: "", lastOutcome: null, startedAt: null });
  }
}
