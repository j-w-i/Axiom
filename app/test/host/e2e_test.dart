import 'dart:async';
import 'dart:io';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

/// Through the real host process. `scripts/check.sh` builds it first; without it this fails.
final hostBinary = File('../engine/target/debug/axiom-host${Platform.isWindows ? '.exe' : ''}')
    .absolute
    .path;

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

  Future<HostLink> link() async => HostLink.connect(
    endpoint: File(paths.endpointFile).readAsStringSync(),
    token: File(paths.token).readAsStringSync(),
  );

  setUp(() async {
    expect(File(hostBinary).existsSync(), isTrue, reason: 'cargo build -p axiom-host first');
    dir = Directory.systemTemp.createTempSync('ax');
    paths = HostPaths('${dir.path}/d');
    await startHost();
  });
  tearDown(() async {
    host?.kill(ProcessSignal.sigkill);
    await host?.exitCode;
    // A host that was asked to stop may hold its files for a moment (Windows).
    for (var n = 0; ; n++) {
      try {
        dir.deleteSync(recursive: true);
        break;
      } on FileSystemException {
        if (n == 50) rethrow;
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
    }
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
      HostLink.connect(endpoint: File(paths.endpointFile).readAsStringSync(), token: 'no'),
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

  test('closing closes the socket at once, and leaves nothing behind', () async {
    final watch = await link();
    Future<int> clients() async => ((await watch.call('host.status'))! as Json)['clients']! as int;
    for (var i = 0; i < 12; i++) {
      (await link()).close();
    }
    var n = 99;
    for (var i = 0; i < 100 && n > 1; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 30));
      n = await clients();
    }
    expect(n, 1);
    watch.close();
  });

  test('a host that does not answer is given up on', () async {
    final silent = await ServerSocket.bind(
      InternetAddress('${dir.path}/silent.sock', type: InternetAddressType.unix),
      0,
    );
    final sw = Stopwatch()..start();
    await expectLater(
      HostLink.connect(
        endpoint: '${dir.path}/silent.sock',
        token: 'x',
        timeout: const Duration(milliseconds: 300),
      ),
      throwsA(isA<LinkClosed>()),
    );
    expect(sw.elapsedMilliseconds, lessThan(3000));
    await silent.close();
  });

  test('a follow that replaces another gets nothing of the first', () async {
    final l = await link();
    Future<String> made(int words) async {
      final id = ((await l.call('session.create', {'title': 'S'}))! as Json)['session']! as String;
      for (var k = 0; k < 4; k++) {
        await l.call('replay.start', {'session': id, 'path': script(words + k), 'speed': 0.0});
      }
      return id;
    }

    final a = await made(3000), b = await made(10);
    await Future<void>.delayed(const Duration(milliseconds: 600));
    final got = <Event>[];
    l.messages.listen((m) {
      if (m is Events && m.session == a) got.addAll(m.events);
    });
    for (var round = 0; round < 12; round++) {
      unawaited(l.follow(a, 0));
      await Future<void>.delayed(Duration(milliseconds: round));
      unawaited(l.unfollow(a));
      unawaited(l.follow(b, 0));
    }
    got.clear();
    await l.follow(a, 0);
    got.clear(); // only what comes after the answer counts
    await l.unfollow(a);
    got.clear();
    await l.follow(a, 0);
    final from = got.length;
    await Future<void>.delayed(const Duration(milliseconds: 600));
    final seqs = [for (final e in got.skip(from)) e.seq];
    expect(seqs, isNotEmpty);
    expect(seqs.first, 1);
    for (var i = 1; i < seqs.length; i++) {
      expect(seqs[i], greaterThan(seqs[i - 1]));
    }
    l.close();
  });

  testWidgets('switching away and back loses nothing', (t) async {
    await t.runAsync(() async {
      final l = await link();
      Future<String> made(int runs) async {
        final id =
            ((await l.call('session.create', {'title': 'S'}))! as Json)['session']! as String;
        for (var k = 0; k < runs; k++) {
          await l.call('replay.start', {'session': id, 'path': script(2000), 'speed': 0.0});
        }
        return id;
      }

      final a = await made(6), b = await made(1);
      await Future<void>.delayed(const Duration(milliseconds: 800));
      final store = HostStore(paths: paths, binary: hostBinary, spawn: false);
      await store.start();
      Future<void> frames(int ms) async {
        final end = DateTime.now().add(Duration(milliseconds: ms));
        while (DateTime.now().isBefore(end)) {
          await Future<void>.delayed(const Duration(milliseconds: 5));
          await t.pump();
        }
      }

      for (final wait in [0, 5, 25, 60, 120]) {
        await store.open(a);
        await frames(wait);
        await store.open(b);
        await frames(3);
        await store.open(a);
        await frames(900);
        final m = store.current!;
        expect([m.id, m.unknown, m.agents.length], [a, 0, 6], reason: 'after $wait ms');
      }
      store.dispose();
      l.close();
    });
  });

  testWidgets('a session that cannot be followed is its own problem, not the host\'s', (t) async {
    await t.runAsync(() async {
      final store = HostStore(paths: paths, binary: hostBinary, spawn: false);
      await store.start();
      await store.open('s_gone');
      expect(store.phase, HostPhase.connected);
      expect(store.current!.fault, isNotNull);
      await Future<void>.delayed(const Duration(seconds: 2));
      final l = await link();
      expect(((await l.call('host.status'))! as Json)['clients'], 2);
      store.dispose();
      l.close();
    });
  });

  testWidgets('a hidden window keeps up and holds nothing back', (t) async {
    await t.runAsync(() async {
      final store = HostStore(paths: paths, binary: hostBinary, spawn: false);
      await store.start();
      final id = await store.create('S');
      await store.open(id);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
      expect(t.binding.framesEnabled, isFalse);
      for (var k = 0; k < 5; k++) {
        await store.replay(id, script(2000), speed: 0);
      }
      final end = DateTime.now().add(const Duration(seconds: 10));
      bool done() =>
          store.current!.agents.length == 5 &&
          store.current!.agents.values.every((a) => a.ended == Outcome.completed);
      while (!done() && DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 30));
      }
      expect(done(), isTrue);
      expect(store.waiting, 0);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      t.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      store.dispose();
    });
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
