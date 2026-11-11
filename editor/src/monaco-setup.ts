// Monaco wiring: web workers, theme and option mapping.
//
// Worker entry points (Monaco 0.57.0, via the package `exports` map
// `"./*": "./esm/vs/*.js"`), imported with Vite's `?worker` suffix. These
// match the ones recorded in experiments/README.md §E1; the older
// `monaco-editor/esm/vs/...` paths do not resolve.
//   editor : monaco-editor/editor/editor.worker
//   json   : monaco-editor/language/json/json.worker
//   css    : monaco-editor/language/css/css.worker   (labels: css, scss, less)
//   html   : monaco-editor/language/html/html.worker  (labels: html, handlebars, razor)
//   ts     : monaco-editor/language/typescript/ts.worker (labels: typescript, javascript)

import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/language/json/json.worker?worker";
import CssWorker from "monaco-editor/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/language/html/html.worker?worker";
import TsWorker from "monaco-editor/language/typescript/ts.worker?worker";
import type { ConfigurePayload, Theme } from "./bridge";

export const BUNDLED_WORKERS = ["editor", "json", "css", "html", "typescript"] as const;

export function installWorkers(): void {
  (self as unknown as { MonacoEnvironment: monaco.Environment }).MonacoEnvironment = {
    getWorker(_id: string, label: string): Worker {
      switch (label) {
        case "json":
          return new JsonWorker();
        case "css":
        case "scss":
        case "less":
          return new CssWorker();
        case "html":
        case "handlebars":
        case "razor":
          return new HtmlWorker();
        case "typescript":
        case "javascript":
          return new TsWorker();
        default:
          return new EditorWorker();
      }
    },
  };
}

export function monacoThemeFor(theme: Theme): "vs" | "vs-dark" {
  return theme === "light" ? "vs" : "vs-dark";
}

export function editorOptionsFor(c: ConfigurePayload): monaco.editor.IEditorOptions {
  const options: monaco.editor.IEditorOptions = {
    fontSize: c.fontSize,
    wordWrap: c.wordWrap ? "on" : "off",
  };
  // An empty family means "platform monospace": keep Monaco's default.
  if (c.fontFamily !== "") options.fontFamily = c.fontFamily;
  return options;
}

export function applyConfigure(editor: monaco.editor.IStandaloneCodeEditor, c: ConfigurePayload): void {
  monaco.editor.setTheme(monacoThemeFor(c.theme));
  editor.updateOptions(editorOptionsFor(c));
}

export function languageInfos(): readonly {
  id: string;
  extensions?: readonly string[];
  filenames?: readonly string[];
}[] {
  return monaco.languages.getLanguages();
}

/** Maps the bridge's `runCommand` names to Monaco action ids. */
export const COMMAND_ACTIONS: Record<string, string> = {
  undo: "undo",
  redo: "redo",
  find: "actions.find",
  replace: "editor.action.startFindReplaceAction",
  selectAll: "editor.action.selectAll",
  format: "editor.action.formatDocument",
};
