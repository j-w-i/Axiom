// Recovery and save edge cases in the documents store against the real daemon, headless (no
// window): the independent review of AUD-REM-001 (audits/comprehensive-audit/REVIEW.md, N10, and
// the follow-up review's C4, D3 and D6, from its probe rev3_probe_test.dart Q1 and Q2).
import 'dart:io';

import 'package:axiom_desktop/daemon/client.dart';
import 'package:axiom_desktop/daemon/protocol.g.dart';
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

  /// The daemon's watcher reports changes in the folder: a file written now is reported (rather
  /// than sleep and hope it has started; review D6).
  Future<void> watcherOn() async {
    var seen = false;
    final off = client.on(Notifications.fsDidChange, (p) {
      if (p.changes.any((c) => c.path == 'watch-probe.txt' || c.path.isEmpty)) seen = true;
    });
    try {
      // Written again until it is seen: a change made before the watcher started is not reported.
      final probe = File('${a.path}/watch-probe.txt');
      final sw = Stopwatch()..start();
      for (var i = 0; !seen; i++) {
        if (sw.elapsedMilliseconds > 10000) fail('timed out: the watcher reports a change');
        probe.writeAsStringSync('$i');
        await Future<void>.delayed(const Duration(milliseconds: 50));
      }
    } finally {
      off();
    }
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
    await recovery.flush(); // no write of its text is still under way
    expect(recovery.entries(root), isEmpty);
  });

  test('a document deleted on disk and back when the daemon returns is not "deleted"', () async {
    File('${a.path}/x.txt').writeAsStringSync('base\n');
    await ws.openFolder(a.path);
    final d = await openDoc('x.txt');
    type(d, 'mine ');
    await watcherOn();
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

  test('C4: recovered text whose file another program has created since: Save asks "changed", '
      'not "deleted", and does not overwrite', () async {
    final root = await Directory(a.path).resolveSymbolicLinks();
    recovery.keep(root, 'gone.txt', () => 'mine\n', 'deadbeef');
    await recovery.flush();
    await ws.openFolder(a.path);
    await until(() => docs.doc('gone.txt') != null, 'the gone file offered');
    final d = docs.doc('gone.txt')!;
    expect(d.deletedOnDisk, isTrue);
    // Another program writes the file; Save comes before the watcher reports it.
    File('${a.path}/gone.txt').writeAsStringSync('theirs\n');
    await docs.save('gone.txt');
    expect(File('${a.path}/gone.txt').readAsStringSync(), 'theirs\n');
    expect(docs.conflictName, 'gone.txt');
    expect(
      docs.conflictIsDeleted,
      isFalse,
      reason: 'the file is there: the "deleted" question\'s Save would replace it unasked',
    );
    expect(d.deletedOnDisk, isFalse);
    // "Changed on disk" offers Reload, which takes their file.
    await docs.resolveConflict('reload');
    expect(d.text, 'theirs\n');
    expect(d.dirty, isFalse);
    expect(File('${a.path}/gone.txt').readAsStringSync(), 'theirs\n');
  });

  test(
    'D3: recovered text whose folder is gone too: Save creates the folder and the file',
    () async {
      final root = await Directory(a.path).resolveSymbolicLinks();
      recovery.keep(root, 'sub/deeper/gone.txt', () => 'my text\n', 'deadbeef');
      await recovery.flush();
      await ws.openFolder(a.path);
      await until(() => docs.doc('sub/deeper/gone.txt') != null, 'the gone file offered');
      final d = docs.doc('sub/deeper/gone.txt')!;
      expect(d.deletedOnDisk, isTrue);
      expect(docs.banner, contains('gone.txt is no longer on disk: Save creates it again'));
      await docs.save('sub/deeper/gone.txt');
      expect(docs.conflictName, isNull, reason: 'Save asked "deleted on disk" again');
      expect(File('${a.path}/sub/deeper/gone.txt').readAsStringSync(), 'my text\n');
      expect(d.dirty, isFalse);
      expect(d.deletedOnDisk, isFalse);
      await recovery.flush();
      expect(recovery.entries(root), isEmpty);
    },
  );

  test('D3: an open document whose folder is deleted: "deleted" Save creates both again', () async {
    Directory('${a.path}/sub').createSync();
    File('${a.path}/sub/x.txt').writeAsStringSync('base\n');
    await ws.openFolder(a.path);
    final d = await openDoc('sub/x.txt');
    type(d, 'mine ');
    await watcherOn();
    Directory('${a.path}/sub').deleteSync(recursive: true);
    await until(() => d.deletedOnDisk, 'marked deleted');
    await docs.save('sub/x.txt');
    if (docs.conflictName != null) {
      expect(docs.conflictIsDeleted, isTrue);
      await docs.resolveConflict('overwrite'); // the "deleted on disk" question's Save
    }
    expect(docs.conflictName, isNull, reason: 'the question came back: the folder is missing');
    expect(File('${a.path}/sub/x.txt').readAsStringSync(), 'mine base\n');
    expect(d.dirty, isFalse);
  });

  test('D6: an empty recovered text whose file is gone is forgotten, not kept for ever', () async {
    final root = await Directory(a.path).resolveSymbolicLinks();
    recovery.keep(root, 'empty.txt', () => '', 'deadbeef');
    recovery.keep(root, 'gone.txt', () => 'kept\n', 'deadbeef');
    await recovery.flush();
    expect(recovery.entries(root), hasLength(2));
    await ws.openFolder(a.path);
    await until(() => docs.doc('gone.txt') != null, 'the other text offered');
    await until(
      () => recovery.entries(root).every((e) => e.path != 'empty.txt'),
      'the empty entry forgotten',
    );
    expect(docs.doc('empty.txt'), isNull);
    expect(recovery.entries(root).map((e) => e.path), ['gone.txt']);
  });

  test(
    'D6: recovered text over a file that cannot be opened is shown; Save asks, Close forgets it',
    () async {
      final bytes = [0xff, 0xfe, 0x00, 0x80, 0x0a];
      File('${a.path}/bin.dat').writeAsBytesSync(bytes); // not UTF-8 (any more)
      final root = await Directory(a.path).resolveSymbolicLinks();
      recovery.keep(root, 'bin.dat', () => 'my text\n', 'deadbeef');
      await recovery.flush();
      await ws.openFolder(a.path);
      await until(() => docs.doc('bin.dat') != null, 'the text offered');
      final d = docs.doc('bin.dat')!;
      expect(d.text, 'my text\n');
      expect(d.dirty, isTrue);
      expect(d.deletedOnDisk, isFalse);
      expect(docs.banner, contains('bin.dat cannot be opened (it is not a UTF-8 text file)'));
      // Nothing was read of the file: Save asks before it replaces it.
      await docs.save('bin.dat');
      expect(docs.conflictName, 'bin.dat');
      expect(docs.conflictIsDeleted, isFalse);
      expect(File('${a.path}/bin.dat').readAsBytesSync(), bytes);
      // Reload cannot read it: it says why, and the text stays.
      await docs.resolveConflict('reload');
      expect(docs.banner, 'Cannot open bin.dat: it is not a UTF-8 text file');
      expect(d.text, 'my text\n');
      expect(d.dirty, isTrue);
      // Closing it without saving forgets the text.
      docs.close('bin.dat');
      expect(docs.prompt?.kind, 'close');
      await docs.resolvePrompt('discard');
      expect(docs.doc('bin.dat'), isNull);
      await recovery.flush();
      expect(recovery.entries(root), isEmpty);
      expect(File('${a.path}/bin.dat').readAsBytesSync(), bytes);
    },
  );
}
