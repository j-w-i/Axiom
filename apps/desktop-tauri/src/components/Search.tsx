// The Search page of the side bar (M4-UX-001): a query with Match case, Whole word and Regular
// expression toggles, files to include and exclude, and the matches grouped by file. A match row
// is a button (Enter or a click opens the file there); ↑/↓/Home/End move between rows (one Tab
// stop). Mod+Shift+F focuses the query.

import { useEffect, useRef, useSyncExternalStore, type KeyboardEvent } from "react";
import type { SearchMatch } from "../daemon/protocol.gen";
import type { SearchStore } from "../state/search";
import { Field, Spinner } from "../ui/primitives";
import { problemTarget } from "./Problems";
import "../styles/components/search.css";

/** Focus requests for the query field (Mod+Shift+F), across mounts. */
let focusNext = false;
export function requestSearchFocus(): void {
  focusNext = true;
  document.querySelector<HTMLInputElement>("[data-testid=search] input[aria-label='Search']")?.focus();
}

function Toggle({ label, text, pressed, onToggle }: { label: string; text: string; pressed: boolean; onToggle: () => void }) {
  return (
    <button type="button" className="search__toggle" aria-label={label} aria-pressed={pressed} data-hint={label} onClick={onToggle}>
      {text}
    </button>
  );
}

function Preview({ m }: { m: SearchMatch }) {
  // Columns are UTF-16 units, as JavaScript strings index.
  const a = m.previewColumn - 1;
  const b = a + (m.endColumn - m.column);
  return (
    <span className="search__preview">
      {m.preview.slice(0, a)}
      <mark className="search__hit">{m.preview.slice(a, b)}</mark>
      {m.preview.slice(b)}
    </span>
  );
}

export function SearchSide({ store, onOpen }: { store: SearchStore; onOpen: (path: string, line: number, column: number) => void }) {
  const s = useSyncExternalStore(store.subscribe, store.getSnapshot, store.getSnapshot);
  const input = useRef<HTMLInputElement>(null);
  const rows = useRef<(HTMLButtonElement | null)[]>([]);
  useEffect(() => {
    if (focusNext) {
      focusNext = false;
      input.current?.focus();
    }
  }, []);
  const flat = s.groups.flatMap((g) => g.matches);
  const onKey = (e: KeyboardEvent<HTMLButtonElement>, i: number) => {
    const to = problemTarget(e.key, i, flat.length);
    if (to === null) return;
    e.preventDefault();
    rows.current[to]?.focus();
  };
  let index = -1;
  const status = s.error
    ? s.error
    : !s.query
      ? ""
      : s.running && s.total === 0
        ? "Searching…"
        : s.total === 0
          ? "No results"
          : `${s.total}${s.truncated ? "+" : ""} result${s.total === 1 ? "" : "s"} in ${s.groups.length} file${s.groups.length === 1 ? "" : "s"}${s.truncated ? " (the first shown)" : ""}`;
  return (
    <div className="search" data-testid="search">
      <div className="search__form">
        <div className="search__row">
          <Field
            ref={input}
            aria-label="Search"
            placeholder="Search"
            value={s.query}
            spellCheck={false}
            onChange={(e) => store.update({ query: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter") store.now();
              if (e.key === "ArrowDown" && flat.length) {
                e.preventDefault();
                rows.current[0]?.focus();
              }
            }}
            data-hint="Search the workspace's files — Enter"
          />
          <span className="search__toggles">
            <Toggle label="Match case" text="Aa" pressed={s.caseSensitive} onToggle={() => store.update({ caseSensitive: !s.caseSensitive })} />
            <Toggle label="Whole word" text="ab" pressed={s.wholeWord} onToggle={() => store.update({ wholeWord: !s.wholeWord })} />
            <Toggle label="Regular expression" text=".*" pressed={s.regex} onToggle={() => store.update({ regex: !s.regex })} />
          </span>
        </div>
        <Field mono aria-label="Files to include" placeholder="Files to include (e.g. src/**, *.rs)" value={s.include} onChange={(e) => store.update({ include: e.target.value })} data-hint="Only paths matching these globs (comma-separated)" />
        <Field mono aria-label="Files to exclude" placeholder="Files to exclude" value={s.exclude} onChange={(e) => store.update({ exclude: e.target.value })} data-hint="Skip paths matching these globs (comma-separated)" />
      </div>
      <p className="search__status" role="status" aria-live="polite">
        {s.running && s.total > 0 && <Spinner />} {status}
      </p>
      {s.groups.map((g) => (
        <section key={g.path} className="search__group" aria-label={`${g.path}, ${g.matches.length} match${g.matches.length === 1 ? "" : "es"}`}>
          <h3 className="search__file">
            <span className="search__path">{g.path}</span>
            <span className="search__count">{g.matches.length}</span>
          </h3>
          <ul className="search__list">
            {g.matches.map((m) => {
              const i = ++index;
              return (
                <li key={`${m.line}:${m.column}`}>
                  <button
                    ref={(el) => {
                      rows.current[i] = el;
                    }}
                    type="button"
                    className="search__match"
                    tabIndex={i === 0 ? 0 : -1}
                    aria-label={`${g.path}, line ${m.line}: ${m.preview.trim()}`}
                    data-hint={`Open ${g.path} at line ${m.line}`}
                    onClick={() => onOpen(m.path, m.line, m.column)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        onOpen(m.path, m.line, m.column);
                        return;
                      }
                      onKey(e, i);
                    }}
                  >
                    <span className="search__line">{m.line}</span>
                    <Preview m={m} />
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
