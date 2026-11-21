// LSP ↔ Monaco and LSP → Problems conversions (M4-LSP-002; ADR-0018 §3). Pure: no Monaco at
// runtime, so they are unit-tested in Node. Monaco enum values are the numbers of Monaco 0.57's
// declarations (checked in tests/lsp-convert.test.ts against the comments here).
//
// Positions: LSP lines and characters are 0-based UTF-16 code units; Monaco lines and columns are
// 1-based UTF-16 code units. Problems locations are 1-based.

import type { Diagnostic as Problem, Severity } from "../../daemon/protocol.gen";

export interface LspPosition {
  line: number;
  character: number;
}
export interface LspRange {
  start: LspPosition;
  end: LspPosition;
}
export interface MonacoRange {
  startLineNumber: number;
  startColumn: number;
  endLineNumber: number;
  endColumn: number;
}
export interface LspDiagnostic {
  range: LspRange;
  severity?: number;
  code?: string | number | { value: string | number };
  codeDescription?: { href: string };
  source?: string;
  message: string;
  tags?: number[];
  relatedInformation?: { location: { uri: string; range: LspRange }; message: string }[];
}
export interface LspTextEdit {
  range: LspRange;
  newText: string;
}

export const toLspPosition = (lineNumber: number, column: number): LspPosition => ({ line: lineNumber - 1, character: column - 1 });

export const toMonacoRange = (r: LspRange): MonacoRange => ({
  startLineNumber: r.start.line + 1,
  startColumn: r.start.character + 1,
  endLineNumber: r.end.line + 1,
  endColumn: r.end.character + 1,
});

export const toLspRange = (r: MonacoRange): LspRange => ({
  start: toLspPosition(r.startLineNumber, r.startColumn),
  end: toLspPosition(r.endLineNumber, r.endColumn),
});

// ---- URIs ------------------------------------------------------------------------------

/** `file://` URI of an absolute host path (`/a/b` or `C:\a\b`), percent-encoding each segment. */
export function fileUri(absPath: string): string {
  let p = absPath.replace(/\\/g, "/");
  if (/^[A-Za-z]:\//.test(p)) p = `/${p}`;
  return `file://${p.split("/").map((s, i) => (i === 1 && /^[A-Za-z]:$/.test(s) ? s : encodeURIComponent(s))).join("/")}`;
}

/** The workspace-relative path of `uri` under `rootUri`, or null when it is outside. */
export function relativeTo(uri: string, rootUri: string): string | null {
  const norm = (u: string) => u.replace(/\/+$/, "");
  const root = norm(rootUri);
  // Compare decoded forms (servers differ in what they encode, e.g. `:` on Windows drives).
  const dec = (u: string) => {
    try {
      return decodeURIComponent(u);
    } catch {
      return u;
    }
  };
  const a = dec(uri);
  const r = dec(root);
  if (!a.startsWith(`${r}/`)) return null;
  return a.slice(r.length + 1);
}

/** The LSP language id for a file (Monaco's, refined where servers distinguish them). */
export function lspLanguageId(path: string, monacoLanguage: string): string {
  if (/\.tsx$/i.test(path)) return "typescriptreact";
  if (/\.jsx$/i.test(path)) return "javascriptreact";
  return monacoLanguage;
}

// ---- diagnostics -----------------------------------------------------------------------

/** monaco.MarkerSeverity: Hint 1, Info 2, Warning 4, Error 8. */
export function markerSeverity(lsp: number | undefined): 1 | 2 | 4 | 8 {
  switch (lsp) {
    case 2:
      return 4;
    case 3:
      return 2;
    case 4:
      return 1;
    default:
      return 8;
  }
}

function severityName(lsp: number | undefined): Severity {
  return lsp === 2 ? "warning" : lsp === 3 ? "info" : lsp === 4 ? "hint" : "error";
}

function codeText(c: LspDiagnostic["code"]): string | undefined {
  if (c === undefined || c === null) return undefined;
  return typeof c === "object" ? String(c.value) : String(c);
}

/** A Monaco marker (IMarkerData) for an LSP diagnostic. */
export function toMarker(d: LspDiagnostic) {
  const code = codeText(d.code);
  return {
    ...toMonacoRange(d.range),
    severity: markerSeverity(d.severity),
    message: d.message,
    source: d.source,
    code: code && d.codeDescription ? { value: code, target: d.codeDescription.href } : code,
    // monaco.MarkerTag: Unnecessary 1, Deprecated 2 (as LSP's DiagnosticTag).
    tags: d.tags?.filter((t) => t === 1 || t === 2),
  };
}

/** A Problems row (the build diagnostic type) for an LSP diagnostic of a workspace file. */
export function toProblem(d: LspDiagnostic, file: string, serverId: string): Problem {
  return {
    severity: severityName(d.severity),
    message: d.message.split("\n")[0] ?? d.message,
    code: codeText(d.code) ?? null,
    source: d.source ? `${d.source} (${serverId})` : serverId,
    location: {
      file,
      line: d.range.start.line + 1,
      column: d.range.start.character + 1,
      endLine: d.range.end.line + 1,
      endColumn: d.range.end.character + 1,
    },
    related: [],
    rendered: d.message.includes("\n") ? d.message : null,
  };
}

// ---- completion ------------------------------------------------------------------------

/** LSP CompletionItemKind (1-based) → monaco.languages.CompletionItemKind. */
const COMPLETION_KIND: Record<number, number> = {
  1: 18, // Text
  2: 0, // Method
  3: 1, // Function
  4: 2, // Constructor
  5: 3, // Field
  6: 4, // Variable
  7: 5, // Class
  8: 7, // Interface
  9: 8, // Module
  10: 9, // Property
  11: 12, // Unit
  12: 13, // Value
  13: 15, // Enum
  14: 17, // Keyword
  15: 28, // Snippet
  16: 19, // Color
  17: 20, // File
  18: 21, // Reference
  19: 23, // Folder
  20: 16, // EnumMember
  21: 14, // Constant
  22: 6, // Struct
  23: 10, // Event
  24: 11, // Operator
  25: 24, // TypeParameter
};
export const completionKind = (lsp: number | undefined): number => COMPLETION_KIND[lsp ?? 1] ?? 18;

/** LSP SymbolKind (1-based) → monaco.languages.SymbolKind (0-based, same order). */
export const symbolKind = (lsp: number): number => Math.max(0, Math.min(25, lsp - 1));

/** LSP DocumentHighlightKind (Text 1, Read 2, Write 3) → Monaco's (0, 1, 2). */
export const highlightKind = (lsp: number | undefined): number => (lsp ? lsp - 1 : 0);

export interface LspCompletionItem {
  label: string;
  labelDetails?: { detail?: string; description?: string };
  kind?: number;
  tags?: number[];
  detail?: string;
  documentation?: string | { kind: string; value: string };
  sortText?: string;
  filterText?: string;
  preselect?: boolean;
  insertText?: string;
  insertTextFormat?: number;
  textEdit?: LspTextEdit | { newText: string; insert: LspRange; replace: LspRange };
  additionalTextEdits?: LspTextEdit[];
  commitCharacters?: string[];
}

/** The parts of a Monaco CompletionItem that come from the LSP item (range defaults to the
 * word at the position, given by the caller). */
export function toCompletion(item: LspCompletionItem, defaultRange: MonacoRange) {
  const te = item.textEdit;
  let range: MonacoRange | { insert: MonacoRange; replace: MonacoRange } = defaultRange;
  let insertText = item.insertText ?? item.label;
  if (te) {
    insertText = te.newText;
    range = "range" in te ? toMonacoRange(te.range) : { insert: toMonacoRange(te.insert), replace: toMonacoRange(te.replace) };
  }
  return {
    label: item.labelDetails ? { label: item.label, detail: item.labelDetails.detail, description: item.labelDetails.description } : item.label,
    kind: completionKind(item.kind),
    tags: item.tags?.includes(1) ? [1] : undefined,
    detail: item.detail,
    documentation: toMarkdown(item.documentation),
    sortText: item.sortText,
    filterText: item.filterText,
    preselect: item.preselect,
    insertText,
    // monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet = 4.
    insertTextRules: item.insertTextFormat === 2 ? 4 : undefined,
    range,
    commitCharacters: item.commitCharacters,
    additionalTextEdits: item.additionalTextEdits?.map((e) => ({ range: toMonacoRange(e.range), text: e.newText })),
  };
}

// ---- markup ------------------------------------------------------------------------------

type MarkedString = string | { language: string; value: string };

/** LSP MarkupContent / MarkedString(s) → a Monaco IMarkdownString (undefined when empty). */
export function toMarkdown(v: string | { kind?: string; value: string; language?: string } | MarkedString[] | null | undefined): { value: string } | undefined {
  if (v === null || v === undefined) return undefined;
  if (Array.isArray(v)) {
    const parts = v.map((x) => toMarkdown(x)?.value).filter((x): x is string => !!x);
    return parts.length ? { value: parts.join("\n\n") } : undefined;
  }
  if (typeof v === "string") return v ? { value: v } : undefined;
  if ("language" in v && v.language) return { value: `\`\`\`${v.language}\n${v.value}\n\`\`\`` };
  if (v.kind === "plaintext") return v.value ? { value: v.value.replace(/[\\`*_{}[\]()#+\-.!|<>]/g, "\\$&") } : undefined;
  return v.value ? { value: v.value } : undefined;
}

// ---- edits -------------------------------------------------------------------------------

export interface FileEdits {
  uri: string;
  /** The document version the edits were computed for, when the server said. */
  version?: number | null;
  edits: { range: MonacoRange; text: string }[];
}

/** The text edits of a WorkspaceEdit, per file (`changes` or `documentChanges` text edits).
 * File operations (create, rename, delete) are returned separately: M4 does not apply them. */
export function workspaceEditFiles(edit: {
  changes?: Record<string, LspTextEdit[]>;
  documentChanges?: ({ textDocument: { uri: string; version?: number | null }; edits: LspTextEdit[] } | { kind: string })[];
} | null | undefined): { files: FileEdits[]; fileOperations: number } {
  const files: FileEdits[] = [];
  let fileOperations = 0;
  if (!edit) return { files, fileOperations };
  if (edit.documentChanges) {
    for (const c of edit.documentChanges) {
      if ("kind" in c) {
        fileOperations++;
        continue;
      }
      files.push({ uri: c.textDocument.uri, version: c.textDocument.version, edits: c.edits.map((e) => ({ range: toMonacoRange(e.range), text: e.newText })) });
    }
  } else if (edit.changes) {
    for (const [uri, edits] of Object.entries(edit.changes)) files.push({ uri, edits: edits.map((e) => ({ range: toMonacoRange(e.range), text: e.newText })) });
  }
  return { files, fileOperations };
}

// ---- locations ---------------------------------------------------------------------------

export interface Located {
  uri: string;
  range: MonacoRange;
  /** For a LocationLink: the range to reveal (`targetSelectionRange`). */
  selection?: MonacoRange;
}

/** Location | Location[] | LocationLink[] | null → a list. */
export function locations(v: unknown): Located[] {
  if (!v) return [];
  const arr = Array.isArray(v) ? v : [v];
  const out: Located[] = [];
  for (const x of arr as Record<string, unknown>[]) {
    if (typeof x.targetUri === "string") {
      out.push({
        uri: x.targetUri,
        range: toMonacoRange(x.targetRange as LspRange),
        selection: x.targetSelectionRange ? toMonacoRange(x.targetSelectionRange as LspRange) : undefined,
      });
    } else if (typeof x.uri === "string" && x.range) {
      out.push({ uri: x.uri, range: toMonacoRange(x.range as LspRange) });
    }
  }
  return out;
}

/** Incremental `contentChanges` for Monaco content changes (in the order Monaco reports them,
 * which applies correctly one after another). */
export function contentChanges(changes: readonly { range: MonacoRange; text: string; rangeLength: number }[]) {
  return changes.map((c) => ({ range: toLspRange(c.range), rangeLength: c.rangeLength, text: c.text }));
}
