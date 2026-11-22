// Requirement E-3 (CRLF round-trip byte-exact) in the Flutter shell (audit finding 2).

import 'package:axiom_desktop/state/documents.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

void main() {
  test('the line break is the file\'s own', () {
    expect(lineBreakOf('a\r\nb\r\n'), TextLineBreak.crlf);
    expect(lineBreakOf('a\rb'), TextLineBreak.cr);
    expect(lineBreakOf('a\nb\r\n'), TextLineBreak.lf);
    expect(lineBreakOf('one line'), TextLineBreak.lf);
  });

  test('a CRLF file is the same bytes, and not dirty, until it is edited', () {
    const text = 'fn main() {\r\n    println!();\r\n}\r\n';
    final d = Doc(path: 'src/main.rs', text: text, savedVersion: 'v1');
    expect(d.text, text);
    // A caret move (the first notification) does not make it dirty.
    d.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 4);
    d.refreshDirty();
    expect(d.dirty, isFalse);
    // An edit keeps CRLF everywhere, the new line included.
    d.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 15);
    d.controller.replaceSelection('\n    x();');
    d.refreshDirty();
    expect(d.dirty, isTrue);
    expect(d.text, 'fn main() {\r\n    println!();\r\n    x();\r\n}\r\n');
    expect(d.secondary.text, d.text);
  });

  test('a file with mixed line breaks is not dirty until edited, and keeps them (AUD-023)', () {
    const text = 'one\r\ntwo\nthree\r\n';
    final d = Doc(path: 'mixed.txt', text: text, savedVersion: 'v1');
    d.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 1);
    d.refreshDirty();
    expect(d.dirty, isFalse);
    // Saved unchanged: the same bytes.
    expect(d.diskText(d.text), text);
    // A same-line edit keeps every line's own break.
    d.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 3);
    d.controller.replaceSelection('!');
    d.refreshDirty();
    expect(d.dirty, isTrue);
    expect(d.diskText(d.text), 'one\r\ntwo!\nthree\r\n');
    expect(mixedBreaks('a\nb\n'), isNull);
    expect(mixedBreaks('a\r\nb\rc\n'), ['\r\n', '\r', '\n']);
  });
}
