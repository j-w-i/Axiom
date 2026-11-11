import * as monaco from "monaco-editor";
import { BRIDGE_VERSION, type Host } from "./bridge";
import { MONACO_VERSION } from "./version";
import { DocumentStore, inferLanguageId } from "./documents";
import { createFakeHost } from "./dev-host";
import {
  applyConfigure,
  COMMAND_ACTIONS,
  installWorkers,
  languageInfos,
  monacoThemeFor,
} from "./monaco-setup";

declare global {
  interface Window {
    qt?: { webChannelTransport: unknown };
    QWebChannel?: new (transport: unknown, onReady: (channel: { objects: Record<string, unknown> }) => void) => unknown;
  }
}

installWorkers();

function connectHost(): Promise<Host> {
  if (window.qt && window.QWebChannel) {
    const QWebChannel = window.QWebChannel;
    const transport = window.qt.webChannelTransport;
    return new Promise((resolve, reject) => {
      new QWebChannel(transport, (channel) => {
        const host = channel.objects["host"];
        if (host) resolve(host as Host);
        else reject(new Error("QWebChannel has no `host` object"));
      });
    });
  }
  // Plain browser: dev harness with a fake host.
  const fake = createFakeHost();
  (window as unknown as { __fakeHost: unknown }).__fakeHost = fake;
  return Promise.resolve(fake.host);
}

function errorDetail(e: unknown): { message: string; detail: string } {
  if (e instanceof Error) return { message: e.message, detail: e.stack ?? "" };
  return { message: String(e), detail: "" };
}

async function main(): Promise<void> {
  const host = await connectHost();

  const report = (e: unknown) => host.reportError(errorDetail(e));
  window.addEventListener("error", (ev) => {
    host.reportError({
      message: ev.message || "Uncaught error",
      detail: ev.error instanceof Error ? (ev.error.stack ?? "") : `${ev.filename}:${ev.lineno}:${ev.colno}`,
    });
  });
  window.addEventListener("unhandledrejection", (ev) => report(ev.reason));

  const container = document.getElementById("editor");
  if (!container) throw new Error("#editor element missing");
  const editor = monaco.editor.create(container, {
    model: null,
    theme: monacoThemeFor("dark"),
    automaticLayout: true,
  });

  const store = new DocumentStore(host, (path, text) =>
    monaco.editor.createModel(
      text,
      inferLanguageId(path, languageInfos()),
      monaco.Uri.from({ scheme: "file", path: `/${path}` }),
    ),
  );

  let activePath: string | null = null;
  const viewStates = new Map<string, monaco.editor.ICodeEditorViewState | null>();

  const activate = (path: string) => {
    const model = store.model(path);
    if (!model) return;
    if (activePath && activePath !== path) viewStates.set(activePath, editor.saveViewState());
    activePath = path;
    editor.setModel(model as monaco.editor.ITextModel);
    const vs = viewStates.get(path);
    if (vs) editor.restoreViewState(vs);
    editor.focus();
  };

  // Shell -> page.
  host.configure.connect((c) => applyConfigure(editor, c));
  host.openDocument.connect((p) => {
    const replacing = store.has(p.path);
    store.open(p);
    if (p.activate || (replacing && activePath === p.path)) activate(p.path);
  });
  host.activateDocument.connect((p) => activate(p.path));
  host.closeDocument.connect((p) => {
    const wasActive = activePath === p.path;
    // Detach before disposing so the editor never holds a disposed model.
    if (wasActive) {
      editor.setModel(null);
      activePath = null;
    }
    viewStates.delete(p.path);
    store.close(p.path);
  });
  host.requestSave.connect((p) => store.requestSave(p.path));
  host.documentSaved.connect((p) => store.documentSaved(p.path));
  host.saveFailed.connect((p) => store.saveFailed(p.path));
  host.revealPosition.connect((p) => {
    activate(p.path);
    const position = { lineNumber: p.line, column: p.column };
    editor.setPosition(position);
    editor.revealPositionInCenter(position);
  });
  host.runCommand.connect((p) => {
    const action = COMMAND_ACTIONS[p.command];
    if (action) editor.trigger("host", action, null);
  });

  // Page -> shell.
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => {
    if (activePath) store.requestSave(activePath);
  });
  editor.onDidChangeCursorPosition((e) => {
    if (!activePath) return;
    const sel = editor.getSelection();
    const model = editor.getModel();
    const selectionLength = sel && model ? model.getValueInRange(sel).length : 0;
    store.cursorMoved(activePath, e.position.lineNumber, e.position.column, selectionLength);
  });
  monaco.editor.registerLinkOpener({
    open(uri) {
      host.openLink({ url: uri.toString() });
      return true;
    },
  });

  // Subscribed to everything: now the handshake.
  host.ready({ bridgeVersion: BRIDGE_VERSION as 1, monacoVersion: MONACO_VERSION });
}

main().catch((e) => {
  console.error(e);
  // The host may not exist (connect failed); nothing else to do.
});
