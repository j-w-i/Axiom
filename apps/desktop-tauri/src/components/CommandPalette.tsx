// Command palette (Mod+K) and quick open of already-listed files (Mod+P).
// Quick open searches only what the explorer has listed: a workspace-wide file
// index is M4 epic 4.5 and belongs to the daemon, not the UI.

import * as RDialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useRef, useState } from "react";

export interface PaletteCommand {
  id: string;
  group: string;
  label: string;
  shortcut?: string;
  run: () => void;
}

function score(query: string, text: string): number {
  if (!query) return 1;
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  const idx = t.indexOf(q);
  if (idx >= 0) return 1000 - idx - t.length * 0.01;
  // subsequence match
  let ti = 0;
  for (const c of q) {
    ti = t.indexOf(c, ti);
    if (ti < 0) return 0;
    ti++;
  }
  return 100 - t.length * 0.01;
}

export function CommandPalette({
  open,
  onOpenChange,
  commands,
  placeholder,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  commands: PaletteCommand[];
  placeholder: string;
}) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const list = useRef<HTMLUListElement>(null);
  const results = useMemo(
    () =>
      commands
        .map((c) => ({ c, s: score(query, c.label) }))
        .filter((r) => r.s > 0)
        .sort((a, b) => b.s - a.s)
        .slice(0, 200)
        .map((r) => r.c),
    [commands, query],
  );
  useEffect(() => {
    if (open) {
      setQuery("");
      setIndex(0);
    }
  }, [open]);
  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const choose = (c: PaletteCommand | undefined) => {
    if (!c) return;
    onOpenChange(false);
    // Let the dialog close (and return focus) before running.
    setTimeout(c.run, 0);
  };

  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="overlay" />
        <RDialog.Content className="palette" aria-describedby={undefined} data-testid="palette">
          <RDialog.Title className="visually-hidden">Command palette</RDialog.Title>
          <input
            className="palette__input"
            autoFocus
            placeholder={placeholder}
            value={query}
            role="combobox"
            aria-expanded
            aria-controls="palette-list"
            aria-activedescendant={results[index] ? `pal-${results[index]!.id}` : undefined}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") setIndex((i) => Math.min(results.length - 1, i + 1));
              else if (e.key === "ArrowUp") setIndex((i) => Math.max(0, i - 1));
              else if (e.key === "Enter") choose(results[index]);
              else return;
              e.preventDefault();
            }}
          />
          <ul className="palette__list" role="listbox" id="palette-list" ref={list}>
            {results.length === 0 && <li className="palette__empty">No matches</li>}
            {results.map((c, i) => (
              <li
                key={c.id}
                id={`pal-${c.id}`}
                className="palette__item"
                role="option"
                aria-selected={i === index}
                onMouseMove={() => setIndex(i)}
                onClick={() => choose(c)}
              >
                <span className="palette__group">{c.group}</span>
                <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{c.label}</span>
                {c.shortcut && <span className="menu__shortcut">{c.shortcut}</span>}
              </li>
            ))}
          </ul>
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}
