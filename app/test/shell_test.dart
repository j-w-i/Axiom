import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/patterns/session_panel.dart';
import 'package:axiom/shell/find.dart';
import 'package:axiom/shell/new_session.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/shell/start.dart';
import 'package:axiom/shell/window.dart';
import 'package:axiom/transcript/transcript_view.dart';
import 'package:axiom/ui/controls.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:axiom/ui/commands.dart';
import 'package:flutter/foundation.dart';
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
    expect(find.text('Double token rotation'), findsOneWidget, reason: 'said once');
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

  testWidgets('the window is moved by its header, and what is pressed there is pressed', (t) async {
    final was = window, asked = <String>[];
    window = (what) async => asked.add(what);
    addTearDown(() => window = was);
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s, titleInset: 72), size: const Size(1100, 700), dpr: 1);
    for (final at in [find.text('Axiom'), find.byType(MoveArea).last]) {
      await t.drag(at, const Offset(40, 10), kind: PointerDeviceKind.mouse, warnIfMissed: false);
      await t.pump();
      expect(asked, ['move']);
      asked.clear();
    }
    final empty = t.getTopRight(find.text('Axiom')) + const Offset(-2, 6);
    await t.tapAt(empty, kind: PointerDeviceKind.mouse);
    await t.pump(const Duration(seconds: 1));
    await t.tapAt(empty, kind: PointerDeviceKind.mouse);
    await t.pump(const Duration(seconds: 1));
    expect(asked, isEmpty, reason: 'two single clicks');
    await t.tapAt(empty, kind: PointerDeviceKind.mouse);
    await t.pump(const Duration(milliseconds: 50));
    await t.tapAt(empty, kind: PointerDeviceKind.mouse);
    await t.pump(const Duration(seconds: 1));
    expect(asked, ['zoom']);
    asked.clear();
    // Nor is a key held back, or taken for the header.
    final key = find.bySemanticsLabel('Hide the sidebar');
    await t.tap(key, kind: PointerDeviceKind.mouse);
    await t.pump();
    expect(key, findsNothing);
    expect(asked, isEmpty);
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

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
    expect(find.byType(StartPage), findsOneWidget, reason: 'one begins');
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

  testWidgets('the window\'s name and what it shows share a line', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    final a = t.getCenter(find.text('Axiom')).dy;
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

  testWidgets('a session that is open no more leaves the window standing', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    s
      ..current = null
      ..notifyListeners();
    await t.pump();
    expect(t.takeException(), isNull);
    expect(find.byType(StartPage), findsOneWidget, reason: 'one begins');
    s.dispose();
  });

  testWidgets('the strip says where the work is, until something has failed', (t) async {
    for (final (path, last) in [
      ('/home/me/axiom', 'axiom'),
      ('/home/me/axiom/', 'axiom'),
      (r'C:\work\axiom', 'axiom'),
      ('/', '/'),
      ('/home/me/a\u202Eb', r'a\u{202E}b'),
    ]) {
      final s = store()
        ..phase = HostPhase.connected
        ..current = (SessionModel('s')
          ..apply([
            Event(1, 1, SessionCreated('Work', path)),
            const Event(2, 2, AgentAdded('a', 'Ada', x, null, access: Access.ask)),
          ]));
      await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
      // Over its sessions in the list; over the display once the list is put away.
      expect(find.bySemanticsLabel('Folder $path'), findsOneWidget);
      expect(find.text(last), findsOneWidget, reason: path);
      await t.tap(find.bySemanticsLabel('Hide the sidebar'));
      await t.pump();
      expect(find.bySemanticsLabel('Folder $path'), findsOneWidget);
      expect(find.text('Work'), findsOneWidget);
      await t.tap(find.bySemanticsLabel('Show the sidebar'));
      await t.pump();
      s
        ..damaged = 'row 3'
        ..notifyListeners();
      await t.pump();
      expect(find.textContaining('The journal is damaged'), findsOneWidget);
      s.dispose();
    }
    final s = store()..phase = HostPhase.connected;
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    expect(find.bySemanticsLabel(RegExp('^Folder ')), findsNothing, reason: 'nothing is open');
    await t.tap(find.bySemanticsLabel('Accounts'));
    await t.pump();
    expect(find.byType(AccountsPage), findsOneWidget);
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
    expect(find.bySemanticsLabel('Accounts'), findsOneWidget);
    await snap(t, 'shell-narrow');
    await t.sendKeyEvent(LogicalKeyboardKey.tab);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pumpAndSettle();
    expect(find.byType(SessionPanel), findsOneWidget);
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
    for (final (width, left) in [(860.0, 228.0), (1079.0, 228.0), (1080.0, 264.0)]) {
      await mount(t, Shell(s, titleInset: 58), size: Size(width, 520));
      expect(find.byWidgetPredicate((w) => w is AxButton && w.label == 'Sessions'), findsNothing);
      expect(find.text('Other work'), findsWidgets);
      expect(t.getRect(find.byType(TranscriptView)).left, closeTo(left, 2));
    }
    s.dispose();
  });

  SessionInfo info(String id, String title, String? folder, int created, {int updated = 1}) =>
      SessionInfo.decode({
        'id': id,
        'title': title,
        'workspace': folder,
        'created': created,
        'updated': updated,
        'last_seq': 1,
        'running': false,
      });

  testWidgets('sessions stand under their folders, and stay where they are', (t) async {
    final opened = <String>[];
    final s = Opens(opened)
      ..phase = HostPhase.connected
      ..sessions = [
        // As the host lists them: the last used first.
        info('a', 'First of web', '/me/web', 1, updated: 9),
        info('b', 'Of the other web', '/you/web', 2, updated: 8),
        info('c', 'Second of web', '/me/web', 3, updated: 7),
        info('d', 'Nowhere', null, 4, updated: 6),
      ];
    await mount(t, Shell(s), size: const Size(1100, 900), dpr: 1);
    await t.pump();
    opened.clear();
    List<double> places() => [
      for (final x in ['No folder', 'Nowhere', 'me/web', 'Second of web', 'First of web'])
        t.getTopLeft(find.text(x).first).dy,
    ];
    final before = places();
    expect(before, [...before]..sort());
    // Two folders of one name are told apart, and said whole.
    expect(find.text('you/web'), findsOneWidget);
    expect(find.bySemanticsLabel('Folder /you/web'), findsOneWidget);
    await t.tap(find.text('First of web'));
    expect(opened, ['a']);
    s
      ..current = (SessionModel('a')
        ..apply([
          const Event(1, 1, SessionCreated('First of web', '/me/web')),
          const Event(2, 2, AgentAdded('x', 'Ada', x, null, access: Access.ask)),
        ]))
      // It has been used since: that moves nothing.
      ..sessions = [...s.sessions.reversed]
      ..notifyListeners();
    await t.pump();
    expect(places(), before);
    expect(
      t.getTopLeft(find.byType(AgentRow)).dy,
      greaterThan(t.getTopLeft(find.text('First of web').first).dy),
    );
    // A new one in that folder.
    await t.tap(find.bySemanticsLabel('New session in you/web'));
    await t.pump();
    expect(find.bySemanticsLabel('Folder: /you/web'), findsOneWidget);
    s.dispose();
  });

  Future<void> keys(WidgetTester t, LogicalKeyboardKey k) async {
    await t.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await t.sendKeyEvent(k);
    await t.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await t.pump();
    await t.pump();
  }

  testWidgets('the window\'s own keys, and the way back from a page', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 700), dpr: 1);
    await keys(t, LogicalKeyboardKey.keyN);
    expect(find.byType(StartPage), findsOneWidget);
    // Nothing of a session is chosen while a page is shown.
    expect(t.widget<AgentRow>(find.byType(AgentRow)).selected, isFalse);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pump();
    expect(find.byType(StartPage), findsNothing);
    await keys(t, LogicalKeyboardKey.keyN);
    await t.tap(find.bySemanticsLabel('Cancel'));
    await t.pump();
    expect(find.byType(TranscriptView), findsOneWidget);
    await keys(t, LogicalKeyboardKey.comma);
    expect(find.byType(AccountsPage), findsOneWidget);
    await t.tap(find.text('Done'));
    await t.pump();
    await keys(t, LogicalKeyboardKey.keyB);
    expect(find.byType(SessionPanel), findsNothing);
    expect(t.getRect(find.byType(TranscriptView)).left, lessThan(9));
    await t.tap(find.bySemanticsLabel('Show the sidebar'));
    await t.pump();
    expect(find.byType(SessionPanel), findsOneWidget);
    s.dispose();
  });

  testWidgets('a key goes to another session, the stop key stops, F6 goes around', (t) async {
    final opened = <String>[];
    final s = Opens(opened)
      ..phase = HostPhase.connected
      ..current = recorded()
      ..sessions = [
        info('p', 'One', '/me/web', 3),
        info('q', 'Two', '/me/web', 2),
        info('r', 'Three', '/me/docs', 1),
      ];
    await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
    final order = sessionOrder(s);
    Future<void> ctrl(LogicalKeyboardKey k, {bool shift = false}) async {
      if (shift) await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await keys(t, k);
      if (shift) await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    }

    await ctrl(LogicalKeyboardKey.digit1);
    await ctrl(LogicalKeyboardKey.digit3);
    expect(opened, [order[0], order[2]].where((x) => x != s.current!.id));
    // There is no ninth.
    await ctrl(LogicalKeyboardKey.digit9);
    expect(opened.length, lessThanOrEqualTo(2));
    opened.clear();
    await ctrl(LogicalKeyboardKey.tab);
    await ctrl(LogicalKeyboardKey.tab, shift: true);
    final at = order.indexOf(s.current!.id), n = order.length;
    expect(opened, [order[(at + 1) % n], order[(at - 1) % n]]);
    // Around the window, and back.
    FocusNode? where() => FocusManager.instance.primaryFocus;
    await t.sendKeyEvent(LogicalKeyboardKey.f6);
    await t.pump();
    final first = where();
    await t.sendKeyEvent(LogicalKeyboardKey.f6);
    await t.pump();
    expect(where(), isNot(first));
    s.dispose();
  });

  test('no two commands have one key', () {
    for (final mac in [true, false]) {
      debugDefaultTargetPlatformOverride = mac ? TargetPlatform.macOS : TargetPlatform.linux;
      final all = Command.values.map((c) => c.keys).nonNulls.toList();
      expect(all.toSet().length, all.length, reason: '$all');
      expect(Command.find.keys, mac ? '⌘K' : 'Ctrl+K');
      expect(Command.allow.keys, mac ? '⇧⌘Y' : 'Ctrl+Shift+Y');
    }
    debugDefaultTargetPlatformOverride = null;
    // Nor by another place a keyboard has it at.
    final taken = <(LogicalKeyboardKey, bool, bool, bool)>{};
    for (final c in Command.values) {
      for (final k in [?c.key, ...c.also]) {
        for (final shift in c.eitherShift ? [true, false] : [c.shift]) {
          expect(taken.add((k, shift, c.control, c.bare)), isTrue, reason: '$c');
        }
      }
    }
  });

  testWidgets('room can be made in a conversation that can act and is not at work', (t) async {
    for (final (access, works, can) in [
      (Access.ask, false, true),
      (Access.ask, true, false),
      (null, false, false),
    ]) {
      final m = SessionModel('s')
        ..apply([
          const Event(1, 1, SessionCreated('T', '/w')),
          Event(
            2,
            2,
            AgentAdded('a', 'Lead', const Execution(harness: 'axiom'), null, access: access),
          ),
          const Event(3, 3, RunStarted('r', 'a')),
          const Event(4, 4, ItemStarted('i', 'r', 'a', ItemKind.prose, '', null)),
          const Event(5, 5, ItemEnded('i', Outcome.completed, 'x', null)),
          if (!works) const Event(6, 6, RunEnded('r', Outcome.completed, null)),
        ]);
      final s = store()
        ..phase = HostPhase.connected
        ..current = m;
      await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
      await keys(t, LogicalKeyboardKey.keyK);
      await t.enterText(
        find.descendant(of: find.byType(SearchBox), matching: find.byType(EditableText)),
        'summarise',
      );
      await t.pump();
      expect(find.textContaining('Summarise the'), can ? findsOneWidget : findsNothing);
      await t.pumpWidget(const SizedBox());
      s.dispose();
    }
  });

  testWidgets('a folder asked for in a terminal is where a session could begin', (t) async {
    final s = store()
      ..phase = HostPhase.connected
      ..current = recorded();
    await mount(t, Shell(s), size: const Size(1100, 760), dpr: 1);
    expect(find.byType(StartPage), findsNothing);
    s.onFolder!('/me/from-a-terminal');
    await t.pump();
    expect(find.byType(StartPage), findsOneWidget);
    expect(find.textContaining('from-a-terminal'), findsWidgets);
    await t.pumpWidget(const SizedBox());
    expect(s.onFolder, isNull);
    s.dispose();
  });

  testWidgets('everything can be found, and gone to by the keyboard', (t) async {
    final opened = <String>[];
    final s = Opens(opened)
      ..phase = HostPhase.connected
      ..current = recorded()
      ..sessions = [
        info('p', 'Queue backpressure', '/me/web', 1),
        info('q', 'Release notes', '/me/docs', 2),
        for (var k = 0; k < 60; k++) info('n$k', 'Numbered $k', null, 3 + k),
      ];
    await mount(t, Shell(s), size: const Size(1100, 760), dpr: 2);
    await keys(t, LogicalKeyboardKey.keyK);
    expect(find.text('Search sessions and actions'), findsOneWidget);
    expect(find.textContaining(' more'), findsOneWidget, reason: 'what is not shown is counted');
    await snap(t, 'find');
    final field = find.descendant(of: find.byType(SearchBox), matching: find.byType(EditableText));
    await t.enterText(field, 'docs');
    await t.pump();
    expect(
      find.descendant(of: find.byType(SearchBox), matching: find.text('Queue backpressure')),
      findsNothing,
    );
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    expect(opened, ['q']);
    expect(find.byType(SearchBox), findsNothing);
    // The arrows choose; Escape leaves things as they are.
    await keys(t, LogicalKeyboardKey.keyK);
    await t.enterText(field, 'numbered');
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await t.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await t.pump();
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.pump();
    expect(opened, ['q', 'n2']);
    await keys(t, LogicalKeyboardKey.keyK);
    await t.enterText(field, 'no such thing');
    await t.pump();
    expect(find.text('No results'), findsOneWidget);
    await t.sendKeyEvent(LogicalKeyboardKey.enter);
    await t.sendKeyEvent(LogicalKeyboardKey.escape);
    await t.pump();
    expect(find.byType(SearchBox), findsNothing);
    expect(opened, ['q', 'n2']);
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
