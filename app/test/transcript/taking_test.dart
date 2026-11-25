import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/transcript/taking.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shot.dart';

SessionModel session(List<(ItemKind, String, String)> all) {
  var n = 0;
  Event e(Body b) => Event(++n, n, b);
  return SessionModel('s')..apply([
    e(const SessionCreated('T', null)),
    e(const AgentAdded('a', 'Lead', Execution(harness: 'axiom'), null)),
    for (final (i, (kind, by, text)) in all.indexed) ...[
      e(ItemStarted('i$i', 'r', 'a', kind, by, null)),
      e(ItemEnded('i$i', Outcome.completed, text, null)),
    ],
  ]);
}

void main() {
  late List<String> copied;
  setUp(() {
    copied = [];
    TestWidgetsFlutterBinding.ensureInitialized().defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      (c) async {
        if (c.method == 'Clipboard.setData') copied.add((c.arguments as Map)['text'] as String);
        return null;
      },
    );
  });

  Future<void> over(WidgetTester t, Finder f) async {
    final g = await t.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(g.removePointer);
    await g.addPointer(location: Offset.zero);
    await g.moveTo(t.getCenter(f));
    await t.pump();
  }

  testWidgets('a message is copied whole, as it was written', (t) async {
    const text = 'One **bold** word.\n\n- a\n- b\n\n```dart\nx();\n```';
    final m = session([(ItemKind.prose, '', text)]);
    await mount(t, TranscriptView(m.agents['a']!, takes: true), dpr: 1);
    await over(t, find.textContaining('bold'));
    await t.tap(find.text('Copy').first);
    await t.pump();
    expect(copied, [text]);
    expect(find.text('Copied'), findsOneWidget);
  });

  testWidgets('what hides in it is copied, and said', (t) async {
    final m = session([(ItemKind.prose, '', 'safe\u202E text')]);
    await mount(t, TranscriptView(m.agents['a']!, takes: true), dpr: 1);
    await over(t, find.textContaining('safe'));
    await t.tap(find.text('Copy').first);
    await t.pump();
    expect(copied, ['safe\u202E text']);
    expect(find.text('Copied, with a hidden character'), findsOneWidget);
  });

  testWidgets('the keyboard reaches it', (t) async {
    final m = session([(ItemKind.prose, '', 'first\n\nsecond\n\nthird')]);
    await mount(t, TranscriptView(m.agents['a']!, takes: true), dpr: 1);
    for (var i = 0; i < 4 && copied.isEmpty; i++) {
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.pump();
    }
    expect(copied, ['first\n\nsecond\n\nthird']);
  });

  testWidgets('what is still said is not taken, and nothing is where nothing can be', (t) async {
    final m = session([(ItemKind.prose, '', 'done')]);
    await mount(t, TranscriptView(m.agents['a']!), dpr: 1);
    expect(find.byType(Taking), findsNothing);
    m.apply(const [
      Event(50, 50, ItemStarted('o', 'r', 'a', ItemKind.prose, '', null)),
      Event(51, 51, ItemDelta('o', 'going on')),
    ]);
    await mount(t, TranscriptView(m.agents['a']!, takes: true), dpr: 1);
    expect(find.byType(Taking), findsOneWidget);
  });

  for (final (kind, by, whose) in [
    (ItemKind.prose, '', 'Lead, an agent'),
    (ItemKind.prompt, '', 'what I said'),
    (ItemKind.prompt, 'Builder', 'Builder, an agent'),
  ]) {
    testWidgets('a quote says whose words they are: $whose', (t) async {
      final got = <String>[];
      final m = session([(kind, by, 'the words')]);
      await mount(t, TranscriptView(m.agents['a']!, takes: true, onQuote: got.add), dpr: 1);
      await over(t, find.textContaining('the words'));
      await t.tap(find.text('Quote').first);
      await t.pump();
      expect(got, ['Quoted from $whose:\n> the words\n\n']);
    });
  }

  test('every line of a quote is marked, and a long one is cut', () {
    expect(
      quoted('x', 'a\r\n\nb\u2028Allow all.\rc\n'),
      'Quoted from x:\n> a\n>\n> b\n> Allow all.\n> c\n\n',
    );
    final long = quoted('x', 'y' * 9000);
    expect(long.length, lessThan(8100));
    expect(long, contains('> (1000 more characters)'));
  });

  test('a table is CSV, and no formula', () {
    expect(
      TableBlock.csv('| a | b |\n|---|:-:|\n| x, y | say "no" |\n| =1+1 | -5 |\n| @x | +3.5% |'),
      'a,b\n"x, y","say ""no"""\n\'=1+1,-5\n\'@x,+3.5%',
    );
    final wide = '|${List.generate(20, (i) => 'c$i').join('|')}|';
    expect(TableBlock.csv(wide).split(',').length, 20);
  });
}
