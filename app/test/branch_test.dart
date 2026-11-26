import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/execution.dart';
import 'package:axiom/shell/drafts.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/talk.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/editable.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'agents_test.dart' show Asked, key, settled, x;
import 'shot.dart';

const program = Execution(harness: 'claude-code');

/// Two runs, the second with words said meanwhile and a step before its answer.
List<Body> conversation({Execution execution = x, bool apart = false}) => [
  const SessionCreated('Fix the cache', '/w'),
  AgentAdded('a', 'Lead', execution, null, access: Access.ask, apart: apart),
  const RunStarted('r1', 'a'),
  ...said('p1', 'r1', 'Make the cache forget.'),
  ...answered('w1', 'r1', 'It forgets.'),
  const RunEnded('r1', Outcome.completed, null),
  const RunStarted('r2', 'a'),
  ...said('o1', 'r2', 'Reviewer has finished.', by: 'Reviewer'),
  ...said('p2', 'r2', 'Now the tests.'),
  ...answered('w2', 'r2', 'I look first.'),
  const ItemStarted('t', 'r2', 'a', ItemKind.tool, 'list', {}),
  const ItemEnded('t', Outcome.completed, '', null),
  ...said('p3', 'r2', 'All of them.'),
  ...answered('w3', 'r2', 'They pass.'),
  const RunEnded('r2', Outcome.completed, null),
  const AgentStateChanged('a', AgentPhase.done),
];
List<Body> said(String id, String run, String text, {String by = '', String agent = 'a'}) => [
  ItemStarted(id, run, agent, ItemKind.prompt, by, null),
  ItemEnded(id, Outcome.completed, text, null),
];
List<Body> answered(String id, String run, String text, {String agent = 'a'}) => [
  ItemStarted(id, run, agent, ItemKind.prose, '', null),
  ItemEnded(id, Outcome.completed, text, null),
];
SessionModel session(List<Body> all) =>
    SessionModel('s')..apply([for (final (n, b) in all.indexed) Event(n + 1, n, b)]);

List<String> places(AgentModel a) => [
  for (final i in a.items)
    if (a.canBranch(i)) i.id,
];

void main() {
  group('where it can be branched', () {
    test('what began a run, and the answer that ended one', () {
      expect(places(session(conversation()).agents['a']!), ['p1', 'w1', 'p2', 'w3']);
    });

    test('not of a program, which keeps its own conversation', () {
      expect(places(session(conversation(execution: program)).agents['a']!), isEmpty);
    });

    test('not an answer that is not at its end, or was cut short', () {
      final all = conversation().take(5).toList();
      expect(places(session(all).agents['a']!), ['p1']);
      for (final o in [Outcome.cancelled, Outcome.failed, Outcome.interrupted]) {
        final m = session([...all, RunEnded('r1', o, null)]);
        expect(places(m.agents['a']!), ['p1'], reason: o.name);
      }
      // A run without words has none, and not that of the run before.
      final m = session([
        ...conversation(),
        const RunStarted('r3', 'a'),
        ...said('p4', 'r3', 'More.'),
        const RunEnded('r3', Outcome.completed, null),
      ]);
      expect(places(m.agents['a']!), ['p1', 'w1', 'p2', 'w3', 'p4']);
    });
  });

  group('a branch', () {
    AgentModel of(Branched from) => session([
      ...conversation(),
      AgentAdded('b', 'Lead 2', x, null, access: Access.ask, from: from),
      ...said('q', 'r9', 'Otherwise.', agent: 'b'),
    ]).agents['b']!;
    List<String> texts(AgentModel a) => [for (final i in a.items) i.plain];

    test('begins with what was shown before it', () {
      final b = of(const Branched('a', 'p2'));
      expect(texts(b), [
        'Make the cache forget.',
        'It forgets.',
        'Reviewer has finished.',
        'Branched from Lead',
        'Otherwise.',
      ]);
      expect(b.said, ['Otherwise.', 'Make the cache forget.']);
      // And can be branched there again, as at what is its own.
      expect(places(b), ['p1', 'w1', 'q']);
    });

    test('or with it', () {
      final b = of(const Branched('a', 'w1', kept: true));
      expect(texts(b).take(3), ['Make the cache forget.', 'It forgets.', 'Branched from Lead']);
      expect(places(b), ['p1', 'w1', 'q']);
    });

    test('leaves the other as it was, and takes nothing that comes after', () {
      final m = session([
        ...conversation(),
        const AgentAdded('b', 'Lead 2', x, null, from: Branched('a', 'p2')),
        const RunStarted('r3', 'a'),
        ...said('p4', 'r3', 'More.'),
      ]);
      expect(m.agents['a']!.items.length, 9);
      expect(m.agents['b']!.items.length, 4);
    });

    test('of what is not known begins empty', () {
      expect(of(const Branched('z', 'p2')).items.map((i) => i.id), ['q']);
      expect(of(const Branched('a', 'z')).items.map((i) => i.id), ['q']);
    });
  });

  test('a name nobody has', () {
    expect(nextName('Lead', []), 'Lead 2');
    expect(nextName('Lead', ['Lead', 'Lead 2', 'Lead 3']), 'Lead 4');
    expect(nextName('Lead 2', ['Lead', 'Lead 2']), 'Lead 3');
    expect(nextName('Model 5000', []), 'Model 5000 2');
  });

  group('two presses', () {
    Future<Asked> shell(WidgetTester t, SessionModel m, {BranchInfo? place}) async {
      final s = Asked()
        ..current = m
        ..connections = const [key];
      if (place != null) s.place = place;
      addTearDown(s.dispose);
      await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
      return s;
    }

    Future<void> press(WidgetTester t, String words, String key) async {
      final g = await t.createGesture(kind: PointerDeviceKind.mouse);
      await g.addPointer(location: Offset.zero);
      await g.moveTo(t.getCenter(find.textContaining(words, findRichText: true).first));
      await t.pump();
      final shown = find.ancestor(
        of: find.text(key),
        matching: find.byWidgetPredicate((w) => w is Opacity && w.opacity == 1),
      );
      await t.tap(find.descendant(of: shown.last, matching: find.text(key)));
      await g.removePointer();
      await settled(t);
    }

    Finder box(String label) => find.descendant(
      of: find.byWidgetPredicate((w) => w is AxEditable && w.label == label),
      matching: find.byType(EditableText),
    );
    Future<void> send(WidgetTester t) async {
      // A click meant for something else does not send.
      await settled(t);
      await t.tap(find.bySemanticsLabel('Send'));
      await settled(t);
    }

    testWidgets('something is said differently', (t) async {
      final m = session(conversation());
      final s = await shell(t, m);
      await t.enterText(box('Message Lead'), 'half a thought');
      await press(t, 'Now the tests', 'Edit');
      expect(s.calls.last, ['look branch', 's', 'a', 'p2']);
      expect(find.text('Said differently, in a new branch'), findsOneWidget);
      expect(find.text('In the same folder. Its files stay as they are.'), findsOneWidget);
      // What it runs on is not chosen here: that is the other agent's.
      expect(find.byType(ExecutionKey), findsNothing);
      final words = box('Message the new branch');
      expect(t.widget<EditableText>(words).controller.text, 'Now the tests.');
      expect(t.widget<EditableText>(words).focusNode.hasFocus, isTrue);
      expect(s.calls.any((c) => c.first == 'branch'), isFalse, reason: 'nothing is made yet');
      await snap(t, 'branch');
      await t.enterText(words, 'Now the tests, quickly.');
      await send(t);
      expect(s.calls.where((c) => c.first == 'branch' || c.first == 'prompt'), [
        ['branch', 's', 'a', 'p2', 'Lead 2'],
        ['prompt', 's', 'a_new', 'Now the tests, quickly.'],
      ]);
      // The branch is shown, with what was before it.
      expect(box('Message Lead 2'), findsOneWidget);
      await snap(t, 'branch-made');
      expect(find.text('Branched from Lead'), findsOneWidget);
      expect(find.textContaining('They pass', findRichText: true), findsNothing);
      // What was being written to the other is there still.
      final talk = t.widget<Talk>(find.byType(Talk));
      expect(talk.drafts!.of('s', 'a').text.text, 'half a thought');
    });

    testWidgets('it goes on from an answer, with the files or without', (t) async {
      const place = BranchInfo(apart: true, files: true);
      final s = await shell(t, session(conversation(apart: true)), place: place);
      await press(t, 'They pass', 'Branch');
      expect(find.text('In a new branch, from this answer on'), findsOneWidget);
      final files = find.byType(AxCheck);
      expect(t.widget<AxCheck>(files).value, isTrue);
      await snap(t, 'branch-files');
      await t.enterText(box('Message the new branch'), 'Go on.');
      await send(t);
      expect(s.calls, contains(equals(['branch', 's', 'a', 'w3', 'Lead 2', 'files'])));
    });

    testWidgets('the files are left where they are', (t) async {
      const place = BranchInfo(apart: true, files: true);
      final s = await shell(t, session(conversation(apart: true)), place: place);
      await press(t, 'They pass', 'Branch');
      await t.tap(find.byType(AxCheck));
      await settled(t);
      await t.enterText(box('Message the new branch'), 'Go on.');
      await send(t);
      expect(s.calls, contains(equals(['branch', 's', 'a', 'w3', 'Lead 2'])));
    });

    testWidgets('what is not kept is not offered', (t) async {
      await shell(t, session(conversation(apart: true)), place: const BranchInfo(apart: true));
      await press(t, 'They pass', 'Branch');
      expect(find.byType(AxCheck), findsNothing);
      expect(
        find.text('In a fresh copy of the folder. Its files then are not kept.'),
        findsOneWidget,
      );
    });

    testWidgets('it is thought better of', (t) async {
      final s = await shell(t, session(conversation()));
      await t.enterText(box('Message Lead'), 'half a thought');
      await press(t, 'Now the tests', 'Edit');
      await t.enterText(box('Message the new branch'), 'another');
      // Another place instead: what had been written waits still.
      await press(t, 'Make the cache', 'Edit');
      expect(
        t.widget<EditableText>(box('Message the new branch')).controller.text,
        'Make the cache forget.',
      );
      await t.tap(find.text('Cancel'));
      await settled(t);
      expect(t.widget<EditableText>(box('Message Lead')).controller.text, 'half a thought');
      expect(find.byType(ExecutionKey), findsOneWidget);
      await send(t);
      expect(s.calls.last, ['prompt', 's', 'a', 'half a thought']);
    });

    testWidgets('it cannot be, and that is said', (t) async {
      final s = await shell(t, session(conversation()))
        ..failure = const Failed(1, 'bad_request', 'the conversation cannot be branched here')
        ..failsAt = 'look branch';
      await press(t, 'They pass', 'Branch');
      expect(find.textContaining('cannot be branched here'), findsOneWidget);
      expect(box('Message Lead'), findsOneWidget);
      // Made, and then not: the words are there to be sent again.
      s.failsAt = 'branch';
      await press(t, 'They pass', 'Branch');
      await t.enterText(box('Message the new branch'), 'Go on.');
      await send(t);
      expect(t.widget<EditableText>(box('Message the new branch')).controller.text, 'Go on.');
      expect(s.calls.any((c) => c.first == 'prompt'), isFalse);
    });

    testWidgets('what could not be said waits where it was to be said', (t) async {
      final s = await shell(t, session(conversation()))
        ..failure = const Failed(1, 'busy', 'too much is running')
        ..failsAt = 'prompt';
      await press(t, 'They pass', 'Branch');
      await t.enterText(box('Message the new branch'), 'Go on.');
      await send(t);
      expect(box('Message Lead 2'), findsOneWidget);
      expect(t.widget<EditableText>(box('Message Lead 2')).controller.text, 'Go on.');
      expect(find.textContaining('too much is running'), findsOneWidget);
      expect(s.calls.where((c) => c.first == 'branch').length, 1);
    });

    testWidgets('only where it can be, and not of a program', (t) async {
      await shell(t, session(conversation(execution: program)));
      for (final w in ['Edit', 'Branch']) {
        expect(find.text(w), findsNothing);
      }
    });

    testWidgets('by the keyboard', (t) async {
      final s = await shell(t, session(conversation()));
      final keys = find.byWidgetPredicate((w) => w is AxButton && w.label == 'Branch');
      expect(keys, findsNWidgets(2));
      final node = Focus.of(
        t.element(find.descendant(of: keys.last, matching: find.text('Branch'))),
      );
      node.requestFocus();
      await t.pump();
      await t.sendKeyEvent(LogicalKeyboardKey.enter);
      await settled(t);
      expect(s.calls.last, ['look branch', 's', 'a', 'w3']);
    });
  });
}
