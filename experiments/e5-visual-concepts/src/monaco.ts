// Monaco for the prototypes. Themes are generated from the concept's CSS
// custom properties (the same rule production follows in
// apps/desktop-tauri/src/editor/surface.ts): syntax colours are design tokens.

import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";

let installed = false;
export function setupMonaco(): typeof monaco {
  if (!installed) {
    (self as unknown as { MonacoEnvironment: monaco.Environment }).MonacoEnvironment = {
      getWorker: () => new EditorWorker(),
    };
    installed = true;
  }
  return monaco;
}

function hex(c: string): string {
  const ctx = document.createElement("canvas").getContext("2d");
  if (!ctx) return "#000000";
  ctx.fillStyle = c;
  const v = ctx.fillStyle;
  if (v.startsWith("#")) return v;
  // rgba(...) from translucent tokens
  const m = /rgba?\(([^)]+)\)/.exec(v);
  if (!m) return "#000000";
  const [r, g, b, a = "1"] = m[1]!.split(",").map((s) => s.trim());
  const h = (n: number) => n.toString(16).padStart(2, "0");
  return `#${h(+r!)}${h(+g!)}${h(+b!)}${h(Math.round(+a * 255))}`;
}

/** Define (or redefine) a Monaco theme named `name` from tokens on `root`. */
export function themeFrom(root: HTMLElement, name: string, base: "vs" | "vs-dark"): void {
  const css = getComputedStyle(root);
  const v = (n: string) => hex(css.getPropertyValue(n).trim() || "#ff00ff");
  const fg = (n: string) => v(n).slice(1, 7);
  monaco.editor.defineTheme(name, {
    base,
    inherit: true,
    rules: [
      { token: "", foreground: fg("--syn-text") },
      { token: "comment", foreground: fg("--syn-comment"), fontStyle: "italic" },
      { token: "keyword", foreground: fg("--syn-keyword"), fontStyle: css.getPropertyValue("--syn-keyword-style").trim() || undefined },
      { token: "keyword.type", foreground: fg("--syn-type") },
      { token: "type", foreground: fg("--syn-type") },
      { token: "type.identifier", foreground: fg("--syn-type") },
      { token: "identifier", foreground: fg("--syn-text") },
      { token: "string", foreground: fg("--syn-string") },
      { token: "string.escape", foreground: fg("--syn-number") },
      { token: "number", foreground: fg("--syn-number") },
      { token: "constant", foreground: fg("--syn-number") },
      { token: "delimiter", foreground: fg("--syn-punct") },
      { token: "operator", foreground: fg("--syn-punct") },
      { token: "annotation", foreground: fg("--syn-macro") },
      { token: "attribute", foreground: fg("--syn-macro") },
      { token: "keyword.md", foreground: fg("--syn-keyword") },
      { token: "variable.md", foreground: fg("--syn-type") },
      { token: "strong", fontStyle: "bold" },
    ],
    colors: {
      "editor.background": v("--editor-bg"),
      "editor.foreground": v("--syn-text"),
      "editorGutter.background": v("--editor-gutter"),
      "editor.lineHighlightBackground": v("--editor-line"),
      "editor.lineHighlightBorder": "#00000000",
      "editorLineNumber.foreground": v("--editor-lnum"),
      "editorLineNumber.activeForeground": v("--editor-lnum-active"),
      "editor.selectionBackground": v("--editor-selection"),
      "editor.inactiveSelectionBackground": v("--editor-selection"),
      "editorCursor.foreground": v("--editor-cursor"),
      "editorIndentGuide.background1": v("--editor-guide"),
      "editorIndentGuide.activeBackground1": v("--editor-guide-active"),
      "editorWidget.background": v("--surface-overlay"),
      "editorWidget.border": v("--line-strong"),
      "editorBracketMatch.background": "#00000000",
      "editorBracketMatch.border": v("--editor-guide-active"),
      "focusBorder": "#00000000",
      "scrollbar.shadow": "#00000000",
      "scrollbarSlider.background": v("--scroll-thumb"),
      "scrollbarSlider.hoverBackground": v("--scroll-thumb-hover"),
      "scrollbarSlider.activeBackground": v("--scroll-thumb-hover"),
      "minimap.background": v("--editor-bg"),
      "editorOverviewRuler.border": "#00000000",
      "editorWarning.foreground": v("--warn"),
      "editorError.foreground": v("--danger"),
    },
  });
}
