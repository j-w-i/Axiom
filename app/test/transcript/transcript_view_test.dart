import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/transcript/safe_text.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/surface.dart';
import 'package:axiom/ui/press.dart';
import 'package:axiom/ui/tokens.dart';

import 'dart:io';
import 'dart:math';

import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shot.dart';

const x = Execution(harness: 'h');

class Feed {
  final m = SessionModel('s');
  int seq = 0;
  AgentModel get agent => m.agents['a']!;
  Feed() {
    add([const SessionCreated('T', null), const AgentAdded('a', 'A', x, null)]);
  }
  void add(List<Body> b) => m.apply([for (final e in b) Event(++seq, seq, e)]);
  void open(String i, [ItemKind k = ItemKind.prose, String name = '', Object? args]) =>
      add([ItemStarted(i, 'r', 'a', k, name, args)]);
  void text(String i, String t) => add([ItemDelta(i, t)]);
  void end(String i, String t, [Object? result]) =>
      add([ItemEnded(i, Outcome.completed, t, result)]);
}

int built(WidgetTester t) => t.widgetList(find.byType(BlockView, skipOffstage: false)).length;

void main() {
  testWidgets('only what is visible is built, however long the session', (t) async {
    final f = Feed()..open('i');
    f.text('i', 'A paragraph of ordinary length.\n\n' * 50000);
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(f.agent.items.single.doc.closed.length, 50000);
    expect(built(t), lessThan(120));
  });

  testWidgets('a delta rebuilds only the block being written', (t) async {
    final f = Feed()..open('i');
    f.text('i', '${'Closed paragraph.\n\n' * 20}open');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final before = t.elementList(find.byType(BlockView)).map((e) => e.widget).toSet();
    f.text('i', ' more');
    await t.pump();
    final after = t.elementList(find.byType(BlockView)).map((e) => e.widget).toSet();
    expect(after.difference(before).length, 1);
    expect(find.text('open more', findRichText: true), findsOneWidget);
  });

  testWidgets('growing by a block looks at what grew, not at what is there', (t) async {
    final f = Feed()..open('i');
    f.text('i', 'A paragraph of ordinary length.\n\n' * 50000);
    final rows = TranscriptRows(f.agent);
    final first = rows.rows.first;
    f.text('i', 'one more.\n\nand');
    expect(rows.sync(), isTrue);
    expect(rows.rows.length, 50002);
    expect(identical(rows.rows.first, first), isTrue);
    for (var i = 0; i < rows.rows.length; i += 997) {
      expect(rows.rows[i].ordinal, i);
    }
    expect(rows.sync(), isFalse);
  });

  test('overlapping items grow in their own places', () {
    final f = Feed()
      ..open('p')
      ..text('p', 'a\n\nb')
      ..open('t', ItemKind.tool, 'read')
      ..open('q')
      ..text('q', 'x\n\ny');
    final rows = TranscriptRows(f.agent);
    String show() => rows.rows
        .map(
          (r) => r.isEnd
              ? '${r.item?.id ?? ''}${r.index}[${r.work.sublist(r.from, r.to).map((w) => w.id).join(' ')}]'
              : '${r.item!.id}${r.index}',
        )
        .join(' ');
    expect(show(), 'p0[t] q0 q1[]');
    f
      ..text('p', '\n\nc\n\nd')
      ..text('q', '\n\nz');
    expect(rows.sync(), isTrue);
    expect(show(), 'p0[t] q0 q1 q2[]');
    f.end('p', 'a\n\nb\n\nc\n\nd');
    rows.sync();
    expect(show(), 'p0[t] q0 q1 q2[]');
    // The end says less than was streamed.
    f.end('q', 'x');
    rows.sync();
    expect(show(), 'p0[t] q0');
    f.open('r', ItemKind.tool, 'x');
    rows.sync();
    expect(show(), 'p0[t] q0[r]');
    expect([for (final r in rows.rows) r.ordinal], [for (var i = 0; i < rows.rows.length; i++) i]);
  });

  test('long passages keep only their last blocks beside the work; much work is cut into rows', () {
    final f = Feed()..open('p');
    f.text('p', [for (var i = 0; i < 20; i++) 'P$i'].join('\n\n'));
    f.end('p', [for (var i = 0; i < 20; i++) 'P$i'].join('\n\n'));
    for (var i = 0; i < 70; i++) {
      f
        ..open('t$i', ItemKind.tool, 'read')
        ..end('t$i', '');
    }
    final rows = TranscriptRows(f.agent).rows;
    expect(rows.where((r) => !r.isEnd).length, 20 - TranscriptRows.beside);
    final ends = rows.where((r) => r.isEnd).toList();
    expect(ends.map((r) => r.to - r.from), [30, 30, 10]);
    expect(ends.first.index, 14);
  });

  test('however it grows, the rows hold every block and every piece of work once, in order', () {
    final r = Random(5);
    for (var seed = 0; seed < 200; seed++) {
      final f = Feed();
      final rows = TranscriptRows(f.agent);
      final open = <String>[];
      var n = 0;
      for (var step = 0; step < 40; step++) {
        switch (r.nextInt(5)) {
          case 0:
            final id = 'p${n++}';
            f.open(id);
            open.add(id);
          case 1:
            final id = 't${n++}';
            f.open(id, ItemKind.tool, 'x');
            open.add(id);
          case 2 || 3 when open.isNotEmpty:
            final id = open[r.nextInt(open.length)];
            f.text(id, id.startsWith('p') ? 'w${r.nextInt(9)}\n\n' * r.nextInt(5) : 'o');
          case 4 when open.isNotEmpty:
            final id = open.removeAt(r.nextInt(open.length));
            f.end(id, r.nextBool() ? 'short' : 'a\n\nb\n\nc');
          case _:
        }
        if (r.nextBool()) rows.sync();
      }
      rows.sync();
      final blocks = <String>[], work = <String>[];
      for (final (k, row) in rows.rows.indexed) {
        expect(row.ordinal, k);
        if (!row.isEnd) {
          blocks.add('${row.item!.id}:${row.index}');
        } else {
          if (row.from == 0 && row.item != null) {
            for (var b = row.index; b < row.item!.doc.closed.length; b++) {
              blocks.add('${row.item!.id}:$b');
            }
          }
          work.addAll(row.work.sublist(row.from, row.to).map((w) => w.id));
        }
      }
      expect(blocks, [
        for (final i in f.agent.items.where((i) => i.rich))
          for (var b = 0; b < i.doc.closed.length; b++) '${i.id}:$b',
      ]);
      expect(work, [for (final i in f.agent.items.where((i) => !i.rich)) i.id]);
    }
  });

  testWidgets('wide, the work stands beside the words it followed; narrow, under them', (t) async {
    final f = Feed()..open('p');
    f
      ..end('p', 'I will read the file.')
      ..open('t', ItemKind.tool, 'read', {'path': 'a.dart'})
      ..end('t', '', {'summary': '20 lines'})
      ..open('q')
      ..end('q', 'It is fine.');
    await mount(t, TranscriptView(f.agent), size: const Size(1200, 500), dpr: 1);
    Rect of(String x) => t.getRect(find.text(x, findRichText: true));
    expect(of('a.dart').left, greaterThan(of('I will read the file.').right));
    expect((of('a.dart').center.dy - of('I will read the file.').center.dy).abs(), lessThan(2));
    expect(of('It is fine.').top, greaterThan(of('a.dart').bottom));
    await mount(t, TranscriptView(f.agent), size: const Size(700, 500), dpr: 1);
    expect(of('a.dart').top, greaterThan(of('I will read the file.').bottom));
  });

  testWidgets('an end shorter than what was streamed is shown as it is', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 5; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), dpr: 1);
    f.end('i', 'Paragraph 0.\n\nOther.');
    await t.pump();
    expect(t.takeException(), isNull);
    expect(find.text('Other.', findRichText: true), findsOneWidget);
    expect(find.text('Paragraph 3.', findRichText: true), findsNothing);
  });

  testWidgets('what is being read does not move, by a single pixel, while text arrives', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 300; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), dpr: 1);
    await t.pump();
    final c = t.state<TranscriptViewState>(find.byType(TranscriptView)).scroll;
    c.jumpTo(c.offset - 900);
    await t.pump();
    final seen = t
        .widgetList<RichText>(find.byType(RichText))
        .map((r) => r.text.toPlainText())
        .firstWhere((x) => x.startsWith('Paragraph 2'));
    Offset where() => t.getTopLeft(find.text(seen, findRichText: true));
    final y = where();
    for (var k = 0; k < 10; k++) {
      f.text('i', ' grows.\n\nAnother $k');
      await t.pump();
      expect(where(), y);
    }
    expect(find.text('Latest'), findsOneWidget);
    await t.tap(find.text('Latest'));
    await t.pumpAndSettle();
    expect(find.text('Another 9', findRichText: true), findsOneWidget);
    expect(find.text('Latest'), findsNothing);
  });

  testWidgets('at the end it follows', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 300; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), dpr: 1);
    await t.pump();
    for (var k = 0; k < 40; k++) {
      f.text('i', '\n\nNew $k.');
      await t.pump();
    }
    expect(find.text('New 39.', findRichText: true), findsOneWidget);
    expect(t.getBottomLeft(find.text('New 39.', findRichText: true)).dy, lessThan(900));
  });

  testWidgets('short content starts at the top and stays there as it passes any number', (t) async {
    final f = Feed()..open('i');
    f.text('i', 'First.\n\n');
    await mount(t, TranscriptView(f.agent), size: const Size(900, 2400), dpr: 1);
    final y = t.getTopLeft(find.text('First.', findRichText: true));
    for (var k = 0; k < 60; k++) {
      f.text('i', 'P$k.\n\n');
      await t.pump();
      expect(t.getTopLeft(find.text('First.', findRichText: true)), y);
    }
  });

  for (final up in [true, false]) {
    testWidgets('a fling ${up ? 'up' : 'down'} goes as far while text arrives as without', (
      t,
    ) async {
      Future<List<int>> fling({required bool stream}) async {
        final f = Feed()..open('i');
        f.text('i', [for (var i = 0; i < 400; i++) 'Paragraph $i.'].join('\n\n'));
        await mount(
          t,
          TranscriptView(f.agent, key: UniqueKey()),
          size: const Size(800, 600),
          dpr: 1,
        );
        if (!up) {
          t.state<TranscriptViewState>(find.byType(TranscriptView)).toStart();
          await t.pump();
          await t.pump();
        }
        await t.fling(find.byType(CustomScrollView), Offset(0, up ? 300 : -300), 3000);
        final seen = <int>[];
        for (var k = 0; k < 40; k++) {
          await t.pump(const Duration(milliseconds: 16));
          if (stream && k % 4 == 0) f.text('i', '\n\nNew block $k');
          seen.add(
            t
                .widgetList<RichText>(find.byType(RichText))
                .map((r) => RegExp(r'^Paragraph (\d+)').firstMatch(r.text.toPlainText())?.group(1))
                .whereType<String>()
                .map(int.parse)
                .reduce((a, b) => a < b ? a : b),
          );
        }
        return seen;
      }

      final quiet = await fling(stream: false);
      expect(await fling(stream: true), quiet);
      expect(quiet.first, isNot(quiet.last));
    });
  }

  testWidgets('a hand on the view is not overruled at the end', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 200; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), size: const Size(800, 600), dpr: 1);
    await t.pump();
    final s = t.state<TranscriptViewState>(find.byType(TranscriptView)).scroll;
    final g = await t.startGesture(t.getCenter(find.byType(CustomScrollView)));
    await g.moveBy(const Offset(0, 24));
    await g.moveBy(const Offset(0, 1));
    await t.pump();
    final gap = s.position.maxScrollExtent - s.offset;
    expect(gap, greaterThan(0));
    for (var k = 0; k < 5; k++) {
      final at = s.offset;
      f.text('i', ' more words to make it longer and longer' * 9);
      await t.pump();
      expect(s.offset, at);
    }
    await g.up();
    await t.pumpAndSettle();
  });

  testWidgets('opening output leaves what was pressed where it was', (t) async {
    final f = Feed();
    for (var k = 0; k < 30; k++) {
      f
        ..open('p$k')
        ..end('p$k', 'Passage $k.')
        ..open('t$k', ItemKind.tool, 'read', {'path': 'file$k'})
        ..end('t$k', 'out\n' * 300, {'summary': 's'});
    }
    await mount(t, TranscriptView(f.agent), size: const Size(800, 600), dpr: 1);
    await t.pump();
    for (final k in [29, 27]) {
      final y = t.getTopLeft(find.text('file$k')).dy;
      await t.tap(find.text('file$k'));
      await t.pump();
      await t.pump();
      expect(t.getTopLeft(find.text('file$k')).dy, closeTo(y, .01));
      expect(find.textContaining('out\nout', findRichText: true), findsWidgets);
      await t.tap(find.text('file$k'));
      await t.pump();
      await t.pump();
      expect(find.textContaining('out\nout', findRichText: true), findsNothing);
    }
    // Back at the end, it is followed again.
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    t.state<TranscriptViewState>(find.byType(TranscriptView)).toEnd();
    await t.pumpAndSettle();
    expect(find.text('Latest'), findsNothing);
  });

  testWidgets('output opened for one agent is not open for another', (t) async {
    final f = Feed()..open('t', ItemKind.tool, 'shell', {'command': 'make'});
    f.end('t', 'line one', {'summary': 'ok'});
    Future<void> show(AgentModel a) => mount(t, TranscriptView(a), dpr: 1);
    await show(f.agent);
    await t.tap(find.text('make'));
    await t.pump();
    final g = Feed()..open('u', ItemKind.tool, 'shell', {'command': 'test'});
    g.end('u', 'x', {'summary': 'ok'});
    await show(g.agent);
    await show(f.agent);
    expect(find.textContaining('line one', findRichText: true), findsNothing);
  });

  testWidgets('beside work that goes on, what is written does not build again', (t) async {
    final f = Feed()..open('p');
    f
      ..text('p', [for (var i = 0; i < 5; i++) '```js\nlet a$i;\n```'].join('\n\n'))
      ..text('p', '\n\nand now')
      ..open('t', ItemKind.tool, 'shell', {'command': 'make'});
    await mount(t, TranscriptView(f.agent), dpr: 1);
    Set<Widget> blocks() => t.elementList(find.byType(BlockView)).map((e) => e.widget).toSet();
    final before = blocks();
    expect(before.length, 6);
    f.text('t', 'out');
    await t.pump();
    expect(blocks().difference(before), isEmpty);
    f.text('p', ' more');
    await t.pump();
    expect(blocks().difference(before).length, 1);
  });

  testWidgets('a request has the same shape while it is written and after', (t) async {
    final f = Feed()..open('q', ItemKind.prompt);
    f.text('q', 'First.\n\n');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    f.text('q', 'Second.\n\n');
    await t.pump();
    f.text('q', 'Third.');
    await t.pump();
    double gap(String a, String b) =>
        t.getTopLeft(find.text(b, findRichText: true)).dy -
        t.getBottomLeft(find.text(a, findRichText: true)).dy;
    // Its tile is round where it begins and ends, and nowhere else.
    List<(bool, bool)> tiles() => [
      for (final b in t.widgetList<DecoratedBox>(
        find.descendant(of: find.byType(BlockView), matching: find.byType(DecoratedBox)),
      ))
        if ((b.decoration as BoxDecoration).borderRadius case final BorderRadius r?)
          (r.topLeft.x > 0, r.bottomLeft.x > 0),
    ];
    const want = [(true, false), (false, false), (false, true)];
    expect(tiles(), want);
    final open = [gap('First.', 'Second.'), gap('Second.', 'Third.')];
    expect(open.first, closeTo(open.last, .01));
    f.end('q', 'First.\n\nSecond.\n\nThird.');
    await t.pump();
    expect([gap('First.', 'Second.'), gap('Second.', 'Third.')], open);
    expect(tiles(), want);
  });

  testWidgets('work stands on the first line of the words it stands beside', (t) async {
    for (final (words, first) in [
      ('A paragraph.', 'A paragraph.'),
      ([for (var i = 0; i < 9; i++) '- Item $i'].join('\n'), 'Item 3'),
      ('Before.\n\n${[for (var i = 0; i < 6; i++) '## Head $i'].join('\n\n')}', 'Head 0'),
    ]) {
      final f = Feed()..open('p');
      f
        ..end('p', words)
        ..open('t', ItemKind.tool, 'read', {'path': 'a.dart'})
        ..end('t', '');
      await mount(t, TranscriptView(f.agent, key: UniqueKey()), dpr: 1);
      final a = t.getCenter(find.text(first, findRichText: true)).dy;
      final b = t.getCenter(find.text('a.dart')).dy;
      expect((a - b).abs(), lessThan(1.2), reason: first);
    }
  });

  testWidgets('a number stands clear of the words after it', (t) async {
    final f = Feed()..open('p');
    f.end('p', '1. Step one\n2. Step two\n\n10. Ten');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    for (final (mark, words) in [('1.', 'Step one'), ('2.', 'Step two')]) {
      final m = t.getRect(find.text(mark)), w = t.getRect(find.text(words, findRichText: true));
      expect(w.left - m.right, inInclusiveRange(6, 10));
    }
  });

  testWidgets('line ends of another system are line ends', (t) async {
    final f = Feed()..open('t', ItemKind.tool, 'shell', {'command': 'dir'});
    f.end('t', 'one\r\ntwo\r\n', {'summary': 'ok'});
    await mount(t, TranscriptView(f.agent), dpr: 1);
    await t.tap(find.text('dir'));
    await t.pump();
    expect(find.textContaining('one\ntwo', findRichText: true), findsOneWidget);
    expect(find.textContaining(r'\u{D}', findRichText: true), findsNothing);
  });

  testWidgets('a command of any length costs what is shown of it', (t) async {
    final f = Feed()
      ..open('t', ItemKind.tool, 'shell', {'command': '${String.fromCharCode(0xE9)}x' * 500000});
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final line = t.widget<WorkLine>(find.byType(WorkLine));
    expect(line.detail.length, lessThan(420));
    f.text('t', 'out');
    await t.pump();
    final labels = t.widgetList<Pressable>(find.byType(Pressable)).map((p) => p.label);
    expect(labels.every((l) => l.length < 500), isTrue);
  });

  testWidgets('both ends are a key away, however far', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 20000; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), dpr: 1);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.home);
    await t.pump();
    expect(find.text('Paragraph 0.', findRichText: true), findsOneWidget);
    expect(built(t), lessThan(140));
    await t.sendKeyEvent(LogicalKeyboardKey.pageDown);
    await t.pump();
    expect(find.text('Paragraph 0.', findRichText: true), findsNothing);
    await t.sendKeyEvent(LogicalKeyboardKey.end);
    await t.pump();
    await t.pump();
    expect(find.text('Paragraph 19999.', findRichText: true), findsOneWidget);
    expect(built(t), lessThan(220));
  });

  testWidgets('text can be selected and copied; code copies what was written', (t) async {
    String? board;
    t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (c) async {
      if (c.method == 'Clipboard.setData') board = (c.arguments as Map)['text'] as String;
      return null;
    });
    final f = Feed()..open('i');
    f.text('i', 'Some words here.\n\n```\nnpm i\n```\n');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final at = t.getRect(find.text('Some words here.', findRichText: true));
    final g = await t.startGesture(
      at.centerLeft + const Offset(1, 0),
      kind: PointerDeviceKind.mouse,
    );
    await g.moveTo(at.centerRight + const Offset(20, 0));
    await g.up();
    await t.pump();
    final mac = debugDefaultTargetPlatformOverride == null && Platform.isMacOS;
    await t.sendKeyDownEvent(mac ? LogicalKeyboardKey.metaLeft : LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyC);
    await t.sendKeyUpEvent(mac ? LogicalKeyboardKey.metaLeft : LogicalKeyboardKey.controlLeft);
    await t.pump();
    await t.tap(find.text('Copy'));
    await t.pump();
    expect(board, 'npm i');
  });

  testWidgets('an earlier item that grows does not move what is read below it', (t) async {
    final f = Feed()
      ..open('p')
      ..text('p', 'Early.\n\n')
      ..open('q')
      ..text('q', [for (var i = 0; i < 120; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final c = t.state<TranscriptViewState>(find.byType(TranscriptView)).scroll;
    c.jumpTo(c.offset - 600);
    await t.pump();
    final seen = t
        .widgetList<RichText>(find.byType(RichText))
        .map((r) => r.text.toPlainText())
        .firstWhere((x) => x.startsWith('Paragraph '));
    final y = t.getTopLeft(find.text(seen, findRichText: true));
    for (var k = 0; k < 5; k++) {
      f.text('p', 'Grown $k.\n\n');
      await t.pump();
      expect(t.getTopLeft(find.text(seen, findRichText: true)), y);
    }
  });

  testWidgets('a change of width keeps the place', (t) async {
    final f = Feed()..open('i');
    f.text(
      'i',
      [for (var i = 0; i < 300; i++) 'Paragraph $i ${'with some words ' * (i % 9)}'].join('\n\n'),
    );
    Future<void> at(double w) async {
      await mount(t, TranscriptView(f.agent), size: Size(w, 700), dpr: 1);
      await t.pump();
    }

    await at(900);
    final st = t.state<TranscriptViewState>(find.byType(TranscriptView));
    st.scroll.jumpTo(st.scroll.offset - 1500);
    await t.pump();
    String first() => t
        .widgetList<RichText>(find.byType(RichText))
        .map((r) => r.text.toPlainText())
        .firstWhere((x) {
          if (!x.startsWith('Paragraph')) return false;
          final r = t.getRect(find.text(x, findRichText: true));
          return r.bottom > 0 && r.top < 700;
        });
    final before = first();
    for (final w in [520.0, 700.0, 420.0, 900.0]) {
      await at(w);
      expect(first(), before, reason: '$w');
    }
  });

  testWidgets('the first frame is already at the end', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 400; i++) 'Paragraph $i.'].join('\n\n'));
    t.view.devicePixelRatio = 1;
    t.view.physicalSize = const Size(900, 700);
    addTearDown(t.view.reset);
    await t.pumpWidget(
      WidgetsApp(
        color: const Color(0xFF000000),
        pageRouteBuilder: <T>(RouteSettings s, WidgetBuilder b) =>
            PageRouteBuilder<T>(settings: s, pageBuilder: (c, _, _) => b(c)),
        home: Ax(theme: const AxTheme(), child: TranscriptView(f.agent)),
      ),
    );
    expect(find.text('Paragraph 399.', findRichText: true), findsOneWidget);
    expect(
      t.getBottomLeft(find.text('Paragraph 399.', findRichText: true)).dy,
      inInclusiveRange(600, 700),
    );
  });

  testWidgets('many short rows fill a tall window from its top', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 400; i++) '- $i'].join('\n'));
    await mount(t, TranscriptView(f.agent), size: const Size(900, 3600), dpr: 1);
    final st = t.state<TranscriptViewState>(find.byType(TranscriptView));
    expect(st.scroll.position.minScrollExtent, lessThan(0), reason: 'there are more than fit');
    final shown = [
      for (var i = 0; i < 400; i++)
        if (find.text('$i', findRichText: true).evaluate().isNotEmpty)
          t.getRect(find.text('$i', findRichText: true)),
    ].where((r) => r.bottom > 0 && r.top < 3600).toList();
    expect(shown.first.top, lessThan(40));
    expect(shown.last.bottom, greaterThan(3500));
  });

  testWidgets('Home is the very beginning', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 5000; i++) 'Paragraph $i.'].join('\n\n'));
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final st = t.state<TranscriptViewState>(find.byType(TranscriptView))..toStart();
    await t.pump();
    expect([st.scroll.offset, st.scroll.position.minScrollExtent], [0, 0]);
    await t.pump();
    expect(st.scroll.offset, 0);
  });

  testWidgets('an item long left behind still shows what it gains', (t) async {
    final f = Feed()
      ..open('p')
      ..text('p', 'Start');
    for (var i = 0; i < 70; i++) {
      f
        ..open('t$i', ItemKind.tool, 'read', {'path': 'f$i'})
        ..end('t$i', '');
    }
    await mount(t, TranscriptView(f.agent), size: const Size(900, 3000), dpr: 1);
    f.text('p', ' and end.\n\nMore');
    await t.pump();
    expect(find.text('Start and end.', findRichText: true), findsOneWidget);
    expect(find.text('More', findRichText: true), findsOneWidget);
  });

  testWidgets('what is copied reads as it stands', (t) async {
    String? board;
    t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (c) async {
      if (c.method == 'Clipboard.setData') board = (c.arguments as Map)['text'] as String;
      return null;
    });
    final f = Feed()..open('i');
    f
      ..text(
        'i',
        'First para.\n\nSecond para.\n\n- one\n- two\n\n```\nlet a;\n```\n\n|h|k|\n|-|-|\n|1|2|\n\n',
      )
      ..open('t', ItemKind.tool, 'read', {'path': 'a.dart'})
      ..end('t', '', {'summary': '3 lines'});
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final s = t.state<SelectableRegionState>(find.byType(SelectableRegion));
    s.selectAll(SelectionChangedCause.keyboard);
    await t.pump();
    Actions.invoke(
      t.element(find.text('First para.', findRichText: true)),
      CopySelectionTextIntent.copy,
    );
    await t.pump();
    expect(
      board!.trim(),
      'First para.\n\nSecond para.\n\n• one\n• two\nlet a;\n\nh\tk\n1\t2\n\n\nRead a.dart  3 lines',
    );
  });

  testWidgets('a right click offers to copy', (t) async {
    final f = Feed()..open('i');
    f.text('i', 'Some words here.\n\n');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    final at = t.getRect(find.text('Some words here.', findRichText: true));
    await t.tapAt(at.center, buttons: kSecondaryButton, kind: PointerDeviceKind.mouse);
    await t.pumpAndSettle();
    expect(find.text('Copy'), findsOneWidget);
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('copying code takes all its parts; the key is seen when focused', (t) async {
    String? board;
    t.binding.defaultBinaryMessenger.setMockMethodCallHandler(SystemChannels.platform, (c) async {
      if (c.method == 'Clipboard.setData') board = (c.arguments as Map)['text'] as String;
      return null;
    });
    final code = [for (var i = 0; i < 300; i++) 'line $i'].join('\n');
    final f = Feed()..open('i');
    f.text('i', '```py\n$code\n```\n');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    t.state<TranscriptViewState>(find.byType(TranscriptView)).toStart();
    await t.pump();
    final key = find.ancestor(of: find.text('Copy'), matching: find.byType(Opacity)).first;
    expect(t.widget<Opacity>(key).opacity, 0);
    for (var i = 0; i < 4 && t.widget<Opacity>(key).opacity == 0; i++) {
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      await t.pump();
    }
    expect(t.widget<Opacity>(key).opacity, 1);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    expect(board, code);
  });

  testWidgets('half characters never spoil the text around them', (t) async {
    final f = Feed()..open('i');
    f.text('i', 'a &#xD800; b ${'x' * 1999}🙂 end\n\n```\n${'y' * 1999}🙂 tail\n```\n');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(t.takeException(), isNull);
    expect(find.textContaining('end', findRichText: true), findsWidgets);
    expect(oneLine('${'a' * 799}🙂 title').runes.where((c) => c >= 0xD800 && c <= 0xDFFF), isEmpty);
  });

  testWidgets('a failed run says why', (t) async {
    final f = Feed()
      ..add([const RunStarted('r', 'a')])
      ..open('i');
    f
      ..end('i', 'Trying.')
      ..add([const RunEnded('r', Outcome.failed, '429: limit reached')]);
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(find.text('429: limit reached'), findsOneWidget);
  });

  testWidgets('whose request it is, and what others call their tools', (t) async {
    final f = Feed()
      ..open('p', ItemKind.prompt, 'Lead')
      ..end('p', 'Look at it.')
      ..open('q', ItemKind.prompt)
      ..end('q', 'And this.')
      ..open('t', ItemKind.tool, 'agent', {'do': 'tell', 'name': 'Looker', 'text': 'Look.'})
      ..end('t', 'ok', {'summary': 'Looker'})
      ..open('u', ItemKind.tool, 'Bash', {'description': 'builds', 'command': 'make all'})
      ..end('u', 'ok');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(find.text('From Lead'), findsOneWidget);
    expect(find.bySemanticsLabel('Request from Lead'), findsOneWidget);
    expect(find.bySemanticsLabel('Request'), findsOneWidget);
    expect(find.text('tell Looker: Look.'), findsOneWidget);
    expect(find.text('make all'), findsOneWidget);
    expect(ToolRow.iconOf('Bash'), AxIcon.terminal);
    expect(ToolRow.iconOf('WebFetch'), AxIcon.web);
  });

  testWidgets('tool output opens on request', (t) async {
    final f = Feed()..open('t', ItemKind.tool, 'shell', {'command': 'make'});
    f.end('t', 'line one\nline two', {'summary': 'ok'});
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(find.textContaining('line two', findRichText: true), findsNothing);
    await t.tap(find.text('make'));
    await t.pump();
    expect(find.textContaining('line two', findRichText: true), findsOneWidget);
  });

  testWidgets('what the host could not keep is said, and is not made up', (t) async {
    final f = Feed()
      ..add([
        const ItemStarted('t', 'r', 'a', ItemKind.tool, 'write', null, argsCut: 300000),
        const ItemEnded('t', Outcome.completed, '', null, resultCut: 4200000),
        // A tool may really have these.
        const ItemStarted('u', 'r', 'a', ItemKind.tool, 'cut', {'truncated': 7}),
      ]);
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(find.text('300 kB of arguments, not kept'), findsOneWidget);
    expect(find.text('4.2 MB, not kept'), findsOneWidget);
    expect(find.text('7'), findsOneWidget);
  });

  testWidgets('long results and names do not overflow', (t) async {
    final f = Feed()..open('t', ItemKind.tool, 'n' * 300, {'path': 'p' * 900});
    f.end('t', '', {'summary': 's' * 900});
    await mount(t, TranscriptView(f.agent), size: const Size(420, 300), dpr: 1);
    expect(t.takeException(), isNull);
  });

  testWidgets('items that overlap are all shown, each in its place', (t) async {
    final f = Feed()
      ..open('p')
      ..text('p', 'First I read.\n\nThen ')
      ..open('t1', ItemKind.tool, 'read', {'path': 'a.dart'})
      ..open('t2', ItemKind.tool, 'search', {'pattern': 'foo'});
    await mount(t, TranscriptView(f.agent), size: const Size(700, 700), dpr: 1);
    f
      ..text('p', 'this continues.\n\nAnd closes')
      ..end('t2', '', {'summary': '3 hits'})
      ..end('t1', '', {'summary': '20 lines'})
      ..end('p', 'First I read.\n\nThen this continues.\n\nAnd closes');
    await t.pump();
    expect(find.text('First I read.', findRichText: true), findsOneWidget);
    expect(find.text('Then this continues.', findRichText: true), findsOneWidget);
    expect(find.text('And closes', findRichText: true), findsOneWidget);
    expect(find.byType(WorkLine), findsNWidgets(2));
    expect(find.text('20 lines'), findsOneWidget);
    final y = [
      for (final s in ['First I read.', 'And closes', 'a.dart', 'foo'])
        t.getTopLeft(find.text(s, findRichText: true).first).dy,
    ];
    expect(y, [...y]..sort());
  });

  testWidgets('control codes and direction marks never reach the screen', (t) async {
    final f = Feed()..open('i');
    f.text('i', 'a\u001b[31mred `co\u0007de` end\n\n```\nx\u009by\n```\n');
    await mount(t, TranscriptView(f.agent), dpr: 1);
    for (final r in t.widgetList<RichText>(find.byType(RichText))) {
      expect(
        RegExp('[\u0000-\u0008\u000b-\u001f\u007f-\u009f]').hasMatch(r.text.toPlainText()),
        isFalse,
      );
    }
  });

  testWidgets('links show their words, ask before opening, and nothing is fetched', (t) async {
    final f = Feed()..open('i');
    f.text(
      'i',
      'See [the docs](https://example.com/x?a=1) and ![img](https://evil.example/p.png)\n\n',
    );
    final asked = <String>[];
    await mount(t, TranscriptView(f.agent, onLink: asked.add), dpr: 1);
    expect(find.byType(Image), findsNothing);
    expect(asked, isEmpty);
    final rich = t.widget<RichText>(find.textContaining('the docs', findRichText: true));
    final spans = <TextSpan>[];
    rich.text.visitChildren((s) {
      if (s is TextSpan && s.recognizer != null) spans.add(s);
      return true;
    });
    expect(spans.map((s) => s.text), ['the docs', 'img']);
  });

  testWidgets('every kind of block draws, in both themes', (t) async {
    final f = Feed()..open('q', ItemKind.prompt);
    f
      ..text('q', 'Please fix the double rotation.')
      ..end('q', 'Please fix the double rotation.')
      ..open('i');
    const md =
        '# Findings\n\nThe check happens **before** the lock, see `refresh()` and *why*.\n\n'
        '- one\n- two with `code`\n  - deeper\n1. first\n\n> quoted words\n\n'
        '```dart\nfinal x = await lock.acquire("a"); // now\nreturn x ?? 12;\n```\n\n'
        '| File | Lines |\n|---|---|\n| refresh.ts | 220 |\n| store.ts | 18 |\n\n---\n\n'
        '## Next\n\nWriting the test';
    f.text('i', md);
    f.open('t', ItemKind.tool, 'read', {'path': 'src/auth/refresh.ts'});
    f.end('t', '', {'summary': '220 lines'});
    f.open('u', ItemKind.tool, 'shell', {'command': 'pnpm test auth'});
    for (final (n, c) in [('dark', AxColors.dark), ('light', AxColors.light)]) {
      await mount(
        t,
        Surface(c.bay, radius: 0, child: TranscriptView(f.agent)),
        size: const Size(760, 660),
        theme: AxTheme(colors: c),
      );
      expect(t.takeException(), isNull);
      await snap(t, 'transcript-$n');
    }
  });
}
