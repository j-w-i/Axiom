// Language servers in the editor (FL-EDIT-002), against the real daemon and the in-repo fake
// server (tests/helpers/fake-lsp), as the Tauri suite lsp.mjs: diagnostics (underline and
// Problems row), the status segment, completion, F12, F2 across files (unsaved, undoable),
// formatting, and the sync the server received.
// Run: apps/desktop-flutter/scripts/e2e.sh lsp_test.dart

import 'dart:io';

import 'package:axiom_desktop/editor/lsp/lsp_view.dart';
import 'package:axiom_desktop/state/lsp.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('language servers in the editor (fake-lsp)', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-lsp-data-');
    final ws = tempDir('axiom-fl-lsp-ws-');
    final log = '${data.path}/fake-lsp.log';
    writeFile(data, 'providers/fakerust/provider.toml', '''
schema = "axiom.provider/1"
id = "fakerust"
name = "Fake Rust"
description = "Test: fake-lsp for Rust files."
version = "0.1.0"

[project]
any_file = ["fake.marker"]

[[lsp]]
id = "fake"
languages = ["rust"]
command = ["fake-lsp"]
''');
    writeFile(ws, 'fake.marker', '');
    writeFile(
      ws,
      'src/main.rs',
      'fn helper() -> i32 { 1 }  \nfn main() {\n    let total = value();\n    ERROR\n}\n',
    );
    writeFile(ws, 'src/other.rs', 'pub fn value() -> i32 {\n    2\n}\n');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });

    final app = await boot(
      tester,
      data.path,
      env: {'FAKE_LSP_LOG': log, 'FAKE_LSP_DEFINITION': fileUri('${ws.path}/src/other.rs')},
    );
    await app.workspace.openFolder(ws.path);
    await until(
      tester,
      () => app.lsp.servers.any((s) => s.serverId == 'fake'),
      what: 'the server listed',
    );
    await app.documents.open('src/main.rs');
    await until(tester, () => app.documents.activePath == 'src/main.rs', what: 'main.rs');
    final main = app.documents.doc('src/main.rs')!;

    // 1. The diagnostic: an underline in the editor and a Problems row with its source.
    await until(
      tester,
      () => main.diagnostics.isNotEmpty,
      what: 'the diagnostic',
      timeout: const Duration(seconds: 30),
    );
    expect(main.diagnostics.single.range.start, (line: 3, character: 4));
    await key(tester, LogicalKeyboardKey.keyJ, mod: true);
    await tester.pump(const Duration(milliseconds: 200));
    expect(named('Error: an ERROR here at src/main.rs:4:5, from fake (fake)'), findsOneWidget);
    // 1b. The status bar names the server and its state.
    await until(
      tester,
      () => named('fake, ready').evaluate().isNotEmpty,
      what: 'the status segment',
    );

    // 2. Ctrl+Space offers the server's completions.
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 24);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.space, ctrl: true);
    await until(tester, () => named('Suggestions').evaluate().isNotEmpty, what: 'the suggestions');
    await tester.pump(const Duration(milliseconds: 100));
    expect(named('alpha'), findsOneWidget);
    expect(named('beta'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    expect(named('Suggestions'), findsNothing);

    // 3. F12 opens the definition in another file, at its place.
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 18);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.f12);
    await until(tester, () => app.documents.activePath == 'src/other.rs', what: 'other.rs');
    await until(tester, () => app.documents.cursorText == 'Ln 1, Col 1', what: 'the cursor at 1:1');

    // 4. F2 renames in both files: unsaved, files on disk unchanged, undoable.
    await app.documents.open('src/main.rs');
    await until(tester, () => app.documents.activePath == 'src/main.rs', what: 'main.rs again');
    await tester.pump(const Duration(milliseconds: 200));
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 18);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.f2);
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Rename value'), findsOneWidget);
    await typeFocused(tester, 'amount');
    await key(tester, LogicalKeyboardKey.enter);
    final other = app.documents.doc('src/other.rs')!;
    await until(
      tester,
      () => other.text.contains('fn amount()') && main.text.contains('amount()'),
      what: 'the rename',
    );
    expect(main.dirty && other.dirty, isTrue);
    expect(File('${ws.path}/src/other.rs').readAsStringSync(), contains('fn value()'));
    main.controller.undo();
    await tester.pump();
    expect(main.text, contains('value()'));

    // 5. Format Document (Ctrl+Shift+I) formats through the server.
    expect(main.text.split('\n').first, endsWith(' '));
    await key(tester, LogicalKeyboardKey.keyI, ctrl: true, shift: true);
    await until(tester, () => !main.text.split('\n').first.endsWith(' '), what: 'formatting');

    // 6. The server received the sync and the requests.
    final got = File(log).readAsStringSync();
    for (final m in [
      'textDocument/didOpen',
      'textDocument/didChange',
      'textDocument/rename',
      'textDocument/formatting',
      'textDocument/definition',
      'textDocument/completion',
    ]) {
      expect(got, contains(m), reason: m);
    }
    await quit(tester, app);
    semantics.dispose();
  });

  // The real servers (CI Ubuntu requires them: AXIOM_REQUIRE_LSP=1; skipped here when absent).
  final require = Platform.environment['AXIOM_REQUIRE_LSP'] == '1';
  bool runs(String cmd, List<String> args) {
    try {
      return Process.runSync(cmd, args).exitCode == 0;
    } on ProcessException {
      return false;
    }
  }

  for (final real in [
    (
      name: 'rust-analyzer',
      ok: runs('rust-analyzer', ['--version']),
      files: {
        'Cargo.toml':
            '[package]\nname = "lsp_e2e"\nversion = "0.1.0"\nedition = "2021"\n\n[workspace]\n',
        'src/main.rs': 'mod util;\n\nfn main() {\n    let p = util::Point { x: 1 };\n    let n: i32 = util::helper();\n    let bad: i32 = "text";\n    println!("{} {n} {bad}", p.x);\n    p.\n}\n',
        'src/util.rs': 'pub struct Point {\n    pub x: i32,\n}\n\nimpl Point {\n    pub fn norm(&self) -> i32 {\n        self.x\n    }\n}\n\npub fn helper() -> i32 {\n    42\n}\n',
      },
      open: 'src/main.rs',
      completion: (line: 7, character: 6),
      errorLine: 5,
      definition: (line: 4, character: 24),
      target: 'src/util.rs',
    ),
    (
      name: 'typescript-language-server',
      ok: runs('typescript-language-server', ['--version']),
      files: {
        'package.json': '{ "name": "lsp-e2e", "version": "1.0.0", "private": true }\n',
        'tsconfig.json': '{ "compilerOptions": { "strict": true, "target": "es2022", "module": "es2022", "moduleResolution": "bundler" } }\n',
        'src/main.ts': 'import { helper, point } from "./util";\n\nconst n: number = helper();\nconst bad: number = "text";\nconsole.log(n, bad);\npoint.\n',
        'src/util.ts': 'export function helper(): number {\n  return 42;\n}\n\nexport const point = { norm(): number { return 1; } };\n',
      },
      open: 'src/main.ts',
      completion: (line: 5, character: 6),
      errorLine: 3,
      definition: (line: 2, character: 20),
      target: 'src/util.ts',
    ),
  ]) {
    testWidgets('${real.name}: diagnostics, completion, F12, F2', (tester) async {
      if (!real.ok) {
        if (require) fail('${real.name} is required (AXIOM_REQUIRE_LSP=1) but does not run');
        markTestSkipped('${real.name} is not installed here');
        return;
      }
      final data = tempDir('axiom-fl-real-data-');
      final ws = tempDir('axiom-fl-real-ws-');
      real.files.forEach((p, t) => writeFile(ws, p, t));
      addTearDown(() {
        data.deleteSync(recursive: true);
        ws.deleteSync(recursive: true);
      });
      final app = await boot(tester, data.path);
      await app.workspace.openFolder(ws.path);
      await until(tester, () => app.lsp.servers.isNotEmpty, what: 'the servers');
      await app.documents.open(real.open);
      await until(tester, () => app.documents.activePath == real.open, what: real.open);
      final doc = app.documents.doc(real.open)!;
      // Diagnostics (servers may take a while to load the project).
      await until(
        tester,
        () => doc.diagnostics.any((d) => d.severity == 1 && d.range.start.line == real.errorLine),
        what: 'the type error',
        timeout: const Duration(minutes: 3),
      );
      // Completion offers norm (retried while the server indexes).
      final features = find.byType(LspEditorFeatures);
      var labels = <String>[];
      for (var i = 0; i < 20 && !labels.any((l) => l.contains('norm')); i++) {
        doc.controller.selection = CodeLineSelection.collapsed(
          index: real.completion.line,
          offset: real.completion.character,
        );
        await tester.pump();
        await tester.state<LspEditorFeaturesState>(features.first).complete();
        await tester.pump(const Duration(milliseconds: 200));
        labels = tester.state<LspEditorFeaturesState>(features.first).shownLabels;
        if (!labels.any((l) => l.contains('norm'))) {
          await Future<void>.delayed(const Duration(seconds: 3));
        }
      }
      expect(labels, contains(contains('norm')));
      await key(tester, LogicalKeyboardKey.escape);
      // F12 opens the definition's file.
      doc.controller.selection = CodeLineSelection.collapsed(
        index: real.definition.line,
        offset: real.definition.character,
      );
      await tester.pump();
      await key(tester, LogicalKeyboardKey.f12);
      await until(
        tester,
        () => app.documents.activePath == real.target,
        what: 'the definition',
        timeout: const Duration(seconds: 30),
      );
      // F2 renames across both files.
      await app.documents.open(real.open);
      await until(tester, () => app.documents.activePath == real.open, what: 'back');
      await tester.pump(const Duration(milliseconds: 200));
      doc.controller.selection = CodeLineSelection.collapsed(
        index: real.definition.line,
        offset: real.definition.character,
      );
      await tester.pump();
      await key(tester, LogicalKeyboardKey.f2);
      await tester.pump(const Duration(milliseconds: 300));
      await typeFocused(tester, 'renamedThing');
      await key(tester, LogicalKeyboardKey.enter);
      await until(
        tester,
        () => [
          real.open,
          real.target,
        ].every((p) => app.documents.doc(p)?.text.contains('renamedThing') ?? false),
        what: 'the rename in every file',
        timeout: const Duration(seconds: 60),
      );
      await quit(tester, app);
    });
  }
}
