import 'dart:math';

import 'package:axiom/transcript/markdown.dart';
import 'package:flutter_test/flutter_test.dart';

List<Block> parse(String s, [List<int>? cuts]) {
  final d = MarkdownDoc();
  var at = 0, k = 0;
  while (at < s.length) {
    final n = cuts == null ? s.length : min(cuts[k++ % cuts.length], s.length - at);
    d.append(s.substring(at, at + n));
    at += n;
    d.tail; // looking must not change anything
    d.beforeTail;
  }
  d.finish();
  return d.closed;
}

String kinds(List<Block> b) => b.map((x) => x.kind.name[0] + (x.continues ? '+' : '')).join();

void main() {
  test('blocks', () {
    final b = parse(
      '# One\n\nText *a*\nsecond line\n\n- x\n- y\n  more\n\n  after blank\n1. n\n\n'
      '> q\n> r\n\n```dart\na\n\n# not a heading\n```\n---\n| a | b |\n|--|--|\n| 1 | 2 |\n\nend',
    );
    expect(b, const [
      Block(BlockKind.heading, 'One', level: 1),
      Block(BlockKind.paragraph, 'Text *a*\nsecond line'),
      Block(BlockKind.item, 'x', marker: '•'),
      Block(BlockKind.item, 'y\nmore\n\nafter blank', marker: '•'),
      Block(BlockKind.item, 'n', marker: '1.'),
      Block(BlockKind.quote, 'q\nr'),
      Block(BlockKind.code, 'a\n\n# not a heading', language: 'dart'),
      Block(BlockKind.rule, ''),
      Block(BlockKind.table, '| a | b |\n|--|--|\n| 1 | 2 |'),
      Block(BlockKind.paragraph, 'end'),
    ]);
  });

  test('a fence is closed by its own kind and length only', () {
    expect(parse('````\n```\n~~~\n````\nx'), const [
      Block(BlockKind.code, '```\n~~~'),
      Block(BlockKind.paragraph, 'x'),
    ]);
  });

  test('an unfinished fence is code', () {
    expect(parse('```sh\nls').single, const Block(BlockKind.code, 'ls', language: 'sh'));
  });

  test('carriage returns are not text, wherever the pieces are cut', () {
    for (final cuts in [
      [1],
      [2],
      [3],
      [99],
    ]) {
      expect(parse('a\r\nb\r\n\r\nc', cuts), const [
        Block(BlockKind.paragraph, 'a\nb'),
        Block(BlockKind.paragraph, 'c'),
      ]);
    }
  });

  test('the cuts do not matter', () {
    final r = Random(7);
    const parts = [
      '# h\n',
      'word ',
      '\n',
      '\n\n',
      '- i\n',
      '  - deep\n',
      '1) n\n',
      '> q\n',
      '```\n',
      '~~~x\n',
      '| t |\n',
      '---\n',
      '`c` **b** ',
      '\r\n',
      '\t',
      '🙂é',
      '    code\n',
      '###',
      '*',
      '``',
    ];
    for (var n = 0; n < 400; n++) {
      final s = [for (var i = 0; i < r.nextInt(60); i++) parts[r.nextInt(parts.length)]].join();
      final whole = parse(s);
      for (var c = 0; c < 4; c++) {
        final cuts = [for (var i = 0; i < 9; i++) 1 + r.nextInt(7)];
        expect(parse(s, cuts), whole, reason: '${s.replaceAll('\n', r'\n')} cut by $cuts');
      }
    }
  });

  test('a paragraph without end is cut into bounded blocks, losing nothing', () {
    final words = [for (var i = 0; i < 9000; i++) 'word$i'].join(' ');
    final b = parse(words);
    expect(b.length, greaterThan(10));
    expect(b.every((x) => x.text.length <= MarkdownDoc.maxText), isTrue);
    expect(b.first.continues, isFalse);
    expect(b.skip(1).every((x) => x.continues), isTrue);
    expect(b.map((x) => x.text).join(' '), words);
    expect(parse(words, [13]), b);
  });

  test('cutting never splits a character', () {
    final s = '🙂' * 5000;
    for (final b in parse(s)) {
      expect(b.text.runes.every((c) => c == 0x1F642), isTrue);
    }
  });

  test('long code is cut by lines', () {
    final b = parse('```py\n${[for (var i = 0; i < 1000; i++) 'l$i'].join('\n')}\n```\n');
    expect(b.length, (1000 / MarkdownDoc.maxCodeLines).ceil());
    expect(b.expand((x) => x.text.split('\n')).length, 1000);
    expect(b.every((x) => x.language == 'py'), isTrue);
  });

  test('the tail shows what is being written and is never part of the closed blocks', () {
    final d = MarkdownDoc()..append('one\n\ntw');
    expect(d.closed, const [Block(BlockKind.paragraph, 'one')]);
    expect(d.tail, const Block(BlockKind.paragraph, 'tw'));
    d.append('o\n## He');
    expect(d.tail, const Block(BlockKind.heading, 'He', level: 2));
    expect(d.beforeTail, const Block(BlockKind.paragraph, 'two'));
    d.append('ad\n');
    expect(d.closed.last, const Block(BlockKind.heading, 'Head', level: 2));
    expect(d.tail, isNull);
  });

  test('appending costs what arrives, not what is there', () {
    int time(int blocks) {
      final d = MarkdownDoc()..append('para graph\n\n' * blocks);
      final sw = Stopwatch()..start();
      for (var i = 0; i < 2000; i++) {
        d.append('tok ');
        d.tail;
      }
      return sw.elapsedMicroseconds;
    }

    time(10);
    final small = time(100), large = time(100000);
    expect(large, lessThan(small * 4 + 20000));
  });

  test('visible() removes what could mislead', () {
    expect(visible('a\u001b[31mb\u202Ec\u200Bd\te\nf\r'), 'a\uFFFD[31mb\uFFFDc\uFFFDd\te\nf');
    const plain = 'plain é 🙂 日本';
    expect(identical(visible(plain), plain), isTrue);
  });
}
