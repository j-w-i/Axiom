// The editor surface inside the Tauri webview. It reuses the Qt build's
// editor package logic unchanged — `DocumentStore` (bridge-1 dirty tracking,
// serialised saves, snapshot/cursor throttling) and `monaco-setup` (workers,
// option mapping, command ids) from ../../../../editor/src — and implements
// the bridge-1 shell→page signals as direct calls (`EditorPort`). The
// page→shell slots go to the `EditorListener` (the DocumentsStore).
//
// Monaco runs in-process with the shell UI here, unlike Qt where it is a
// separate WebEngine page; the bridge message set is kept so the editor can
// be moved into an isolated <iframe>/webview later without touching stores.

import * as monaco from "monaco-editor";
import { DocumentStore, inferLanguageId } from "@editor/documents";
import { applyConfigure, COMMAND_ACTIONS, installWorkers, languageInfos } from "@editor/monaco-setup";
import { MONACO_VERSION } from "@editor/version";
import type { ConfigurePayload, OpenDocumentPayload } from "@editor/bridge";
import type { EditorListener, EditorPort } from "../state/documents";

let workersInstalled = false;

/** Monaco themes derived from the design tokens so the editor matches the shell. */
function defineThemes(): void {
  const css = getComputedStyle(document.documentElement);
  const read = (theme: "light" | "dark", name: string): string => {
    const probe = document.createElement("div");
    probe.dataset.theme = theme;
    probe.style.display = "none";
    document.body.appendChild(probe);
    const v = getComputedStyle(probe).getPropertyValue(name).trim();
    probe.remove();
    return v || css.getPropertyValue(name).trim();
  };
  const hex = (c: string): string => {
    // Resolve any CSS colour to #rrggbb through a canvas (Monaco needs hex).
    const ctx = document.createElement("canvas").getContext("2d");
    if (!ctx) return "#000000";
    ctx.fillStyle = c;
    return ctx.fillStyle;
  };
  for (const theme of ["light", "dark"] as const) {
    monaco.editor.defineTheme(`axiom-${theme}`, {
      base: theme === "light" ? "vs" : "vs-dark",
      inherit: true,
      rules: [],
      colors: {
        "editor.background": hex(read(theme, "--bg")),
        "editorGutter.background": hex(read(theme, "--bg")),
        "editor.lineHighlightBackground": hex(read(theme, "--bg-hover")) + "80",
        "editor.lineHighlightBorder": "#00000000",
        "editorLineNumber.foreground": hex(read(theme, "--text-faint")),
        "editorLineNumber.activeForeground": hex(read(theme, "--text-muted")),
        "editorWidget.background": hex(read(theme, "--bg-elevated")),
        "editorWidget.border": hex(read(theme, "--border-strong")),
        "focusBorder": hex(read(theme, "--accent-line")),
        "scrollbarSlider.background": hex(read(theme, "--scrollbar")) + "99",
      },
    });
  }
}

export class EditorSurface implements EditorPort {
  private readonly editor: monaco.editor.IStandaloneCodeEditor;
  private readonly store: DocumentStore;
  private activePath: string | null = null;
  private readonly viewStates = new Map<string, monaco.editor.ICodeEditorViewState | null>();
  private readonly disposables: monaco.IDisposable[] = [];
  private theme: "light" | "dark" = "dark";

  constructor(container: HTMLElement, private readonly listener: EditorListener) {
    if (!workersInstalled) {
      installWorkers();
      workersInstalled = true;
    }
    defineThemes();
    this.editor = monaco.editor.create(container, {
      model: null,
      theme: "axiom-dark",
      automaticLayout: true,
      fontLigatures: true,
      minimap: { enabled: true, renderCharacters: false, maxColumn: 80 },
      scrollBeyondLastLine: false,
      smoothScrolling: true,
      cursorSmoothCaretAnimation: "on",
      padding: { top: 8 },
      renderLineHighlight: "all",
      stickyScroll: { enabled: true },
    });
    this.store = new DocumentStore(listener, (path, text) =>
      monaco.editor.createModel(
        text,
        inferLanguageId(path, languageInfos()),
        monaco.Uri.from({ scheme: "file", path: `/${path}` }),
      ),
    );

    // Page → shell.
    this.editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      if (this.activePath) this.store.requestSave(this.activePath);
    });
    this.disposables.push(
      this.editor.onDidChangeCursorSelection(() => {
        if (!this.activePath) return;
        const sel = this.editor.getSelection();
        const model = this.editor.getModel();
        const pos = this.editor.getPosition();
        if (!pos) return;
        const selectionLength = sel && model ? model.getValueInRange(sel).length : 0;
        this.store.cursorMoved(this.activePath, pos.lineNumber, pos.column, selectionLength);
      }),
      monaco.editor.registerLinkOpener({
        open: (uri) => {
          listener.openLink({ url: uri.toString() });
          return true;
        },
      }),
    );
  }

  /** The handshake (bridge §Handshake): everything is wired, so say ready. */
  announceReady(): void {
    this.listener.ready({ bridgeVersion: 1, monacoVersion: MONACO_VERSION });
  }

  dispose(): void {
    this.store.dispose();
    for (const d of this.disposables) d.dispose();
    this.editor.dispose();
  }

  focus(): void {
    this.editor.focus();
  }

  private activate(path: string): void {
    const model = this.store.model(path);
    if (!model) return;
    if (this.activePath && this.activePath !== path) this.viewStates.set(this.activePath, this.editor.saveViewState());
    this.activePath = path;
    this.editor.setModel(model as monaco.editor.ITextModel);
    const vs = this.viewStates.get(path);
    if (vs) this.editor.restoreViewState(vs);
    this.editor.focus();
  }

  // ---- EditorPort (shell → page) ----------------------------------------------

  configure(c: ConfigurePayload): void {
    this.theme = c.theme;
    applyConfigure(this.editor, c);
    monaco.editor.setTheme(`axiom-${this.theme}`);
  }

  openDocument(p: OpenDocumentPayload): void {
    const replacing = this.store.has(p.path);
    const vs = replacing && this.activePath === p.path ? this.editor.saveViewState() : null;
    this.store.open(p);
    if (p.activate || (replacing && this.activePath === p.path)) {
      this.activate(p.path);
      if (vs) this.editor.restoreViewState(vs);
    }
  }

  activateDocument(p: { path: string }): void {
    this.activate(p.path);
  }

  closeDocument(p: { path: string }): void {
    if (this.activePath === p.path) {
      this.editor.setModel(null);
      this.activePath = null;
    }
    this.viewStates.delete(p.path);
    this.store.close(p.path);
  }

  requestSave(p: { path: string }): void {
    this.store.requestSave(p.path);
  }

  documentSaved(p: { path: string }): void {
    this.store.documentSaved(p.path);
  }

  saveFailed(p: { path: string }): void {
    this.store.saveFailed(p.path);
  }

  revealPosition(p: { path: string; line: number; column: number }): void {
    this.activate(p.path);
    const position = { lineNumber: p.line, column: p.column };
    this.editor.setPosition(position);
    this.editor.revealPositionInCenter(position);
  }

  runCommand(p: { command: string }): void {
    const action = COMMAND_ACTIONS[p.command];
    if (!action) return;
    this.editor.focus();
    this.editor.trigger("host", action, null);
  }
}
