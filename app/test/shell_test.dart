import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:axiom/shell/shell.dart';
import 'package:axiom/ui/tokens.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

import 'shot.dart';

const x = Execution(harness: 'replay', model: 'Opus 5.5');

/// A session as the host would send it; the real host drives test/host/e2e_test.dart.
SessionModel recorded() {
  var n = 0;
  Event e(Body b) => Event(++n, n, b);
  return SessionModel('s')..apply([
    e(const SessionCreated('Double token rotation', null)),
    e(const AgentAdded('a', 'Builder', x, null)),
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
    expect(find.text('axiom-host was not found beside the application.'), findsOneWidget);
    s.phase = HostPhase.connected;
    // ignore: invalid_use_of_protected_member, invalid_use_of_visible_for_testing_member
    s.notifyListeners();
    await t.pump();
    expect(find.text('No sessions yet'), findsOneWidget);
    s.dispose();
  });

  testWidgets('nothing overflows from 760 px, in both themes, at large text', (t) async {
    for (final c in [AxColors.dark, AxColors.light]) {
      for (final scale in [1.0, 1.5]) {
        final s = store()
          ..phase = HostPhase.unreachable
          ..current = recorded();
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
