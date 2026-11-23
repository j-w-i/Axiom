// Data safety in the documents store against the real daemon, headless (no window): the
// independent review of AUD-REM-001 (audits/comprehensive-audit/REVIEW.md, B1, B2, B3, B6a),
// from its probes P1-P7. P4 and P5 passed before the fixes and guard what they kept.
import 'dart:async';
import 'dart:io';

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/state/documents.dart';
import 'package:axiom_desktop/state/recovery.dart';
import 'package:axiom_desktop/state/workspace.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:re_editor/re_editor.dart';

String daemonPath() {
  final env = Platform.environment['AXIOMD_UNDER_TEST'];
  if (env != null && env.isNotEmpty) return env;
  final p = '${Directory.current.parent.parent.path}/target/debug/axiomd';
  if (!File(p).existsSync()) fail('axiomd not found at $p: run `cargo build -p axiomd`');
  return p;
}

final daemon = daemonPath();

Future<void> until(bool Function() ok, String what, {int ms = 10000}) async {
  final sw = Stopwatch()..start();
  while (!ok()) {
    if (sw.elapsedMilliseconds > ms) fail('timed out: $what');
    await Future<void>.delayed(const Duration(milliseconds: 20));
  }
}

void type(Doc d, String s) {
  d.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
  for (final ch in s.split('')) {
    d.controller.replaceSelection(ch);
  }
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory data, a, b, rec;
  late DaemonClient client;
  late WorkspaceStore ws;
  late DocumentsStore docs;
  late RecoveryStore recovery;

  setUp(() async {
    data = Directory.systemTemp.createTempSync('rv-data-');
    a = Directory.systemTemp.createTempSync('rv-a-');
    b = Directory.systemTemp.createTempSync('rv-b-');
    rec = Directory.systemTemp.createTempSync('rv-rec-');
    client = DaemonClient(DaemonConfig(program: daemon, args: ['--data-dir', data.path]));
    expect(await client.start(), isA<DaemonReady>());
    ws = WorkspaceStore(client);
    recovery = RecoveryStore(rec.path, delay: const Duration(milliseconds: 50));
    docs = DocumentsStore(client, ws, recovery: recovery);
  });
  tearDown(() async {
    await client.shutdown();
    for (final d in [data, a, b, rec]) {
      d.deleteSync(recursive: true);
    }
  });

  test('B1: reloading a clean document leaves no recovery entry', () async {
    File('${a.path}/x.txt').writeAsStringSync('one\n');
    await ws.openFolder(a.path);
    await docs.open('x.txt');
    await until(() => docs.doc('x.txt') != null, 'open');
    await Future<void>.delayed(const Duration(milliseconds: 500));
    File('${a.path}/x.txt').writeAsStringSync('two\n');
    await until(() => docs.doc('x.txt')!.text == 'two\n', 'reloaded');
    await Future<void>.delayed(const Duration(milliseconds: 300));
    final d = docs.doc('x.txt')!;
    expect(d.dirty, isFalse);
    expect(recovery.entries(ws.root), isEmpty, reason: 'clean document has a recovery entry');
  });

  test('B2: recovered text over a newer file asks before it is saved', () async {
    File('${a.path}/x.txt').writeAsStringSync('newer on disk\n');
    final root = (await Directory(a.path).resolveSymbolicLinks());
    // An entry as a crashed session would have left it (savedVersion of an older disk text).
    recovery.keep(root, 'x.txt', () => 'my stale edit\n', 'deadbeef');
    await recovery.flush();
    await ws.openFolder(a.path);
    await until(() => docs.doc('x.txt')?.dirty ?? false, 'recovered');
    final d = docs.doc('x.txt')!;
    expect(d.text, 'my stale edit\n');
    expect(d.changedOnDisk, isTrue);
    await docs.save('x.txt');
    expect(docs.conflictName, isNotNull, reason: 'newer disk text overwritten without asking');
    expect(File('${a.path}/x.txt').readAsStringSync(), 'newer on disk\n');
  });

  test('B3: a save held up by format-on-save does not write into the next folder', () async {
    File('${a.path}/README.md').writeAsStringSync('# same\n');
    File('${b.path}/README.md').writeAsStringSync('# same\n');
    await ws.openFolder(a.path);
    await docs.open('README.md');
    await until(() => docs.doc('README.md') != null, 'open A');
    type(docs.doc('README.md')!, 'A: ');
    final gate = Completer<void>();
    docs.beforeSave = (_) => gate.future; // a slow formatter
    final saving = docs.save('README.md');
    docs.requestOpenFolder(b.path);
    expect(docs.prompt, isNotNull);
    await docs.resolvePrompt('discard');
    await until(() => ws.root.isNotEmpty && ws.rootLoaded && ws.root != '' && !ws.opening, 'B');
    gate.complete();
    await saving;
    final bText = File('${b.path}/README.md').readAsStringSync();
    expect(bText, '# same\n', reason: "A's text written into B");
  });

  test('two saves with an edit between: the newer text is on disk, no conflict', () async {
    File('${a.path}/x.txt').writeAsStringSync('base\n');
    await ws.openFolder(a.path);
    await docs.open('x.txt');
    await until(() => docs.doc('x.txt') != null, 'open');
    final d = docs.doc('x.txt')!;
    type(d, '1');
    final s1 = docs.save('x.txt');
    type(d, '2');
    final s2 = docs.save('x.txt');
    await Future.wait([s1, s2]);
    expect(File('${a.path}/x.txt').readAsStringSync(), '21base\n');
    expect(docs.conflictName, isNull);
    expect(d.dirty, isFalse);
  });

  test(
    'daemon killed, file changed meanwhile, restart: the document is marked and Save asks',
    () async {
      File('${a.path}/x.txt').writeAsStringSync('base\n');
      await ws.openFolder(a.path);
      await docs.open('x.txt');
      await until(() => docs.doc('x.txt') != null, 'open');
      final d = docs.doc('x.txt')!;
      type(d, 'mine ');
      Process.killPid(client.pid!, ProcessSignal.sigkill);
      await until(() => d.detached, 'detached');
      File('${a.path}/x.txt').writeAsStringSync('theirs\n');
      expect(await client.restart(), isA<DaemonReady>());
      await ws.openFolder(ws.lostRoot);
      await until(() => !d.detached, 'attached');
      expect(d.text, 'mine base\n');
      expect(d.changedOnDisk, isTrue);
      await docs.save('x.txt');
      expect(docs.conflictName, 'x.txt');
      expect(File('${a.path}/x.txt').readAsStringSync(), 'theirs\n');
    },
  );

  test('B1: a reload before quitting does not revert a later change on the next start', () async {
    File('${a.path}/x.txt').writeAsStringSync('one\n');
    await ws.openFolder(a.path);
    await docs.open('x.txt');
    await until(() => docs.doc('x.txt') != null, 'open');
    await Future<void>.delayed(const Duration(milliseconds: 500));
    File('${a.path}/x.txt').writeAsStringSync('two\n'); // e.g. git pull while open
    await until(() => docs.doc('x.txt')!.text == 'two\n', 'reloaded');
    await recovery.flush();
    await docs.saveTabState();
    await client.shutdown(); // a normal quit
    File('${a.path}/x.txt').writeAsStringSync('three\n'); // e.g. git checkout while closed
    client = DaemonClient(DaemonConfig(program: daemon, args: ['--data-dir', data.path]));
    expect(await client.start(), isA<DaemonReady>());
    ws = WorkspaceStore(client);
    docs = DocumentsStore(client, ws, recovery: RecoveryStore(rec.path));
    await ws.openFolder(a.path);
    await until(() => docs.doc('x.txt') != null, 'reopened');
    await Future<void>.delayed(const Duration(milliseconds: 500));
    final d = docs.doc('x.txt')!;
    expect(d.text, 'three\n');
  });

  test("B6a: mixed line breaks: adding a line keeps every other line's break", () {
    const text = 'one\r\ntwo\nthree\r\n';
    final d = Doc(path: 'm.txt', text: text, savedVersion: 'v');
    d.controller.selection = const CodeLineSelection.collapsed(index: 3, offset: 0);
    d.controller.replaceSelection('four');
    d.controller.selection = const CodeLineSelection.collapsed(index: 3, offset: 4);
    d.controller.replaceSelection('\n');
    final disk = d.diskText(d.text);
    expect(disk.startsWith('one\r\ntwo\nthree\r\n'), isTrue);
  });
}
