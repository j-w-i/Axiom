// The diff view (M4-GIT-002): a file's HEAD text against the working copy in Monaco's diff editor,
// over the editor area. The right side is the file's own buffer (opened in the background if it
// was not), so it stays editable and its edits are the document's; the left side is read-only.
// Esc or Close returns to the editor. Loaded with Monaco (lazy).

import * as monaco from "monaco-editor";
import { useEffect, useRef, useState } from "react";
import type { DocumentsStore } from "../state/documents";
import type { GitStore } from "../state/git";
import * as Icon from "../ui/icons";
import { IconButton } from "../ui/primitives";

async function fileModel(path: string, documents: DocumentsStore): Promise<monaco.editor.ITextModel | null> {
  const uri = monaco.Uri.from({ scheme: "file", path: `/${path}` });
  if (!monaco.editor.getModel(uri)) await documents.openInBackground(path);
  // The editor surface creates the model once it has the document.
  for (let i = 0; i < 100 && !monaco.editor.getModel(uri); i++) await new Promise((r) => setTimeout(r, 50));
  return monaco.editor.getModel(uri);
}

export default function DiffHost({ git, documents, path }: { git: GitStore; documents: DocumentsStore; path: string }) {
  const el = useRef<HTMLDivElement>(null);
  const [error, setError] = useState("");
  const mark = git.getSnapshot().marks[path];
  const name = path.split("/").pop() ?? path;

  useEffect(() => {
    let disposed = false;
    let editor: monaco.editor.IStandaloneDiffEditor | null = null;
    let original: monaco.editor.ITextModel | null = null;
    void (async () => {
      const modified = await fileModel(path, documents);
      if (disposed) return;
      if (!modified) {
        setError(`${name} cannot be opened.`);
        return;
      }
      const head = mark?.kind === "untracked" || mark?.kind === "added" ? "" : ((await git.headText(path)) ?? "");
      if (disposed || !el.current) return;
      original = monaco.editor.createModel(head, modified.getLanguageId(), monaco.Uri.from({ scheme: "axiom-git-head", path: `/${path}` }));
      editor = monaco.editor.createDiffEditor(el.current, {
        automaticLayout: true,
        originalEditable: false,
        readOnly: false,
        renderSideBySide: true,
        useInlineViewWhenSpaceIsLimited: true,
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--font-code").trim(),
        fontSize: parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--code-size")) || 13,
        editContext: false,
      } as monaco.editor.IStandaloneDiffEditorConstructionOptions);
      editor.setModel({ original, modified });
      // Esc closes the diff, unless one of Monaco's own widgets wants it.
      const when = "!suggestWidgetVisible && !findWidgetVisible && !renameInputVisible && !referenceSearchVisible && !parameterHintsVisible";
      for (const side of [editor.getOriginalEditor(), editor.getModifiedEditor()]) side.addCommand(monaco.KeyCode.Escape, () => git.closeDiff(), when);
      editor.getModifiedEditor().focus();
    })();
    return () => {
      disposed = true;
      editor?.dispose();
      original?.dispose();
    };
  }, [path, git, documents, mark?.kind, name]);

  return (
    <section
      className="diff-host"
      aria-label={`Changes in ${path} since HEAD`}
      data-testid="diff-view"
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.defaultPrevented) {
          e.preventDefault();
          git.closeDiff();
        }
      }}
    >
      <header className="diff-host__bar">
        <span className="diff-host__title">
          <span className="diff-host__name">{name}</span>
          <span className="diff-host__sides">HEAD ↔ Working Tree</span>
        </span>
        <IconButton label="Open the file" hint="Open the file in the editor" onClick={() => (git.closeDiff(), documents.open(path))}>
          <Icon.File />
        </IconButton>
        <IconButton label="Close the diff" hint="Close the diff — Esc" onClick={() => git.closeDiff()} data-testid="diff-close">
          <Icon.Close />
        </IconButton>
      </header>
      {error ? (
        <p className="diff-host__error" role="alert">
          {error}
        </p>
      ) : (
        <div ref={el} className="diff-host__editor" />
      )}
    </section>
  );
}
