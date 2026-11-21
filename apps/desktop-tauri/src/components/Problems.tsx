// The console's Problems view (workspace-spec.md §2.4, §5.1; M2-UI-003): diagnostics of the
// latest run of each provider task, grouped by file, severity first then position. A row is a
// button: Enter or a click opens the file at the position. ↑/↓ move between rows (one Tab stop).

import { useRef, useSyncExternalStore, type KeyboardEvent } from "react";
import type { Diagnostic } from "../daemon/protocol.gen";
import { groupProblems, positionText, type ProblemsStore } from "../state/problems";
import "../styles/components/problems.css";

const GLYPH: Record<Diagnostic["severity"], string> = { error: "✖", warning: "▲", info: "●", hint: "·" };
const SEVERITY_NAME: Record<Diagnostic["severity"], string> = { error: "Error", warning: "Warning", info: "Info", hint: "Hint" };

/** Where ↑/↓/Home/End move among `count` rows (no wrap). */
export function problemTarget(key: string, index: number, count: number): number | null {
  switch (key) {
    case "ArrowDown":
      return Math.min(count - 1, index + 1);
    case "ArrowUp":
      return Math.max(0, index - 1);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

export function ProblemsView({ store, onOpen, id }: { store: ProblemsStore; onOpen: (d: Diagnostic) => void; id: string }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const all = store.all();
  const groups = groupProblems(all);
  const flat = groups.flatMap((g) => g.rows);
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const runs = Object.values(s.runs);
  const pending = runs.some((r) => !s.byTask[r.taskId]?.complete);

  if (all.length === 0) {
    return (
      <div className="problems problems--empty" id={id} role="tabpanel" aria-label="Problems" data-testid="problems">
        {runs.length === 0 ? "Run a build or test task to see its problems" : pending ? "No problems so far" : "No problems in the last run"}
      </div>
    );
  }

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const to = problemTarget(e.key, i, flat.length);
    if (to === null) return;
    e.preventDefault();
    refs.current[to]?.focus();
  };

  let index = -1;
  return (
    <div className="problems" id={id} role="tabpanel" aria-label="Problems" data-testid="problems">
      {groups.map((g) => (
        <section key={g.file || "(none)"} className="problems__group" aria-label={g.file || "No location"}>
          <h3 className="problems__file">
            <span className="problems__path">{g.file || "No location"}</span>
            <span className="problems__count">{g.rows.length}</span>
          </h3>
          <ul className="problems__list">
            {g.rows.map((r) => {
              const i = ++index;
              const d = r.diagnostic;
              const at = positionText(d);
              return (
                <li key={r.id}>
                  <button
                    ref={(el) => {
                      refs.current[i] = el;
                    }}
                    type="button"
                    className="problem"
                    data-severity={d.severity}
                    tabIndex={i === 0 ? 0 : -1}
                    aria-disabled={!d.location || undefined}
                    aria-label={`${SEVERITY_NAME[d.severity]}: ${d.message}${d.code ? ` (${d.code})` : ""}${at ? ` at ${at}` : ""}`}
                    data-hint={d.location ? `Open ${at}` : d.message}
                    title={d.rendered ?? undefined}
                    onKeyDown={(e) => onKeyDown(e, i)}
                    onClick={() => d.location && onOpen(d)}
                  >
                    <span className="problem__glyph" aria-hidden="true">
                      {GLYPH[d.severity]}
                    </span>
                    <span className="problem__message">{d.message}</span>
                    {d.code && <span className="problem__code">{d.code}</span>}
                    {at && <span className="problem__at">{at}</span>}
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
