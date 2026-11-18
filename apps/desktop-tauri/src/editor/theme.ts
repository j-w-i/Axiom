// Monaco themes from the design tokens (design-system.md §2.2; M1.6-EDITOR-002). Pure: the
// token reader is passed in, so the rule table is unit-tested without a DOM or Monaco. The
// table reproduces the prototype's `themeFrom` (experiments/e5-visual-concepts/src/monaco.ts).

import type * as monaco from "monaco-editor";

/** A CSS colour as Monaco hex: #rrggbb, or #rrggbbaa when translucent. `canvasStyle` is what
 * a canvas 2D context's fillStyle reads back for the colour (a hex string, or an RGBA function when translucent). */
export function hexFromCanvasStyle(canvasStyle: string): string {
  if (canvasStyle.startsWith("#")) return canvasStyle.toLowerCase();
  const m = /rgba?\(([^)]+)\)/.exec(canvasStyle);
  if (!m) return "#000000";
  const [r = "0", g = "0", b = "0", a = "1"] = m[1]!.split(/[\s,/]+/).filter(Boolean);
  const h = (n: number) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  const alpha = a.endsWith("%") ? parseFloat(a) / 100 : parseFloat(a);
  return `#${h(+r)}${h(+g)}${h(+b)}${alpha >= 1 ? "" : h(alpha * 255)}`;
}

/** Every token the themes read. */
export const THEME_TOKENS = [
  "--syn-text", "--syn-comment", "--syn-keyword", "--syn-type", "--syn-string", "--syn-number", "--syn-punct", "--syn-macro",
  "--editor-bg", "--editor-gutter", "--editor-line", "--editor-lnum", "--editor-lnum-active", "--editor-selection",
  "--editor-cursor", "--editor-guide", "--editor-guide-active",
  "--surface-overlay", "--line-strong", "--scroll-thumb", "--scroll-thumb-hover", "--warn", "--danger",
] as const;
export type ThemeToken = (typeof THEME_TOKENS)[number];

/**
 * The theme for one appearance. `color(token)` returns Monaco hex; `keywordStyle` is the value
 * of --syn-keyword-style ("" in the recommended direction, or e.g. "bold").
 */
export function themeData(base: "vs" | "vs-dark", color: (token: ThemeToken) => string, keywordStyle: string): monaco.editor.IStandaloneThemeData {
  const fg = (t: ThemeToken) => color(t).slice(1, 7); // rules take rrggbb without '#'
  const none = "#00000000";
  return {
    base,
    inherit: true,
    rules: [
      { token: "", foreground: fg("--syn-text") },
      { token: "comment", foreground: fg("--syn-comment"), fontStyle: "italic" },
      { token: "keyword", foreground: fg("--syn-keyword"), ...(keywordStyle ? { fontStyle: keywordStyle } : {}) },
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
      "editor.background": color("--editor-bg"),
      "editor.foreground": color("--syn-text"),
      "editorGutter.background": color("--editor-gutter"),
      "editor.lineHighlightBackground": color("--editor-line"),
      "editor.lineHighlightBorder": none,
      "editorLineNumber.foreground": color("--editor-lnum"),
      "editorLineNumber.activeForeground": color("--editor-lnum-active"),
      "editor.selectionBackground": color("--editor-selection"),
      "editor.inactiveSelectionBackground": color("--editor-selection"),
      "editorCursor.foreground": color("--editor-cursor"),
      "editorIndentGuide.background1": color("--editor-guide"),
      "editorIndentGuide.activeBackground1": color("--editor-guide-active"),
      "editorWidget.background": color("--surface-overlay"),
      "editorWidget.border": color("--line-strong"),
      "editorBracketMatch.background": none,
      "editorBracketMatch.border": color("--editor-guide-active"),
      // Focus is shown by the focused-region rule (SHELL-002), not a ring round the editor.
      "focusBorder": none,
      "scrollbar.shadow": none,
      "scrollbarSlider.background": color("--scroll-thumb"),
      "scrollbarSlider.hoverBackground": color("--scroll-thumb-hover"),
      "scrollbarSlider.activeBackground": color("--scroll-thumb-hover"),
      "minimap.background": color("--editor-bg"),
      "editorOverviewRuler.border": none,
      "editorWarning.foreground": color("--warn"),
      "editorError.foreground": color("--danger"),
    },
  };
}

/** Line height for a font size, keeping the --code-size / --code-leading ratio (13 / 20). */
export function lineHeightFor(fontSize: number, codeSize = 13, codeLeading = 20): number {
  return Math.round((fontSize * codeLeading) / codeSize);
}
