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
import { COMMAND_ACTIONS, editorOptionsFor, installWorkers, languageInfos } from "@editor/monaco-setup";
import { MONACO_VERSION } from "@editor/version";
import type { ConfigurePayload, OpenDocumentPayload } from "@editor/bridge";
import type { EditorListener, EditorPort } from "../state/documents";
import { hexFromCanvasStyle, lineHeightFor, themeData, type ThemeToken } from "./theme";

let workersInstalled = false;

/** Monaco hex for any CSS colour, through a canvas (keeps alpha: #rrggbbaa). */
function hex(c: string): string {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return "#000000";
  ctx.fillStyle = c;
  return hexFromCanvasStyle(String(ctx.fillStyle));
}

/** Both Monaco themes from the tokens of each appearance (read through a hidden probe). */
function defineThemes(): void {
  for (const theme of ["light", "dark"] as const) {
    const probe = document.createElement("div");
    probe.dataset.theme = theme;
    probe.style.display = "none";
    document.body.appendChild(probe);
    const css = getComputedStyle(probe);
    // Every token is defined (tests/tokens.test.ts); an empty value would read back as black.
    const color = (t: ThemeToken) => hex(css.getPropertyValue(t).trim());
    monaco.editor.defineTheme(`axiom-${theme}`, themeData(theme === "light" ? "vs" : "vs-dark", color, css.getPropertyValue("--syn-keyword-style").trim()));
    probe.remove();
  }
}

let chatThemes = false;

/** Monaco's colouring of a code block for the Chat transcript (M3-UI-001): HTML with Monaco's
 * own escaping, in the app's theme. `language` is a Markdown fence label (`rs`, `ts`, `sh`…). */
export async function colorizeCode(code: string, language: string, dark: boolean): Promise<string> {
  if (!chatThemes) {
    defineThemes();
    chatThemes = true;
  }
  monaco.editor.setTheme(dark ? "axiom-dark" : "axiom-light");
  const alias: Record<string, string> = {
    rs: "rust", ts: "typescript", tsx: "typescript", js: "javascript", jsx: "javascript", py: "python",
    sh: "shell", bash: "shell", zsh: "shell", console: "shell", yml: "yaml", md: "markdown", "c++": "cpp",
    "c#": "csharp", cs: "csharp", golang: "go", kt: "kotlin", rb: "ruby", toml: "ini",
  };
  const id = alias[language.toLowerCase()] ?? language.toLowerCase();
  const known = monaco.languages.getLanguages().some((l) => l.id === id);
  return monaco.editor.colorize(code, known ? id : "plaintext", { tabSize: 4 });
}

/** The default code font when the setting is empty (design-system.md §5): --font-code, 13 / 20. */
function codeFont(): { family: string; size: number; leading: number } {
  const css = getComputedStyle(document.documentElement);
  return {
    family: css.getPropertyValue("--font-code").trim(),
    size: parseFloat(css.getPropertyValue("--code-size")) || 13,
    leading: parseFloat(css.getPropertyValue("--code-leading")) || 20,
  };
}

export class EditorSurface implements EditorPort {
  private readonly editor: monaco.editor.IStandaloneCodeEditor;
  private readonly store: DocumentStore;
  private activePath: string | null = null;
  private readonly viewStates = new Map<string, monaco.editor.ICodeEditorViewState | null>();
  private readonly disposables: monaco.IDisposable[] = [];
  /** The Monaco theme last applied: a theme is applied with one setTheme call, only when it changes. */
  private applied = "axiom-dark";
  private readonly font = codeFont();

  constructor(container: HTMLElement, private readonly listener: EditorListener) {
    if (!workersInstalled) {
      installWorkers();
      workersInstalled = true;
    }
    defineThemes();
    this.editor = monaco.editor.create(container, this.createOptions());
    this.store = new DocumentStore(listener, (path, text) =>
      monaco.editor.createModel(
        text,
        inferLanguageId(path, languageInfos()),
        monaco.Uri.from({ scheme: "file", path: `/${path}` }),
      ),
    );

    // Bundled fonts may finish loading after Monaco measured its glyphs. A face loads lazily, when
    // first used, so `fonts.ready` alone can resolve before it (seen on macOS and Windows: the caret
    // drifted 1.2 px from the text in CI run 44239034330); remeasure whenever loading finishes.
    void document.fonts?.ready.then(() => monaco.editor.remeasureFonts());
    const onFontsLoaded = () => monaco.editor.remeasureFonts();
    document.fonts?.addEventListener("loadingdone", onFontsLoaded);
    this.disposables.push({ dispose: () => document.fonts?.removeEventListener("loadingdone", onFontsLoaded) });
    // Tokens change with the appearance (and in development on a stylesheet update): the
    // themes are regenerated then, never on the typing path.
    const retheme = () => {
      // Redefining the active theme refreshes the editor; switching applies the other one.
      defineThemes();
      const t = document.documentElement.dataset.theme;
      if (t === "light" || t === "dark") this.applyTheme(t);
    };
    const mo = new MutationObserver(retheme);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    this.disposables.push({ dispose: () => mo.disconnect() });
    if (import.meta.hot) import.meta.hot.on("vite:afterUpdate", retheme);

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

  /** The options both editors are created with (the right group's too, M4-UX-002). */
  private createOptions(): monaco.editor.IStandaloneEditorConstructionOptions {
    return {
      model: null,
      theme: "axiom-dark",
      fontFamily: this.font.family,
      fontSize: this.font.size,
      lineHeight: this.font.leading,
      automaticLayout: true,
      fontLigatures: true,
      minimap: { enabled: true, renderCharacters: false, maxColumn: 80 },
      scrollBeyondLastLine: false,
      smoothScrolling: true,
      cursorSmoothCaretAnimation: "on",
      padding: { top: 8 },
      renderLineHighlight: "all",
      stickyScroll: { enabled: true },
      // One input path on all three engines. Monaco 0.53+ defaults to
      // Chromium's EditContext API where it exists — only WebView2 has it
      // (Linux WebKitGTK and macOS WKWebView do not) — so the Windows build
      // would take a different path for every keystroke than the other two.
      // It also cannot be driven by scripted input, which leaves the Windows
      // build's typing untestable, and Chromium has shipped regressions that
      // wedge it mid-session (stablyai/orca#7747: "editors stop accepting
      // keyboard input until the app is restarted"). The textarea path is the
      // one both other platforms use and every browser used before 2024.
      // Reverting is this one line if EditContext is wanted for IME reasons.
      editContext: false,
    };
  }

  // ---- the right editor group (M4-UX-002) ------------------------------------------
  // A second Monaco editor over the same models (one buffer, two views). Created when the editor
  // splits, disposed when it unsplits. Mod+S saves its document; its cursor is reported while it
  // has focus; each document keeps its own view state there.

  private secondary: {
    editor: monaco.editor.IStandaloneCodeEditor;
    path: string | null;
    views: Map<string, monaco.editor.ICodeEditorViewState | null>;
    subs: monaco.IDisposable[];
  } | null = null;
  private lastConfig: ConfigurePayload | null = null;

  attachSecondary(container: HTMLElement, onCursor: (path: string, line: number, column: number, selectionLength: number) => void): void {
    this.detachSecondary();
    const editor = monaco.editor.create(container, this.createOptions());
    if (this.lastConfig) this.configureOne(editor, this.lastConfig);
    const sec = { editor, path: null as string | null, views: new Map<string, monaco.editor.ICodeEditorViewState | null>(), subs: [] as monaco.IDisposable[] };
    const report = () => {
      const pos = editor.getPosition();
      const model = editor.getModel();
      const sel = editor.getSelection();
      if (!sec.path || !pos) return;
      onCursor(sec.path, pos.lineNumber, pos.column, sel && model ? model.getValueInRange(sel).length : 0);
    };
    editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
      if (sec.path) this.store.requestSave(sec.path);
    });
    sec.subs.push(
      editor.onDidChangeCursorSelection(() => {
        if (editor.hasTextFocus()) report();
      }),
      editor.onDidFocusEditorText(report),
    );
    this.secondary = sec;
    if (this.secondaryWanted) this.showSecondary(this.secondaryWanted);
  }

  /** The document the right group should show: kept so it can be shown once its model exists
   * (after a restart the tabs reopen after the split is known). */
  private secondaryWanted: string | null = null;

  /** Shows a document in the right group (null: nothing). */
  showSecondary(path: string | null): void {
    this.secondaryWanted = path;
    const sec = this.secondary;
    if (!sec || sec.path === path) return;
    if (sec.path) sec.views.set(sec.path, sec.editor.saveViewState());
    const model = path ? this.store.model(path) : undefined;
    sec.path = model ? path : null;
    sec.editor.setModel((model as monaco.editor.ITextModel | undefined) ?? null);
    const vs = path ? sec.views.get(path) : undefined;
    if (vs) sec.editor.restoreViewState(vs);
  }

  focusSecondary(): boolean {
    if (!this.secondary?.path) return false;
    this.secondary.editor.focus();
    return true;
  }

  detachSecondary(): void {
    if (!this.secondary) return;
    for (const d of this.secondary.subs) d.dispose();
    this.secondary.editor.dispose();
    this.secondary = null;
  }

  /** The handshake (bridge §Handshake): everything is wired, so say ready. */
  announceReady(): void {
    this.listener.ready({ bridgeVersion: 1, monacoVersion: MONACO_VERSION });
  }

  dispose(): void {
    this.detachSecondary();
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
    this.lastConfig = c;
    this.configureOne(this.editor, c);
    if (this.secondary) this.configureOne(this.secondary.editor, c);
    this.applyTheme(c.theme);
  }

  private configureOne(editor: monaco.editor.IStandaloneCodeEditor, c: ConfigurePayload): void {
    editor.updateOptions(editorOptionsFor(c));
    // An empty family keeps Monaco's current one in the shared mapping; here it means the
    // bundled code font. The line height keeps the 13 / 20 ratio at any size.
    editor.updateOptions({
      ...(c.fontFamily === "" ? { fontFamily: this.font.family } : {}),
      lineHeight: lineHeightFor(c.fontSize, this.font.size, this.font.leading),
    });
  }

  private applyTheme(theme: "light" | "dark"): void {
    const name = `axiom-${theme}`;
    if (name === this.applied) return;
    this.applied = name;
    monaco.editor.setTheme(name);
  }

  openDocument(p: OpenDocumentPayload): void {
    const replacing = this.store.has(p.path);
    const vs = replacing && this.activePath === p.path ? this.editor.saveViewState() : null;
    this.store.open(p);
    if (this.secondaryWanted === p.path && this.secondary && this.secondary.path !== p.path) this.showSecondary(p.path);
    if (p.activate || (replacing && this.activePath === p.path)) {
      this.activate(p.path);
      if (vs) this.editor.restoreViewState(vs);
    }
  }

  activateDocument(p: { path: string }): void {
    this.activate(p.path);
  }

  closeDocument(p: { path: string }): void {
    if (this.secondary?.path === p.path) this.showSecondary(null);
    this.secondary?.views.delete(p.path);
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
    // Setting the position a fresh model already has (1:1) fires no cursor event: report it, so
    // the status bar shows where the reveal landed.
    const at = this.editor.getPosition();
    if (at) this.store.cursorMoved(p.path, at.lineNumber, at.column, 0);
  }

  runCommand(p: { command: string }): void {
    const action = COMMAND_ACTIONS[p.command];
    if (!action) return;
    this.editor.focus();
    this.editor.trigger("host", action, null);
  }
}
