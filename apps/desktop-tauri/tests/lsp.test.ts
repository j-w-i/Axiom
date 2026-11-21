// The language-server client's pure parts and its store (M4-LSP-002): LSP ↔ Monaco conversions
// (positions, kinds, markup, edits, locations, diagnostics), and the store's servers, Problems
// rows and requests through the daemon.
import { describe, expect, it } from "vitest";
import type { DaemonClient } from "../src/daemon/client";
import {
  completionKind,
  contentChanges,
  fileUri,
  highlightKind,
  locations,
  lspLanguageId,
  markerSeverity,
  relativeTo,
  symbolKind,
  toCompletion,
  toLspPosition,
  toMarkdown,
  toMarker,
  toMonacoRange,
  toProblem,
  workspaceEditFiles,
} from "../src/editor/lsp/convert";
import { LspStore } from "../src/state/lsp";
import { ProblemsStore } from "../src/state/problems";
import type { WorkspaceStore } from "../src/state/workspace";

const range = (sl: number, sc: number, el: number, ec: number) => ({ start: { line: sl, character: sc }, end: { line: el, character: ec } });

describe("LSP ↔ Monaco conversions", () => {
  it("positions and ranges: 0-based ↔ 1-based", () => {
    expect(toLspPosition(1, 1)).toEqual({ line: 0, character: 0 });
    expect(toMonacoRange(range(2, 4, 2, 9))).toEqual({ startLineNumber: 3, startColumn: 5, endLineNumber: 3, endColumn: 10 });
    expect(contentChanges([{ range: { startLineNumber: 1, startColumn: 4, endLineNumber: 1, endColumn: 5 }, rangeLength: 1, text: "z" }])).toEqual([
      { range: range(0, 3, 0, 4), rangeLength: 1, text: "z" },
    ]);
  });

  it("URIs: encoded segments, paths under the root, language ids", () => {
    expect(fileUri("/home/me/my project/a#b.rs")).toBe("file:///home/me/my%20project/a%23b.rs");
    expect(fileUri("C:\\Users\\me\\p")).toBe("file:///C:/Users/me/p");
    expect(relativeTo("file:///home/me/my%20project/src/a.rs", "file:///home/me/my%20project")).toBe("src/a.rs");
    expect(relativeTo("file:///home/me/my project/src/a.rs", "file:///home/me/my%20project/")).toBe("src/a.rs");
    expect(relativeTo("file:///home/me/other/a.rs", "file:///home/me/my%20project")).toBeNull();
    expect(relativeTo("file:///home/me/my%20projectX/a.rs", "file:///home/me/my%20project")).toBeNull();
    expect(lspLanguageId("src/App.tsx", "typescript")).toBe("typescriptreact");
    expect(lspLanguageId("a.jsx", "javascript")).toBe("javascriptreact");
    expect(lspLanguageId("a.rs", "rust")).toBe("rust");
  });

  it("kinds map to Monaco 0.57's enums", () => {
    expect(completionKind(2)).toBe(0); // Method
    expect(completionKind(3)).toBe(1); // Function
    expect(completionKind(22)).toBe(6); // Struct
    expect(completionKind(14)).toBe(17); // Keyword
    expect(completionKind(15)).toBe(28); // Snippet
    expect(completionKind(undefined)).toBe(18); // Text
    expect(symbolKind(12)).toBe(11); // Function
    expect(symbolKind(23)).toBe(22); // Struct
    expect(highlightKind(3)).toBe(2); // Write
    expect([1, 2, 3, 4, undefined].map(markerSeverity)).toEqual([8, 4, 2, 1, 8]);
  });

  it("completion: textEdit, insert/replace, snippets, label details", () => {
    const word = { startLineNumber: 1, startColumn: 5, endLineNumber: 1, endColumn: 7 };
    const a = toCompletion({ label: "len", kind: 2, insertText: "len()", insertTextFormat: 1 }, word);
    expect(a).toMatchObject({ label: "len", kind: 0, insertText: "len()", range: word, insertTextRules: undefined });
    const b = toCompletion({ label: "push", insertTextFormat: 2, textEdit: { range: range(0, 4, 0, 6), newText: "push(${1:value})" }, labelDetails: { detail: "(v)" } }, word);
    expect(b.insertText).toBe("push(${1:value})");
    expect(b.insertTextRules).toBe(4);
    expect(b.range).toEqual({ startLineNumber: 1, startColumn: 5, endLineNumber: 1, endColumn: 7 });
    expect(b.label).toEqual({ label: "push", detail: "(v)", description: undefined });
    const c = toCompletion({ label: "x", textEdit: { newText: "xx", insert: range(0, 0, 0, 1), replace: range(0, 0, 0, 2) } }, word);
    expect(c.range).toEqual({ insert: toMonacoRange(range(0, 0, 0, 1)), replace: toMonacoRange(range(0, 0, 0, 2)) });
  });

  it("markup: MarkupContent, MarkedString and lists; plaintext is escaped", () => {
    expect(toMarkdown({ kind: "markdown", value: "**b**" })).toEqual({ value: "**b**" });
    expect(toMarkdown({ kind: "plaintext", value: "a*b" })).toEqual({ value: "a\\*b" });
    expect(toMarkdown([{ language: "rust", value: "fn a()" }, "doc"])).toEqual({ value: "```rust\nfn a()\n```\n\ndoc" });
    expect(toMarkdown("")).toBeUndefined();
    expect(toMarkdown(null)).toBeUndefined();
  });

  it("workspace edits: changes and documentChanges; file operations counted, not applied", () => {
    const e1 = workspaceEditFiles({ changes: { "file:///w/a.rs": [{ range: range(0, 3, 0, 4), newText: "z" }] } });
    expect(e1.files).toEqual([{ uri: "file:///w/a.rs", edits: [{ range: toMonacoRange(range(0, 3, 0, 4)), text: "z" }] }]);
    const e2 = workspaceEditFiles({
      documentChanges: [
        { textDocument: { uri: "file:///w/a.rs", version: 3 }, edits: [{ range: range(1, 0, 1, 0), newText: "x" }] },
        { kind: "rename" },
      ],
    });
    expect(e2.files[0]!.version).toBe(3);
    expect(e2.fileOperations).toBe(1);
    expect(workspaceEditFiles(null).files).toEqual([]);
  });

  it("locations: Location, Location[], LocationLink[]", () => {
    expect(locations({ uri: "file:///a", range: range(0, 0, 0, 1) })).toHaveLength(1);
    const links = locations([{ targetUri: "file:///b", targetRange: range(0, 0, 5, 0), targetSelectionRange: range(1, 3, 1, 7) }]);
    expect(links[0]).toMatchObject({ uri: "file:///b", selection: toMonacoRange(range(1, 3, 1, 7)) });
    expect(locations(null)).toEqual([]);
  });

  it("diagnostics: markers and Problems rows labelled with their server", () => {
    const d = { range: range(4, 17, 4, 23), severity: 1, code: { value: "E0308" }, codeDescription: { href: "https://x" }, source: "rustc", message: "mismatched types\nexpected i32", tags: [1, 9] };
    expect(toMarker(d)).toMatchObject({ startLineNumber: 5, startColumn: 18, severity: 8, code: { value: "E0308", target: "https://x" }, tags: [1] });
    expect(toProblem(d, "src/main.rs", "rust-analyzer")).toEqual({
      severity: "error",
      message: "mismatched types",
      code: "E0308",
      source: "rustc (rust-analyzer)",
      location: { file: "src/main.rs", line: 5, column: 18, endLine: 5, endColumn: 24 },
      related: [],
      rendered: "mismatched types\nexpected i32",
    });
    expect(toProblem({ range: range(0, 0, 0, 1), severity: 2, message: "w" }, "a.ts", "tsls").source).toBe("tsls");
  });
});

function fakeDaemon() {
  const listeners = new Map<string, ((v: unknown) => void)[]>();
  const calls: { method: string; params: Record<string, unknown> }[] = [];
  const cancelled: string[] = [];
  const client = {
    request: async (method: string, params: Record<string, unknown> = {}) => {
      calls.push({ method, params });
      if (method === "lsp/servers") return { servers: [{ serverId: "rust-analyzer", providerId: "rust", languages: ["rust"], state: "idle" }, { serverId: "tsls", providerId: "node", languages: ["typescript", "javascript"], state: "missing", installHint: "npm i" }] };
      if (method === "fs/readFile") return { text: "external", version: "v" };
      return {};
    },
    cancellable: (method: string, params: Record<string, unknown>) => {
      calls.push({ method, params });
      return { promise: Promise.resolve({ result: { echoed: params.method } }), cancel: () => cancelled.push(String(params.method)) };
    },
    onNotification: (method: string, fn: (v: unknown) => void) => {
      listeners.set(method, [...(listeners.get(method) ?? []), fn]);
      return () => {};
    },
  };
  const emit = (method: string, params: unknown) => listeners.get(method)?.forEach((f) => f(params));
  return { client: client as unknown as DaemonClient, calls, cancelled, emit };
}

function fakeWorkspace(root = "/home/me/proj") {
  const handlers: Record<string, (() => void)[]> = { opened: [], closed: [] };
  const ws = {
    getSnapshot: () => ({ id: "w1", root }),
    subscribe: () => () => {},
    on: (e: string, fn: () => void) => void handlers[e]!.push(fn),
  };
  return { ws: ws as unknown as WorkspaceStore, fire: (e: string) => handlers[e]!.forEach((f) => f()) };
}

describe("the language-server store", () => {
  it("loads the servers when a workspace opens and follows their state", async () => {
    const d = fakeDaemon();
    const w = fakeWorkspace();
    const s = new LspStore(d.client, w.ws);
    w.fire("opened");
    await new Promise((r) => setTimeout(r, 0));
    expect(s.getSnapshot().servers.map((x) => x.state)).toEqual(["idle", "missing"]);
    expect(s.serverFor("typescriptreact")?.serverId).toBe("tsls");
    d.emit("lsp/serverState", { workspaceId: "w1", server: { serverId: "rust-analyzer", providerId: "rust", languages: ["rust"], state: "ready", capabilities: {} } });
    expect(s.serverFor("rust")?.state).toBe("ready");
    d.emit("lsp/serverState", { workspaceId: "other", server: { serverId: "rust-analyzer", providerId: "rust", languages: ["rust"], state: "failed" } });
    expect(s.serverFor("rust")?.state).toBe("ready");
    s.noteLanguage("src/a.rs", "rust");
    expect(s.serverForPath("src/a.rs")?.serverId).toBe("rust-analyzer");
    w.fire("closed");
    expect(s.getSnapshot().servers).toEqual([]);
  });

  it("diagnostics become Problems rows (merged with builds) and reach the editor's listener", () => {
    const d = fakeDaemon();
    const w = fakeWorkspace("/home/me/my proj");
    const s = new LspStore(d.client, w.ws);
    const problems = new ProblemsStore(d.client, w.ws);
    problems.attach(s);
    let notified = 0;
    problems.subscribe(() => notified++);
    const seen: (string | null)[] = [];
    s.onPublish((p) => seen.push(p.path));
    const uri = "file:///home/me/my%20proj/src/main.rs";
    d.emit("lsp/notification", { workspaceId: "w1", serverId: "rust-analyzer", method: "textDocument/publishDiagnostics", params: { uri, diagnostics: [{ range: range(1, 0, 1, 3), severity: 1, message: "bad" }] } });
    d.emit("lsp/notification", { workspaceId: "w1", serverId: "rust-analyzer", method: "textDocument/publishDiagnostics", params: { uri: "file:///usr/lib/x.rs", diagnostics: [{ range: range(0, 0, 0, 1), message: "outside" }] } });
    expect(seen).toEqual(["src/main.rs", null]);
    expect(problems.all()).toHaveLength(1);
    expect(problems.all()[0]).toMatchObject({ source: "rust-analyzer", location: { file: "src/main.rs", line: 2 } });
    expect(notified).toBeGreaterThan(0);
    // An empty publish clears the file; a stopped server's problems go.
    d.emit("lsp/notification", { workspaceId: "w1", serverId: "rust-analyzer", method: "textDocument/publishDiagnostics", params: { uri, diagnostics: [] } });
    expect(problems.all()).toHaveLength(0);
    d.emit("lsp/notification", { workspaceId: "w1", serverId: "rust-analyzer", method: "textDocument/publishDiagnostics", params: { uri, diagnostics: [{ range: range(1, 0, 1, 3), message: "again" }] } });
    d.emit("lsp/serverState", { workspaceId: "w1", server: { serverId: "rust-analyzer", providerId: "rust", languages: ["rust"], state: "stopped" } });
    expect(problems.all()).toHaveLength(0);
  });

  it("requests carry the workspace and server; cancelling sends $/cancel; URIs map both ways", async () => {
    const d = fakeDaemon();
    const w = fakeWorkspace("/home/me/my proj");
    const s = new LspStore(d.client, w.ws);
    let cancel: (() => void) | undefined;
    const r = await s.request("rust-analyzer", "textDocument/hover", { x: 1 }, (c) => (cancel = c));
    expect(r).toEqual({ echoed: "textDocument/hover" });
    expect(d.calls.at(-1)).toEqual({ method: "lsp/request", params: { workspaceId: "w1", serverId: "rust-analyzer", method: "textDocument/hover", params: { x: 1 } } });
    cancel?.();
    expect(d.cancelled).toEqual(["textDocument/hover"]);
    expect(s.uriOf("src/a b.rs")).toBe("file:///home/me/my%20proj/src/a%20b.rs");
    expect(s.pathOf(s.uriOf("src/a b.rs"))).toBe("src/a b.rs");
    expect(await s.readExternal("file:///usr/lib/x.rs")).toBe("external");
    expect(d.calls.at(-1)).toEqual({ method: "fs/readFile", params: { workspaceId: "w1", path: "file:///usr/lib/x.rs" } });
  });

  it("progress titles show while a server works", () => {
    const d = fakeDaemon();
    const s = new LspStore(d.client, fakeWorkspace().ws);
    d.emit("lsp/notification", { workspaceId: "w1", serverId: "ra", method: "$/progress", params: { token: "t", value: { kind: "begin", title: "Indexing" } } });
    expect(s.getSnapshot().progress).toEqual({ ra: "Indexing" });
    d.emit("lsp/notification", { workspaceId: "w1", serverId: "ra", method: "$/progress", params: { token: "t", value: { kind: "end" } } });
    expect(s.getSnapshot().progress).toEqual({});
  });
});
