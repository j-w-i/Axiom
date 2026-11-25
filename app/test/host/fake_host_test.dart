import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:axiom/host/session_model.dart';
import 'package:flutter_test/flutter_test.dart';

import 'unix_server.dart';

/// A host that misbehaves on purpose. The real one drives e2e_test.dart.
class FakeHost {
  FakeHost._(this.dir, this.server);
  final Directory dir;
  final UnixServer server;
  int follows = 0;
  List<Object> sessions = [];
  int created = 0, connections = 0;

  /// The answer to `host.waiting`.
  List<Object> needs = [];

  /// The answer to `host.status`; null answers nothing at all.
  Json? status = {'result': <String, Object?>{}};

  /// Answers nothing at all any more, and does not say so.
  bool dead = false;
  final listed = <Object?>[];
  final asked = <String>[];
  Socket? last;

  /// What to write when a session is followed, after its answer; null answers nothing at all.
  Iterable<Object>? Function(String session) onFollow = (_) => const [];

  static Future<FakeHost> start({bool? relayed}) async {
    final dir = Directory.systemTemp.createTempSync('af');
    Directory('${dir.path}/run').createSync();
    final path = '${dir.path}/run/s';
    final server = await UnixServer.bind(path, relayed: relayed);
    File('${dir.path}/run/endpoint').writeAsStringSync(path);
    File('${dir.path}/run/token').writeAsStringSync('t');
    final h = FakeHost._(dir, server);
    server.listen((s) {
      h.connections++;
      h.last = s;
      s.done.ignore();
      s.cast<List<int>>().transform(utf8.decoder).transform(const LineSplitter()).listen((l) {
        final m = json.decode(l) as Json;
        final id = m['id'];
        if (h.dead) return;
        h.asked.add(m['method']! as String);
        void say(Object o) => s.write('${json.encode(o)}\n');
        switch (m['method']) {
          case 'session.follow':
            h.follows++;
            final out = h.onFollow((m['params']! as Json)['session']! as String);
            if (out == null) return;
            say({'id': id, 'result': <String, Object?>{}});
            out.forEach(say);
          case 'session.list':
            h.listed.add(m['params']);
            say({'id': id, 'result': h.sessions});
          case 'host.waiting':
            say({'id': id, 'result': h.needs});
          case 'host.status':
            if (h.status case final s?) say({'id': id, ...s});
          case 'session.create':
            h.created++;
            say({
              'id': id,
              'result': {'session': 'new'},
            });
          case _:
            say({'id': id, 'result': <String, Object?>{}});
        }
      }, onError: (_) {});
    });
    return h;
  }

  Future<void> stop() async {
    await server.close();
    dir.deleteSync(recursive: true);
  }
}

class Counted extends SessionModel {
  Counted(super.id);
  int applied = 0;
  @override
  void apply(List<Event> events) {
    applied++;
    super.apply(events);
  }
}

void main() {
  late FakeHost h;
  setUp(() async => h = await FakeHost.start());
  tearDown(() => h.stop());

  HostStore store({
    Duration every = const Duration(seconds: 20),
    Duration patience = const Duration(seconds: 30),
  }) => HostStore(
    paths: HostPaths(h.dir.path),
    binary: null,
    spawn: false,
    listEvery: every,
    patience: patience,
  );

  Json event(int seq, Json body) => {'seq': seq, 'at': 1, 'body': body};
  Json events(List<Json> e) => {'session': 'x', 'events': e};

  Future<HostStore> follow(WidgetTester t, int last, Duration d) async {
    h.onFollow = (s) => [
      {'session': s, 'resync': last},
    ];
    final s = store();
    await s.start();
    await s.open('x');
    final end = DateTime.now().add(d);
    while (s.current!.fault == null && DateTime.now().isBefore(end)) {
      await Future<void>.delayed(const Duration(milliseconds: 20));
    }
    return s;
  }

  testWidgets('a history that keeps changing is given up on, in words', (t) async {
    await t.runAsync(() async {
      final s = await follow(t, 0, const Duration(seconds: 4));
      expect(s.current!.fault, isNotNull);
      expect(h.follows, lessThan(6));
      expect(s.phase, HostPhase.connected);
      s.dispose();
    });
  });

  testWidgets('told again and again that it is behind, it slows down', (t) async {
    await t.runAsync(() async {
      final s = await follow(t, 7, const Duration(seconds: 4));
      expect(h.follows, lessThan(12));
      expect(s.phase, HostPhase.connected);
      s.dispose();
    });
  });

  testWidgets('a fault ends the session here and nothing follows by itself', (t) async {
    await t.runAsync(() async {
      h.onFollow = (s) => [
        {'session': s, 'fault': 'journal: damaged'},
      ];
      final s = store();
      await s.start();
      await s.open('x');
      await Future<void>.delayed(const Duration(milliseconds: 300));
      expect([s.current!.fault, h.follows], ['journal: damaged', 1]);
      s.dispose();
    });
  });

  testWidgets('a damaged journal and a list that was cut are known', (t) async {
    await t.runAsync(() async {
      Json session(int i) => {
        'id': 's$i',
        'title': 'T',
        'created': 1,
        'updated': 1,
        'last_seq': 1,
        'running': false,
      };
      h.sessions = [session(0)];
      final s = store();
      await s.start();
      expect([s.older, s.damaged, s.phase], [false, null, HostPhase.connected]);
      h.sessions = [for (var i = 0; i < HostStore.listed; i++) session(i)];
      h.status = {
        'result': {'damaged': 'row 3 is missing'},
      };
      await s.refresh();
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect([s.older, s.damaged], [true, 'row 3 is missing']);
      expect(h.listed.last, {'limit': HostStore.listed});
      s.dispose();
    });
  });

  for (final (what, answer) in <(String, Json?)>[
    (
      'an error',
      {
        'error': {'code': 'internal', 'message': 'status broke'},
      },
    ),
    ('nothing', null),
    ('something else', {'result': 3}),
    (
      'a number for words',
      {
        'result': {'damaged': 3},
      },
    ),
  ]) {
    testWidgets('a host whose status is $what is still a host', (t) async {
      await t.runAsync(() async {
        h.status = answer;
        final s = store();
        await s.start();
        expect([s.phase, s.problem, s.damaged], [HostPhase.connected, null, null]);
        expect(await s.create('T').timeout(const Duration(seconds: 1)), 'new');
        await Future<void>.delayed(const Duration(milliseconds: 400));
        expect([h.created, h.connections, s.phase], [1, 1, HostPhase.connected]);
        s.dispose();
      });
    });
  }

  testWidgets('the list is asked for again by itself', (t) async {
    await t.runAsync(() async {
      final s = store(every: const Duration(milliseconds: 60));
      await s.start();
      await Future<void>.delayed(const Duration(milliseconds: 400));
      expect(h.listed.length, greaterThan(2));
      s.dispose();
    });
  });

  testWidgets('a host that is silent is asked again on another connection', (t) async {
    await t.runAsync(() async {
      final s = store(
        every: const Duration(milliseconds: 60),
        patience: const Duration(milliseconds: 200),
      );
      await s.start();
      final phases = <HostPhase>{};
      s.addListener(() => phases.add(s.phase));
      h.dead = true;
      final end = DateTime.now().add(const Duration(seconds: 10));
      while (!phases.contains(HostPhase.unreachable) && DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect(phases, contains(HostPhase.unreachable));
      expect(s.problem, contains('did not answer'));
      h.dead = false;
      while (s.phase != HostPhase.connected && DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect([s.phase, h.connections > 1], [HostPhase.connected, true]);
      s.dispose();
    });
  });

  testWidgets('what arrived is kept when the connection is lost', (t) async {
    await t.runAsync(() async {
      h.onFollow = (_) => [
        events([
          event(1, {'type': 'session.created', 'title': 'Kept', 'workspace': null}),
        ]),
      ];
      final s = store();
      await s.start();
      await s.open('x');
      // Nothing is drawn, so nothing has been applied by a frame.
      while (s.waiting == 0 && s.current!.seq == 0) {
        await Future<void>.delayed(const Duration(milliseconds: 1));
      }
      expect(s.current!.seq, 0);
      h.last!.destroy();
      while (s.phase == HostPhase.connected) {
        await Future<void>.delayed(const Duration(milliseconds: 1));
      }
      expect([s.current!.title, s.current!.seq], ['Kept', 1]);
      s.dispose();
    });
  });

  testWidgets('trying a failed session again begins it anew', (t) async {
    await t.runAsync(() async {
      h.onFollow = (s) => [
        events([
          event(1, {'type': 'session.created', 'title': 'T', 'workspace': null}),
        ]),
        {'session': s, 'fault': 'journal: damaged'},
      ];
      final s = store();
      await s.start();
      await s.open('x');
      while (s.current!.fault == null) {
        await Future<void>.delayed(const Duration(milliseconds: 1));
      }
      expect(s.waiting, 1, reason: 'the event still waits');
      h.onFollow = (_) => const [];
      h.asked.clear();
      await s.open('x');
      expect([s.waiting, s.current!.fault, s.current!.seq], [0, null, 0]);
      expect(h.asked, ['session.unfollow', 'session.follow']);
      s.dispose();
    });
  });

  testWidgets('much text is applied in slices', (t) async {
    await t.runAsync(() async {
      final m = Counted('x');
      h.onFollow = (_) => [
        events([
          event(1, {'type': 'session.created', 'title': 'T', 'workspace': null}),
          for (var i = 2; i < 34; i++)
            event(i, {'type': 'item.delta', 'item': 'none', 'text': 'x' * 48000}),
        ]),
      ];
      final s = store();
      await s.start();
      s.current = m;
      await s.open('x');
      final end = DateTime.now().add(const Duration(seconds: 5));
      while (m.seq < 33 && DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      expect(m.seq, 33);
      expect(m.applied, greaterThan(12));
      s.dispose();
    });
  });

  test('what needs a person is asked for when it has changed, once at a time', () async {
    final st = store();
    addTearDown(st.dispose);
    Json waits(String q) => {
      'session': 's',
      'title': 'T',
      'agent': 'a',
      'name': 'Lead',
      'request': q,
      'what': 'asks',
      'subject': 'x',
      'at': 1,
    };
    h.needs = [waits('q1')];
    await st.start();
    Future<void> until(int n) async {
      for (var i = 0; i < 200 && st.needs.length != n; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 20));
      }
      expect(st.needs.length, n);
    }

    await until(1);
    expect(st.needs.single.request, 'q1');
    h.needs = [waits('q1'), waits('q2')];
    for (var i = 0; i < 50; i++) {
      h.last!.write('${json.encode({'waiting': 2})}\n');
    }
    await until(2);
    await Future<void>.delayed(const Duration(milliseconds: 200));
    final asked = h.asked.where((m) => m == 'host.waiting').length;
    expect(asked, lessThan(52), reason: 'never two at once');
    expect(st.phase, HostPhase.connected);
  });

  test('closing with data in flight raises nothing', () async {
    Object? raised;
    await runZonedGuarded(() async {
      h.onFollow = (s) => [
        for (var i = 1; i < 4000; i++)
          {
            'session': s,
            'events': [
              {
                'seq': i,
                'at': 1,
                'body': {'type': 'item.delta', 'item': 'i', 'text': 'x' * 200},
              },
            ],
          },
      ];
      for (var k = 0; k < 10; k++) {
        final l = await HostLink.connect(endpoint: '${h.dir.path}/run/s', token: 't');
        l.messages.listen((_) {});
        unawaited(l.follow('x', 0).catchError((_) {}));
        await Future<void>.delayed(Duration(milliseconds: k));
        l.close();
      }
      await Future<void>.delayed(const Duration(milliseconds: 300));
    }, (e, _) => raised = e);
    expect(raised, isNull);
  });

  test('what is not consumed stops the reading, and consuming resumes it', () async {
    h.onFollow = (s) => [
      for (var b = 0; b < 100; b++)
        {
          'session': s,
          'events': [
            for (var i = 0; i < 400; i++)
              {
                'seq': b * 400 + i + 1,
                'at': 1,
                'body': {'type': 'item.delta', 'item': 'i', 'text': 'word '},
              },
          ],
        },
    ];
    final l = await HostLink.connect(endpoint: '${h.dir.path}/run/s', token: 't');
    var got = 0, owed = 0;
    var hold = true;
    l.messages.listen((m) {
      if (m is! Events) return;
      got += m.events.length;
      hold ? owed += m.events.length : l.consumed(m.events.length);
    });
    await l.follow('x', 0);
    await Future<void>.delayed(const Duration(milliseconds: 500));
    expect(got, lessThan(HostLink.highWater * 2));
    hold = false;
    l.consumed(owed);
    for (var i = 0; i < 100 && got < 40000; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 30));
    }
    expect(got, 40000);
    l.close();
  });

  testWidgets('a relayed host is as any other', (t) async {
    await t.runAsync(() async {
      final r = await FakeHost.start(relayed: true);
      r.onFollow = (s) => [
        events([
          for (var i = 1; i < 400; i++)
            event(i, {'type': 'item.delta', 'item': 'none', 'text': 'x' * 2000}),
        ]),
        {'session': s, 'fault': 'the end'},
      ];
      final s = HostStore(paths: HostPaths(r.dir.path), binary: null, spawn: false);
      await s.start();
      expect(s.phase, HostPhase.connected);
      await s.open('x');
      final end = DateTime.now().add(const Duration(seconds: 10));
      while (s.current!.seq < 399 && DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 10));
      }
      expect([s.current!.seq, s.current!.fault], [399, 'the end']);
      r.last!.destroy();
      while (s.phase == HostPhase.connected && DateTime.now().isBefore(end)) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      expect(s.phase, HostPhase.unreachable);
      s.dispose();
      await r.stop();
    });
  });
}
