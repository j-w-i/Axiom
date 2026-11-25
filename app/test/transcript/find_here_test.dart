import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/transcript/find_here.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/commands.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import '../shot.dart';
import 'taking_test.dart' show session;

void main() {
  final commands = Commands();
  Future<void> show(WidgetTester t, SessionModel m) async {
    final a = m.agents['a']!;
    await mount(
      t,
      CommandScope(
        commands,
        child: FindHere(a, builder: (k) => TranscriptView(a, key: k)),
      ),
      dpr: 1,
    );
    commands.run(Command.findHere);
    await t.pump();
    await t.pump();
  }

  Future<void> look(WidgetTester t, String words) async {
    await t.enterText(find.byType(EditableText), words);
    await t.pump(const Duration(milliseconds: 150));
    // Who searches is another isolate: in real time.
    for (var i = 0; i < 100 && find.textContaining('Searching').evaluate().isNotEmpty; i++) {
      await t.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 20)));
      await t.pump();
    }
    await t.pump();
    await t.pump();
  }

  testWidgets('the first words of a long session are found and shown', (t) async {
    final m = session([
      (ItemKind.prompt, '', 'The needle is here.'),
      (ItemKind.prose, '', 'An ordinary paragraph.\n\n' * 50000),
    ]);
    await show(t, m);
    expect(find.textContaining('needle'), findsNothing, reason: 'it is far above');
    await look(t, 'NEEDLE');
    expect(find.text('1 of 1'), findsOneWidget);
    final at = t.getRect(find.textContaining('needle is'));
    expect(at.top, greaterThan(0));
    expect(at.bottom, lessThan(400));
  });

  testWidgets('Enter and Shift+Enter step through, around, and Escape leaves', (t) async {
    final m = session([
      for (var i = 0; i < 3; i++) (ItemKind.prose, '', 'word $i\n\n${'between\n\n' * 60}'),
    ]);
    await show(t, m);
    await look(t, 'word');
    for (final n in [2, 3, 1]) {
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await t.pump();
      await t.pump();
      expect(find.text('$n of 3'), findsOneWidget);
      expect(find.text('word ${n - 1}').hitTestable(), findsOneWidget);
    }
    await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await t.pump();
    expect(find.text('3 of 3'), findsOneWidget);
    await look(t, 'no such');
    expect(find.text('No results'), findsOneWidget);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pump();
    expect(find.byType(EditableText), findsNothing);
  });

  testWidgets('what a tool gave is found, and opened', (t) async {
    var n = 100;
    Event e(Body b) => Event(++n, n, b);
    final m = session([(ItemKind.prose, '', 'first')])
      ..apply([
        e(const ItemStarted('t', 'r', 'a', ItemKind.tool, 'shell', {'command': 'ls'})),
        e(const ItemEnded('t', Outcome.completed, 'only-in-the-output', null)),
      ]);
    await show(t, m);
    await look(t, 'only-in-the-output');
    expect(find.text('1 of 1'), findsOneWidget);
    expect(find.textContaining('only-in-the-output'), findsWidgets);
  });

  test('words are found as they are written and as they are read', () {
    const all = [
      ['Some **bold** words', 'a_name_of_code'],
      ['nothing'],
    ];
    expect(placesOf(all, 'bold words'), [(0, 0)]);
    expect(placesOf(all, '**bold'), [(0, 0)]);
    expect(placesOf(all, 'a_name'), [(0, 1)]);
    expect(placesOf(all, '  '), isEmpty);
    expect(placesOf(all, '*'), [(0, 0)]);
    expect(placesOf([List.filled(9000, 'x')], 'x').length, mostPlaces);
  });
}
