// The editor end to end (FL-EDIT-001), against the real daemon. The Tauri suites it follows:
// smoke 5 (save), 6a/6b (external changes, the conflict), 8 (restore), split.mjs, and Go to File.
// Text is typed through the document's controller (simulated key presses carry no text input on
// desktop); every command goes through its real key.
// Run: apps/desktop-flutter/scripts/e2e.sh editor_test.dart

import 'dart:io';

import 'package:axiom_desktop/state/documents.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'support.dart';

void type(Doc d, String text) {
  for (final ch in text.split('')) {
    d.controller.replaceSelection(ch);
  }
}

Future<void> tapText(WidgetTester tester, String label) async {
  await tester.tap(find.text(label).last);
  await tester.pump(const Duration(milliseconds: 200));
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the editor end to end', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-ed-data-');
    final ws = tempDir('axiom-fl-ed-ws-');
    writeFile(ws, 'src/main.rs', 'fn main() {\n    println!("hi");\n}\n');
    writeFile(ws, 'src/deep/nested/needles.rs', '// needle\n');
    writeFile(ws, 'README.md', '# hi\n');
    writeFile(ws, 'notes.txt', 'alpha\n');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    File disk(String p) => File('${ws.path}/$p');

    var app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'the root');
    await tester.pump(const Duration(milliseconds: 200));
    final docs = app.documents;

    // 1. Opening from the explorer shows the file in a tab, with highlighting and the status bar.
    await tester.tap(named('README.md, file'));
    await tester.pump();
    await key(tester, LogicalKeyboardKey.enter);
    await until(tester, () => docs.activePath == 'README.md', what: 'README open');
    await tester.pump(const Duration(milliseconds: 200));
    expect(find.byType(CodeEditor), findsOneWidget);
    expect(find.text('Markdown'), findsOneWidget);

    // 2. Go to File (Mod+P): a file the explorer never listed, by a fuzzy query.
    expect(app.workspace.node('src/deep/nested/needles.rs'), isNull);
    await key(tester, LogicalKeyboardKey.keyP, mod: true);
    await tester.pump(const Duration(milliseconds: 300));
    expect(named('Go to File'), findsOneWidget);
    await typeFocused(tester, 'ndlsrs');
    await until(
      tester,
      () => find.textContaining('needles').evaluate().isNotEmpty,
      what: 'the file listed',
    );
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => docs.activePath == 'src/deep/nested/needles.rs',
      what: 'needles open',
    );

    // 3. Mod+S saves what was typed; the bytes on disk are the document's.
    await docs.open('src/main.rs');
    await until(tester, () => docs.activePath == 'src/main.rs', what: 'main.rs');
    await tester.pump(const Duration(milliseconds: 200));
    final main = docs.doc('src/main.rs')!;
    main.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
    type(main, '// edited\n');
    await tester.pump();
    expect(main.dirty, isTrue);
    expect(named('main.rs, unsaved'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.keyS, mod: true);
    await until(tester, () => !main.dirty, what: 'saved');
    expect(disk('src/main.rs').readAsStringSync(), main.text);
    expect(disk('src/main.rs').readAsStringSync(), startsWith('// edited\n'));

    // 4. Go to line (Ctrl+G) places the caret; the status bar shows it.
    await key(tester, LogicalKeyboardKey.keyG, mod: false, ctrl: true);
    await tester.pump(const Duration(milliseconds: 300));
    await typeFocused(tester, '3');
    await key(tester, LogicalKeyboardKey.enter);
    await until(tester, () => docs.cursorText.startsWith('Ln 3, Col 1'), what: 'Ln 3');

    // 5. A clean document reloads when the file changes on disk.
    await docs.open('notes.txt');
    await until(tester, () => docs.doc('notes.txt') != null, what: 'notes');
    disk('notes.txt').writeAsStringSync('beta\n');
    await until(tester, () => docs.doc('notes.txt')!.text == 'beta\n', what: 'the reload');

    // 6. Dirty + changed on disk: saving asks; Overwrite writes ours.
    final notes = docs.doc('notes.txt')!;
    notes.controller.selection = const CodeLineSelection.collapsed(index: 1, offset: 0);
    type(notes, 'ours\n');
    await tester.pump();
    disk('notes.txt').writeAsStringSync('theirs\n');
    await until(tester, () => notes.changedOnDisk, what: 'changed on disk');
    await key(tester, LogicalKeyboardKey.keyS, mod: true);
    await until(
      tester,
      () => find.text('notes.txt changed on disk').evaluate().isNotEmpty,
      what: 'the conflict',
    );
    await tapText(tester, 'Overwrite');
    await until(tester, () => !notes.dirty, what: 'overwritten');
    expect(disk('notes.txt').readAsStringSync(), 'beta\nours\n');

    // 7. Closing a dirty tab asks; Don't Save closes it without writing.
    await docs.open('README.md');
    await until(tester, () => docs.activePath == 'README.md', what: 'README');
    type(docs.doc('README.md')!, 'scratch ');
    await tester.pump();
    await key(tester, LogicalKeyboardKey.keyW, mod: true);
    await until(
      tester,
      () => find.text('Save the changes to README.md?').evaluate().isNotEmpty,
      what: 'the question',
    );
    await tapText(tester, "Don't Save");
    await until(tester, () => docs.doc('README.md') == null, what: 'closed');
    expect(disk('README.md').readAsStringSync(), '# hi\n');

    // 8. The split: Mod+\ shows main.rs on the right with its own editor over the same text;
    //    an edit there is the document's; Mod+S there saves.
    await docs.open('src/main.rs');
    await until(tester, () => docs.activePath == 'src/main.rs', what: 'main.rs');
    await key(tester, LogicalKeyboardKey.backslash, mod: true);
    await until(tester, () => docs.split?.active == 'src/main.rs', what: 'the right group');
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byType(CodeEditor), findsNWidgets(2));
    expect(named('Right editor group'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.digit2, mod: true);
    await tester.pump(const Duration(milliseconds: 100));
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'editor 2:src/main.rs');
    // Typed in the right group's view; the document (and the left view) follow.
    main.secondary.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
    for (final ch in 'R'.split('')) {
      main.secondary.replaceSelection(ch);
    }
    await tester.pump();
    expect(main.controller.text, startsWith('R// edited'));
    await tester.pump();
    await key(tester, LogicalKeyboardKey.keyS, mod: true);
    await until(tester, () => !main.dirty, what: 'saved from the right');
    expect(disk('src/main.rs').readAsStringSync(), startsWith('R// edited'));
    await key(tester, LogicalKeyboardKey.digit1, mod: true);
    await tester.pump(const Duration(milliseconds: 100));
    expect(FocusManager.instance.primaryFocus?.debugLabel, 'editor 1:src/main.rs');

    // 8b. The status bar names the language; its picker changes it (M5.5-LANG-001).
    expect(main.language.id, 'rust');
    await tester.tap(named('Language: Rust'));
    await tester.pump(const Duration(milliseconds: 150));
    await typeFocused(tester, 'Python');
    await tester.pump();
    await key(tester, LogicalKeyboardKey.enter);
    await until(tester, () => main.language.id == 'python', what: 'the picked language');
    await tester.pump();
    expect(named('Language: Python'), findsOneWidget);

    // 9. Restart: the tabs, the active one and the split come back.
    final before = [for (final d in docs.docs) d.path];
    await quit(tester, app);
    app = await boot(tester, data.path);
    await until(
      tester,
      () => app.documents.docs.length == before.length,
      what: 'the tabs restored',
    );
    expect([for (final d in app.documents.docs) d.path], before);
    expect(app.documents.activePath, 'src/main.rs');
    await until(
      tester,
      () => app.documents.split?.active == 'src/main.rs',
      what: 'the split restored',
    );

    // 10. Closing the right group's last tab ends the split; the document stays open.
    await tester.pump(const Duration(milliseconds: 300));
    await tester.tap(named('Close main.rs in the right group'));
    await tester.pump(const Duration(milliseconds: 200));
    expect(app.documents.split, isNull);
    expect(app.documents.doc('src/main.rs'), isNotNull);
    await quit(tester, app);
    semantics.dispose();
  });
}
