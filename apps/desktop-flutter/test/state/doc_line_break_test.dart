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

  // Review B6a: the breaks follow their lines when lines are added or removed.
  group('mixed line breaks when the number of lines changes (AUD-023)', () {
    // The editor joins with the file's first break (CRLF here).
    String disk(String file, String edited) =>
        Doc(path: 'm.txt', text: file, savedVersion: 'v').diskText(edited);

    test('a line added in the middle takes the break of the line above', () {
      expect(
        disk('one\r\ntwo\nthree\r\n', 'one\r\ntwo\r\nnew\r\nthree\r\n'),
        'one\r\ntwo\nnew\nthree\r\n',
      );
    });
    test('a line added at the end', () {
      expect(
        disk('one\r\ntwo\nthree\r\n', 'one\r\ntwo\r\nthree\r\nfour\r\n'),
        'one\r\ntwo\nthree\r\nfour\r\n',
      );
      // No final break: the old last line gets the break of the line above it.
      expect(disk('one\r\ntwo\nthree', 'one\r\ntwo\r\nthree\r\nfour'), 'one\r\ntwo\nthree\nfour');
    });
    test('a line added at the start', () {
      expect(disk('one\ntwo\r\nthree\n', 'zero\none\ntwo\nthree\n'), 'zero\none\ntwo\r\nthree\n');
    });
    test('a line removed keeps the others\' breaks', () {
      expect(
        disk('one\r\ntwo\nthree\r\nfour\n', 'one\r\nthree\r\nfour\r\n'),
        'one\r\nthree\r\nfour\n',
      );
      expect(
        disk('one\r\ntwo\nthree\r\nfour\n', 'one\r\ntwo\r\nthree\r\n'),
        'one\r\ntwo\nthree\r\n',
      );
    });
    test('the whole text replaced: each place keeps its break, lines added take the one above', () {
      expect(disk('one\r\ntwo\n', 'x\r\ny\r\nz\r\nw'), 'x\r\ny\nz\nw');
    });
    test('a line changed and one added: the changed one keeps its own break', () {
      expect(
        disk('one\r\ntwo\nthree\r\n', 'one\r\nTWO\r\nnew\r\nthree\r\n'),
        'one\r\nTWO\nnew\nthree\r\n',
      );
    });
  });
}
