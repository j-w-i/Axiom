import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/transcript_parts.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/surface.dart';
import 'package:axiom/ui/tokens.dart';
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

  testWidgets('the same list, long or short, costs a frame the same', (t) async {
    Future<int> cost(int blocks) async {
      final f = Feed()..open('i');
      f.text('i', 'A paragraph of ordinary length.\n\n' * blocks);
      await mount(t, TranscriptView(f.agent), dpr: 1);
      final sw = Stopwatch();
      for (var i = 0; i < 60; i++) {
        // Every second frame closes a block, so the list itself changes too.
        f.text('i', i.isEven ? 'word ' : 'end.\n\n');
        sw.start();
        await t.pump();
        sw.stop();
      }
      return sw.elapsedMicroseconds;
    }

    await cost(300);
    final short = await cost(300), long = await cost(50000);
    expect(long, lessThan(short * 2), reason: 'short $short us, long $long us');
  });

  testWidgets('what is being read stays put while text arrives', (t) async {
    final f = Feed()..open('i');
    f.text('i', [for (var i = 0; i < 300; i++) 'Paragraph $i.'].join('\n\n'));
    final c = ScrollController();
    await mount(t, TranscriptView(f.agent, controller: c), dpr: 1);
    c.jumpTo(3000);
    await t.pump();
    final seen = find.textContaining('Paragraph 2', findRichText: true).evaluate().first;
    final text = (seen.widget as RichText).text.toPlainText();
    Offset where() => t.getTopLeft(find.text(text, findRichText: true));
    final y = where().dy;
    f.text('i', ' Tail grows.\n\nAnd a new one.\n\nAnd another');
    await t.pump();
    // The viewport is anchored to the end: what is read moves up by exactly what was added.
    final moved = y - where().dy;
    expect(moved, greaterThanOrEqualTo(0));
    expect(c.offset, 3000);
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
