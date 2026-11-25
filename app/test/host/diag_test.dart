import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:axiom/host/host_store.dart';
import 'package:axiom/host/link.dart';
import 'package:flutter_test/flutter_test.dart';

void say(String s) {
  stderr.writeln('${DateTime.now().toIso8601String()} $s');
}

Future<(Directory, ServerSocket)> host({required bool resync}) async {
  final dir = Directory.systemTemp.createTempSync('af');
  Directory('${dir.path}/run').createSync();
  final path = '${dir.path}/run/s';
  final server = await ServerSocket.bind(InternetAddress(path, type: InternetAddressType.unix), 0);
  File('${dir.path}/run/endpoint').writeAsStringSync(path);
  File('${dir.path}/run/token').writeAsStringSync('t');
  server.listen((s) {
    say('accepted');
    s.done.then((_) => say('server socket done'), onError: (Object e) => say('done error $e'));
    s.cast<List<int>>().transform(utf8.decoder).transform(const LineSplitter()).listen((l) {
      final m = json.decode(l) as Map<String, Object?>;
      say('got ${m['method']}');
      final list = m['method'] == 'session.list';
      s.write('${json.encode({'id': m['id'], 'result': list ? const <Object?>[] : <String, Object?>{}})}\n');
      if (resync && m['method'] == 'session.follow') {
        s.write('${json.encode({'session': 'x', 'resync': 7})}\n');
      }
    }, onError: (Object e) => say('read error $e'), onDone: () => say('read done'));
  });
  return (dir, server);
}

void main() {
  for (final (name, resync, secs) in [('idle 5', false, 5), ('resync 1', true, 1), ('resync 5', true, 5)]) {
    test('store $name', () async {
      final (dir, server) = await host(resync: resync);
      final s = HostStore(paths: HostPaths(dir.path), binary: null, spawn: false);
      await s.start();
      say('started ${s.phase} ${s.problem}');
      await s.open('x').timeout(const Duration(seconds: 3), onTimeout: () => say('open timed out'));
      for (var i = 0; i < secs * 10; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      say('dispose');
      s.dispose();
      say('close');
      await server.close().timeout(const Duration(seconds: 3), onTimeout: () { say('close timed out'); return server; });
      say('delete');
      try {
        dir.deleteSync(recursive: true);
      } on Object catch (e) {
        say('delete: $e');
      }
      say('end');
    });
  }
  test('link idle 5', () async {
    final (dir, server) = await host(resync: false);
    final l = await HostLink.connect(endpoint: '${dir.path}/run/s', token: 't');
    say('connected');
    await Future<void>.delayed(const Duration(seconds: 5));
    say('close link');
    l.close();
    await server.close().timeout(const Duration(seconds: 3), onTimeout: () { say('close timed out'); return server; });
    say('delete');
    try {
      dir.deleteSync(recursive: true);
    } on Object catch (e) {
      say('delete: $e');
    }
    say('end');
  });
}
