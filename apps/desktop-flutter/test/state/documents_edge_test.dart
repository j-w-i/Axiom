// Recovery and save edge cases in the documents store against the real daemon, headless (no
// window): the independent review of AUD-REM-001 (audits/comprehensive-audit/REVIEW.md, N10).
import 'dart:io';

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/state/documents.dart';
import 'package:axiom_desktop/state/recovery.dart';
import 'package:axiom_desktop/state/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

import 'documents_safety_test.dart' show daemon, type, until;

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late Directory data, a, rec;
  late DaemonClient client;
  late WorkspaceStore ws;
  late DocumentsStore docs;
  late RecoveryStore recovery;

  setUp(() async {
    data = Directory.systemTemp.createTempSync('rv-data-');
    a = Directory.systemTemp.createTempSync('rv-a-');
    rec = Directory.systemTemp.createTempSync('rv-rec-');
    client = DaemonClient(DaemonConfig(program: daemon, args: ['--data-dir', data.path]));
    expect(await client.start(), isA<DaemonReady>());
    ws = WorkspaceStore(client);
    recovery = RecoveryStore(rec.path, delay: const Duration(milliseconds: 50));
    docs = DocumentsStore(client, ws, recovery: recovery);
  });
  tearDown(() async {
    await client.shutdown();
    for (final d in [data, a, rec]) {
      d.deleteSync(recursive: true);
    }
  });

  Future<Doc> openDoc(String path) async {
    await docs.open(path);
    await until(() => docs.doc(path) != null, 'open $path');
    return docs.doc(path)!;
  }

  Future<void> killAndRestart(Doc d, {void Function()? whileDown}) async {
    Process.killPid(client.pid!, ProcessSignal.sigkill);
    await until(() => d.detached, 'detached');
    whileDown?.call();
    expect(await client.restart(), isA<DaemonReady>());
    await ws.openFolder(ws.lostRoot);
    await until(() => !d.detached, 'attached');
  }

  test('recovered text whose file is gone opens as an unsaved document; Save creates it', () async {
    File('${a.path}/kept.txt').writeAsStringSync('kept\n');
    final root = await Directory(a.path).resolveSymbolicLinks();
    // An entry as a crashed session would have left it; the file was deleted since.
    recovery.keep(root, 'gone.txt', () => 'my unsaved text\n', 'deadbeef');
    await recovery.flush();
    await ws.openFolder(a.path);
    await until(() => docs.doc('gone.txt') != null, 'the gone file offered');
    final d = docs.doc('gone.txt')!;
    expect(d.text, 'my unsaved text\n');
    expect(d.dirty, isTrue);
    expect(d.deletedOnDisk, isTrue);
    expect(d.readOnly, isFalse);
    expect(docs.banner, contains('gone.txt is no longer on disk'));
    // The text is still kept while it is unsaved.
    expect(recovery.entries(root).map((e) => e.text), ['my unsaved text\n']);
    await docs.save('gone.txt');
    expect(File('${a.path}/gone.txt').readAsStringSync(), 'my unsaved text\n');
    expect(docs.conflictName, isNull);
    expect(d.dirty, isFalse);
    expect(d.deletedOnDisk, isFalse);
    await Future<void>.delayed(const Duration(milliseconds: 200));
    expect(recovery.entries(root), isEmpty);
  });

  test('a document deleted on disk and back when the daemon returns is not "deleted"', () async {
    File('${a.path}/x.txt').writeAsStringSync('base\n');
    await ws.openFolder(a.path);
    final d = await openDoc('x.txt');
    type(d, 'mine ');
    await Future<void>.delayed(const Duration(milliseconds: 300)); // the watcher is on
    File('${a.path}/x.txt').deleteSync();
    await until(() => d.deletedOnDisk, 'marked deleted');
    await killAndRestart(
      d,
      whileDown: () => File('${a.path}/x.txt').writeAsStringSync('base\n'), // e.g. git checkout
    );
    expect(d.text, 'mine base\n');
    expect(d.deletedOnDisk, isFalse, reason: 'the file is there again');
    expect(d.changedOnDisk, isFalse, reason: 'it is the text the edit was based on');
    await docs.save('x.txt');
    expect(docs.conflictName, isNull);
    expect(File('${a.path}/x.txt').readAsStringSync(), 'mine base\n');
  });

  test(
    'two queued saves, the first conflicts: one question, and Overwrite writes the newer text',
    () async {
      File('${a.path}/x.txt').writeAsStringSync('base\n');
      await ws.openFolder(a.path);
      final d = await openDoc('x.txt');
      type(d, 'A');
      File('${a.path}/x.txt').writeAsStringSync('theirs\n');
      final s1 = docs.save('x.txt');
      type(d, 'B');
      final s2 = docs.save('x.txt');
      await Future.wait([s1, s2]);
      expect(docs.conflictName, 'x.txt');
      await docs.resolveConflict('overwrite');
      expect(docs.conflictName, isNull, reason: 'the second save asked again');
      expect(File('${a.path}/x.txt').readAsStringSync(), 'BAbase\n');
      expect(d.dirty, isFalse);
    },
  );

  test('"Save" in the quit question while the backend is stopped says why it cannot', () async {
    File('${a.path}/x.txt').writeAsStringSync('base\n');
    await ws.openFolder(a.path);
    final d = await openDoc('x.txt');
    type(d, 'mine ');
    var quit = false;
    docs.onQuitConfirmed = () => quit = true;
    Process.killPid(client.pid!, ProcessSignal.sigkill);
    await until(() => d.detached, 'detached');
    docs.requestQuit();
    expect(docs.prompt?.kind, 'quit');
    await docs.resolvePrompt('save');
    expect(quit, isFalse);
    expect(docs.banner, startsWith('Could not save x.txt: the backend is stopped'));
    expect(d.text, 'mine base\n');
    expect(d.dirty, isTrue);
    // Back: the line goes and Save works.
    expect(await client.restart(), isA<DaemonReady>());
    await ws.openFolder(ws.lostRoot);
    await until(() => !d.detached, 'attached');
    expect(docs.banner, isEmpty);
    await docs.save('x.txt');
    expect(File('${a.path}/x.txt').readAsStringSync(), 'mine base\n');
  });
}
