import { lazy, Suspense, useEffect, useState, useSyncExternalStore } from "react";
import type { DocumentsStore } from "../state/documents";
import * as Icon from "../ui/icons";
import { ErrorBoundary } from "./ErrorBoundary";
import { Kbd } from "../ui/primitives";
import "../styles/components/states.css";

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
  return (
    <>
      <div className="tabs" role="tablist" aria-label="Open documents">
        {s.docs.map((d) => {
          const active = d.path === s.activePath;
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
                {d.dirty ? <span className="tab__dot" /> : <Icon.Close width={12} height={12} />}
              </span>
            </div>
          );
        })}
      </div>
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
