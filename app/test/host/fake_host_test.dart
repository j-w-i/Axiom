import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:axiom/host/proto.dart';
import 'package:flutter_test/flutter_test.dart';

/// A host that misbehaves on purpose. The real one drives e2e_test.dart.
class FakeHost {
  FakeHost._(this.dir, this.server);
  final Directory dir;
  final ServerSocket server;
  int follows = 0;
  List<Object> sessions = [];
  Json status = {};
  final listed = <Object?>[];

  /// What to write when a session is followed, after its answer; null answers nothing at all.
  Iterable<Object>? Function(String session) onFollow = (_) => const [];

  static Future<FakeHost> start() async {
    final dir = Directory.systemTemp.createTempSync('af');
    Directory('${dir.path}/run').createSync();
    final path = '${dir.path}/run/s';
    final server = await ServerSocket.bind(
      InternetAddress(path, type: InternetAddressType.unix),
      0,
    );
    File('${dir.path}/run/endpoint').writeAsStringSync(path);
    File('${dir.path}/run/token').writeAsStringSync('t');
    final h = FakeHost._(dir, server);
    server.listen((s) {
      s.done.ignore();
      s.cast<List<int>>().transform(utf8.decoder).transform(const LineSplitter()).listen((l) {
        final m = json.decode(l) as Json;
        final id = m['id'];
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
          case 'host.status':
            say({'id': id, 'result': h.status});
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

void main() {
  late FakeHost h;
  setUp(() async => h = await FakeHost.start());
  tearDown(() => h.stop());

  HostStore store() => HostStore(paths: HostPaths(h.dir.path), binary: null, spawn: false);

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
      h.status = {'damaged': 'row 3 is missing'};
      await s.refresh();
      expect([s.older, s.damaged], [true, 'row 3 is missing']);
      expect(h.listed.last, {'limit': HostStore.listed});
      s.dispose();
    });
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
}
