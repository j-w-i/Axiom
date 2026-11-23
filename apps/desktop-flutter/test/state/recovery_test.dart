// Unsaved text kept on disk (AUD-001).

import 'dart:io';

import 'package:axiom_desktop/state/recovery.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('kept text is listed for its own root and forgotten on request', () async {
    final dir = Directory.systemTemp.createTempSync('axiom-recovery-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final r = RecoveryStore(dir.path, delay: const Duration(milliseconds: 10));
    var text = 'first';
    r.keep('/a', 'src/x.rs', () => text, 'v1');
    text = 'second'; // read when written, so a burst writes the latest text once
    await r.flush();
    expect(r.entries('/b'), isEmpty);
    final e = r.entries('/a').single;
    expect((e.path, e.text, e.savedVersion), ('src/x.rs', 'second', 'v1'));
    r.forget('/a', 'src/x.rs');
    expect(r.entries('/a'), isEmpty);
  });

  test('forgetting a path while its text is being written: the entry does not come back', () async {
    // Review N10: the write was under way (awaiting the file system) when the document was
    // saved; it finished after forget() and left an entry the next start offered back.
    final dir = Directory.systemTemp.createTempSync('axiom-recovery-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final r = RecoveryStore(dir.path, delay: const Duration(hours: 1));
    r.keep('/a', 'x.txt', () => 'unsaved', 'v1');
    final writing = r.flush(); // the write starts and yields at its first await
    r.forget('/a', 'x.txt'); // saved meanwhile
    await writing;
    expect(r.entries('/a'), isEmpty);
    // Kept again after that: written as usual.
    r.keep('/a', 'x.txt', () => 'again', 'v2');
    await r.flush();
    expect(r.entries('/a').single.text, 'again');
  });

  test('flush waits for a write a timer started', () async {
    final dir = Directory.systemTemp.createTempSync('axiom-recovery-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final r = RecoveryStore(dir.path, delay: Duration.zero);
    r.keep('/a', 'x.txt', () => 'unsaved', 'v1');
    await Future<void>.delayed(Duration.zero); // the timer fires; its write is under way
    await r.flush();
    expect(r.entries('/a').single.text, 'unsaved');
  });

  test('fnv64 is stable, unsigned hex', () {
    expect(fnv64(''), 'cbf29ce484222325');
    expect(fnv64('a'), 'af63dc4c8601ec8c');
    expect(fnv64('/home/x'), matches(RegExp(r'^[0-9a-f]{16}$')));
  });

  test('a store without a directory keeps nothing', () async {
    final r = RecoveryStore(null);
    r.keep('/a', 'x', () => 't', 'v');
    await r.flush();
    expect(r.entries('/a'), isEmpty);
  });
}
