import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/surface.dart';
import 'package:axiom/ui/tokens.dart';

import 'dart:io';

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
    String show() => rows.rows.map((r) => '${r.item.id}${r.index}${r.open ? '*' : ''}').join(' ');
    expect(show(), 'p0 p1* t0* q0 q1*');
    f
      ..text('p', '\n\nc\n\nd')
      ..text('q', '\n\nz');
    rows.sync();
    expect(show(), 'p0 p1 p2 p3* t0* q0 q1 q2*');
    f.end('p', 'a\n\nb\n\nc\n\nd');
    rows.sync();
    expect(show(), 'p0 p1 p2 p3 t0* q0 q1 q2*');
    // The end says less than was streamed.
    f.end('q', 'x');
    rows.sync();
    expect(show(), 'p0 p1 p2 p3 t0* q0');
    f.open('r', ItemKind.tool, 'x');
    rows.sync();
    expect(show(), 'p0 p1 p2 p3 t0* q0 r0*');
    expect([for (final r in rows.rows) r.ordinal], [for (var i = 0; i < rows.rows.length; i++) i]);
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
    final c = ScrollController();
    await mount(t, TranscriptView(f.agent, controller: c), dpr: 1);
    await t.pump();
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

  testWidgets('tool output opens on request', (t) async {
    final f = Feed()..open('t', ItemKind.tool, 'shell', {'command': 'make'});
    f.end('t', 'line one\nline two', {'summary': 'ok'});
    await mount(t, TranscriptView(f.agent), dpr: 1);
    expect(find.textContaining('line two', findRichText: true), findsNothing);
    await t.tap(find.text('make'));
    await t.pump();
    expect(find.textContaining('line two', findRichText: true), findsOneWidget);
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
    await mount(t, TranscriptView(f.agent), dpr: 1);
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
