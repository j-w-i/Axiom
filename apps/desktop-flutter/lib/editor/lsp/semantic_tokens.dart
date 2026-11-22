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
    'function' || 'method' => y.function,
    'macro' || 'decorator' => y.macro,
    'keyword' || 'modifier' || 'selfKeyword' => y.keyword,
    'comment' => y.comment,
    'string' || 'regexp' => y.string,
    'number' || 'boolean' => y.number,
    'operator' || 'punctuation' => y.punct,
    // Enum cases (`.failure`) are members, as VS Code colours them.
    'parameter' || 'variable' || 'property' || 'enumMember' => y.variable,
    _ => null,
  };
  if (colour == null) return null;
  // Never italic: the owner (2026-11-22). A standard-library symbol or a parameter is told by its
  // colour, as the grammar's are.
  return TextStyle(
    color: colour,
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

/// [tokens] coloured from [to] instead of [from]: a theme switch recolours the server's tokens
/// at once rather than at the next edit (AUD-055). Each colour of [from]'s palette maps to the
/// same role's colour in [to]; the decoration is kept.
Map<int, List<SemanticRange>> rethemeSemantic(
  Map<int, List<SemanticRange>> tokens,
  SyntaxColors from,
  SyntaxColors to,
) {
  if (identical(from, to) || tokens.isEmpty) return tokens;
  final map = <Color, Color>{
    from.type: to.type,
    from.function: to.function,
    from.macro: to.macro,
    from.keyword: to.keyword,
    from.comment: to.comment,
    from.string: to.string,
    from.number: to.number,
    from.punct: to.punct,
    from.variable: to.variable,
  };
  return {
    for (final e in tokens.entries)
      e.key: [
        for (final r in e.value)
          (
            start: r.start,
            end: r.end,
            style: r.style.copyWith(color: map[r.style.color] ?? r.style.color),
          ),
      ],
  };
}
