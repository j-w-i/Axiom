@TestOn('mac-os || linux')
library;

import 'dart:async';
import 'dart:io';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:flutter_test/flutter_test.dart';

/// Through the real host process. `scripts/check.sh` builds it first; without it this fails.
final hostBinary = File('../engine/target/debug/axiom-host').absolute.path;

void main() {
  late Directory dir;
  late HostPaths paths;
  Process? host;

  Future<void> startHost() async {
    final p = host = await Process.start(
      hostBinary,
      const [],
      environment: {'AXIOM_DATA': paths.data},
    );
    unawaited(p.stderr.drain<void>());
    await p.stdout.first;
  }

  Future<HostLink> link() async =>
      HostLink.connect(endpoint: paths.endpoint, token: File(paths.token).readAsStringSync());

  setUp(() async {
    expect(File(hostBinary).existsSync(), isTrue, reason: 'cargo build -p axiom-host first');
    // Short: a Unix socket's name is limited.
    dir = Directory('/tmp').createTempSync('ax');
    paths = HostPaths('${dir.path}/d');
    await startHost();
  });
  tearDown(() async {
    host?.kill(ProcessSignal.sigkill);
    await host?.exitCode;
    dir.deleteSync(recursive: true);
  });

  String script(int words, {int after = 0}) {
    final f = File('${dir.path}/s$words.jsonl')
      ..writeAsStringSync(
        [
          '{"do":"agent","name":"Lead","model":"m"}',
          '{"do":"start","item":"p","kind":"prose"}',
          for (var i = 0; i < words; i++)
            '{"after":$after,"do":"text","item":"p","text":"w$i${i % 20 == 19 ? r'\n\n' : ' '}"}',
          '{"do":"end","item":"p","outcome":"completed"}',
          '{"do":"finish","outcome":"completed"}',
        ].join('\n'),
      );
    return f.path;
  }

  test('a replay streams from the host to the model', () async {
    final l = await link();
    final id = ((await l.call('session.create', {'title': 'S'}))! as Json)['session']! as String;
    final m = SessionModel(id);
    final done = Completer<void>();
    l.messages.listen((x) {
      if (x case Events(:final events)) {
        m.apply(events);
        if (events.any((e) => e.body is RunEnded)) done.complete();
      }
    });
    await l.call('session.follow', {'session': id});
    await l.call('replay.start', {'session': id, 'path': script(2000), 'speed': 1.0});
    await done.future.timeout(const Duration(seconds: 10));
    final a = m.agents.values.single;
    expect([a.name, a.execution.harness, a.execution.model], ['Lead', 'replay', 'm']);
    final blocks = a.items.single.doc.closed;
    expect(blocks.length, 100);
    expect(blocks.expand((b) => b.text.split(' ')), [for (var i = 0; i < 2000; i++) 'w$i']);
    l.close();
  });

  test('a wrong token and a missing host are errors with words', () async {
    await expectLater(
      HostLink.connect(endpoint: paths.endpoint, token: 'no'),
      throwsA(isA<Failed>().having((f) => f.code, 'code', 'unauthorised')),
    );
    await expectLater(
      HostLink.connect(endpoint: '${dir.path}/nothing.sock', token: 'x'),
      throwsA(isA<LinkClosed>()),
    );
    final l = await link();
    await expectLater(l.call('no.such'), throwsA(isA<Failed>()));
    expect(((await l.call('host.status'))! as Json)['pid'], host!.pid);
    l.close();
  });

  testWidgets('the store survives the host being killed, and shows how the run ended', (t) async {
    await t.runAsync(() async {
      final store = HostStore(paths: paths, binary: hostBinary, spawn: false);
      Future<void> settle(bool Function() ok) async {
        final end = DateTime.now().add(const Duration(seconds: 15));
        while (!ok()) {
          if (DateTime.now().isAfter(end)) fail('timed out: ${store.phase} ${store.problem}');
          await Future<void>.delayed(const Duration(milliseconds: 20));
          await t.pump();
        }
      }

      unawaited(store.start());
      await settle(() => store.phase == HostPhase.connected);
      final id = await store.create('S');
      await store.open(id);
      await store.replay(id, script(50000, after: 1));
      AgentModel? agent() => store.current!.agents.values.firstOrNull;
      await settle(() => (agent()?.items.firstOrNull?.doc.closed.length ?? 0) > 3);
      final before = agent()!.items.single.doc.closed.map((b) => b.text).toList();

      host!.kill(ProcessSignal.sigkill);
      await host!.exitCode;
      await settle(() => store.phase == HostPhase.unreachable);
      await startHost();
      await settle(() => agent()!.items.single.outcome != null);

      expect(agent()!.items.single.outcome, Outcome.interrupted);
      final after = agent()!.items.single.doc.closed.map((b) => b.text).toList();
      expect(after.take(before.length), before, reason: 'nothing that was shown is lost');
      expect(store.phase, HostPhase.connected);
      expect(store.sessions.single.running, isFalse);
      store.dispose();
    });
  });

  testWidgets('the store starts a host that outlives it', (t) async {
    await t.runAsync(() async {
      host!.kill(ProcessSignal.sigkill);
      await host!.exitCode;
      final store = HostStore(paths: paths, binary: hostBinary);
      await store.start();
      expect(store.phase, HostPhase.connected, reason: store.problem);
      store.dispose();
      final l = await link();
      final pid = ((await l.call('host.status'))! as Json)['pid'];
      expect(pid, isNot(host!.pid));
      await l.call('host.shutdown');
      l.close();
    });
  });
}
