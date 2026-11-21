// The code view: re_editor (MIT) styled from the design tokens, with re_highlight colouring
// (ADR-0020 §3). The documents store owns the text; this widget presents one document's
// controller.

import 'package:flutter/widgets.dart';
import 'package:re_editor/re_editor.dart';
import 'package:re_highlight/languages/bash.dart';
import 'package:re_highlight/languages/c.dart';
import 'package:re_highlight/languages/cpp.dart';
import 'package:re_highlight/languages/css.dart';
import 'package:re_highlight/languages/dart.dart';
import 'package:re_highlight/languages/go.dart';
import 'package:re_highlight/languages/ini.dart';
import 'package:re_highlight/languages/java.dart';
import 'package:re_highlight/languages/javascript.dart';
import 'package:re_highlight/languages/json.dart';
import 'package:re_highlight/languages/markdown.dart';
import 'package:re_highlight/languages/python.dart';
import 'package:re_highlight/languages/rust.dart';
import 'package:re_highlight/languages/sql.dart';
import 'package:re_highlight/languages/typescript.dart';
import 'package:re_highlight/languages/xml.dart';
import 'package:re_highlight/languages/yaml.dart';
import 'package:re_highlight/re_highlight.dart';

import '../ui/theme.dart';
import '../ui/tokens.dart';

/// Languages by file extension (and a few names); the LSP language id is the map's value.
const _byExtension = {
  'rs': 'rust',
  'ts': 'typescript',
  'tsx': 'typescript',
  'mts': 'typescript',
  'cts': 'typescript',
  'js': 'javascript',
  'jsx': 'javascript',
  'mjs': 'javascript',
  'cjs': 'javascript',
  'json': 'json',
  'md': 'markdown',
  'py': 'python',
  'sh': 'bash',
  'bash': 'bash',
  'toml': 'ini',
  'ini': 'ini',
  'yaml': 'yaml',
  'yml': 'yaml',
  'html': 'xml',
  'xml': 'xml',
  'svg': 'xml',
  'css': 'css',
  'c': 'c',
  'h': 'c',
  'cc': 'cpp',
  'cpp': 'cpp',
  'hpp': 'cpp',
  'go': 'go',
  'java': 'java',
  'dart': 'dart',
  'sql': 'sql',
};

/// The highlight language for [path], or null (plain text).
String? languageOf(String path) {
  final name = path.split('/').last;
  if (name == 'Cargo.lock') return 'ini';
  final dot = name.lastIndexOf('.');
  if (dot < 0) return null;
  return _byExtension[name.substring(dot + 1).toLowerCase()];
}

/// A display name for the status bar.
String languageName(String? lang) => switch (lang) {
  null => 'Plain Text',
  'rust' => 'Rust',
  'typescript' => 'TypeScript',
  'javascript' => 'JavaScript',
  'json' => 'JSON',
  'markdown' => 'Markdown',
  'python' => 'Python',
  'bash' => 'Shell',
  'ini' => 'TOML',
  'yaml' => 'YAML',
  'xml' => 'XML',
  'css' => 'CSS',
  'c' => 'C',
  'cpp' => 'C++',
  'go' => 'Go',
  'java' => 'Java',
  'dart' => 'Dart',
  'sql' => 'SQL',
  _ => lang,
};

final _modes = <String, Mode>{
  'rust': langRust,
  'typescript': langTypescript,
  'javascript': langJavascript,
  'json': langJson,
  'markdown': langMarkdown,
  'python': langPython,
  'bash': langBash,
  'ini': langIni,
  'yaml': langYaml,
  'xml': langXml,
  'css': langCss,
  'c': langC,
  'cpp': langCpp,
  'go': langGo,
  'java': langJava,
  'dart': langDart,
  'sql': langSql,
};

/// highlight.js scopes → the syntax palette (the same roles the Monaco theme used).
Map<String, TextStyle> syntaxTheme(SyntaxColors y) {
  TextStyle c(Color col, {bool italic = false}) =>
      TextStyle(color: col, fontStyle: italic ? FontStyle.italic : null);
  return {
    'root': TextStyle(color: y.text),
    'comment': c(y.comment, italic: true),
    'quote': c(y.comment, italic: true),
    'doctag': c(y.keyword),
    'keyword': c(y.keyword),
    'selector-tag': c(y.keyword),
    'section': c(y.keyword),
    'name': c(y.keyword),
    'type': c(y.type),
    'title.class': c(y.type),
    'title.class.inherited': c(y.type),
    'built_in': c(y.type),
    'class': c(y.type),
    'title': c(y.text),
    'title.function': c(y.text),
    'string': c(y.string),
    'regexp': c(y.string),
    'template-tag': c(y.string),
    'subst': c(y.text),
    'number': c(y.number),
    'literal': c(y.number),
    'symbol': c(y.number),
    'bullet': c(y.number),
    'char.escape': c(y.number),
    'operator': c(y.punct),
    'punctuation': c(y.punct),
    'meta': c(y.macro),
    'meta.keyword': c(y.macro),
    'meta.string': c(y.string),
    'attr': c(y.macro),
    'attribute': c(y.macro),
    'variable': c(y.text),
    'variable.language': c(y.keyword),
    'params': c(y.text),
    'property': c(y.text),
    'tag': c(y.punct),
    'emphasis': const TextStyle(fontStyle: FontStyle.italic),
    'strong': const TextStyle(fontWeight: FontWeight.w600),
    'link': c(y.macro),
  };
}

/// One document in the editor.
class CodeView extends StatelessWidget {
  const CodeView({
    super.key,
    required this.controller,
    required this.language,
    this.focusNode,
    this.scrollController,
    this.findController,
    this.fontSize = 13,
    this.fontFamily,
    this.wordWrap = false,
    this.readOnly = false,
    this.onChanged,
    this.findBuilder,
    this.onSave,
    this.overrides = const {},
    this.gutterKey,
  });

  final CodeLineEditingController controller;
  final String? language;
  final FocusNode? focusNode;
  final CodeScrollController? scrollController;
  final CodeFindController? findController;
  final double fontSize;
  final String? fontFamily;
  final bool wordWrap;
  final bool readOnly;
  final ValueChanged<CodeLineEditingValue>? onChanged;
  final CodeFindBuilder? findBuilder;

  /// Mod+S inside the editor (re_editor takes the key for its save intent).
  final VoidCallback? onSave;

  /// Extra key overrides (the completion popup takes the arrows, Enter, Tab and Escape).
  final Map<Type, Action<Intent>> overrides;

  /// Measures the gutter (popups are placed relative to the text).
  final GlobalKey? gutterKey;

  @override
  Widget build(BuildContext context) {
    final t = context.axiom;
    final c = t.colors;
    final y = c.syntax;
    final mode = language == null ? null : _modes[language];
    return CodeEditor(
      controller: controller,
      focusNode: focusNode,
      scrollController: scrollController,
      findController: findController,
      findBuilder: findBuilder,
      readOnly: readOnly,
      wordWrap: wordWrap,
      onChanged: onChanged,
      padding: const EdgeInsets.only(left: 6, right: 12, top: 6, bottom: 64),
      shortcutOverrideActions: {
        ...overrides,
        if (onSave != null)
          CodeShortcutSaveIntent: CallbackAction<CodeShortcutSaveIntent>(
            onInvoke: (_) {
              onSave!();
              return null;
            },
          ),
      },
      style: CodeEditorStyle(
        fontSize: fontSize,
        fontFamily: (fontFamily == null || fontFamily!.isEmpty) ? AxiomType.mono : fontFamily,
        fontHeight: 1.5,
        textColor: y.text,
        backgroundColor: c.ground,
        selectionColor: c.selection,
        highlightColor: c.accentSoft,
        cursorColor: c.accent,
        cursorWidth: 2,
        cursorLineColor: y.currentLine,
        chunkIndicatorColor: y.lineNumber,
        codeTheme: mode == null
            ? null
            : CodeHighlightTheme(
                languages: {language!: CodeHighlightThemeMode(mode: mode)},
                theme: syntaxTheme(y),
              ),
      ),
      indicatorBuilder: (context, editingController, chunkController, notifier) => Row(
        key: gutterKey,
        children: [
          DefaultCodeLineNumber(
            controller: editingController,
            notifier: notifier,
            textStyle: TextStyle(
              fontFamily: AxiomType.mono,
              fontSize: fontSize * 0.92,
              color: y.lineNumber,
            ),
            focusedTextStyle: TextStyle(
              fontFamily: AxiomType.mono,
              fontSize: fontSize * 0.92,
              color: y.lineNumberActive,
            ),
          ),
          const SizedBox(width: 10),
        ],
      ),
    );
  }
}
