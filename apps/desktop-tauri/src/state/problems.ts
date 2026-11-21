// Diagnostics from provider tasks (`build/diagnostics`, M2-BUILD-001) for the console's
// Problems view (M2-UI-003). Batches carry only new diagnostics; they are collected per task id
// as they arrive (even before the start's response is handled), and each provider task shows its
// latest run only: a new run of the same task replaces the previous run's problems.

import type { DaemonClient } from "../daemon/client";
import type { BuildDiagnostics, Diagnostic } from "../daemon/protocol.gen";
import { Observable } from "./observable";
import type { WorkspaceStore } from "./workspace";

export interface ProblemsState {
  /** provider/task key → the task id of its latest run, and that run's label. */
  runs: Record<string, { taskId: string; label: string }>;
  /** task id → diagnostics received so far, and whether the set is complete. */
  byTask: Record<string, { diagnostics: Diagnostic[]; complete: boolean }>;
}

export interface ProblemRow {
  /** Stable within a snapshot: `<file>#<index>`. */
  id: string;
  diagnostic: Diagnostic;
}

export interface ProblemGroup {
  /** Workspace path, or "" for diagnostics without a location. */
  file: string;
  rows: ProblemRow[];
}

const SEVERITY_ORDER = { error: 0, warning: 1, info: 2, hint: 3 } as const;

/** Grouped by file (files in name order, "no location" last); within a file by severity, then
 * line and column. Pure. */
export function groupProblems(diagnostics: Diagnostic[]): ProblemGroup[] {
  const groups = new Map<string, Diagnostic[]>();
  for (const d of diagnostics) {
    const file = d.location?.file ?? "";
    groups.set(file, [...(groups.get(file) ?? []), d]);
  }
  const files = [...groups.keys()].sort((a, b) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
  return files.map((file) => {
    const rows = [...(groups.get(file) ?? [])].sort(
      (a, b) =>
        SEVERITY_ORDER[a.severity] - SEVERITY_ORDER[b.severity] ||
        (a.location?.line ?? 0) - (b.location?.line ?? 0) ||
        (a.location?.column ?? 0) - (b.location?.column ?? 0),
    );
    return { file, rows: rows.map((diagnostic, i) => ({ id: `${file}#${i}`, diagnostic })) };
  });
}

export function counts(diagnostics: Diagnostic[]): { errors: number; warnings: number; total: number } {
  let errors = 0;
  let warnings = 0;
  for (const d of diagnostics) {
    if (d.severity === "error") errors++;
    else if (d.severity === "warning") warnings++;
  }
  return { errors, warnings, total: diagnostics.length };
}

/** `file:line:col` as shown in a row. */
export function positionText(d: Diagnostic): string {
  const l = d.location;
  if (!l) return "";
  return `${l.file}:${l.line}${l.column ? `:${l.column}` : ""}`;
}

export class ProblemsStore extends Observable<ProblemsState> {
  constructor(client: DaemonClient, workspace: WorkspaceStore) {
    super({ runs: {}, byTask: {} });
    client.onNotification<BuildDiagnostics>("build/diagnostics", (p) => {
      const prev = this.state.byTask[p.taskId] ?? { diagnostics: [], complete: false };
      this.set({
        byTask: {
          ...this.state.byTask,
          [p.taskId]: { diagnostics: [...prev.diagnostics, ...p.diagnostics], complete: p.complete },
        },
      });
    });
    workspace.on("closed", () => this.set({ runs: {}, byTask: {} }));
  }

  /** A new run of `key` (provider/task) started as `taskId`: its problems replace the last run's. */
  begin(key: string, taskId: string, label: string): void {
    const old = this.state.runs[key]?.taskId;
    const byTask = { ...this.state.byTask };
    if (old && old !== taskId) delete byTask[old];
    // Task ids that belong to no run (diagnostics that arrived for a run since replaced) go too.
    const live = new Set([...Object.values(this.state.runs).map((r) => r.taskId), taskId]);
    for (const id of Object.keys(byTask)) if (!live.has(id) && id !== taskId) delete byTask[id];
    this.set({ runs: { ...this.state.runs, [key]: { taskId, label } }, byTask });
  }

  /** Every current problem: the latest run of each provider task. */
  all(): Diagnostic[] {
    return Object.values(this.state.runs).flatMap((r) => this.state.byTask[r.taskId]?.diagnostics ?? []);
  }
}
