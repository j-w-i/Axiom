// shell-local.json survives overlapping flushes and a damaged file (AUD-089).

import 'dart:io';

import 'package:axiom_desktop/app/local_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('two flushes in flight leave the later values, valid', () async {
    final dir = Directory.systemTemp.createTempSync('axiom-prefs-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final p = await LocalPrefs.load(dir.path);
    p.set('long', 'x' * 5000);
    final a = p.flush();
    p.set('long', null);
    p.set('short', 1.0);
    final b = p.flush();
    await Future.wait([a, b]);
    final again = await LocalPrefs.load(dir.path);
    expect(again.get<String>('long'), isNull);
    expect(again.number('short', 0), 1.0);
    p.dispose();
  });

  test('a damaged file falls back to the backup', () async {
    final dir = Directory.systemTemp.createTempSync('axiom-prefs-');
    addTearDown(() => dir.deleteSync(recursive: true));
    final p = await LocalPrefs.load(dir.path);
    p.set('side.width', 300.0);
    await p.flush();
    p.set('side.width', 310.0);
    await p.flush(); // the 300 file is now the backup
    File('${dir.path}/shell-local.json').writeAsStringSync('{"side.wid');
    final again = await LocalPrefs.load(dir.path);
    expect(again.number('side.width', 0), 300.0);
    p.dispose();
  });
}
