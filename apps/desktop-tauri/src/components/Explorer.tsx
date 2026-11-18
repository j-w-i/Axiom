// File explorer: a virtualised, keyboard-navigable ARIA tree over WorkspaceStore.
// Only the rows in view are rendered, so a 10 000-entry expanded tree costs
// the same DOM as a 40-entry one.

import { useEffect, useMemo, useRef, useState, useSyncExternalStore, type CSSProperties } from "react";
import type { WorkspaceStore } from "../state/workspace";
import * as Icon from "../ui/icons";
import "../styles/components/tree.css";

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

export function Explorer({
  workspace,
  onOpenFile,
  dirty,
}: {
  workspace: WorkspaceStore;
  onOpenFile: (path: string) => void;
  /** Paths of documents open and dirty in the editor (the unsaved dot). */
  dirty?: ReadonlySet<string>;
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

  const focusIndex = Math.max(0, rows.findIndex((r) => r.node.path === focused));
  const ensureVisible = (i: number) => {
    const el = scroller.current;
    if (!el) return;
    const top = i * ROW_H;
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

  if (!s.id) {
    return <div className="tree-empty">{s.opening ? "Opening…" : "No folder open"}</div>;
  }

  const first = Math.max(0, Math.floor(view.top / ROW_H) - OVERSCAN);
  const last = Math.min(rows.length, Math.ceil((view.top + view.height) / ROW_H) + OVERSCAN);
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
      <div style={{ height: rows.length * ROW_H, position: "relative" }}>
        {rows.slice(first, last).map(({ node, depth }, k) => {
          const i = first + k;
          const isDir = node.kind === "directory";
          const unsaved = !isDir && !!dirty?.has(node.path);
          const glyph = glyphFor(node.name);
          return (
            <div key={node.path} style={{ position: "absolute", top: i * ROW_H, left: 0, right: 0 }}>
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
                <div className="tree-row tree-row--error" style={{ "--depth": depth + 1 } as CSSProperties}>
                  {node.error}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
