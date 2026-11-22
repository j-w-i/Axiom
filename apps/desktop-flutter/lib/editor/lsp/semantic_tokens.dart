// LSP semantic tokens (M5.5-EDIT-002, ADR-0021 §2): the server's encoded tokens decoded into
// per-line ranges, each with the syntax colour its type maps to, drawn over the grammar's colours.

import 'package:flutter/painting.dart';

import '../../ui/tokens.dart';

/// One coloured range on a line.
typedef SemanticRange = ({int start, int end, TextStyle style});

/// Token types to the syntax palette's roles; types not listed keep the grammar's colour.
TextStyle? semanticStyle(String type, List<String> modifiers, SyntaxColors y) {
  final colour = switch (type) {
    'namespace' ||
    'type' ||
    'class' ||
    'enum' ||
    'interface' ||
    'struct' ||
    'typeParameter' ||
    'builtinType' ||
    'selfType' => y.type,
    'function' || 'method' || 'macro' || 'decorator' => y.macro,
    'keyword' || 'modifier' || 'selfKeyword' => y.keyword,
    'comment' => y.comment,
    'string' || 'regexp' => y.string,
    'number' || 'enumMember' || 'boolean' => y.number,
    'operator' || 'punctuation' => y.punct,
    'parameter' || 'variable' || 'property' => y.text,
    _ => null,
  };
  if (colour == null) return null;
  return TextStyle(
    color: colour,
    fontStyle: type == 'parameter' || modifiers.contains('defaultLibrary')
        ? FontStyle.italic
        : null,
    decoration: modifiers.contains('deprecated') ? TextDecoration.lineThrough : null,
  );
}

/// Decodes `SemanticTokens.data` (five integers per token: delta line, delta start, length, type,
/// modifier bits) with the server's legend into ranges by line.
Map<int, List<SemanticRange>> decodeSemanticTokens(
  List<int> data,
  List<String> types,
  List<String> modifiers,
  SyntaxColors y,
) {
  final out = <int, List<SemanticRange>>{};
  var line = 0, start = 0;
  for (var i = 0; i + 4 < data.length; i += 5) {
    final dLine = data[i], dStart = data[i + 1], length = data[i + 2];
    final type = data[i + 3], bits = data[i + 4];
    line += dLine;
    start = dLine == 0 ? start + dStart : dStart;
    if (type < 0 || type >= types.length || length <= 0) continue;
    final mods = [
      for (var b = 0; b < modifiers.length && b < 31; b++)
        if (bits & (1 << b) != 0) modifiers[b],
    ];
    final style = semanticStyle(types[type], mods, y);
    if (style == null) continue;
    (out[line] ??= []).add((start: start, end: start + length, style: style));
  }
  return out;
}
