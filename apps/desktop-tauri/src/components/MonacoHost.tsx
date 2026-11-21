import * as monaco from "monaco-editor";
import { useEffect, useRef } from "react";
import { EditorGit } from "../editor/git/gutter";
import { EditorLsp } from "../editor/lsp/client";
import { EditorSurface } from "../editor/surface";
import type { DocumentsStore } from "../state/documents";
import type { GitStore } from "../state/git";
import type { LspStore } from "../state/lsp";

/** Mounts one Monaco editor for the app's lifetime and attaches it to the store. */
export default function MonacoHost({ documents, lsp, git, hidden }: { documents: DocumentsStore; lsp?: LspStore; git?: GitStore; hidden: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!ref.current) return;
    let surface: EditorSurface | null = null;
    let language: EditorLsp | null = null;
    let changes: EditorGit | null = null;
    try {
      // The language-server client first: it tracks the models the surface creates (M4-LSP-002).
      if (lsp) language = new EditorLsp(lsp, documents);
      if (git) changes = new EditorGit(git);
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
      language?.dispose();
      changes?.dispose();
    };
  }, [documents, lsp, git]);
  return <div ref={ref} className="editor-host" hidden={hidden} data-testid="editor" />;
}
