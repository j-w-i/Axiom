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
