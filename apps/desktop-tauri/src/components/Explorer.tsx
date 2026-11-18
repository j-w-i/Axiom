// File explorer: a virtualised, keyboard-navigable ARIA tree over WorkspaceStore.
// Only the rows in view are rendered, so a 10 000-entry expanded tree costs
// the same DOM as a 40-entry one. Its non-content states (opening, empty folder, listing
// errors) follow workspace-spec.md §6 (M1.6-SHELL-007).

import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { WorkspaceStore } from "../state/workspace";
import * as Icon from "../ui/icons";
import { Button } from "../ui/primitives";
import "../styles/components/tree.css";
import "../styles/components/states.css";

/** Row height, px: fixed, because virtualisation computes positions from it (--row-h). */
const ROW_H = 22;

/** The glyph chip of a file: a two-letter label and the hue family (design-system.md §7). */
export function glyphFor(name: string): { label: string; kind: "rs" | "md" | "toml" | "other" } {
  const dot = name.lastIndexOf(".");
  const ext = dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
  if (ext === "rs") return { label: "rs", kind: "rs" };
  if (ext === "md") return { label: "md", kind: "md" };
  if (ext === "toml") return { label: "tm", kind: "toml" };
  if (ext === "lock") return { label: "lk", kind: "toml" };
  return { label: ext.slice(0, 2), kind: "other" };
}
const OVERSCAN = 8;
/** Slot index of each row (an error row under a directory takes one more slot) and the total. */
export function rowSlots(hasError: boolean[]): { slot: number[]; total: number } {
  const slot: number[] = [];
  let n = 0;
  for (const e of hasError) {
    slot.push(n);
    n += e ? 2 : 1;
  }
  return { slot, total: n };
}

/** The first row whose slot is at or after `target` (slots ascend); rows.length if none. */
function firstAtOrAfter(slot: number[], target: number): number {
  let lo = 0;
  let hi = slot.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (slot[mid]! < target) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Skeleton rows are shown this long while a folder is listed; then "Still listing…". */
export const SKELETON_MS = 1000;
/** Skeleton bar widths (% of the row): static, no shimmer (motion-system.md §5). */
const SKELETON = [62, 48, 70, 40, 56, 44];

export function Explorer({
  workspace,
  onOpenFile,
  dirty,
  onReopen,
}: {
  workspace: WorkspaceStore;
  onOpenFile: (path: string) => void;
  /** Paths of documents open and dirty in the editor (the unsaved dot). */
  dirty?: ReadonlySet<string>;
  /** Retry after the workspace root could not be listed: open the folder again. */
  onReopen?: () => void;
}) {
  const s = useSyncExternalStore(workspace.subscribe, workspace.getSnapshot);
  const rows = useMemo(() => workspace.visibleRows(), [workspace, s.treeVersion]);
  const [focused, setFocused] = useState<string>("");
  const [selected, setSelected] = useState<string>("");
  const scroller = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ top: 0, height: 600 });

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

  // A directory that could not be listed takes a second 22 px slot for its error row, so the
  // rows after it move down instead of being drawn over it.
  const slots = useMemo(() => rowSlots(rows.map((r) => !!r.node.error)), [rows]);
  const focusIndex = Math.max(0, rows.findIndex((r) => r.node.path === focused));
  const ensureVisible = (i: number) => {
    const el = scroller.current;
    if (!el) return;
    const top = (slots.slot[i] ?? i) * ROW_H;
    if (top < el.scrollTop) el.scrollTop = top;
    else if (top + ROW_H > el.scrollTop + el.clientHeight) el.scrollTop = top + ROW_H - el.clientHeight;
  };
  const moveTo = (i: number) => {
    const r = rows[Math.max(0, Math.min(rows.length - 1, i))];
    if (!r) return;
    setFocused(r.node.path);
    ensureVisible(rows.indexOf(r));
  };
  const activate = (path: string) => {
    const n = workspace.node(path);
    if (!n) return;
    setSelected(path);
    setFocused(path);
    if (n.kind === "directory") workspace.toggle(path);
    else onOpenFile(path);
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    const cur = rows[focusIndex];
    switch (e.key) {
      case "ArrowDown": moveTo(focusIndex + 1); break;
      case "ArrowUp": moveTo(focusIndex - 1); break;
      case "Home": moveTo(0); break;
      case "End": moveTo(rows.length - 1); break;
      case "PageDown": moveTo(focusIndex + Math.floor(view.height / ROW_H)); break;
      case "PageUp": moveTo(focusIndex - Math.floor(view.height / ROW_H)); break;
      case "ArrowRight":
        if (cur?.node.kind === "directory") {
          if (!cur.node.expanded) workspace.setExpanded(cur.node.path, true);
          else moveTo(focusIndex + 1);
        }
        break;
      case "ArrowLeft":
        if (cur?.node.kind === "directory" && cur.node.expanded) workspace.setExpanded(cur.node.path, false);
        else if (cur) {
          const parent = cur.node.path.includes("/") ? cur.node.path.slice(0, cur.node.path.lastIndexOf("/")) : "";
          const pi = rows.findIndex((r) => r.node.path === parent);
          if (pi >= 0) moveTo(pi);
        }
        break;
      case "Enter":
      case " ":
        if (cur) activate(cur.node.path);
        break;
      default:
        return;
    }
    e.preventDefault();
  };

  if (!s.id && !s.opening) return <div className="tree-state">No folder open</div>;
  if (!s.rootLoaded) {
    if (s.opening) return <Listing />;
    return (
      <div className="tree-state tree-state--error" role="alert" data-testid="tree-error">
        <span className="tree-state__glyph" aria-hidden="true">✖</span>
        <span className="tree-state__text">The folder could not be listed, so its files are not shown.</span>
        {onReopen && (
          <Button variant="quiet" size="sm" onClick={onReopen} data-hint="List the folder again">
            Retry
          </Button>
        )}
      </div>
    );
  }
  if (rows.length === 0) {
    return (
      <div className="tree-state" data-testid="tree-empty">
        <span className="tree-state__text">This folder is empty</span>
        <Button variant="quiet" size="sm" onClick={() => workspace.applyChanges([""])} data-hint="List the folder again">
          <Icon.Refresh /> Refresh
        </Button>
      </div>
    );
  }

  const first = Math.max(0, firstAtOrAfter(slots.slot, Math.floor(view.top / ROW_H)) - OVERSCAN);
  const last = Math.min(rows.length, firstAtOrAfter(slots.slot, Math.ceil((view.top + view.height) / ROW_H)) + OVERSCAN);
  const activeId = rows[focusIndex] ? `tree-${rows[focusIndex]!.node.path}` : undefined;

  return (
    <div
      ref={scroller}
      className="tree"
      role="tree"
      aria-label="Files"
      tabIndex={0}
      aria-activedescendant={activeId}
      onKeyDown={onKeyDown}
      onScroll={(e) => setView({ top: e.currentTarget.scrollTop, height: e.currentTarget.clientHeight })}
      data-testid="explorer"
    >
      <div style={{ height: slots.total * ROW_H, position: "relative" }}>
        {rows.slice(first, last).map(({ node, depth }, k) => {
          const i = first + k;
          const isDir = node.kind === "directory";
          const unsaved = !isDir && !!dirty?.has(node.path);
          const glyph = glyphFor(node.name);
          return (
            <div key={node.path} style={{ position: "absolute", top: (slots.slot[i] ?? i) * ROW_H, left: 0, right: 0 }}>
              <div
                id={`tree-${node.path}`}
                className="tree-row"
                role="treeitem"
                aria-level={depth + 1}
                aria-expanded={isDir ? node.expanded : undefined}
                aria-selected={selected === node.path}
                aria-label={unsaved ? `${node.name}, unsaved` : undefined}
                data-focused={focused === node.path}
                data-ignored={node.ignored}
                data-path={node.path}
                title={node.path}
                style={{ "--depth": depth } as CSSProperties}
                onClick={() => activate(node.path)}
              >
                {Array.from({ length: depth }, (_, g) => (
                  <span key={g} className="tree-row__guide" style={{ "--g": g } as CSSProperties} aria-hidden="true" />
                ))}
                <span className="tree-row__chevron" data-open={isDir ? node.expanded : undefined} aria-hidden="true">
                  {isDir && <Icon.Chevron />}
                </span>
                {isDir ? (
                  <span className="tree-row__icon" aria-hidden="true">
                    {node.expanded ? <Icon.FolderOpen /> : <Icon.Folder />}
                  </span>
                ) : node.kind === "symlink" ? (
                  <span className="tree-row__icon" aria-hidden="true">
                    <Icon.Link />
                  </span>
                ) : (
                  <span className="glyph" data-kind={glyph.kind} aria-hidden="true">
                    {glyph.label}
                  </span>
                )}
                <span className="tree-row__name">{node.name}</span>
                {node.loading && !node.loaded && <span className="spinner" aria-label="Loading" />}
                {unsaved && (
                  <span className="tree-row__dot" aria-hidden="true">
                    ●
                  </span>
                )}
              </div>
              {node.error && (
                <div className="tree-row tree-row--error" style={{ "--depth": depth + 1 } as CSSProperties} title={node.error}>
                  <span className="tree-row__glyph" aria-hidden="true">✖</span>
                  <span className="tree-row__name">{node.error}</span>
                  <button
                    type="button"
                    className="tree-row__retry"
                    data-hint="List this folder again"
                    onClick={(e) => {
                      e.stopPropagation();
                      // The existing refresh path: re-expanding a directory lists it again.
                      workspace.setExpanded(node.path, false);
                      workspace.setExpanded(node.path, true);
                    }}
                  >
                    Retry
                  </button>
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Opening a workspace: six static skeleton rows for up to a second, then "Still listing…". */
function Listing() {
  const [late, setLate] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setLate(true), SKELETON_MS);
    return () => clearTimeout(t);
  }, []);
  if (late) {
    return (
      <div className="tree-state" role="status" data-testid="tree-opening">
        <span className="tree-state__text tree-state__text--faint">Still listing…</span>
      </div>
    );
  }
  return (
    <div className="tree-skeleton" role="status" aria-label="Listing the folder" data-testid="tree-opening">
      {SKELETON.map((w, i) => (
        <div key={i} className="tree-skeleton__row" aria-hidden="true">
          <span className="tree-skeleton__bar" style={{ width: `${w}%` }} />
        </div>
      ))}
    </div>
  );
}
