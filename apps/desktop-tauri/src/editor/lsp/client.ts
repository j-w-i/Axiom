// The editor's language-server client (M4-LSP-002; ADR-0018 §3): Monaco models of workspace files
// are synced to the daemon's servers (`lsp/notify`), Monaco's providers ask them (`lsp/request`),
// and their diagnostics become markers. One instance per editor surface; servers come from the
// LspStore. Monaco's bundled `monaco.lsp.MonacoLspClient` is not used: it syncs every model to
// one server and registers its providers for every language.

import * as monaco from "monaco-editor";
import type { ServerInfo } from "../../daemon/protocol.gen";
import type { LspStore } from "../../state/lsp";
import {
  contentChanges,
  highlightKind,
  locations,
  lspLanguageId,
  symbolKind,
  toCompletion,
  toLspPosition,
  toLspRange,
  toMarkdown,
  toMarker,
  toMonacoRange,
  workspaceEditFiles,
  type FileEdits,
  type LspCompletionItem,
  type LspRange,
  type LspTextEdit,
} from "./convert";

/** What the client needs from the documents store. */
export interface DocumentsPort {
  /** Opens a workspace file without activating it; resolves once its model exists. */
  openInBackground(path: string): Promise<void>;
  openAt(path: string, line: number, column: number): void;
  onSaved(fn: (path: string) => void): () => void;
}

type Model = monaco.editor.ITextModel;

/** The workspace path of a model of the documents store (`file:///<path>`), else null. */
function modelPath(m: Model): string | null {
  if (m.uri.scheme !== "file" || m.uri.authority) return null;
  const p = m.uri.path.replace(/^\//, "");
  return p && !p.startsWith("/") ? p : null;
}

interface Synced {
  serverId: string;
  uri: string;
  languageId: string;
  sub: monaco.IDisposable;
}

const TS_LANGS = ["typescript", "javascript"];
/** Monaco URI scheme of read-only models of files outside the workspace. */
const EXTERNAL = "axiom-external";

export class EditorLsp {
  private readonly synced = new Map<Model, Synced>();
  private readonly providers = new Map<string, monaco.IDisposable[]>();
  private readonly disposables: monaco.IDisposable[] = [];
  private readonly unsubs: (() => void)[] = [];
  /** Monaco's own TypeScript service is off while a TypeScript server serves the language. */
  private builtinTsOff = false;

  constructor(
    private readonly lsp: LspStore,
    private readonly documents: DocumentsPort,
  ) {
    this.disposables.push(
      monaco.editor.onDidCreateModel((m) => this.track(m)),
      monaco.editor.onWillDisposeModel((m) => this.untrack(m)),
      monaco.editor.registerEditorOpener({
        openCodeEditor: (source, resource, selection) => this.open(source, resource, selection),
      }),
    );
    this.unsubs.push(
      lsp.subscribe(() => this.refresh()),
      lsp.onPublish((p) => {
        const model = this.modelFor(p.uri);
        if (model) {
          const markers = p.diagnostics.map((d) => {
            const m = toMarker(d);
            return { ...m, code: typeof m.code === "object" ? { value: m.code.value, target: monaco.Uri.parse(m.code.target) } : m.code };
          });
          monaco.editor.setModelMarkers(model, `lsp:${p.serverId}`, markers);
        }
      }),
      documents.onSaved((path) => {
        const m = monaco.editor.getModel(monaco.Uri.from({ scheme: "file", path: `/${path}` }));
        const s = m && this.synced.get(m);
        if (s) void this.lsp.notify(s.serverId, "textDocument/didSave", { textDocument: { uri: s.uri } }).catch(() => {});
      }),
    );
    this.refresh();
  }

  dispose(): void {
    for (const d of this.disposables) d.dispose();
    for (const u of this.unsubs) u();
    for (const list of this.providers.values()) for (const d of list) d.dispose();
    this.providers.clear();
    for (const m of [...this.synced.keys()]) this.untrack(m);
    this.setBuiltinTs(true);
  }

  // ---- servers and providers ------------------------------------------------------------

  private refresh(): void {
    const ready = this.lsp.getSnapshot().servers.filter((s) => s.state === "ready");
    // Providers: one set per ready server, re-registered when its capabilities change.
    const keep = new Set<string>();
    for (const s of ready) {
      const key = `${s.serverId}#${JSON.stringify(s.capabilities ?? {})}`;
      keep.add(key);
      if (!this.providers.has(key)) this.providers.set(key, this.register(s));
    }
    for (const [key, list] of [...this.providers]) {
      if (keep.has(key)) continue;
      for (const d of list) d.dispose();
      this.providers.delete(key);
    }
    // Monaco's TypeScript service would duplicate a TypeScript server's results.
    const tsServer = this.lsp.getSnapshot().servers.some((s) => s.languages.some((l) => TS_LANGS.includes(l)) && (s.state === "ready" || s.state === "starting"));
    this.setBuiltinTs(!tsServer);
    // Models of languages that got a server (the servers load after the first models).
    for (const m of monaco.editor.getModels()) if (!this.synced.has(m)) this.track(m);
  }

  private setBuiltinTs(on: boolean): void {
    if (on === !this.builtinTsOff) return;
    this.builtinTsOff = !on;
    const mode = {
      completionItems: on, hovers: on, documentSymbols: on, definitions: on, references: on, documentHighlights: on,
      rename: on, diagnostics: on, documentRangeFormattingEdits: on, signatureHelp: on, onTypeFormattingEdits: on,
      codeActions: on, inlayHints: on,
    };
    monaco.typescript.typescriptDefaults.setModeConfiguration(mode);
    monaco.typescript.javascriptDefaults.setModeConfiguration(mode);
  }

  /** The Monaco language ids a server's providers apply to. */
  private selector(s: ServerInfo): string[] {
    return s.languages.flatMap((l) => (l === "typescriptreact" ? ["typescript"] : l === "javascriptreact" ? ["javascript"] : [l]));
  }

  /** The server and URI for a model, or null when it is not synced to `serverId`. */
  private target(model: Model, serverId: string): { uri: string } | null {
    const s = this.synced.get(model);
    return s && s.serverId === serverId ? { uri: s.uri } : null;
  }

  private ask<T>(serverId: string, method: string, params: unknown, token: monaco.CancellationToken): Promise<T | null> {
    return this.lsp
      .request<T>(serverId, method, params, (cancel) => token.onCancellationRequested(cancel))
      .catch(() => null);
  }

  private register(s: ServerInfo): monaco.IDisposable[] {
    const caps = (s.capabilities ?? {}) as Record<string, unknown>;
    const id = s.serverId;
    const sel = this.selector(s);
    const has = (k: string) => !!caps[k];
    const out: monaco.IDisposable[] = [];
    const docPos = (model: Model, position: monaco.Position) => {
      const t = this.target(model, id);
      return t ? { textDocument: { uri: t.uri }, position: toLspPosition(position.lineNumber, position.column) } : null;
    };

    if (has("completionProvider")) {
      const cp = caps.completionProvider as { triggerCharacters?: string[]; resolveProvider?: boolean };
      type Item = monaco.languages.CompletionItem & { _lsp?: LspCompletionItem };
      out.push(
        monaco.languages.registerCompletionItemProvider(sel, {
          triggerCharacters: cp.triggerCharacters,
          provideCompletionItems: async (model, position, context, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            const r = await this.ask<LspCompletionItem[] | { isIncomplete: boolean; items: LspCompletionItem[] }>(
              id,
              "textDocument/completion",
              { ...p, context: { triggerKind: context.triggerKind + 1, triggerCharacter: context.triggerCharacter } },
              token,
            );
            if (!r) return null;
            const items = Array.isArray(r) ? r : r.items;
            const w = model.getWordUntilPosition(position);
            const range = { startLineNumber: position.lineNumber, startColumn: w.startColumn, endLineNumber: position.lineNumber, endColumn: w.endColumn };
            return {
              incomplete: !Array.isArray(r) && r.isIncomplete,
              suggestions: items.map((it) => ({ ...(toCompletion(it, range) as monaco.languages.CompletionItem), _lsp: it }) as Item),
            };
          },
          resolveCompletionItem: cp.resolveProvider
            ? async (item: Item, token) => {
                if (!item._lsp) return item;
                const r = await this.ask<LspCompletionItem>(id, "completionItem/resolve", item._lsp, token);
                if (!r) return item;
                const c = toCompletion(r, item.range as monaco.IRange);
                return { ...item, detail: c.detail ?? item.detail, documentation: c.documentation ?? item.documentation, additionalTextEdits: c.additionalTextEdits ?? item.additionalTextEdits };
              }
            : undefined,
        }),
      );
    }
    if (has("hoverProvider")) {
      out.push(
        monaco.languages.registerHoverProvider(sel, {
          provideHover: async (model, position, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            const r = await this.ask<{ contents: Parameters<typeof toMarkdown>[0]; range?: LspRange }>(id, "textDocument/hover", p, token);
            const md = r && toMarkdown(r.contents);
            return md ? { contents: [md], range: r.range ? toMonacoRange(r.range) : undefined } : null;
          },
        }),
      );
    }
    if (has("signatureHelpProvider")) {
      const sp = caps.signatureHelpProvider as { triggerCharacters?: string[]; retriggerCharacters?: string[] };
      out.push(
        monaco.languages.registerSignatureHelpProvider(sel, {
          signatureHelpTriggerCharacters: sp.triggerCharacters,
          signatureHelpRetriggerCharacters: sp.retriggerCharacters,
          provideSignatureHelp: async (model, position, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            type Sig = { label: string; documentation?: Parameters<typeof toMarkdown>[0]; parameters?: { label: string | [number, number]; documentation?: Parameters<typeof toMarkdown>[0] }[]; activeParameter?: number };
            const r = await this.ask<{ signatures: Sig[]; activeSignature?: number; activeParameter?: number }>(id, "textDocument/signatureHelp", p, token);
            if (!r || !r.signatures?.length) return null;
            return {
              value: {
                signatures: r.signatures.map((sg) => ({
                  label: sg.label,
                  documentation: toMarkdown(sg.documentation),
                  parameters: (sg.parameters ?? []).map((pa) => ({ label: pa.label, documentation: toMarkdown(pa.documentation) })),
                  activeParameter: sg.activeParameter,
                })),
                activeSignature: r.activeSignature ?? 0,
                activeParameter: r.activeParameter ?? 0,
              },
              dispose: () => {},
            };
          },
        }),
      );
    }
    if (has("definitionProvider")) {
      out.push(
        monaco.languages.registerDefinitionProvider(sel, {
          provideDefinition: async (model, position, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            const r = await this.ask<unknown>(id, "textDocument/definition", p, token);
            return this.toMonacoLocations(r);
          },
        }),
      );
    }
    if (has("referencesProvider")) {
      out.push(
        monaco.languages.registerReferenceProvider(sel, {
          provideReferences: async (model, position, context, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            const r = await this.ask<unknown>(id, "textDocument/references", { ...p, context }, token);
            return this.toMonacoLocations(r);
          },
        }),
      );
    }
    if (has("documentHighlightProvider")) {
      out.push(
        monaco.languages.registerDocumentHighlightProvider(sel, {
          provideDocumentHighlights: async (model, position, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            const r = await this.ask<{ range: LspRange; kind?: number }[]>(id, "textDocument/documentHighlight", p, token);
            return r?.map((h) => ({ range: toMonacoRange(h.range), kind: highlightKind(h.kind) })) ?? null;
          },
        }),
      );
    }
    if (has("renameProvider")) {
      const prepare = typeof caps.renameProvider === "object" && !!(caps.renameProvider as { prepareProvider?: boolean }).prepareProvider;
      out.push(
        monaco.languages.registerRenameProvider(sel, {
          provideRenameEdits: async (model, position, newName, token) => {
            const p = docPos(model, position);
            if (!p) return null;
            const r = await this.lsp
              .request<Parameters<typeof workspaceEditFiles>[0]>(id, "textDocument/rename", { ...p, newName }, (c) => token.onCancellationRequested(c))
              .catch((e: unknown) => ({ rejectReason: e instanceof Error ? e.message : String(e) }));
            if (r && "rejectReason" in r) return { edits: [], rejectReason: r.rejectReason };
            return this.toMonacoEdit(r);
          },
          resolveRenameLocation: prepare
            ? async (model, position, token) => {
                const p = docPos(model, position);
                if (!p) return null;
                const r = await this.ask<LspRange | { range: LspRange; placeholder: string } | { defaultBehavior: boolean }>(id, "textDocument/prepareRename", p, token);
                if (!r) return { range: new monaco.Range(position.lineNumber, position.column, position.lineNumber, position.column), text: "", rejectReason: "This element cannot be renamed." };
                if ("defaultBehavior" in r) return null;
                const range = "range" in r ? r.range : r;
                const mr = toMonacoRange(range);
                return { range: mr, text: "placeholder" in r ? r.placeholder : model.getValueInRange(mr) };
              }
            : undefined,
        }),
      );
    }
    const options = (model: Model) => ({ tabSize: model.getOptions().tabSize, insertSpaces: model.getOptions().insertSpaces });
    if (has("documentFormattingProvider")) {
      out.push(
        monaco.languages.registerDocumentFormattingEditProvider(sel, {
          provideDocumentFormattingEdits: async (model, _o, token) => {
            const t = this.target(model, id);
            if (!t) return null;
            const r = await this.ask<LspTextEdit[]>(id, "textDocument/formatting", { textDocument: t, options: options(model) }, token);
            return r?.map((e) => ({ range: toMonacoRange(e.range), text: e.newText })) ?? null;
          },
        }),
      );
    }
    if (has("documentRangeFormattingProvider")) {
      out.push(
        monaco.languages.registerDocumentRangeFormattingEditProvider(sel, {
          provideDocumentRangeFormattingEdits: async (model, range, _o, token) => {
            const t = this.target(model, id);
            if (!t) return null;
            const r = await this.ask<LspTextEdit[]>(id, "textDocument/rangeFormatting", { textDocument: t, range: toLspRange(range), options: options(model) }, token);
            return r?.map((e) => ({ range: toMonacoRange(e.range), text: e.newText })) ?? null;
          },
        }),
      );
    }
    if (has("codeActionProvider")) {
      out.push(
        monaco.languages.registerCodeActionProvider(sel, {
          provideCodeActions: async (model, range, context, token) => {
            const t = this.target(model, id);
            if (!t) return null;
            // The markers in the range (Monaco does not say which owner set them; servers match
            // the diagnostics they published and ignore the rest).
            const diagnostics = context.markers.map((m) => ({ range: toLspRange(m), message: m.message, severity: m.severity === 8 ? 1 : m.severity === 4 ? 2 : m.severity === 2 ? 3 : 4, source: m.source, code: typeof m.code === "object" ? m.code.value : m.code }));
            type Action = { title: string; kind?: string; isPreferred?: boolean; edit?: Parameters<typeof workspaceEditFiles>[0]; command?: unknown };
            const r = await this.ask<Action[]>(id, "textDocument/codeAction", { textDocument: t, range: toLspRange(range), context: { diagnostics, only: context.only ? [context.only] : undefined } }, token);
            if (!r) return null;
            // Edits only: commands that would run in the server are not executed in M4.
            const actions: monaco.languages.CodeAction[] = [];
            for (const a of r.filter((x) => x.edit && !x.command)) {
              const edit = await this.toMonacoEdit(a.edit);
              if (edit) actions.push({ title: a.title, kind: a.kind, isPreferred: a.isPreferred, edit });
            }
            return { actions, dispose: () => {} };
          },
        }),
      );
    }
    if (has("documentSymbolProvider")) {
      type Sym = { name: string; detail?: string; kind: number; range?: LspRange; selectionRange?: LspRange; location?: { range: LspRange }; children?: Sym[] };
      const toSym = (x: Sym): monaco.languages.DocumentSymbol => {
        const range = toMonacoRange(x.range ?? x.location!.range);
        return { name: x.name, detail: x.detail ?? "", kind: symbolKind(x.kind), tags: [], range, selectionRange: x.selectionRange ? toMonacoRange(x.selectionRange) : range, children: x.children?.map(toSym) };
      };
      out.push(
        monaco.languages.registerDocumentSymbolProvider(sel, {
          provideDocumentSymbols: async (model, token) => {
            const t = this.target(model, id);
            if (!t) return null;
            const r = await this.ask<Sym[]>(id, "textDocument/documentSymbol", { textDocument: t }, token);
            return r?.filter((x) => x.range || x.location).map(toSym) ?? null;
          },
        }),
      );
    }
    return out;
  }

  // ---- locations and edits --------------------------------------------------------------

  /** The Monaco model for a URI: a workspace file's (if open) or an external one's. */
  private modelFor(uri: string): Model | null {
    return monaco.editor.getModel(this.monacoUri(uri));
  }

  /** Workspace files are `file:///<path>` (the documents store's models); files outside the
   * workspace get the `axiom-external` scheme with their absolute path, so the two never mix. */
  private monacoUri(uri: string): monaco.Uri {
    const path = this.lsp.pathOf(uri);
    if (path !== null) return monaco.Uri.from({ scheme: "file", path: `/${path}` });
    return monaco.Uri.from({ scheme: EXTERNAL, path: monaco.Uri.parse(uri).path });
  }

  /** Locations as Monaco sees them: workspace files under their model URIs; external files get a
   * read-only model (their text comes from the daemon), so a peek can show them. */
  private async toMonacoLocations(r: unknown): Promise<monaco.languages.Location[] | null> {
    const list = locations(r);
    if (!list.length) return null;
    const out: monaco.languages.Location[] = [];
    for (const l of list) {
      const range = l.selection ?? l.range;
      const uri = this.monacoUri(l.uri);
      if (uri.scheme === EXTERNAL && !monaco.editor.getModel(uri)) {
        const text = await this.lsp.readExternal(l.uri).catch(() => null);
        if (text === null) continue;
        const lang = monaco.languages.getLanguages().find((x) => x.extensions?.some((e) => uri.path.endsWith(e)))?.id;
        monaco.editor.createModel(text, lang, uri);
      }
      out.push({ uri, range });
    }
    return out;
  }

  /** A WorkspaceEdit as Monaco's, opening (in the background) the workspace files it changes so
   * every edit lands in a model, undoable and unsaved: the person saves. */
  private async toMonacoEdit(edit: Parameters<typeof workspaceEditFiles>[0]): Promise<monaco.languages.WorkspaceEdit | null> {
    const { files } = workspaceEditFiles(edit);
    for (const f of files) {
      const path = this.lsp.pathOf(f.uri);
      if (path !== null && !this.modelFor(f.uri)) await this.documents.openInBackground(path);
    }
    return this.monacoEdit(files);
  }

  private monacoEdit(files: FileEdits[]): monaco.languages.WorkspaceEdit | null {
    const edits: monaco.languages.IWorkspaceTextEdit[] = [];
    for (const f of files) {
      const model = this.modelFor(f.uri);
      if (!model) continue;
      for (const e of f.edits) edits.push({ resource: model.uri, textEdit: e, versionId: undefined });
    }
    return edits.length ? { edits } : null;
  }

  /** Cross-file navigation (F12 into another file): the documents store opens it in a tab. A
   * file outside the workspace has no tab: it is shown in a peek instead. */
  private open(source: monaco.editor.ICodeEditor, resource: monaco.Uri, selection: monaco.IRange | monaco.IPosition | undefined): boolean {
    if (resource.scheme === EXTERNAL) {
      void source.getAction("editor.action.peekDefinition")?.run();
      return true;
    }
    if (resource.scheme !== "file" || resource.authority) return false;
    const path = resource.path.replace(/^\//, "");
    if (!path) return false;
    const line = selection ? ("startLineNumber" in selection ? selection.startLineNumber : selection.lineNumber) : 1;
    const column = selection ? ("startColumn" in selection ? selection.startColumn : selection.column) : 1;
    this.documents.openAt(path, line, column);
    return true;
  }

  // ---- document sync -----------------------------------------------------------------

  private track(model: Model): void {
    if (this.synced.has(model)) return;
    const path = modelPath(model);
    if (!path) return;
    const languageId = lspLanguageId(path, model.getLanguageId());
    this.lsp.noteLanguage(path, languageId);
    const server = this.lsp.serverFor(languageId);
    if (!server || server.state === "missing" || server.state === "failed") return;
    const uri = this.lsp.uriOf(path);
    const serverId = server.serverId;
    const sub = model.onDidChangeContent((e) => {
      // The server's sync kind as it is now (it may not have been ready when the model opened).
      // Before it is ready the daemon's mirror takes incremental changes and re-opens the
      // document with its full text once the server is up.
      const now = this.lsp.getSnapshot().servers.find((x) => x.serverId === serverId);
      const sync = (now?.capabilities as { textDocumentSync?: number | { change?: number } } | undefined)?.textDocumentSync ?? 2;
      const kind = typeof sync === "number" ? sync : (sync.change ?? 2);
      const changes = kind === 1 ? [{ text: model.getValue() }] : contentChanges(e.changes);
      void this.lsp.notify(serverId, "textDocument/didChange", { textDocument: { uri, version: model.getVersionId() }, contentChanges: changes }).catch(() => {});
    });
    this.synced.set(model, { serverId, uri, languageId, sub });
    void this.lsp
      .notify(serverId, "textDocument/didOpen", { textDocument: { uri, languageId, version: model.getVersionId(), text: model.getValue() } })
      .catch(() => {});
  }

  private untrack(model: Model): void {
    const path = modelPath(model);
    if (path) this.lsp.noteLanguage(path, null);
    const s = this.synced.get(model);
    if (!s) return;
    s.sub.dispose();
    this.synced.delete(model);
    monaco.editor.setModelMarkers(model, `lsp:${s.serverId}`, []);
    void this.lsp.notify(s.serverId, "textDocument/didClose", { textDocument: { uri: s.uri } }).catch(() => {});
  }
}
