import * as monaco from "monaco-editor";
import { useEffect, useRef } from "react";
import { EditorSurface } from "../editor/surface";
import type { DocumentsStore } from "../state/documents";

/** Mounts one Monaco editor for the app's lifetime and attaches it to the store. */
export default function MonacoHost({ documents, hidden }: { documents: DocumentsStore; hidden: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    let surface: EditorSurface | null = null;
    try {
      surface = new EditorSurface(ref.current, documents);
      documents.attachEditor(surface);
      surface.announceReady();
      // The code face is font-display: swap; once it has loaded, Monaco must
      // remeasure or the caret is placed with the fallback's metrics.
      void document.fonts.ready.then(() => monaco.editor.remeasureFonts());
      (window as unknown as { __axiomEditor?: EditorSurface }).__axiomEditor = surface;
    } catch (e) {
      documents.showBanner(`The editor failed to start: ${e instanceof Error ? e.message : String(e)}`);
    }
    return () => {
      documents.detachEditor();
      surface?.dispose();
    };
  }, [documents]);
  return <div ref={ref} className="editor-host" hidden={hidden} data-testid="editor" />;
}
