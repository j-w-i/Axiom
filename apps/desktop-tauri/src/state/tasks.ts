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
  private stopRequested = false;
  private generation = 0;
  private buffer = new OutputBuffer();
  private nextRowId = 0;
  private frame: number | null = null;

  constructor(
    private readonly client: DaemonClient,
    private readonly workspace: WorkspaceStore,
  ) {
    super({ running: false, label: "", statusText: "", lastOutcome: null, history: [], rows: [], dropped: 0 });
    client.onNotification<OutputParams>("task/output", (p) => {
      if (!this.taskId || p.taskId !== this.taskId) return;
      this.buffer.append(p.seq, p.stream, p.data);
      this.scheduleFlush();
    });
    client.onNotification<ExitedParams>("task/exited", (p) => {
      if (!this.taskId || p.taskId !== this.taskId) return;
      this.onExited(p);
    });
    workspace.on("opened", () => void this.loadHistory());
    workspace.on("closed", () => {
      this.resetState();
      this.set({ history: [] });
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
    this.set({ rows, dropped });
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

  // ---- running ---------------------------------------------------------------

  run(commandLine: string): void {
    const cmd = commandLine.trim();
    if (!cmd || !this.canRun) return;
    void this.start({ shell: commandLine }, cmd, true);
  }

  rerunLast(): void {
    const last = this.state.history[0];
    if (last) this.run(last);
  }

  /** Runs an explicit argv (no shell); used to exercise `spawn_failed`. */
  runArgv(argv: string[]): void {
    if (!this.canRun) return;
    void this.start({ argv }, argv.join(" "), false);
  }

  private async start(command: unknown, label: string, record: boolean): Promise<void> {
    this.resetOutput();
    this.starting = true;
    this.stopRequested = false;
    this.set({ running: true, label, statusText: `Running: ${label}`, lastOutcome: null });
    if (record) this.pushHistory(label);
    const gen = this.generation;
    try {
      const r = await this.client.request<{ taskId: string }>("task/start", {
        workspaceId: this.workspace.getSnapshot().id,
        command,
        label,
      });
      if (gen !== this.generation) return;
      this.starting = false;
      this.taskId = r.taskId;
      if (this.stopRequested) this.stop();
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
      this.set({ running: false, label: "", statusText: "Failed to start" });
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
    this.stopRequested = false;
    this.resetOutput();
    this.set({ running: false, label: "", statusText: "", lastOutcome: null });
  }
}
