import 'dart:math';

import 'package:axiom/transcript/inline.dart';
import 'package:axiom/transcript/markdown.dart';
import 'package:axiom/transcript/safe_text.dart';
import 'package:flutter_test/flutter_test.dart';

List<Block> parse(String s, [List<int>? cuts]) {
  final d = MarkdownDoc();
  var at = 0, k = 0;
  while (at < s.length) {
    final n = cuts == null ? s.length : min(cuts[k++ % cuts.length], s.length - at);
    d.append(s.substring(at, at + n));
    at += n;
    d.tail; // looking must not change anything
  }
  d.finish();
  return d.closed;
}

List<String> show(String s) => parse(s).map((b) => '$b').toList();
String html(String s) => inlineRuns(s).map((r) {
  var t = r.text;
  if (r.code) t = '`$t`';
  if (r.em) t = '/$t/';
  if (r.strong) t = '!$t!';
  if (r.del) t = '~$t~';
  if (r.url != null) t = '<${r.url}|$t>';
  return t;
}).join();

void main() {
  group('blocks', () {
    void t(String name, String source, List<String> want) =>
        test(name, () => expect(show(source), want));

    t('ordinary', '# One\n\nText *a*\nsecond\n\nend', [
      'heading1(One)',
      r'paragraph(Text *a*\nsecond)',
      'paragraph(end)',
    ]);
    t('headings need a space and lose closing marks', '#no\n## Yes ##\n### C#', [
      'paragraph(#no)',
      'heading2(Yes)',
      'heading3(C#)',
    ]);
    t('setext', 'Title\n===\n\nSub\n---\n\n---', ['heading1(Title)', 'heading2(Sub)', 'rule()']);
    t('a list', '- a\n- b\n  more\n* c', ['item>•(a)', r'item>•(b\nmore)', 'item>•(c)']);
    t('nesting by two, four or a tab is one level each', '- a\n  - b\n    - c\n- d', [
      'item>•(a)',
      'item>>•(b)',
      'item>>>•(c)',
      'item>•(d)',
    ]);
    t('four spaces', '- a\n    - b\n- c', ['item>•(a)', 'item>>•(b)', 'item>•(c)']);
    t('tabs', '- a\n\t- b', ['item>•(a)', 'item>>•(b)']);
    t('numbers', '1. a\n   - b\n2) c\n10. d', [
      'item>1.(a)',
      'item>>•(b)',
      'item>2)(c)',
      'item>10.(d)',
    ]);
    t('a paragraph inside an item, and after the list', '- a\n\n  b\n\nc', [
      'item>•(a)',
      'paragraph>(b)',
      'paragraph(c)',
    ]);
    t('back to the outer item', '1. a\n   - b\n\n   c', [
      'item>1.(a)',
      'item>>•(b)',
      'paragraph>(c)',
    ]);
    t('tasks', '- [ ] a\n- [x] b\n- [link](u)', [
      'item>todo(a)',
      'item>done(b)',
      'item>•([link](u))',
    ]);
    t('code in an item, at its indent', '1. run\n   ```sh\n   npm i\n     x\n   ```\n   then', [
      'item>1.(run)',
      r'code>:sh(npm i\n  x)',
      'paragraph>(then)',
    ]);
    t('code deep in a list', '- a\n  - b\n\n      ```sh\n      go\n      ```', [
      'item>•(a)',
      'item>>•(b)',
      'code>>:sh(go)',
    ]);
    t('code under a wide number', '10. a\n\n    ```js\n    x()\n    ```', [
      'item>10.(a)',
      'code>:js(x())',
    ]);
    t('a year does not start a list', 'It was\n1986. Great.', [r'paragraph(It was\n1986. Great.)']);
    t('a list may follow a paragraph directly', 'Do:\n- a\n- b', [
      'paragraph(Do:)',
      'item>•(a)',
      'item>•(b)',
    ]);
    t('a fence is closed by its own kind and length', '````\n```\n~~~\n````\nx', [
      r'code(```\n~~~)',
      'paragraph(x)',
    ]);
    t('an indented fence inside code does not close it', '```md\n    ```\nx\n```', [
      r'code:md(    ```\nx)',
    ]);
    t('an unfinished fence is code', '```sh\nls', ['code:sh(ls)']);
    t('an empty fence is an empty block', '```\n```', ['code()']);
    t('indented code', 'a\n\n    x\n\n      y\n\nb', [
      'paragraph(a)',
      r'code(x\n\n  y)',
      'paragraph(b)',
    ]);
    t('a table after text without a blank line', 'See:\n| a | b |\n|---|---|\n| 1 | 2 |\n\nend', [
      'paragraph(See:)',
      r'table(| a | b |\n|---|---|\n| 1 | 2 |)',
      'paragraph(end)',
    ]);
    t('a table without outer pipes', 'a | b\n--|--\n1 | 2', [r'table(a | b\n--|--\n1 | 2)']);
    t('a bar alone is text', '| not a table', ['paragraph(| not a table)']);
    t('quotes', '> a\n> b\nlazy\n\nc', [r'quote(a\nb\nlazy)', 'paragraph(c)']);
    t('a list under a number by two spaces', '1. a\n  - b\n2. c', [
      'item>1.(a)',
      'item>>•(b)',
      'item>2.(c)',
    ]);
    t('what follows a table with a bar in it', '|a|b|\n|-|-|\n|1|2|\n- see|x\n> q|r', [
      r'table(|a|b|\n|-|-|\n|1|2|)',
      'item>•(see|x)',
      'quote(q|r)',
    ]);
    t('rules', 'a\n\n***\n* * *\n_ _ _', ['paragraph(a)', 'rule()', 'rule()', 'rule()']);
  });

  test('carriage returns are not text, wherever the pieces are cut', () {
    for (final cuts in [
      [1],
      [2],
      [3],
      [99],
    ]) {
      expect(parse('a\r\nb\r\n\r\nc\r', cuts).map((b) => b.text.trimRight()), ['a\nb', 'c']);
    }
  });

  const parts = [
    '# h\n',
    'word ',
    '\n',
    '\n\n',
    '- i\n',
    '  - deep\n',
    '    - deeper\n',
    '1) n\n',
    '12. n\n',
    '> q\n',
    '```\n',
    '~~~x\n',
    '   ```\n',
    '| t | u |\n',
    '|--|--|\n',
    '---\n',
    '===\n',
    '`c` **b** ',
    '\r\n',
    '\t',
    '🙂é',
    '    code\n',
    '###',
    '*',
    '``',
    '[ ] ',
    '[l](u) ',
    '\\',
    '  ',
  ];
  String random(Random r) =>
      [for (var i = 0; i < r.nextInt(50); i++) parts[r.nextInt(parts.length)]].join();

  test('the cuts do not matter', () {
    final r = Random(7);
    for (var n = 0; n < 1500; n++) {
      final s = random(r);
      final whole = parse(s);
      for (var c = 0; c < 3; c++) {
        final cuts = [for (var i = 0; i < 9; i++) 1 + r.nextInt(7)];
        expect(parse(s, cuts), whole, reason: '${s.replaceAll('\n', r'\n')} cut by $cuts');
      }
    }
  });

  test('what is shown while writing is what stopping there would give', () {
    final r = Random(11);
    for (var n = 0; n < 400; n++) {
      final s = random(r);
      final d = MarkdownDoc();
      for (var i = 0; i < s.length; i++) {
        d.append(s[i]);
        final line = s.substring(0, i + 1).split('\n').last;
        // An undecided line is left out, and so it is from the reference.
        if (MarkdownDoc.undecided(line.replaceAll('\r', ''))) continue;
        if (s.codeUnitAt(i) & 0xFC00 == 0xD800 || s[i] == '\r') continue;
        final inFence = d.tail.isNotEmpty && d.tail.last.kind == BlockKind.code;
        if (inFence) continue; // a closing fence being written is held back; covered below
        final now = [...d.closed, ...d.tail];
        expect(now, parse(s.substring(0, i + 1)), reason: s.substring(0, i + 1));
      }
    }
  });

  test('marks being written do not flicker', () {
    final d = MarkdownDoc()..append('Text');
    final seen = <String>{};
    for (final c in '\n\n- it\n\n```py\nx\n```\n\n> q\n\n| a | b |\n|---|---|\n| 1 | 2 |\n'.split(
      '',
    )) {
      d.append(c);
      for (final b in d.tail) {
        seen.add(b.text);
      }
    }
    for (final bad in ['-', '`', '``', '```', '```p', '>', '|', '|---', 'x\n``', 'x\n```']) {
      expect(seen, isNot(contains(bad)));
    }
  });

  test('a paragraph without end is cut into bounded blocks, losing nothing', () {
    final words = [for (var i = 0; i < 9000; i++) 'word$i'].join(' ');
    final b = parse(words);
    expect(b.length, greaterThan(10));
    expect(b.every((x) => x.text.length <= MarkdownDoc.maxText), isTrue);
    expect(b.first.continues, isFalse);
    expect(b.skip(1).every((x) => x.continues), isTrue);
    expect(b.map((x) => x.text.trim()).join(' '), words);
    expect(parse(words, [13]), b);
  });

  test('cutting never splits a character', () {
    for (final b in parse('🙂' * 5000)) {
      expect(b.text.runes.every((c) => c == 0x1F642), isTrue);
    }
  });

  test('long code is cut by lines, and a line of code is never cut', () {
    final b = parse('```py\n${[for (var i = 0; i < 1000; i++) 'l$i'].join('\n')}\n```\n');
    expect(b.length, (1000 / MarkdownDoc.maxCodeLines).ceil());
    expect(b.expand((x) => x.text.split('\n')).length, 1000);
    final long = 'x' * 20000;
    expect(parse('```\n$long\n```', [777]).single.text, long);
  });

  test('a long table is cut between rows and keeps its head', () {
    final rows = [for (var i = 0; i < 2000; i++) '| $i | value $i |'];
    final b = parse('| a | b |\n|--|--|\n${rows.join('\n')}\n');
    expect(b.length, greaterThan(1));
    expect(b.every((x) => x.text.startsWith('| a | b |\n|--|--|\n')), isTrue);
    expect(b.expand((x) => x.text.split('\n').skip(2)), rows);
  });

  group('work is bounded', () {
    // Operations are not counted, so the limits are far from what is normal: each of these took
    // seconds before it was fixed.
    void quick(String name, void Function() f) => test(name, () {
      f();
      final sw = Stopwatch()..start();
      f();
      expect(sw.elapsedMilliseconds, lessThan(400));
    });
    quick('open links', () => inlineRuns('[a](' * 1000));
    quick('open brackets', () => inlineRuns('[' * 4000));
    quick('emphasis', () => inlineRuns('*a_b' * 1000));
    quick('mixed marks', () => inlineRuns('**a *b __c _d ~~e `f ' * 190));
    quick('ticks', () => inlineRuns([for (var i = 1; i < 80; i++) '`' * i].join(' ')));
    quick('entities', () => inlineRuns('&' * 4000));
    quick('angles', () => inlineRuns('<' * 4000));
    quick('urls', () => inlineRuns('https://a(' * 400));
    quick('a heading of spaces', () => parse('# a${' ' * 200000}x'));
    quick('a fence of spaces', () => parse('```${' ' * 200000}x'));
    quick('a line without end, in pieces', () {
      final d = MarkdownDoc();
      for (var i = 0; i < 60000; i++) {
        d.append('tok ');
      }
    });
    quick('deep nesting', () => parse([for (var i = 0; i < 2000; i++) '${' ' * i}- x'].join('\n')));
  });

  test('appending to a long document costs what appending to a short one does', () {
    int ops(int blocks) {
      final d = MarkdownDoc()..append('para graph\n\n' * blocks);
      var n = 0;
      for (var i = 0; i < 50; i++) {
        d.append('tok ');
        n += d.tail.fold(0, (k, b) => k + b.text.length);
      }
      return n;
    }

    expect(ops(100000), ops(100));
  });

  group('inline', () {
    void t(String s, String want) => test(s, () => expect(html(s), want));
    t('a **b** *c* ~~d~~ `e`', 'a !b! /c/ ~d~ `e`');
    t('***x***', '!/x/!');
    t('**bold *both* bold**', '!bold !!/both/!! bold!');
    t('snake_case_name and a__b__c', 'snake_case_name and a__b__c');
    t('2 ** 3 ** 4', '2 ** 3 ** 4');
    t(r'\*not\* \`code\`', '*not* `code`');
    t('a &lt; b &amp; c &#39;d&#39;', "a < b & c 'd'");
    t('one<br>two', 'one\ntwo');
    t('[words](https://a.b/c)', '<https://a.b/c|words>');
    t('[`code`](u)', '<u|`code`>');
    t('[Foo](https://e.org/Foo_(bar)) x', '<https://e.org/Foo_(bar)|Foo> x');
    t('![alt](https://e.org/i.png)', '<https://e.org/i.png|alt>');
    t('see https://a.b/c. And', 'see <https://a.b/c|https://a.b/c>. And');
    t('(https://a.b/c)', '(<https://a.b/c|https://a.b/c>)');
    t('<https://a.b>', '<https://a.b|https://a.b>');
    t('[not a link] (x)', '[not a link] (x)');
    t('`a ``b`` c`', '`a ``b`` c`');
    t('unclosed `tick and **star', 'unclosed `tick and **star');
    t('**bold\nacross**', '!bold across!');
    t('two  \nlines', 'two\nlines');
    t('a_*b*_c', 'a_/b/_c');
    t('[a](<u v>)', '<u v|a>');
    t('[open and https://a.b', '[open and <https://a.b|https://a.b>');
    t('[a [b](c) d](e)', '[a <c|b> d](e)');
    t('a &#xD800; b', 'a &#xD800; b');
    test('a break of two spaces survives to the end', () {
      expect(html(parse('one  \ntwo').single.text), 'one\ntwo');
    });
  });

  group('safe text', () {
    test('ordinary text is returned as it is', () {
      const s = 'plain ascii\twith\nlines';
      expect(identical(visible(s), s), isTrue);
      expect(visible('é 🙂 日本 ×'), 'é 🙂 日本 ×');
    });
    test('what hides or reorders is marked', () {
      for (final c in [
        0x1B,
        0x7,
        0x9B,
        0x202E,
        0x2066,
        0x2028,
        0x115F,
        0xE0041,
        0xFFFC,
        0x13430,
        0x180B,
      ]) {
        final s = 'a${String.fromCharCode(c)}b';
        expect(visible(s), 'a\u{FFFD}b', reason: c.toRadixString(16));
        expect(literal(s), 'a\\u{${c.toRadixString(16).toUpperCase()}}b');
      }
    });
    test('harmless marks go from prose and are spelled out where a person decides', () {
      for (final c in [0xAD, 0x200B, 0x200E, 0x200F, 0xFEFF, 0x61C]) {
        final s = 'a${String.fromCharCode(c)}b';
        expect(visible(s), 'ab', reason: c.toRadixString(16));
        expect(literal(s), 'a\\u{${c.toRadixString(16).toUpperCase()}}b');
      }
    });
    test('where a person decides, nothing unusual passes', () {
      for (final c in [0xFE0F, 0x1D173, 0x1BCA0, 0xFFFE, 0x1680, 0x2800, 0x3164, 0xFFFD, 0xE000]) {
        expect(
          literal('a${String.fromCharCode(c)}b'),
          contains(r'\u{'),
          reason: c.toRadixString(16),
        );
      }
      expect(literal('rm -rf ./é 日本 🙂 "x" | sh'), 'rm -rf ./é 日本 🙂 "x" | sh');
    });
    test('joiners stay where they belong and nowhere else', () {
      final family = String.fromCharCodes([0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467]);
      expect(visible(family), family);
      final persian = String.fromCharCodes([0x645, 0x6CC, 0x200C, 0x62E, 0x648]);
      expect(visible(persian), persian);
      final malayalam = String.fromCharCodes([0xD28, 0xD4D, 0x200D, 0x20, 0x61]);
      expect(visible(malayalam), malayalam);
      expect(visible('a${String.fromCharCode(0x200D)}b'), 'a\u{FFFD}b');
      expect(literal(family), contains(r'\u{200D}'));
    });
    test('halves of characters are not passed on', () {
      final half = String.fromCharCode(0xD83D);
      expect(visible('a$half b'), 'a\u{FFFD} b');
      expect(cutTo('ab🙂', 3), 'ab');
      expect(cutFrom('🙂ab', 3), 'ab');
    });
    test('look-alike spaces are spelled out where a person decides', () {
      expect(literal('rm${String.fromCharCode(0xA0)}-rf'), r'rm\u{A0}-rf');
    });
    test('names are one bounded line', () {
      expect(oneLine('a\nb\t c'), 'a b c');
      expect(oneLine('x' * 999).length, 200);
    });
  });
}
