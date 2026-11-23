// The daemon client against the real axiomd (FL-CORE-001). Builds nothing: run
// `cargo build -p axiomd` first (scripts/check.sh flutter does).

import 'dart:async';
import 'dart:io';

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:flutter_test/flutter_test.dart';

String daemonPath() {
  final env = Platform.environment['AXIOMD_UNDER_TEST'];
  if (env != null && env.isNotEmpty) return env;
  final p = '${Directory.current.parent.parent.path}/target/debug/axiomd';
  if (!File(p).existsSync()) {
    fail('axiomd not found at $p: run `cargo build -p axiomd`');
  }
  return p;
}

bool processAlive(int pid) => Process.runSync('kill', ['-0', '$pid']).exitCode == 0;

Future<void> eventually(
  bool Function() cond, {
  Duration within = const Duration(seconds: 10),
}) async {
  final end = DateTime.now().add(within);
  while (!cond()) {
    if (DateTime.now().isAfter(end)) fail('condition not met within $within');
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}

void main() {
  late Directory data;
  late Directory ws;
  late DaemonClient client;

  setUp(() async {
    data = Directory.systemTemp.createTempSync('axiom-fl-data-');
    ws = Directory.systemTemp.createTempSync('axiom-fl-ws-');
    File('${ws.path}/hello.txt').writeAsStringSync('hello\n');
    client = DaemonClient(DaemonConfig(program: daemonPath(), args: ['--data-dir', data.path]));
  });

  tearDown(() async {
    await client.shutdown();
    data.deleteSync(recursive: true);
    ws.deleteSync(recursive: true);
  });

  Future<String> open() async {
    final r = await client.call(Methods.workspaceOpen, OpenParams(path: ws.path));
    return r.workspaceId;
  }

  test('initialize: ready with the protocol version and the served methods', () async {
    final s = await client.start();
    expect(s, isA<DaemonReady>());
    final ready = s as DaemonReady;
    expect(ready.protocolVersion, protocolVersion);
    expect(ready.methods, containsAll(['fs/readFile', 'workspace/open', 'task/start']));
    expect(ready.serverVersion, isNotEmpty);
  });

  test(
    'a request returns its typed result; a daemon error is a DaemonException with its kind',
    () async {
      await client.start();
      final id = await open();
      final file = await client.call(
        Methods.fsReadFile,
        ReadFileParams(workspaceId: id, path: 'hello.txt'),
      );
      expect(file.text, 'hello\n');
      expect(file.version, isNotEmpty);
      await expectLater(
        client.call(Methods.fsReadFile, ReadFileParams(workspaceId: id, path: 'missing.txt')),
        throwsA(isA<DaemonException>().having((e) => e.kind, 'kind', ErrorKind.notFound)),
      );
    },
  );

  test('notifications arrive in wire order: the start result precedes the task output', () async {
    await client.start();
    final id = await open();
    final events = <String>[];
    final exited = Completer<ExitedParams>();
    client.on(
      Notifications.taskOutput,
      (p) => events.add('output:${p.stream.wire}:${p.data.trim()}'),
    );
    client.on(Notifications.taskExited, (p) {
      events.add('exited:${p.outcome.wire}');
      exited.complete(p);
    });
    // The task prints at once, so its output is on the wire right behind the start result.
    final started = await client.call(
      Methods.taskStart,
      StartParams(
        workspaceId: id,
        command: const CommandArgv(argv: ['sh', '-c', 'echo out; echo err >&2']),
      ),
    );
    events.add('started');
    final ex = await exited.future.timeout(const Duration(seconds: 10));
    expect(started.taskId, ex.taskId);
    expect(events.first, 'started');
    expect(events, containsAll(['output:stdout:out', 'output:stderr:err']));
    expect(events.last, 'exited:succeeded');
    expect(ex.exitCode, 0);
  });

  test(
    'cancelling a request sends \$/cancel; the daemon answers cancelled and stays usable',
    () async {
      await client.start();
      // Enough files that the search is still walking when the cancel arrives.
      for (var d = 0; d < 40; d++) {
        final dir = Directory('${ws.path}/d$d')..createSync();
        for (var f = 0; f < 500; f++) {
          File('${dir.path}/f$f.txt').writeAsStringSync('line one\nneedle $f\nline three\n' * 4);
        }
      }
      final id = await open();
      final token = CancelToken();
      final search = client.call(
        Methods.workspaceSearch,
        SearchParams(workspaceId: id, query: r'ne+dle \d+', regex: true, limit: 1000000),
        cancel: token,
      );
      token.cancel();
      await expectLater(
        search,
        throwsA(isA<DaemonException>().having((e) => e.kind, 'kind', ErrorKind.cancelled)),
      );
      final file = await client.call(
        Methods.fsReadFile,
        ReadFileParams(workspaceId: id, path: 'hello.txt'),
      );
      expect(file.text, 'hello\n');
    },
  );

  test('a request past its timeout fails with timeout', () async {
    final slow = DaemonClient(DaemonConfig(program: daemonPath(), args: ['--data-dir', data.path]));
    addTearDown(slow.shutdown);
    await slow.start();
    final id = (await slow.call(Methods.workspaceOpen, OpenParams(path: ws.path))).workspaceId;
    await expectLater(
      slow.call(
        Methods.taskStart,
        StartParams(
          workspaceId: id,
          command: const CommandArgv(argv: ['sh', '-c', 'true']),
        ),
        timeout: const Duration(microseconds: 1),
      ),
      throwsA(isA<DaemonException>().having((e) => e.kind, 'kind', ErrorKind.timeout)),
    );
  });

  test(
    'a crashed daemon fails the client with the reason; pending requests fail; restart recovers',
    () async {
      await client.start();
      final id = await open();
      final pid = client.pid!;
      // Paused first, so the request is certainly still pending when the daemon dies (under load
      // it could otherwise answer before the kill landed).
      Process.killPid(pid, ProcessSignal.sigstop);
      final pending = client.call(
        Methods.taskStart,
        StartParams(
          workspaceId: id,
          command: const CommandArgv(argv: ['sleep', '5']),
        ),
      );
      Process.killPid(pid, ProcessSignal.sigkill);
      await expectLater(
        pending,
        throwsA(isA<DaemonException>().having((e) => e.message, 'message', 'Backend stopped')),
      );
      await eventually(() => client.state.value is DaemonFailed);
      expect((client.state.value as DaemonFailed).message, contains('exited unexpectedly'));
      final s = await client.restart();
      expect(s, isA<DaemonReady>());
      expect(client.pid, isNot(pid));
      await open();
    },
  );

  test(
    'a request written to a daemon that just died fails; the broken pipe is not uncaught',
    () async {
      await client.start();
      final pid = client.pid!;
      Process.killPid(pid, ProcessSignal.sigkill);
      // Synchronously, so the client has not seen the exit yet and still writes to the pipe.
      final end = DateTime.now().add(const Duration(seconds: 5));
      while (processAlive(pid) && DateTime.now().isBefore(end)) {}
      expect(processAlive(pid), isFalse);
      final calls = [for (var i = 0; i < 20; i++) open()];
      for (final c in calls) {
        await expectLater(c, throwsA(isA<DaemonException>()));
      }
      await eventually(() => client.state.value is DaemonFailed);
      // Time for a late write error to surface as an uncaught one (it fails the test if it does).
      await Future<void>.delayed(const Duration(milliseconds: 300));
    },
  );

  test('shutdown ends the process within its bound and leaves the client stopped', () async {
    await client.start();
    final pid = client.pid!;
    final t = Stopwatch()..start();
    await client.shutdown();
    expect(t.elapsed, lessThan(const Duration(seconds: 6)));
    expect(client.state.value, isA<DaemonStopped>());
    await eventually(() => !processAlive(pid), within: const Duration(seconds: 2));
    await expectLater(
      client.call(Methods.workspaceOpen, OpenParams(path: ws.path)),
      throwsA(isA<DaemonException>()),
    );
  });

  test('a response and a notification in one read: the awaiting code runs first', () async {
    // A fake daemon that answers the second request and sends a notification in a single write,
    // so both lines reach the client in the same chunk.
    final dir = Directory.systemTemp.createTempSync('axiom-fl-fake-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final script = File('${dir.path}/fake-axiomd')
      ..writeAsStringSync(r"""#!/bin/sh
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"0.4","server":{"name":"fake","version":"0"},"methods":["workspace/recent"],"host":{"os":"linux","arch":"x86_64","pathSeparator":"/"}}}'
read -r line
printf '%s\n%s\n' '{"jsonrpc":"2.0","id":2,"result":{"workspaces":[]}}' '{"jsonrpc":"2.0","method":"$/log","params":{"level":"info","message":"after"}}'
read -r line
""");
    Process.runSync('chmod', ['+x', script.path]);
    final fake = DaemonClient(DaemonConfig(program: script.path));
    addTearDown(fake.shutdown);
    expect(await fake.start(), isA<DaemonReady>());
    final order = <String>[];
    fake.onRaw(r'$/log', (_) => order.add('notification'));
    await fake.callRaw('workspace/recent', const <String, Object?>{});
    order.add('response');
    await eventually(() => order.length == 2);
    expect(order, ['response', 'notification']);
  });

  test('200 concurrent requests are each answered once, with their own answer', () async {
    // Was crates/desktop-bridge's concurrent_requests_each_answered_once (AUD-102).
    await client.start();
    for (var i = 0; i < 200; i++) {
      File('${ws.path}/f$i.txt').writeAsStringSync('file $i\n');
    }
    final id = await open();
    final answers = List.filled(200, 0);
    final calls = [
      for (var i = 0; i < 200; i++)
        (i.isEven
                ? client
                      .call(Methods.fsReadFile, ReadFileParams(workspaceId: id, path: 'f$i.txt'))
                      .then((r) => r.text)
                : client.call(Methods.settingsGet, const GetParams()).then((_) => 'file $i\n'))
            .then((text) {
              answers[i]++;
              return text;
            }),
    ];
    final texts = await Future.wait(calls).timeout(const Duration(seconds: 20));
    for (var i = 0; i < 200; i++) {
      expect(texts[i], 'file $i\n', reason: "request $i got another request's answer");
    }
    // A second answer to any of them would complete a future twice (an uncaught error that
    // fails the test) or change a count.
    await Future<void>.delayed(const Duration(milliseconds: 200));
    expect(answers, everyElement(1));
  });

  test(r'a $/cancel racing a fast request: the answer or cancelled, exactly once', () async {
    // Was crates/desktop-bridge's cancel_of_a_forwarded_request_still_answers_once (AUD-102).
    await client.start();
    final outcomes = <Future<String>>[];
    final counts = List.filled(100, 0);
    for (var i = 0; i < 100; i++) {
      final token = CancelToken();
      outcomes.add(
        client
            .call(Methods.workspaceRecent, const Empty(), cancel: token)
            .then<String>(
              (_) => 'result',
              onError: (Object e) {
                expect(
                  e,
                  isA<DaemonException>().having((x) => x.kind, 'kind', ErrorKind.cancelled),
                );
                return 'cancelled';
              },
            )
            .whenComplete(() => counts[i]++),
      );
      token.cancel();
    }
    final got = await Future.wait(outcomes).timeout(const Duration(seconds: 10));
    expect(got, everyElement(anyOf('result', 'cancelled')));
    await Future<void>.delayed(const Duration(milliseconds: 200)); // time for a late answer
    expect(counts, everyElement(1));
    // The daemon is still usable after the race.
    expect(
      (await client.call(Methods.workspaceRecent, const Empty())).workspaces,
      isA<List<RecentWorkspace>>(),
    );
  });

  test("after a restart the old daemon's notifications are not dispatched (AUD-086)", () async {
    // A fake daemon: the first process, told to stop (its stdin closed), still sends a
    // notification on its way out; the second sends its own once it is initialized.
    final dir = Directory.systemTemp.createTempSync('axiom-fl-fake-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final script = File('${dir.path}/fake-axiomd')
      ..writeAsStringSync(r"""#!/bin/sh
count="$(dirname "$0")/count"
n=$(cat "$count" 2>/dev/null || echo 0); n=$((n+1)); echo $n > "$count"
read -r line
id=$(printf '%s' "$line" | sed 's/.*"id":\([0-9]*\).*/\1/')
printf '{"jsonrpc":"2.0","id":%s,"result":{"protocolVersion":"0.6","server":{"name":"fake","version":"0"},"methods":[],"host":{"os":"linux","arch":"x86_64","pathSeparator":"/"}}}\n' "$id"
if [ "$n" = 1 ]; then
  while read -r line; do :; done
  printf '%s\n' '{"jsonrpc":"2.0","method":"$/log","params":{"level":"info","message":"old"}}'
  sleep 0.3
else
  printf '%s\n' '{"jsonrpc":"2.0","method":"$/log","params":{"level":"info","message":"new"}}'
  while read -r line; do :; done
fi
""");
    Process.runSync('chmod', ['+x', script.path]);
    final fake = DaemonClient(
      DaemonConfig(program: script.path, shutdownAnswerWait: const Duration(milliseconds: 100)),
    );
    addTearDown(fake.shutdown);
    final heard = <String>[];
    fake.onRaw(r'$/log', (p) => heard.add((p! as Map)['message']! as String));
    expect(await fake.start(), isA<DaemonReady>());
    expect(await fake.restart(), isA<DaemonReady>());
    await eventually(() => heard.contains('new'));
    // The old process speaks after its stdin closes and lives 0.3 s more.
    await Future<void>.delayed(const Duration(milliseconds: 800));
    expect(File('${dir.path}/count').readAsStringSync().trim(), '2');
    expect(heard, ['new']);
  });

  test('an answer that does not decode is a DaemonException, not a TypeError (AUD-091)', () async {
    final dir = Directory.systemTemp.createTempSync('axiom-fl-fake-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final script = File('${dir.path}/fake-axiomd')
      ..writeAsStringSync(r"""#!/bin/sh
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"0.6","server":{"name":"fake","version":"0"},"methods":["fs/readFile"],"host":{"os":"linux","arch":"x86_64","pathSeparator":"/"}}}'
read -r line
printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"text":5,"version":null}}'
while read -r line; do :; done
""");
    Process.runSync('chmod', ['+x', script.path]);
    final fake = DaemonClient(
      DaemonConfig(program: script.path, shutdownAnswerWait: const Duration(milliseconds: 100)),
    );
    addTearDown(fake.shutdown);
    expect(await fake.start(), isA<DaemonReady>());
    await expectLater(
      fake.call(Methods.fsReadFile, ReadFileParams(workspaceId: 'w1', path: 'x')),
      throwsA(
        isA<DaemonException>()
            .having((e) => e.kind, 'kind', ErrorKind.internal)
            .having((e) => e.message, 'message', contains('not understood')),
      ),
    );
  });

  test('locateDaemon: explicit, then AXIOMD, then a sibling, then PATH', () {
    final dir = Directory.systemTemp.createTempSync('axiom-fl-locate-');
    addTearDown(() => dir.deleteSync(recursive: true));
    File mk(String sub) {
      final d = Directory('${dir.path}/$sub')..createSync(recursive: true);
      return File('${d.path}/axiomd')..writeAsStringSync('#!/bin/sh\n');
    }

    final explicit = mk('explicit');
    final env = mk('env');
    final sibling = mk('app');
    final onPath = mk('bin');
    final exe = '${sibling.parent.path}/axiom';
    expect(
      locateDaemon(explicit: explicit.path, environment: {'AXIOMD': env.path}, executable: exe),
      explicit.path,
    );
    expect(locateDaemon(environment: {'AXIOMD': env.path}, executable: exe), env.path);
    expect(locateDaemon(environment: {'PATH': onPath.parent.path}, executable: exe), sibling.path);
    expect(
      locateDaemon(environment: {'PATH': onPath.parent.path}, executable: '/nowhere/axiom'),
      onPath.path,
    );
    expect(locateDaemon(environment: const {}, executable: '/nowhere/axiom'), isNull);
  });
}
