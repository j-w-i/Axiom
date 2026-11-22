// Code in Chat coloured like the editor: re_highlight with the editor's languages and palette.

import 'package:flutter/painting.dart';
import 'package:re_highlight/re_highlight.dart';

import '../editor/code_view.dart';
import '../ui/tokens.dart';

final _hl = Highlight()..registerLanguages(highlightModes);

/// Fence names to the editor's language names.
String? _lang(String fence) => switch (fence.toLowerCase()) {
  'rs' || 'rust' => 'rust',
  'ts' || 'tsx' || 'typescript' => 'typescript',
  'js' || 'jsx' || 'javascript' || 'mjs' => 'javascript',
  'json' || 'jsonc' => 'json',
  'sh' || 'bash' || 'shell' || 'console' || 'zsh' => 'bash',
  'py' || 'python' => 'python',
  'toml' || 'ini' => 'ini',
  'yml' || 'yaml' => 'yaml',
  'html' || 'xml' || 'svg' => 'xml',
  'css' => 'css',
  'c' || 'h' => 'c',
  'cpp' || 'c++' || 'cc' => 'cpp',
  'go' => 'go',
  'java' => 'java',
  'dart' => 'dart',
  'sql' => 'sql',
  'md' || 'markdown' => 'markdown',
  _ => null,
};

/// [code] as coloured spans (plain when the language is unknown or highlighting fails).
TextSpan highlightCode(String code, String fence, TextStyle style, SyntaxColors y) {
  final lang = _lang(fence);
  if (lang == null) return TextSpan(text: code, style: style);
  try {
    final r = _hl.highlight(code: code, language: lang);
    final renderer = TextSpanRenderer(style, syntaxTheme(y));
    r.render(renderer);
    return renderer.span ?? TextSpan(text: code, style: style);
  } on Object {
    return TextSpan(text: code, style: style);
  }
}
