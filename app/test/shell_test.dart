import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

const x = Execution(harness: 'replay', model: 'Opus 5.5');

/// A session as the host would send it; the real host drives test/host/e2e_test.dart.
SessionModel recorded({String agent = 'Builder'}) {
  var n = 0;
  Event e(Body b) => Event(++n, n, b);
  return SessionModel('s')..apply([
    e(const SessionCreated('Double token rotation', null)),
    e(AgentAdded('a', agent, x, null)),
    e(const AgentStateChanged('a', AgentPhase.running)),
    e(const ItemStarted('p', 'r', 'a', ItemKind.prompt, '', null)),
    e(const ItemEnded('p', Outcome.completed, 'Make rotation single-flight.', null)),
    e(const ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
    e(const ItemDelta('i', 'I will read the refresh path first.\n\n')),
    e(const ItemStarted('t', 'r', 'a', ItemKind.tool, 'read', {'path': 'src/auth/refresh.ts'})),
    e(const ItemEnded('t', Outcome.completed, '', {'summary': '220 lines'})),
    e(const ItemStarted('j', 'r', 'a', ItemKind.prose, '', null)),
    e(const ItemDelta('j', 'The check happens **before** the lock is held, so `rotate()` runs tw')),
  ]);
}

class Opens extends HostStore {
  Opens(this.opened) : super(paths: HostPaths('/nonexistent'), binary: null, spawn: false);
  final List<String> opened;
  @override
  Future<void> open(String id) async => opened.add(id);
}

HostStore store() => HostStore(paths: HostPaths('/nonexistent'), binary: null, spawn: false);

void main() {
  testWidgets('shows a session as it arrives, and follows it', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700));
    expect(find.text('Double token rotation'), findsOneWidget);
    expect(find.text('Builder'), findsWidgets);
    expect(find.text('Opus 5.5   replay'), findsOneWidget, reason: 'a replay is called one');
    expect(find.text('220 lines'), findsOneWidget);
    await snap(t, 'shell');
    s.current!.apply([
      const Event(12, 12, ItemDelta('j', 'ice.')),
      const Event(13, 13, AgentStateChanged('a', AgentPhase.done)),
    ]);
    await t.pump();
    expect(find.textContaining('runs twice.', findRichText: true), findsOneWidget);
    s.dispose();
  });

  testWidgets('says that the journal is damaged, on one line, and that the list was cut', (
    t,
  ) async {
    final s = store()
      ..phase = HostPhase.connected
      ..damaged = 'row 3\nis missing'
      ..older = true
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    expect(find.textContaining('The journal is damaged: row 3'), findsOneWidget);
    expect(find.textContaining('Older ones are in the journal'), findsOneWidget);
    s.dispose();
  });

  testWidgets('says so when the host is gone, and keeps what was shown', (t) async {
    final s = store()
      ..phase = HostPhase.unreachable
      ..problem = 'the host closed the connection'
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    expect(find.text('The host is not answering. Trying again.'), findsOneWidget);
    expect(find.text('not live'), findsOneWidget);
    expect(find.text('220 lines'), findsOneWidget);
    s.dispose();
  });

  testWidgets('with nothing to show, says what is true', (t) async {
    final s = store()..phase = HostPhase.unreachable;
    await mount(t, Shell(s), dpr: 1);
    expect(find.text('axiom-host was not found beside the application'), findsOneWidget);
    s.phase = HostPhase.connected;
    // ignore: invalid_use_of_protected_member, invalid_use_of_visible_for_testing_member
    s.notifyListeners();
    await t.pump();
    expect(find.text('No sessions yet'), findsOneWidget);
    s.dispose();
  });

  testWidgets('an unreadable session says so and offers to try again', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = (recorded()..failed('journal: damaged'));
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    expect(find.text('This session cannot be shown'), findsOneWidget);
    expect(find.text('journal: damaged'), findsOneWidget);
    expect(find.text('Try again'), findsOneWidget);
    s.dispose();
  });

  testWidgets('a session that is still arriving is not called empty', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = SessionModel('s');
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    expect(find.text('Reading the session'), findsOneWidget);
    s.dispose();
  });

  testWidgets('the title row of the session and of the display share a line', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    final a = t.getCenter(find.text('Double token rotation')).dy;
    final b = t.getCenter(find.text('Builder').last).dy;
    expect((a - b).abs(), lessThan(1.1));
    s.dispose();
  });

  List<Event> busy(int from) => [
    Event(from, 1, UsageReported('a', Usage.decode({'context': .82}), Known.reported)),
    Event(
      from + 1,
      1,
      const RequestOpened('q', 'a', 'asks to run', 'rm -r build', 'in the worktree', Answerable.no),
    ),
  ];

  testWidgets('shows how full the context is and what was asked', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = (recorded()..apply(busy(12)));
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    for (final w in [
      '82%',
      'Builder asks to run (no longer waiting)',
      'rm -r build',
      'in the worktree',
    ]) {
      expect(find.text(w), findsOneWidget);
    }
    s.dispose();
  });

  testWidgets('the keyboard goes through one pane and then the other', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = (recorded()
        ..apply([
          for (var k = 0; k < 3; k++) Event(12 + k, 1, AgentAdded('b$k', 'Agent $k', x, null)),
        ]))
      ..sessions = [
        for (var k = 0; k < 9; k++)
          SessionInfo.decode({
            'id': 'o$k',
            'title': 'Other $k',
            'created': 1,
            'updated': 1,
            'last_seq': 1,
            'running': false,
          }),
      ];
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    final xs = <double>[];
    for (var k = 0; k < 40; k++) {
      await t.sendKeyEvent(LogicalKeyboardKey.tab);
      await t.pump();
      final box = FocusManager.instance.primaryFocus!.context!.findRenderObject()! as RenderBox;
      xs.add(box.localToGlobal(Offset.zero).dx);
    }
    // Once round: in the left pane until it is left, and back in it only at the end.
    final changes = [
      for (var k = 1; k < xs.length; k++)
        if ((xs[k] < 264) != (xs[k - 1] < 264)) k,
    ];
    expect(changes.length, lessThanOrEqualTo(40 ~/ 14 * 2));
    expect(changes.first, greaterThan(10), reason: '$xs');
    s.dispose();
  });

  testWidgets('in a narrow window the list is behind a key', (t) async {
    final opened = <String>[];
    final s = Opens(opened)
      ..phase = HostPhase.connected
      ..current = recorded()
      ..sessions = [
        SessionInfo.decode(const {
          'id': 'o',
          'title': 'Other work',
          'created': 1,
          'updated': 1,
          'last_seq': 1,
          'running': false,
        }),
      ];
    await mount(t, Shell(s, titleInset: 58), size: const Size(760, 520));
    expect(find.bySemanticsLabel('Sessions'), findsOneWidget);
    expect(find.text('Other work'), findsNothing);
    expect(t.getRect(find.byType(TranscriptView)).left, lessThan(10));
    expect(find.bySemanticsLabel('New session'), findsOneWidget);
    expect(find.bySemanticsLabel('Accounts and servers'), findsOneWidget);
    await snap(t, 'shell-narrow');
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pumpAndSettle();
    expect(find.text('Double token rotation'), findsOneWidget);
    expect(find.text('Other work'), findsOneWidget);
    expect(t.takeException(), isNull);
    await snap(t, 'shell-narrow-open');
    // Escape gives the keyboard back to where it was.
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pumpAndSettle();
    expect(find.text('Other work'), findsNothing);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pumpAndSettle();
    await t.tap(find.text('Other work'));
    await t.pumpAndSettle();
    expect(opened, ['o']);
    expect(find.text('Other work'), findsNothing);
    // What waits is in sight without the list.
    s
      ..needs = const [
        Waits('s', 'Double token rotation', 'a', 'Builder', 'q', 'asks to run a command', 'ls', 1),
        Waits('o', 'Other work', 'b', 'Ada', 'q2', 'asks to write a file', 'a.txt', 2),
      ]
      ..notifyListeners();
    await t.pump();
    expect(find.text('2 asking'), findsOneWidget);
    expect(find.bySemanticsLabel('2 asking, open the first'), findsOneWidget);
    // Wider, the list is beside it again, at 224 and at 264.
    for (final (width, left) in [(860.0, 224.0), (1079.0, 224.0), (1080.0, 264.0)]) {
      await mount(t, Shell(s, titleInset: 58), size: Size(width, 520));
      expect(find.bySemanticsLabel('Sessions'), findsNothing);
      expect(find.text('Other work'), findsWidgets);
      expect(t.getRect(find.byType(TranscriptView)).left, closeTo(left, 2));
    }
    s.dispose();
  });

  testWidgets('nothing overflows from 760 px, in both themes, at large text', (t) async {
    for (final (c, agent) in [
      (AxColors.dark, 'Builder'),
      (AxColors.light, 'An agent with a long name that goes on'),
    ]) {
      for (final scale in [1.0, 1.5, 2.0]) {
        final s = store()
          ..phase = HostPhase.unreachable
          ..current = (recorded(agent: agent)..apply(busy(12)));
        await mount(
          t,
          Shell(s, titleInset: 58),
          size: const Size(760, 520),
          theme: AxTheme(colors: c),
          textScale: scale,
          dpr: 1,
        );
        expect(t.takeException(), isNull);
        s.dispose();
      }
    }
  });
}
