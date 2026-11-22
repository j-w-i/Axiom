// M5.5-QA-001: every file of the language fixtures (tests/fixtures/languages) is recognised as its
// language and highlighted by its grammar (or is plain text on purpose).

import 'dart:io';

import 'package:axiom_desktop/editor/languages.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_highlight/re_highlight.dart';

void main() {
  test('every fixture file has its language and highlights', () {
    final root = Directory('../../tests/fixtures/languages');
    expect(root.existsSync(), isTrue);
    final reg = Languages.instance;
    final hl = Highlight();
    final failures = <String>[];
    var files = 0;
    for (final f in root.listSync(recursive: true).whereType<File>()) {
      final rel = f.path.substring(root.path.length + 1);
      if (rel == 'README.md') continue;
      files++;
      final text = f.readAsStringSync();
      final lang = reg.detect(rel, firstLine: text.split('\n').first);
      if (lang.hljs == null) {
        failures.add('$rel: ${lang.id} has no grammar');
        continue;
      }
      hl.registerLanguage(lang.hljs!, reg.mode(lang)!);
      final r = hl.highlight(code: text, language: lang.hljs!);
      if (r.relevance <= 0 && text.trim().isNotEmpty && lang.id != 'json') {
        // A grammar that matched nothing at all in a real file is a wrong detection.
        failures.add('$rel: ${lang.id} matched nothing');
      }
    }
    expect(failures, isEmpty);
    expect(files, greaterThanOrEqualTo(25));
  });
}
