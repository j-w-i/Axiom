// Command bar + streamed output. Rows are virtualised; the view sticks to the
// bottom while the user has not scrolled up.

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, useSyncExternalStore } from "react";
import type { TasksStore } from "../state/tasks";
import type { WorkspaceStore } from "../state/workspace";
import * as Icon from "../ui/icons";
import { ContextMenu } from "../ui/overlays";
import { IconButton } from "../ui/primitives";

const ROW_H = 18;

export interface OutputPanelHandle {
  focusCommand(): void;
}

export const OutputPanel = forwardRef<OutputPanelHandle, { tasks: TasksStore; workspace: WorkspaceStore; ready: boolean }>(
  function OutputPanel({ tasks, workspace, ready }, ref) {
    const s = useSyncExternalStore(tasks.subscribe, tasks.getSnapshot);
    const w = useSyncExternalStore(workspace.subscribe, workspace.getSnapshot);
    const [command, setCommand] = useState("");
    const [historyIndex, setHistoryIndex] = useState(-1);
    const input = useRef<HTMLInputElement>(null);
    const scroller = useRef<HTMLDivElement>(null);
    const stick = useRef(true);
    const [view, setView] = useState({ top: 0, height: 200 });
    const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);

    useImperativeHandle(ref, () => ({ focusCommand: () => input.current?.focus() }), []);

    const copySelection = useCallback(() => {
      const text = window.getSelection()?.toString() ?? "";
      if (text) void navigator.clipboard.writeText(text);
    }, []);

    // Context menu on the output: the same two actions as the Qt panel (copy the
    // selection, copy everything), through ui/ContextMenu (Escape, outside click, arrows).

    useEffect(() => {
      const el = scroller.current;
      if (!el) return;
      const seen = { top: -1, height: -1 };
      const ro = new ResizeObserver(() => {
        // Only height and scroll position matter: a width-only resize (a side-bar drag) must not
        // even call setState (right after a commit React cannot always bail out of an equal update).
        if (el.scrollTop === seen.top && el.clientHeight === seen.height) return;
        seen.top = el.scrollTop;
        seen.height = el.clientHeight;
        setView({ top: el.scrollTop, height: el.clientHeight });
      });
      ro.observe(el);
      return () => ro.disconnect();
    }, []);

    useLayoutEffect(() => {
      const el = scroller.current;
      if (el && stick.current) {
        el.scrollTop = el.scrollHeight;
        setView({ top: el.scrollTop, height: el.clientHeight });
      }
    }, [s.rows]);

    const canRun = !s.running && !!w.id && ready;
    const run = () => {
      if (!command.trim() || !canRun) return;
      tasks.run(command);
      setHistoryIndex(-1);
      stick.current = true;
    };

    const notice = s.dropped > 0 ? 1 : 0;
    const total = s.rows.length + notice;
    const first = Math.max(0, Math.floor(view.top / ROW_H) - 20);
    const last = Math.min(total, Math.ceil((view.top + view.height) / ROW_H) + 20);

    return (
      <section className="panel" aria-label="Output" data-testid="output-panel">
        <div className="panel__bar">
          <span className="panel__title">Output</span>
          <input
            ref={input}
            className="input input--mono"
            placeholder={w.id ? "Run a command in the workspace…  (↑ history)" : "Open a folder to run commands"}
            aria-label="Command"
            value={command}
            disabled={!w.id}
            data-testid="command-input"
            data-hint="Run a command in the workspace — Enter"
            onChange={(e) => setCommand(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") run();
              if (e.key === "ArrowUp" && s.history.length) {
                const i = Math.min(s.history.length - 1, historyIndex + 1);
                setHistoryIndex(i);
                setCommand(s.history[i] ?? "");
                e.preventDefault();
              }
              if (e.key === "ArrowDown") {
                const i = historyIndex - 1;
                setHistoryIndex(Math.max(-1, i));
                setCommand(i < 0 ? "" : (s.history[i] ?? ""));
                e.preventDefault();
              }
            }}
          />
          {s.running ? (
            <button type="button" className="btn btn--sm btn--danger" onClick={() => tasks.stop()} data-testid="stop" data-hint="Stop the task — Shift+F5">
              <Icon.Stop /> Stop
            </button>
          ) : (
            <button type="button" className="btn btn--sm btn--primary" onClick={run} disabled={!canRun || !command.trim()} data-testid="run" data-hint="Run the command — Enter">
              <Icon.Play /> Run
            </button>
          )}
          <IconButton label="Copy output" hint="Copy all output" onClick={() => void navigator.clipboard.writeText(tasks.allText())} disabled={s.rows.length === 0}>
            <Icon.Copy />
          </IconButton>
          <IconButton label="Clear output" onClick={() => tasks.clear()} disabled={s.rows.length === 0}>
            <Icon.Trash />
          </IconButton>
        </div>
        <div
          ref={scroller}
          className="output"
          role="log"
          aria-live="polite"
          data-testid="output"
          onScroll={(e) => {
            const el = e.currentTarget;
            stick.current = el.scrollTop + el.clientHeight >= el.scrollHeight - ROW_H;
            setView({ top: el.scrollTop, height: el.clientHeight });
          }}
          onContextMenu={(e) => {
            if (s.rows.length === 0) return;
            e.preventDefault();
            setMenuAt({ x: e.clientX, y: e.clientY });
          }}
        >
          {total === 0 ? (
            <div className="output__empty">{s.running ? "Waiting for output…" : "No output yet."}</div>
          ) : (
            <div style={{ height: total * ROW_H, position: "relative" }}>
              {Array.from({ length: Math.max(0, last - first) }, (_, k) => {
                const i = first + k;
                const style = { position: "absolute" as const, top: i * ROW_H, left: 0, right: 0 };
                if (notice && i === 0) {
                  return (
                    <div key="notice" className="output__row output__notice" style={style}>
                      … earlier output truncated ({s.dropped} lines)
                    </div>
                  );
                }
                const r = s.rows[i - notice];
                if (!r) return null;
                return (
                  <div key={r.id} className="output__row" data-stream={r.stream} data-kind={r.kind} style={style}>
                    <span className="output__gutter" aria-hidden>{r.stream === "stderr" && r.kind === "line" ? "▌" : ""}</span>
                    <span>{r.stream === "stderr" && r.kind === "line" && <span className="visually-hidden">stderr: </span>}{r.text}</span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <ContextMenu
          at={menuAt}
          label="Output menu"
          testId="output-menu"
          onClose={() => setMenuAt(null)}
          items={[
            { label: "Copy selection", onSelect: copySelection },
            { label: "Copy all", onSelect: () => void navigator.clipboard.writeText(tasks.allText()) },
          ]}
        />
      </section>
    );
  },
);
