// Server decorations follow inserted and removed lines (audit finding 16).

import 'package:axiom_desktop/state/documents.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

void main() {
  test('semantic tokens below an inserted line move down with their line', () {
    final d = Doc(path: 'a.rs', text: 'a\nb\nc\nd\n', savedVersion: 'v');
    const style = TextStyle(color: Color(0xFF000001));
    d.setSemantic({
      0: const [(start: 0, end: 1, style: style)],
      3: const [(start: 0, end: 1, style: style)],
    });
    d.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 1);
    d.controller.replaceSelection('\nx');
    expect(d.semantic.keys.toList()..sort(), [0, 4]);
    // Two lines joined: the token below moves up.
    d.controller.selection = const CodeLineSelection(
      baseIndex: 1,
      baseOffset: 1,
      extentIndex: 2,
      extentOffset: 1,
    );
    d.controller.replaceSelection('');
    expect(d.semantic.keys.toList()..sort(), [0, 3]);
  });
}
