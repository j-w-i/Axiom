// Command palette (Mod+K) and quick open of already-listed files (Mod+P), as designed in
// workspace-spec.md §3 (M1.6-SURFACES-001): overlay material, a mode chip, grouped rows with
// keycaps, one gliding selection, match highlighting in file mode, a footer of keys,
// recent-first commands and designed empty states. Quick open searches only what the
// explorer has listed: a workspace-wide file index is M4 epic 4.5 and belongs to the daemon.

import * as RDialog from "@radix-ui/react-dialog";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { substituteMod } from "../ui/hint";
import * as Icon from "../ui/icons";
import { useReturnFocus } from "../ui/overlays";
import { Kbd, keyLabels } from "../ui/primitives";
import { glyphFor } from "./Explorer";
import "../styles/components/glyph.css";
import "../styles/components/palette.css";

export interface PaletteCommand {
  id: string;
  group: string;
  label: string;
  shortcut?: string;
  run: () => void;
}

export type PaletteMode = "commands" | "files";

/** Subsequence scorer (unchanged): substring matches rank first, earlier and shorter first. */
export function score(query: string, text: string): number {
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

/** The matched ranges [start, end) of `query` in `text`, as the scorer matches it (merged). */
export function matchRanges(query: string, text: string): [number, number][] {
  if (!query) return [];
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  const idx = t.indexOf(q);
  if (idx >= 0) return [[idx, idx + q.length]];
  const out: [number, number][] = [];
  let ti = 0;
  for (const c of q) {
    ti = t.indexOf(c, ti);
    if (ti < 0) return [];
    const last = out[out.length - 1];
    if (last && last[1] === ti) last[1] = ti + 1;
    else out.push([ti, ti + 1]);
    ti++;
  }
  return out;
}

/** The ranked results (at most 200) for a query. */
export function filterCommands(commands: PaletteCommand[], query: string): PaletteCommand[] {
  return commands
    .map((c) => ({ c, s: score(query, c.label) }))
    .filter((r) => r.s > 0)
    .sort((a, b) => b.s - a.s)
    .slice(0, 200)
    .map((r) => r.c);
}

// ---- recent commands (shell-local MRU) ---------------------------------------------------

export const MRU_KEY = "axiom.palette.mru.v1";
export const MRU_MAX = 8;

/** `commands` with the recently run ones first (most recent first), the rest in their order. */
export function orderByMru(commands: PaletteCommand[], mru: readonly string[]): PaletteCommand[] {
  const byId = new Map(commands.map((c) => [c.id, c]));
  const recent = mru.map((id) => byId.get(id)).filter((c): c is PaletteCommand => !!c);
  const seen = new Set(recent.map((c) => c.id));
  return [...recent, ...commands.filter((c) => !seen.has(c.id))];
}

/** The MRU list after running `id`: moved to the front, at most MRU_MAX. */
export function pushMru(mru: readonly string[], id: string): string[] {
  return [id, ...mru.filter((x) => x !== id)].slice(0, MRU_MAX);
}

export function loadMru(storage: Pick<Storage, "getItem"> = localStorage): string[] {
  try {
    const v: unknown = JSON.parse(storage.getItem(MRU_KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string").slice(0, MRU_MAX) : [];
  } catch {
    return [];
  }
}

function saveMru(mru: string[]): void {
  try {
    localStorage.setItem(MRU_KEY, JSON.stringify(mru));
  } catch {
    /* storage full or unavailable: ordering falls back to the default */
  }
}

// ---- the palette -----------------------------------------------------------------------

/** `text` with the parts inside `ranges` (offsets into the full string, shifted by `offset`) marked. */
function highlight(text: string, ranges: [number, number][], offset: number): ReactNode {
  const parts: ReactNode[] = [];
  let at = 0;
  for (const [s0, e0] of ranges) {
    const s = Math.max(0, s0 - offset);
    const e = Math.min(text.length, e0 - offset);
    if (e <= s) continue;
    if (s > at) parts.push(text.slice(at, s));
    parts.push(<mark key={s}>{text.slice(s, e)}</mark>);
    at = e;
  }
  if (at < text.length) parts.push(text.slice(at));
  return parts;
}

/** What the list shows when nothing matches. `openFolder` is set when it offers that action. */
export function emptyState(mode: PaletteMode, hasWorkspace: boolean): { title: string; hint: string; openFolder: boolean } {
  if (mode === "files" && !hasWorkspace) return { title: "Open a folder to search files", hint: "", openFolder: true };
  if (mode === "files") return { title: "No matching files", hint: "Only files listed in the explorer are searched", openFolder: false };
  return { title: "No matching commands", hint: "Try fewer letters", openFolder: false };
}

export function CommandPalette({
  open,
  mode,
  onOpenChange,
  commands,
  hasWorkspace,
  onOpenFolder,
}: {
  open: boolean;
  mode: PaletteMode;
  onOpenChange: (o: boolean) => void;
  commands: PaletteCommand[];
  hasWorkspace: boolean;
  onOpenFolder: () => void;
}) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const [mru, setMru] = useState<string[]>(() => loadMru());
  const list = useRef<HTMLUListElement>(null);
  const returnFocus = useReturnFocus(open);

  const openFolderRow: PaletteCommand = { id: "palette:open-folder", group: "File", label: "Open Folder…", shortcut: "Ctrl+O", run: onOpenFolder };
  const empty = emptyState(mode, hasWorkspace);
  const results = useMemo(() => {
    if (mode === "files" && !hasWorkspace) return [openFolderRow];
    const ordered = mode === "commands" && !query ? orderByMru(commands, mru) : commands;
    return filterCommands(ordered, query);
    // openFolderRow is rebuilt each render; its identity does not matter here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [commands, query, mode, hasWorkspace, mru]);

  useEffect(() => {
    if (open) {
      setQuery("");
      setIndex(0);
    }
  }, [open, mode]);
  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    list.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const choose = (c: PaletteCommand | undefined) => {
    if (!c) return;
    if (mode === "commands" && c.id !== openFolderRow.id) {
      const next = pushMru(mru, c.id);
      setMru(next);
      saveMru(next);
    }
    onOpenChange(false);
    // Let the dialog close (and return focus) before running.
    setTimeout(c.run, 0);
  };

  const mod = substituteMod("Mod");
  const noMatch = results.length === 0 || (mode === "files" && !hasWorkspace);
  return (
    <RDialog.Root open={open} onOpenChange={onOpenChange}>
      <RDialog.Portal>
        <RDialog.Overlay className="overlay" />
        <RDialog.Content className="palette" data-mode={mode} aria-describedby={undefined} data-testid="palette" onCloseAutoFocus={returnFocus}>
          <RDialog.Title className="visually-hidden">{mode === "files" ? "Go to file" : "Command palette"}</RDialog.Title>
          <div className="palette__bar">
            <Icon.Search className="palette__icon" />
            <input
              className="palette__input"
              autoFocus
              placeholder={mode === "files" ? "Go to a listed file…" : "Type a command…"}
              value={query}
              role="combobox"
              aria-expanded
              aria-controls="palette-list"
              aria-autocomplete="list"
              aria-activedescendant={results[index] ? `pal-${results[index]!.id}` : undefined}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") setIndex((i) => Math.min(results.length - 1, i + 1));
                else if (e.key === "ArrowUp") setIndex((i) => Math.max(0, i - 1));
                else if (e.key === "Enter") choose(results[index]);
                else return;
                e.preventDefault();
              }}
            />
            <span className="palette__mode">{mode === "files" ? "Files" : "Commands"}</span>
          </div>
          <ul className="palette__list" role="listbox" id="palette-list" aria-label={mode === "files" ? "Files" : "Commands"} ref={list} style={{ "--sel": index } as CSSProperties}>
            {noMatch && (
              <li className="palette__empty" role="presentation">
                <span className="palette__empty-title">{empty.title}</span>
                {empty.hint && <span className="palette__empty-hint">{empty.hint}</span>}
              </li>
            )}
            {results.length > 0 && <li className="palette__glide" role="presentation" aria-hidden="true" />}
            {results.map((c, i) =>
              mode === "files" && hasWorkspace ? (
                <FileRow key={c.id} c={c} query={query} selected={i === index} onHover={() => setIndex(i)} onChoose={() => choose(c)} />
              ) : (
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
                  <span className="palette__label">{c.label}</span>
                  {c.shortcut && (
                    <span className="keys palette__keys">
                      {keyLabels(c.shortcut.replace(/\+/g, " ")).map((k) => (
                        <Kbd key={k}>{k}</Kbd>
                      ))}
                    </span>
                  )}
                </li>
              ),
            )}
          </ul>
          <div className="palette__foot" aria-hidden="true">
            <span>
              <Kbd>↑</Kbd>
              <Kbd>↓</Kbd> move
            </span>
            <span>
              <Kbd>↵</Kbd> run
            </span>
            <span>
              <Kbd>Esc</Kbd> close
            </span>
            <span className="palette__foot-spacer" />
            <span>{mode === "files" ? `${mod} K for commands` : `${mod} P for files`}</span>
          </div>
        </RDialog.Content>
      </RDialog.Portal>
    </RDialog.Root>
  );
}

/** A quick-open row: glyph chip, file name, directory; the matched characters highlighted. */
function FileRow({ c, query, selected, onHover, onChoose }: { c: PaletteCommand; query: string; selected: boolean; onHover: () => void; onChoose: () => void }) {
  const path = c.label;
  const cut = path.lastIndexOf("/");
  const name = path.slice(cut + 1);
  const dir = cut >= 0 ? path.slice(0, cut) : "";
  const ranges = matchRanges(query, path);
  const glyph = glyphFor(name);
  return (
    <li id={`pal-${c.id}`} className="palette__item" role="option" aria-selected={selected} onMouseMove={onHover} onClick={onChoose}>
      <span className="glyph" data-kind={glyph.kind} aria-hidden="true">
        {glyph.label}
      </span>
      <span className="palette__label">{highlight(name, ranges, cut + 1)}</span>
      {dir && <span className="palette__dir">{highlight(dir, ranges, 0)}</span>}
    </li>
  );
}
