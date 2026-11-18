// The status bar: backend state, task status, cursor position, encoding.

import type { DaemonState } from "../daemon/types";
import type { ShellContext } from "./actions";

export function StatusBar({ ctx, daemon }: { ctx: ShellContext; daemon: DaemonState }) {
  const { task, docs } = ctx;
  const daemonLabel =
    daemon.state === "ready" ? `Connected — axiomd ${daemon.serverVersion}`
    : daemon.state === "starting" ? "Connecting…"
    : "Backend stopped";
  return (
    <footer className="statusbar" role="status" aria-label={daemonLabel}>
      <span className="status-dot" data-state={daemon.state} data-testid="status">
        {daemonLabel}
      </span>
      <span className="statusbar__spacer" />
      {task.statusText && (
        <span className={task.lastOutcome === "failed" ? "statusbar__failed" : undefined} data-testid="task-status">
          {task.running && <span className="spinner" data-static={ctx.ws.id ? "" : undefined} style={{ display: "inline-block", verticalAlign: "-2px", marginRight: 6 }} />}
          {task.statusText}
        </span>
      )}
      {docs.activePath && docs.cursorText && <span>{docs.cursorText}</span>}
      {docs.activePath && <span>UTF-8</span>}
    </footer>
  );
}
