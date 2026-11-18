// The console (workspace-spec.md §2.4; M1.6-SHELL-006): a flat panel header with the task
// meta and tools, virtualised mono output (the view sticks to the bottom while the user has
// not scrolled up), and the recessed `$` prompt with Run/Stop. stderr is marked by a gutter
// glyph and a hidden "stderr:" prefix, not painted red (D6).

import { forwardRef, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { Diagnostic } from "../daemon/protocol.gen";
import { counts, type ProblemsStore } from "../state/problems";
import type { TasksStore } from "../state/tasks";
import type { WorkspaceStore } from "../state/workspace";
import { formatElapsed, useClock } from "../ui/clock";
import { substituteMod } from "../ui/hint";
import * as Icon from "../ui/icons";
import { ContextMenu } from "../ui/overlays";
import { Panel } from "../ui/panel";
import { Count, IconButton, Led, type LedState } from "../ui/primitives";
import { ProblemsView } from "./Problems";
import "../styles/components/console.css";

const ROW_H = 18;

export interface OutputPanelHandle {
  focusCommand(): void;
}

export type ConsoleView = "output" | "problems";

/** Where ←/→/Home/End move between the view tabs. */
export function viewTarget(key: string): ConsoleView | null {
  if (key === "ArrowLeft" || key === "Home") return "output";
  if (key === "ArrowRight" || key === "End") return "problems";
  return null;
}

export const OutputPanel = forwardRef<
  OutputPanelHandle,
  { tasks: TasksStore; workspace: WorkspaceStore; problems: ProblemsStore; ready: boolean; onHide: () => void; onOpenProblem: (d: Diagnostic) => void }
>(
  function OutputPanel({ tasks, workspace, problems, ready, onHide, onOpenProblem }, ref) {
    const s = useSyncExternalStore(tasks.subscribe, tasks.getSnapshot, tasks.getSnapshot);
    const w = useSyncExternalStore(workspace.subscribe, workspace.getSnapshot, workspace.getSnapshot);
    const [command, setCommand] = useState("");
    const [historyIndex, setHistoryIndex] = useState(-1);
    const input = useRef<HTMLInputElement>(null);
    const scroller = useRef<HTMLDivElement>(null);
    const stick = useRef(true);
    const [view, setView] = useState({ top: 0, height: 200 });
    const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
    // The view strip (Output / Problems, M2-UI-003). The count re-renders only the strip.
    const [consoleView, setConsoleView] = useState<ConsoleView>("output");
    const problemCount = useSyncExternalStore(problems.subscribe, () => counts(problems.all()).total, () => 0);
    const tabRefs = useRef<Record<ConsoleView, HTMLButtonElement | null>>({ output: null, problems: null });

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

    // The header does not change with each output batch: memoised, so a batch re-renders only
    // the rows (P12, typing under streaming).
    const empty = s.rows.length === 0;
    const hide = useRef(onHide);
    hide.current = onHide;
    const last0 = tasks.lastLabel;
    const meta = useMemo(
      () => <TaskMeta running={s.running} label={s.label} last={last0} outcome={s.lastOutcome} startedAt={s.startedAt} />,
      [s.running, s.label, last0, s.lastOutcome, s.startedAt],
    );
    const tools = useMemo(
      () => (
        <>
          <IconButton label="Copy all output" hint="Copy all output" onClick={() => void navigator.clipboard.writeText(tasks.allText())} disabled={empty}>
            <Icon.Copy />
          </IconButton>
          <IconButton label="Clear output" hint="Clear the console" onClick={() => tasks.clear()} disabled={empty}>
            <Icon.Trash />
          </IconButton>
          <IconButton label="Hide console" shortcut={`${substituteMod("Mod")}+J`} hint="Hide the console — Mod+J" onClick={() => hide.current()}>
            <Icon.Close />
          </IconButton>
        </>
      ),
      [empty, tasks],
    );
    const heading = (
      <div className="console__views" role="tablist" aria-label="Console views">
        {(["output", "problems"] as const).map((v) => (
          <button
            key={v}
            ref={(el) => {
              tabRefs.current[v] = el;
            }}
            type="button"
            role="tab"
            className="console__view"
            id={`console-tab-${v}`}
            aria-selected={consoleView === v}
            aria-controls={v === "output" ? "console-output" : "console-problems"}
            tabIndex={consoleView === v ? 0 : -1}
            data-view={v}
            aria-label={v === "output" ? "Output" : problemCount > 0 ? `Problems, ${problemCount}` : "Problems"}
            data-hint={v === "output" ? "Show the command output" : "Show problems from the last build"}
            onClick={() => setConsoleView(v)}
            onKeyDown={(e) => {
              const to = viewTarget(e.key);
              if (!to) return;
              e.preventDefault();
              setConsoleView(to);
              tabRefs.current[to]?.focus();
            }}
          >
            {v === "output" ? "Output" : "Problems"}
            {v === "problems" && problemCount > 0 && <Count value={problemCount} />}
          </button>
        ))}
      </div>
    );
    return (
      <Panel title="Console" heading={heading} className="console" testId="output-panel" meta={meta} tools={tools}
      >
        {consoleView === "problems" && <ProblemsView store={problems} onOpen={onOpenProblem} id="console-problems" />}
        <div
          ref={scroller}
          id="console-output"
          hidden={consoleView !== "output"}
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
            <div className="output__empty">{s.running ? "Waiting for output…" : "Run a command in this workspace"}</div>
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
                const err = r.stream === "stderr" && r.kind === "line";
                return (
                  <div key={r.id} className="output__row" data-stream={r.stream} data-kind={r.kind} style={style}>
                    <span className="output__gutter" aria-hidden>{err ? "▏" : ""}</span>
                    <span>{err && <span className="visually-hidden">stderr: </span>}{r.text}</span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
        <div className="prompt">
          <span className="prompt__sigil" aria-hidden="true">$</span>
          <input
            ref={input}
            className="prompt__input"
            placeholder={w.id ? "Run a command  (↑ history)" : "Open a folder to run commands"}
            aria-label="Command"
            value={command}
            disabled={!w.id}
            spellCheck={false}
            autoComplete="off"
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
            <button type="button" className="btn btn--sm btn--danger-quiet" onClick={() => tasks.stop()} data-testid="stop" data-hint="Stop the task — Shift+F5">
              <Icon.Stop /> Stop
            </button>
          ) : (
            <button type="button" className="btn btn--sm btn--quiet" onClick={run} disabled={!canRun || !command.trim()} data-testid="run" data-hint="Run the command — Enter">
              <Icon.Play /> Run
            </button>
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
      </Panel>
    );
  },
);

/** Header meta: the task LED, its label (mono) and the elapsed time while running; afterwards the last command and its outcome. */
function TaskMeta({ running, label, last, outcome, startedAt }: { running: boolean; label: string; last: string; outcome: string | null; startedAt: number | null }) {
  // Its own component, so the 1 Hz clock re-renders only this and not the output rows.
  const now = useClock(running);
  const led: LedState = running ? "run" : outcome === "succeeded" ? "on" : outcome ? "err" : "off";
  const text = running ? label : outcome ? last : "";
  if (!text) return null;
  return (
    <span className="console__meta" data-testid="console-meta" data-state={led}>
      <Led state={led} label={running ? "Task running" : led === "on" ? "Last task succeeded" : "Last task failed"} />
      <span className="console__label" title={text}>{text}</span>
      {running && startedAt !== null && <span className="console__time">{formatElapsed(now - startedAt)}</span>}
    </span>
  );
}
