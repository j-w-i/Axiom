import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type WheelEvent } from "react";
import type { Doc, DocumentsStore } from "../state/documents";
import * as Icon from "../ui/icons";
import { ErrorBoundary } from "./ErrorBoundary";
import { glyphFor } from "./Explorer";
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

/**
 * The tab strip (workspace-spec.md §2.3; M1.6-EDITOR-001): a recessed chrome strip, the active
 * tab a raised key merging into the editor, and one brass lit edge placed over it by transform
 * (written to the DOM, no React state: a resize or activation costs no extra commit).
 */
function TabStrip({ documents, docs, activePath }: { documents: DocumentsStore; docs: Doc[]; activePath: string }) {
  const strip = useRef<HTMLDivElement>(null);
  const glide = useRef<HTMLSpanElement>(null);

  useLayoutEffect(() => {
    const el = strip.current;
    const edge = glide.current;
    if (!el || !edge) return;
    const place = () => {
      const tab = el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
      edge.hidden = !tab;
      if (tab) edge.style.transform = glideTransform(tab.offsetLeft, tab.offsetWidth);
    };
    place();
    const tab = el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
    tab?.scrollIntoView({ inline: "nearest", block: "nearest" });
    // The strip's size and the active tab's width (a title or badge change) move the edge.
    const ro = new ResizeObserver(place);
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
    <div className="tabs" role="tablist" aria-label="Open documents" ref={strip} onWheel={onWheel}>
      {docs.map((d) => {
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
            onKeyDown={(e) => {
              if (e.key === "ArrowRight") documents.cycle(1);
              if (e.key === "ArrowLeft") documents.cycle(-1);
            }}
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
    </div>
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
