// The status bar (workspace-spec.md §2.5; M1.6-SHELL-005): backend, task, hint line, cursor,
// encoding. Only the backend and task segments are live regions; the hint line never is.

import { useSyncExternalStore } from "react";
import type { DaemonState } from "../daemon/types";
import type { LspStore } from "../state/lsp";
import { parseHint, useHint } from "../ui/hint";
import { Kbd, Led, type LedState } from "../ui/primitives";
import type { ShellContext } from "./actions";
import "../styles/components/statusbar.css";

export function StatusBar({ ctx, daemon }: { ctx: ShellContext; daemon: DaemonState }) {
  const { task, docs, ws } = ctx;
  const { client } = ctx.app;
  const hint = parseHint(useHint());
  const stopped = daemon.state === "stopped" || daemon.state === "failed";
  const led: LedState = daemon.state === "ready" ? "on" : daemon.state === "starting" ? "run" : "err";
  const backend = (
    <span className="sb__backend" role="status" data-testid="status" data-state={daemon.state}>
      <Led state={led} />
      {daemon.state === "ready" ? (
        <>
          <span className="visually-hidden">Connected — </span>axiomd {daemon.serverVersion}
        </>
      ) : daemon.state === "starting" ? (
        "Connecting…"
      ) : (
        "Backend stopped"
      )}
    </span>
  );
  return (
    <footer className="statusbar">
      {stopped ? (
        <button type="button" className="sb__seg sb__seg--button" data-hint="Restart the backend" onClick={() => void client.restart()}>
          {backend}
        </button>
      ) : (
        <span className="sb__seg" data-hint={daemon.state === "ready" ? "The Axiom backend is connected" : undefined}>
          {backend}
        </span>
      )}
      {task.statusText && (
        <button type="button" className="sb__seg sb__seg--button" data-hint="Show the console" onClick={ctx.focusRun}>
          <span className={task.lastOutcome === "failed" ? "sb__task sb__task--failed" : "sb__task"} role="status" data-testid="task-status">
            {/* Static while the title row's transport (the one animated indicator) is visible. */}
            {task.running && <span className="spinner" data-static={ws.id ? "" : undefined} />}
            {task.statusText}
          </span>
        </button>
      )}
      <span className="sb__hint" aria-live="off">
        {hint.what}
        {hint.keys.length > 0 && (
          <span className="sb__keys">
            {hint.keys.map((k, i) => (
              <Kbd key={i}>{k}</Kbd>
            ))}
          </span>
        )}
      </span>
      {docs.activePath && <LanguageServer lsp={ctx.app.lsp} path={docs.activePath} />}
      {docs.activePath && docs.cursorText && <span className="sb__seg">{docs.cursorText}</span>}
      {docs.activePath && <span className="sb__seg">UTF-8</span>}
    </footer>
  );
}

/** The active document's language server (M4-LSP-002): its state, and the way out of
 * "not installed" (the install hint) or "failed" (restart). Nothing for a language without one. */
function LanguageServer({ lsp, path }: { lsp: LspStore; path: string }) {
  useSyncExternalStore(lsp.subscribe, lsp.getSnapshot, lsp.getSnapshot);
  const s = lsp.serverForPath(path);
  if (!s || s.state === "idle") return null;
  const progress = lsp.getSnapshot().progress[s.serverId];
  if (s.state === "failed" || s.state === "stopped") {
    return (
      <button type="button" className="sb__seg sb__seg--button" data-testid="lsp-status" data-state={s.state} data-hint={`${s.detail ?? `${s.serverId} stopped.`} Click to restart it.`} onClick={() => void lsp.restart(s.serverId)}>
        <Led state="err" />
        {s.serverId}: {s.state === "failed" ? "failed" : "stopped"} — Restart
      </button>
    );
  }
  if (s.state === "missing") {
    return (
      <span className="sb__seg" data-testid="lsp-status" data-state={s.state} data-hint={s.installHint ?? s.detail ?? undefined} tabIndex={0} aria-label={`${s.serverId} is not installed. ${s.installHint ?? ""}`}>
        <Led state="off" />
        {s.serverId}: not installed
      </span>
    );
  }
  return (
    <span className="sb__seg" data-testid="lsp-status" data-state={s.state} data-hint={s.state === "ready" ? `${s.serverId} provides completion, navigation and diagnostics` : "Starting the language server"}>
      <Led state={s.state === "ready" && !progress ? "on" : "run"} />
      {s.state === "ready" ? (progress ? `${s.serverId}: ${progress}` : s.serverId) : `${s.serverId}: starting…`}
    </span>
  );
}
