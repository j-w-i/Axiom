import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent, type PointerEvent as ReactPointerEvent, type RefObject, type WheelEvent } from "react";
import type { Doc, DocumentsStore } from "../state/documents";
import * as Icon from "../ui/icons";
import { ErrorBoundary } from "./ErrorBoundary";
import { glyphFor } from "./Explorer";
import { currentMotion } from "../ui/motion";
import { Kbd } from "../ui/primitives";
import "../styles/components/glyph.css";
import "../styles/components/states.css";
import "../styles/components/tabs.css";

// Monaco is ~4 MB of JS: it loads when the first workspace opens, not at
// startup, so cold start and idle memory do not pay for it.
const MonacoHost = lazy(() => import("./MonacoHost"));

export function EditorArea({ documents, mod }: { documents: DocumentsStore; mod: string }) {
  const s = useSyncExternalStore(documents.subscribe, documents.getSnapshot);
  // Load Monaco only once a document exists (sticky), after the frame that
  // shows the explorer: parsing ~4 MB of editor code must not delay first paint.
  const [wantEditor, setWantEditor] = useState(false);
  useEffect(() => {
    if (wantEditor || s.docs.length === 0) return;
    const id = requestAnimationFrame(() => setTimeout(() => setWantEditor(true), 0));
    return () => cancelAnimationFrame(id);
  }, [wantEditor, s.docs.length]);
  const active = s.docs.find((d) => d.path === s.activePath);
  return (
    <>
      <TabStrip documents={documents} docs={s.docs} activePath={s.activePath} />
      {active && <Breadcrumbs path={active.path} />}
      <div className="editor-area">
        <ErrorBoundary label="editor">
          <Suspense fallback={null}>
            {wantEditor && <MonacoHost documents={documents} hidden={s.docs.length === 0} />}
          </Suspense>
        </ErrorBoundary>
        {s.docs.length === 0 && (
          <div className="empty-editor" data-testid="empty-editor">
            <dl className="empty-editor__keys">
              {emptyShortcuts(mod).map(([label, keys]) => (
                <div key={label} className="empty-editor__row">
                  <dt>{label}</dt>
                  <dd className="keys">
                    {keys.map((k) => (
                      <Kbd key={k}>{k}</Kbd>
                    ))}
                  </dd>
                </div>
              ))}
            </dl>
          </div>
        )}
      </div>
    </>
  );
}

/** The no-document shortcuts (workspace-spec.md §6): label and keycaps. */
export function emptyShortcuts(mod: string): [string, string[]][] {
  return [
    ["Command palette", [mod, "K"]],
    ["Go to file", [mod, "P"]],
    ["Run command", [mod, mod === "⌘" ? "⇧" : "Shift", "R"]],
    ["Toggle console", [mod, "J"]],
  ];
}

/** The lit edge's transform for a tab at `offsetLeft` with `width` (the element is 100 px wide, origin 0 0). */
export function glideTransform(offsetLeft: number, width: number): string {
  return `translateX(${offsetLeft}px) scaleX(${width / 100})`;
}

/** How the lit edge moves to a new placement: it glides when the active tab changes and jumps
 * (one frame, no transition) on its first placement, after a tab closes (the strip reflows) and on
 * resize (INTERACTION-003). */
export function glideMode(prev: { path: string; count: number } | null, next: { path: string; count: number }): "glide" | "instant" {
  if (!prev) return "instant";
  if (next.count < prev.count) return "instant";
  return next.path !== prev.path ? "glide" : "instant";
}

/** The slot a tab dragged by `dx` lands in (D7, INTERACTION-004): its leading edge (left edge
 * moving left, right edge moving right) passing a sibling's midpoint moves it past that sibling.
 * Edges rather than the centre, so a wide tab can still pass a narrow one at the strip's end. */
export function dropSlot(lefts: number[], widths: number[], from: number, dx: number): number {
  const left = lefts[from]! + dx;
  const right = left + widths[from]!;
  let slot = from;
  if (dx > 0) for (let i = from + 1; i < lefts.length; i++) if (right > lefts[i]! + widths[i]! / 2) slot = i;
  if (dx < 0) for (let i = from - 1; i >= 0; i--) if (left < lefts[i]! + widths[i]! / 2) slot = i;
  return slot;
}

/** The px each tab shifts while one is dragged from `from` towards `slot` (the dragged one: 0). */
export function siblingShift(widths: number[], from: number, slot: number): number[] {
  const w = widths[from] ?? 0;
  return widths.map((_, i) => (i === from ? 0 : slot > from && i > from && i <= slot ? -w : slot < from && i >= slot && i < from ? w : 0));
}

const DRAG_START_PX = 4;
const EDGE_PX = 24;
const SCROLL_PX = 8;

/**
 * Pointer reordering of the tabs (D7, INTERACTION-004): lift after 4 px, follow the pointer on
 * X, siblings make room, a 2 px brass line marks the slot, the strip auto-scrolls at its edges.
 * Drop settles the tab and commits one move(); Escape or a drop off the strip settles it back.
 * Everything during the drag is written to the DOM (no React render per pointer move).
 */
function useTabDrag(strip: RefObject<HTMLDivElement | null>, docs: Doc[], documents: DocumentsStore) {
  const drag = useRef<{
    path: string; from: number; startX: number; startScroll: number; pointerId: number; started: boolean;
    lefts: number[]; widths: number[]; tabs: HTMLElement[]; slot: number; dx: number; clientX: number; clientY: number; frame: number;
  } | null>(null);
  const suppressClick = useRef(false);
  const pendingReset = useRef(false);

  const reset = () => {
    const el = strip.current;
    if (!el) return;
    delete el.dataset.dragging;
    for (const t of el.querySelectorAll<HTMLElement>('[role="tab"]')) {
      t.style.transform = "";
      delete t.dataset.lifted;
      delete t.dataset.settling;
    }
    const line = el.querySelector<HTMLElement>(".tabs__drop");
    if (line) line.hidden = true;
  };
  // A committed move re-renders the strip in the new order; the transforms go in the same frame.
  useLayoutEffect(() => {
    if (!pendingReset.current) return;
    pendingReset.current = false;
    reset();
  });

  const layout = () => {
    const d = drag.current;
    const el = strip.current;
    if (!d || !el) return;
    d.frame = 0;
    const standard = currentMotion() === "standard";
    const scroll = el.scrollLeft - d.startScroll;
    const min = -d.lefts[d.from]!;
    const max = el.scrollWidth - d.lefts[d.from]! - d.widths[d.from]!;
    d.dx = Math.max(min, Math.min(max, d.clientX - d.startX + scroll));
    d.slot = dropSlot(d.lefts, d.widths, d.from, d.dx);
    const shift = siblingShift(d.widths, d.from, d.slot);
    d.tabs.forEach((t, i) => {
      t.style.transform = i === d.from ? `translateX(${d.dx}px)${standard ? " scale(1.02)" : ""}` : shift[i] ? `translateX(${shift[i]}px)` : "";
    });
    const line = el.querySelector<HTMLElement>(".tabs__drop");
    if (line) {
      line.hidden = d.slot === d.from;
      const x = d.slot > d.from ? d.lefts[d.slot]! + d.widths[d.slot]! : d.lefts[d.slot]!;
      line.style.transform = `translateX(${x - 1}px)`;
    }
    // Auto-scroll within 24 px of the strip's edges.
    const r = el.getBoundingClientRect();
    const step = d.clientX < r.left + EDGE_PX ? -SCROLL_PX : d.clientX > r.right - EDGE_PX ? SCROLL_PX : 0;
    if (step && el.scrollLeft + step >= 0 && el.scrollLeft + step <= el.scrollWidth - el.clientWidth) {
      el.scrollLeft += step;
      d.frame = requestAnimationFrame(layout);
    }
  };

  const finish = (commit: boolean) => {
    const d = drag.current;
    const el = strip.current;
    drag.current = null;
    window.removeEventListener("keydown", onEscape, true);
    if (!d || !el) return;
    if (d.frame) cancelAnimationFrame(d.frame);
    if (!d.started) return;
    suppressClick.current = true;
    const tab = d.tabs[d.from]!;
    const slot = commit ? d.slot : d.from;
    // Settle: the tab travels to its slot (or back), then the order commits in one update.
    const target = slot === d.from ? 0 : slot > d.from ? d.lefts[slot]! + d.widths[slot]! - d.lefts[d.from]! - d.widths[d.from]! : d.lefts[slot]! - d.lefts[d.from]!;
    const shift = siblingShift(d.widths, d.from, slot);
    tab.dataset.settling = "";
    tab.style.transform = `translateX(${target}px)`;
    d.tabs.forEach((t, i) => i !== d.from && (t.style.transform = shift[i] ? `translateX(${shift[i]}px)` : ""));
    const line = el.querySelector<HTMLElement>(".tabs__drop");
    if (line) line.hidden = true;
    const ms = currentMotion() === "standard" ? 150 : 0;
    window.setTimeout(() => {
      if (slot !== d.from) {
        pendingReset.current = true;
        documents.move(d.path, slot);
      } else reset();
    }, ms);
  };

  function onEscape(e: globalThis.KeyboardEvent) {
    if (e.key !== "Escape" || !drag.current?.started) return;
    e.preventDefault();
    e.stopPropagation();
    finish(false);
  }

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>, path: string) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".tab__close") || drag.current) return;
    const el = strip.current;
    if (!el) return;
    const tabs = [...el.querySelectorAll<HTMLElement>('[role="tab"]')];
    const from = docs.findIndex((x) => x.path === path);
    if (from < 0 || tabs.length !== docs.length) return;
    drag.current = {
      path, from, startX: e.clientX, startScroll: el.scrollLeft, pointerId: e.pointerId, started: false,
      lefts: tabs.map((t) => t.offsetLeft), widths: tabs.map((t) => t.offsetWidth), tabs, slot: from, dx: 0, clientX: e.clientX, clientY: e.clientY, frame: 0,
    };
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    const el = strip.current;
    if (!d || !el || e.pointerId !== d.pointerId) return;
    d.clientX = e.clientX;
    d.clientY = e.clientY;
    if (!d.started) {
      if (Math.abs(e.clientX - d.startX) < DRAG_START_PX) return;
      d.started = true;
      el.setPointerCapture(e.pointerId);
      el.dataset.dragging = "true";
      d.tabs[d.from]!.dataset.lifted = "";
      window.addEventListener("keydown", onEscape, true);
    }
    if (!d.frame) d.frame = requestAnimationFrame(layout);
  };
  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    const el = strip.current;
    if (!d || !el || e.pointerId !== d.pointerId) return;
    const r = el.getBoundingClientRect();
    // A drop off the strip (more than a strip's height away) cancels: no tear-off.
    const inside = e.clientY >= r.top - r.height && e.clientY <= r.bottom + r.height;
    finish(inside);
  };
  const onPointerCancel = () => finish(false);
  /** Swallows the click that ends a drag (it is not an activation). */
  const onClickCapture = (e: { stopPropagation(): void; preventDefault(): void }) => {
    if (!suppressClick.current) return;
    suppressClick.current = false;
    e.stopPropagation();
    e.preventDefault();
  };
  return { onPointerDown, onPointerMove, onPointerUp, onPointerCancel, onClickCapture };
}

/**
 * The tab strip (workspace-spec.md §2.3; M1.6-EDITOR-001): a recessed chrome strip, the active
 * tab a raised key merging into the editor, and one brass lit edge placed over it by transform
 * (written to the DOM, no React state: a resize or activation costs no extra commit).
 */
function TabStrip({ documents, docs, activePath }: { documents: DocumentsStore; docs: Doc[]; activePath: string }) {
  const strip = useRef<HTMLDivElement>(null);
  const glide = useRef<HTMLSpanElement>(null);
  const placed = useRef<{ path: string; count: number } | null>(null);
  const dragging = useTabDrag(strip, docs, documents);
  const [announce, setAnnounce] = useState("");
  const keyMoved = useRef<string | null>(null);
  // Keyboard reorder keeps focus on the moved tab (React moves its node).
  useLayoutEffect(() => {
    const p = keyMoved.current;
    if (!p) return;
    keyMoved.current = null;
    strip.current?.querySelector<HTMLElement>(`[role="tab"][data-path="${CSS.escape(p)}"]`)?.focus();
  }, [docs]);
  const onTabKey = (e: KeyboardEvent<HTMLDivElement>, d: Doc, i: number) => {
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && (e.key === "PageUp" || e.key === "PageDown")) {
      e.preventDefault();
      const to = i + (e.key === "PageUp" ? -1 : 1);
      if (to < 0 || to >= docs.length) return;
      keyMoved.current = d.path;
      documents.move(d.path, to);
      setAnnounce(`${d.title} moved to position ${to + 1} of ${docs.length}`);
      return;
    }
    if (e.key === "ArrowRight") documents.cycle(1);
    if (e.key === "ArrowLeft") documents.cycle(-1);
  };

  useLayoutEffect(() => {
    const el = strip.current;
    const edge = glide.current;
    if (!el || !edge) return;
    const place = (mode: "glide" | "instant") => {
      const tab = el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
      edge.hidden = !tab;
      if (!tab) return;
      if (mode === "instant") {
        // data-instant removes the transition for this placement only.
        edge.dataset.instant = "";
        edge.style.transform = glideTransform(tab.offsetLeft, tab.offsetWidth);
        void edge.offsetWidth; // commit the jump before the transition comes back
        delete edge.dataset.instant;
      } else {
        edge.style.transform = glideTransform(tab.offsetLeft, tab.offsetWidth);
      }
    };
    const next = { path: activePath, count: docs.length };
    place(glideMode(placed.current, next));
    placed.current = next;
    const tab = el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
    tab?.scrollIntoView({ inline: "nearest", block: "nearest" });
    // The strip's size and the active tab's width (a title or badge change) move the edge.
    let first = true;
    const ro = new ResizeObserver(() => {
      if (first) return void (first = false); // the initial callback: already placed above
      place("instant");
    });
    ro.observe(el);
    if (tab) ro.observe(tab);
    return () => ro.disconnect();
  }, [activePath, docs]);

  // A vertical wheel scrolls the strip sideways (the strip has no scrollbar).
  const onWheel = (e: WheelEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    if (e.deltaX === 0 && e.deltaY !== 0) el.scrollLeft += e.deltaY;
  };

  return (
    <>
    <span className="visually-hidden tabs__announce" aria-live="polite">
      {announce}
    </span>
    <div
      className="tabs"
      role="tablist"
      aria-label="Open documents"
      ref={strip}
      onWheel={onWheel}
      onPointerMove={dragging.onPointerMove}
      onPointerUp={dragging.onPointerUp}
      onPointerCancel={dragging.onPointerCancel}
      onClickCapture={dragging.onClickCapture}
    >
      {docs.map((d, i) => {
        const active = d.path === activePath;
        const glyph = glyphFor(d.title);
        return (
          <div
            key={d.path}
            className="tab"
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            title={d.path + (d.changedOnDisk ? " — changed on disk" : "")}
            data-path={d.path}
            data-dirty={d.dirty}
            data-hint={`${d.path}${d.dirty ? " (unsaved)" : ""} — Mod+Tab`}
            onClick={() => documents.activate(d.path)}
            onAuxClick={(e) => e.button === 1 && documents.close(d.path)}
            onPointerDown={(e) => dragging.onPointerDown(e, d.path)}
            onKeyDown={(e) => onTabKey(e, d, i)}
          >
            <span className="glyph" data-kind={glyph.kind} aria-hidden="true">
              {glyph.label}
            </span>
            <span className="tab__title">{d.title}</span>
            {d.changedOnDisk && <span className="tab__badge">changed on disk</span>}
            <span
              className="tab__close"
              role="button"
              data-hint="Close the document — Mod+W"
              aria-label={`Close ${d.title}${d.dirty ? " (unsaved)" : ""}`}
              onClick={(e) => {
                e.stopPropagation();
                documents.close(d.path);
              }}
            >
              {d.dirty && <span className="tab__dot" aria-hidden="true" />}
              <Icon.Close className="tab__x" width={12} height={12} />
            </span>
          </div>
        );
      })}
      <span className="tabs__glide" ref={glide} aria-hidden="true" hidden />
      <span className="tabs__drop" aria-hidden="true" hidden />
    </div>
    </>
  );
}

/** Path-only breadcrumbs of the active document (no symbol segments: those need LSP, M4). */
function Breadcrumbs({ path }: { path: string }) {
  const parts = path.split("/").filter(Boolean);
  return (
    <nav className="crumbs" aria-label="Location" data-testid="breadcrumbs">
      <ol className="crumbs__list">
        {parts.map((p, i) => (
          <li key={i} className="crumbs__item" aria-current={i === parts.length - 1 ? "location" : undefined}>
            {i > 0 && <Icon.Chevron className="crumbs__sep" aria-hidden="true" />}
            <span className="crumbs__text">{p}</span>
          </li>
        ))}
      </ol>
    </nav>
  );
}
