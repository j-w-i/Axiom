// Semantic tokens (M5.5-EDIT-002, ADR-0021 §2).

import 'package:axiom_desktop/editor/lsp/semantic_tokens.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final y = AxiomColors.dark.syntax;
  const types = ['keyword', 'function', 'variable', 'unknownKind'];
  const mods = ['declaration', 'deprecated', 'defaultLibrary'];

  test('relative positions decode to lines and columns', () {
    // `fn main` on line 0 (keyword at 0, function at 3); `old()` on line 2 at 4, deprecated.
    final r = decodeSemanticTokens([0, 0, 2, 0, 0, 0, 3, 4, 1, 1, 2, 4, 3, 1, 2], types, mods, y);
    expect(r.keys, [0, 2]);
    expect(r[0]!.map((t) => (t.start, t.end)), [(0, 2), (3, 7)]);
    expect(r[0]![0].style.color, y.keyword);
    expect(r[0]![1].style.color, y.macro);
    expect(r[2]!.single.start, 4);
    expect(r[2]!.single.style.decoration, TextDecoration.lineThrough);
  });

  test('unknown types and bad entries keep the grammar colour', () {
    final r = decodeSemanticTokens([0, 0, 3, 3, 0, 0, 4, 0, 9, 0], types, mods, y);
    expect(r, isEmpty);
  });

  test('defaultLibrary symbols are italic', () {
    final s = semanticStyle('function', ['defaultLibrary'], y)!;
    expect(s.fontStyle, FontStyle.italic);
  });
}
