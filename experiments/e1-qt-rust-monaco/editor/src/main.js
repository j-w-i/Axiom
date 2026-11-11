import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import TsWorker from "monaco-editor/language/typescript/ts.worker?worker";

self.MonacoEnvironment = {
  getWorker(_id, label) {
    return label === "typescript" || label === "javascript" ? new TsWorker() : new EditorWorker();
  },
};

const editor = monaco.editor.create(document.getElementById("editor"), {
  value: "", language: "plaintext", theme: "vs-dark", automaticLayout: true,
});
let currentPath = null;

new QWebChannel(qt.webChannelTransport, (channel) => {
  const host = channel.objects.host;

  host.openDocument.connect((path, text, languageId) => {
    currentPath = path;
    const old = editor.getModel();
    editor.setModel(monaco.editor.createModel(text, languageId));
    old?.dispose();
    editor.focus();
    host.documentOpened(path, editor.getModel().getLineCount());
  });

  // Host-initiated commands used by the self-test (and later by agent tools).
  host.applyEdit.connect((line, column, text) => {
    editor.executeEdits("host", [{ range: new monaco.Range(line, column, line, column), text }]);
  });
  host.requestSave.connect(() => host.saveDocument(currentPath, editor.getValue()));
  host.requestState.connect(async () => {
    // Give the TS worker a moment, then report markers to prove workers run.
    const model = editor.getModel();
    let markers = [];
    for (let i = 0; i < 40; i++) {
      markers = monaco.editor.getModelMarkers({ resource: model.uri });
      if (markers.length) break;
      await new Promise((r) => setTimeout(r, 250));
    }
    host.stateReport(JSON.stringify({
      text: editor.getValue(), hasTextFocus: editor.hasTextFocus(),
      secureContext: window.isSecureContext, origin: location.origin,
      markers: markers.map((m) => ({ line: m.startLineNumber, message: m.message })),
      userAgent: navigator.userAgent,
    }));
  });

  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => host.saveDocument(currentPath, editor.getValue()));
  host.editorReady(monaco.editor.getModels().length >= 0 ? "ready" : "?");
});
