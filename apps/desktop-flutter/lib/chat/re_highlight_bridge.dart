// Code in Chat coloured like the editor: re_highlight with the editor's languages and palette.

import 'package:flutter/painting.dart';
import 'package:re_highlight/re_highlight.dart';

import '../editor/code_view.dart';
import '../editor/languages.dart';
import '../ui/tokens.dart';

final _hl = Highlight();
final _registered = <String>{};

/// The highlight.js name for a fence (`rs`, `py`, `c++`), its grammar registered on first use.
String? _lang(String fence) {
  final reg = Languages.instance;
  final l = reg.forFence(fence);
  final h = l?.hljs;
  if (h == null) return null;
  for (final name in [h, ...?_embedded[h]]) {
    if (_registered.add(name)) {
      final m = reg.modeByHljs(name);
      if (m != null) _hl.registerLanguage(name, m);
    }
  }
  return h;
}

const _embedded = {
  'xml': ['css', 'javascript'],
  'vue': ['xml', 'css', 'javascript', 'typescript'],
  'erb': ['ruby', 'xml'],
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
