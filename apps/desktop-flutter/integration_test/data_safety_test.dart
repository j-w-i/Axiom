// Unsaved work is never lost silently (audit batch 1: AUD-001, AUD-002, AUD-019, AUD-024),
// against the real daemon: a daemon crash and "Restart the Backend" keep unsaved text; a crash of
// the shell itself leaves it in the recovery folder for the next start; switching folders drops
// the old folder's documents; a file deleted on disk is saved again rather than "reloaded"; two
// saves in a row are not a conflict.
// Run: apps/desktop-flutter/scripts/e2e.sh data_safety_test.dart

import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('unsaved work survives the daemon and the shell', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-safety-data-');
    final a = tempDir('axiom-safety-a-');
    final b = tempDir('axiom-safety-b-');
    writeFile(a, 'src/main.rs', 'fn main() {}\n');
    writeFile(a, 'README.md', '# A\n');
    writeFile(a, 'gone.txt', 'gone\n');
    writeFile(b, 'README.md', '# B\n');
    addTearDown(() {
      for (final d in [data, a, b]) {
        if (d.existsSync()) d.deleteSync(recursive: true);
      }
    });
    File disk(Directory root, String p) => File('${root.path}/$p');
    void typeAtStart(String path, String text, AppState app) {
      final d = app.documents.doc(path)!;
      d.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
      for (final ch in text.split('')) {
        d.controller.replaceSelection(ch);
      }
    }

    var app = await boot(tester, data.path);
    await app.workspace.openFolder(a.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'folder A');
    await app.documents.open('src/main.rs');
    await until(tester, () => app.documents.doc('src/main.rs') != null, what: 'main.rs');

    // 1. The daemon is killed with unsaved text: the document stays, read-only, then attaches
    // again after Restart the Backend with its text and dirty mark, and saves.
    typeAtStart('src/main.rs', '// one\n', app);
    await tester.pump(const Duration(milliseconds: 100));
    expect(app.documents.doc('src/main.rs')!.dirty, isTrue);
    Process.killPid(app.client.pid!, ProcessSignal.sigkill);
    await until(tester, () => !app.client.isReady, what: 'the daemon gone');
    await tester.pump(const Duration(milliseconds: 300));
    final kept = app.documents.doc('src/main.rs');
    expect(kept, isNotNull, reason: 'the document is kept while the daemon is gone');
    expect(kept!.readOnly, isTrue);
    expect(kept.text, startsWith('// one\n'));
    await tester.tap(named('Restart the Backend').first);
    await until(tester, () => app.client.isReady && app.workspace.rootLoaded, what: 'restarted');
    await until(tester, () => !app.documents.doc('src/main.rs')!.readOnly, what: 'attached');
    final back = app.documents.doc('src/main.rs')!;
    expect(back.text, startsWith('// one\n'));
    expect(back.dirty, isTrue);
    await app.documents.save('src/main.rs');
    await until(tester, () => !back.dirty, what: 'saved after restart');
    expect(disk(a, 'src/main.rs').readAsStringSync(), startsWith('// one\n'));

    // 2. "Restart the Backend" from the palette with unsaved text: nothing is lost.
    typeAtStart('src/main.rs', '// two\n', app);
    await tester.pump(const Duration(milliseconds: 100));
    await app.restartBackend();
    await until(tester, () => app.workspace.rootLoaded, what: 'reopened');
    await until(tester, () => !app.documents.doc('src/main.rs')!.readOnly, what: 'attached');
    expect(app.documents.doc('src/main.rs')!.text, startsWith('// two\n// one\n'));
    expect(app.documents.doc('src/main.rs')!.dirty, isTrue);

    // 3. The shell itself ends with unsaved text (no quit path): the next start offers it back.
    await tester.pump(const Duration(seconds: 2)); // the recovery copy is written
    await app.client.shutdown();
    await tester.pumpWidget(const SizedBox.shrink());
    app = await boot(tester, data.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'folder A reopened');
    await until(
      tester,
      () => app.documents.doc('src/main.rs')?.dirty ?? false,
      what: 'recovered text',
    );
    expect(app.documents.doc('src/main.rs')!.text, startsWith('// two\n// one\n'));
    expect(app.documents.banner, contains('Recovered unsaved changes'));
    await app.documents.save('src/main.rs');
    await until(tester, () => !app.documents.doc('src/main.rs')!.dirty, what: 'saved');

    // 4. Two saves in a row are not a conflict.
    typeAtStart('src/main.rs', '// three\n', app);
    await Future.wait([app.documents.save('src/main.rs'), app.documents.save('src/main.rs')]);
    await tester.pump(const Duration(milliseconds: 200));
    expect(app.documents.conflictName, isNull);
    expect(disk(a, 'src/main.rs').readAsStringSync(), startsWith('// three\n'));

    // 5. A file deleted on disk while dirty: its own question; Save creates it again.
    await app.documents.open('gone.txt');
    await until(tester, () => app.documents.doc('gone.txt') != null, what: 'gone.txt');
    typeAtStart('gone.txt', 'mine ', app);
    disk(a, 'gone.txt').deleteSync();
    await until(tester, () => app.documents.doc('gone.txt')!.deletedOnDisk, what: 'deleted mark');
    await key(tester, LogicalKeyboardKey.keyS, mod: true);
    await until(
      tester,
      () => find.text('gone.txt was deleted on disk').evaluate().isNotEmpty,
      what: 'the deleted question',
    );
    await tester.tap(find.text('Save').last);
    await until(tester, () => disk(a, 'gone.txt').existsSync(), what: 'recreated');
    expect(disk(a, 'gone.txt').readAsStringSync(), 'mine gone\n');

    // 6. Switching folders drops the old folder's documents (their paths were A's).
    await app.documents.open('README.md');
    await until(tester, () => app.documents.doc('README.md') != null, what: 'A README');
    expect(app.documents.doc('README.md')!.text, '# A\n');
    app.documents.requestOpenFolder(b.path);
    // Canonical, as the daemon's root is (macOS temp folders: /var is /private/var).
    final bRoot = b.resolveSymbolicLinksSync();
    await until(tester, () => app.workspace.root == bRoot && app.workspace.rootLoaded, what: 'B');
    await tester.pump(const Duration(milliseconds: 300));
    expect(app.documents.docs.map((d) => d.path), isNot(contains('src/main.rs')));
    expect(app.documents.doc('README.md'), isNull);
    await app.documents.open('README.md');
    await until(tester, () => app.documents.doc('README.md') != null, what: 'B README');
    expect(app.documents.doc('README.md')!.text, '# B\n');

    await quit(tester, app);
    semantics.dispose();
  });
}
