// Snippet parsing and word completion (M5.5-EDIT-001, ADR-0021 §7).

import 'package:axiom_desktop/editor/snippets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('plain text has the final stop at its end', () {
    final s = parseSnippet('println!');
    expect(s.text, 'println!');
    expect(s.stops, [(index: 0, start: 8, end: 8)]);
  });

  test(r'$1, ${2:default} and $0 in Tab order', () {
    final s = parseSnippet(r'for ${1:i} in ${2:items} {$0}');
    expect(s.text, 'for i in items {}');
    expect(s.stops.map((t) => t.index), [1, 2, 0]);
    expect(s.text.substring(s.stops[0].start, s.stops[0].end), 'i');
    expect(s.text.substring(s.stops[1].start, s.stops[1].end), 'items');
    expect(s.stops[2].start, 'for i in items {'.length);
  });

  test('nested placeholders, choices, variables and escapes', () {
    final s = parseSnippet(
      r'${1:new ${2:Map}()} ${3|let,const,var|} $TM_FILENAME ${UNKNOWN:x} \$5 \}',
    );
    expect(s.text, r'new Map() let  x $5 }');
    final one = s.stops.firstWhere((t) => t.index == 1);
    expect(s.text.substring(one.start, one.end), 'new Map()');
    final two = s.stops.firstWhere((t) => t.index == 2);
    expect(s.text.substring(two.start, two.end), 'Map');
    final three = s.stops.firstWhere((t) => t.index == 3);
    expect(s.text.substring(three.start, three.end), 'let');
  });

  test('a repeated stop is visited as one number with each range', () {
    final s = parseSnippet(r'${1:a} = ${1:a} + $2');
    expect(s.stops.where((t) => t.index == 1).length, 2);
    expect(s.stops.last.index, 0);
  });

  test('positions across lines', () {
    expect(positionIn('ab\ncd', 4, 10, 4), (line: 11, column: 1));
    expect(positionIn('abc', 2, 3, 7), (line: 3, column: 9));
  });

  test('document words: frequent first, the word being typed left out', () {
    final w = documentWords([
      'if total = compute(items);',
      'total += compute(more);',
    ], current: 'tot');
    expect(w.first, 'compute');
    expect(w, contains('total'));
    expect(w, isNot(contains('if')));
  });
}
