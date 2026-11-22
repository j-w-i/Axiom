// Language servers in the editor (FL-EDIT-002), against the real daemon and the in-repo fake
// server (tests/helpers/fake-lsp), as the Tauri suite lsp.mjs: diagnostics (underline and
// Problems row), the status segment, completion, F12, F2 across files (unsaved, undoable),
// formatting, and the sync the server received.
// Run: apps/desktop-flutter/scripts/e2e.sh lsp_test.dart

import 'dart:io';

import 'package:axiom_desktop/editor/lsp/lsp_view.dart';
import 'package:axiom_desktop/shell/palette.dart';
import 'package:axiom_desktop/state/lsp.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
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
      env: {
        'FAKE_LSP_LOG': log,
        'FAKE_LSP_DEFINITION': fileUri(
          // Canonical, as the daemon's root is (macOS temp folders: /var is /private/var).
          '${ws.resolveSymbolicLinksSync()}/src/other.rs',
        ),
      },
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

    // 2b. Completion like VS Code (M5.5-EDIT-001): it opens as you type; the selected item's
    //     documentation is resolved beside the list; a snippet's tab stops; an auto-import.
    final original = main.controller.text;
    final features = tester.state<LspEditorFeaturesState>(find.byType(LspEditorFeatures).first);
    Future<void> type(String text) async {
      for (final ch in text.split('')) {
        main.controller.replaceSelection(ch);
        await tester.pump(const Duration(milliseconds: 20));
      }
    }

    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 24);
    await type('pri');
    await until(
      tester,
      () => features.shownLabels.isNotEmpty && features.shownLabels.first == 'printLine',
      what: 'the list as you type',
      detail: () => '${features.shownLabels}',
    );
    await until(
      tester,
      () => find.textContaining('Prints a', findRichText: true).evaluate().isNotEmpty,
      what: 'the resolved documentation',
    );
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    expect(main.controller.codeLines[2].text, endsWith('printLine(message, level)'));
    expect(main.controller.selectedText, 'message');
    await key(tester, LogicalKeyboardKey.tab);
    expect(main.controller.selectedText, 'level');
    await key(tester, LogicalKeyboardKey.tab);
    expect(main.controller.selection.isCollapsed, isTrue);
    expect(main.controller.selection.extentOffset, main.controller.codeLines[2].length);
    await type(' Has');
    await until(
      tester,
      () => features.shownLabels.contains('HashMap'),
      what: 'HashMap offered',
      detail: () => '${features.shownLabels}',
    );
    while (features.shownLabels.isNotEmpty && features.shownLabels.first != 'HashMap') {
      await key(tester, LogicalKeyboardKey.arrowDown);
    }
    await key(tester, LogicalKeyboardKey.enter);
    await tester.pump();
    expect(main.controller.codeLines[0].text, 'use std::collections::HashMap;');
    expect(main.controller.codeLines[3].text, endsWith(' HashMap'));
    main.controller.text = original;
    await tester.pump(const Duration(milliseconds: 100));

    // 2b'. The list opens under the line being typed, never over it, with the names on the word
    //      being completed (FL-EDIT-003), 400 lines down, where arithmetic from the font size
    //      had drifted a line off the editor's own layout.
    main.controller.text = '${'// filler\n' * 400}$original';
    main.controller.selection = const CodeLineSelection.collapsed(index: 402, offset: 24);
    await tester.pump(const Duration(milliseconds: 100));
    await type('pri');
    await until(
      tester,
      () => features.shownLabels.isNotEmpty,
      what: 'the list 400 lines down',
      detail: () => '${features.shownLabels}',
    );
    await tester.pump();
    final editor = tester.renderObject(find.byType(CodeEditor).first);
    final typed = codeEditorPositionRect(editor, const CodeLinePosition(index: 402, offset: 24))!;
    final list = tester.getRect(find.bySemanticsLabel('Suggestions'));
    expect(list.top, greaterThanOrEqualTo(typed.bottom), reason: 'list $list, line $typed');
    expect(
      (list.left + completionTextInset - typed.left).abs(),
      lessThan(1.5),
      reason: 'the names start on the typed word: list $list, word at ${typed.left}',
    );
    await key(tester, LogicalKeyboardKey.escape);
    main.controller.text = original;
    await tester.pump(const Duration(milliseconds: 100));

    // 2c. M5.5-EDIT-002: semantic tokens colour `fn` and the names after it; Mod+T finds a
    //     workspace symbol; Shift+Alt+H lists who calls `value`; Shift+Alt+O sorts the imports.
    await until(
      tester,
      () => main.semantic[0]?.any((r) => r.start == 0 && r.end == 2) ?? false,
      what: 'the semantic tokens',
      detail: () => '${main.semantic}',
    );
    main.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 5);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.keyT, mod: true);
    await until(
      tester,
      () => find.textContaining('helper  src/main.rs').evaluate().isNotEmpty,
      what: 'the workspace symbol',
      detail: () =>
          'palette ${find.byType(Palette).evaluate().length}, asked '
          '${File(log).readAsStringSync().contains('workspace/symbol')}, focus '
          '${FocusManager.instance.primaryFocus}',
    );
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 100));
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 18);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.keyH, shift: true, alt: true);
    await until(
      tester,
      () => find.textContaining('main  src/main.rs').evaluate().isNotEmpty,
      what: 'the caller of value',
    );
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 100));
    main.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
    main.controller.replaceSelection('use b;\nuse a;\n');
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.keyO, shift: true, alt: true);
    await until(
      tester,
      () => main.controller.codeLines[0].text == 'use a;',
      what: 'the imports organized',
      detail: () => main.controller.codeLines[0].text,
    );
    expect(main.controller.codeLines[1].text, 'use b;');
    main.controller.text = original;
    await tester.pump(const Duration(milliseconds: 100));

    // 2d. Hover (the owner, 2026-11-22): the server's Markdown in a card the pointer can move
    //     onto, which stays and scrolls there, and closes once the pointer has left.
    main.controller.text = '// LONGHOVER\n$original';
    await tester.pump(const Duration(milliseconds: 300));
    final hoverEditor = tester.renderObject(find.byType(CodeEditor).first);
    final word = codeEditorPositionRect(hoverEditor, const CodeLinePosition(index: 2, offset: 4))!;
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await mouse.addPointer(location: word.centerLeft + const Offset(4, 0));
    await mouse.moveTo(word.centerLeft + const Offset(6, 0));
    final card = find.bySemanticsLabel(RegExp('^Hover: '));
    await until(tester, () => card.evaluate().isNotEmpty, what: 'the hover card');
    expect(find.text('Long hover', findRichText: true), findsOneWidget, reason: 'Markdown heading');
    // Onto the card, in small steps (as a hand moves), and a moment there: it stays.
    final target = tester.getRect(card).center;
    final start = word.centerLeft + const Offset(6, 0);
    for (var i = 1; i <= 12; i++) {
      await mouse.moveTo(Offset.lerp(start, target, i / 12)!);
      await tester.pump(const Duration(milliseconds: 16));
    }
    await tester.pump(const Duration(milliseconds: 800));
    expect(card, findsOneWidget, reason: 'the card stays under the pointer');
    final scroll = find.descendant(of: card, matching: find.byType(Scrollable)).first;
    final before = tester.state<ScrollableState>(scroll).position.pixels;
    await tester.sendEventToBinding(
      PointerScrollEvent(position: target, scrollDelta: const Offset(0, 240)),
    );
    await tester.pump(const Duration(milliseconds: 100));
    expect(tester.state<ScrollableState>(scroll).position.pixels, greaterThan(before));
    // Away from the card and the word: it closes.
    await mouse.moveTo(
      tester.getRect(find.byType(CodeEditor).first).bottomRight - const Offset(8, 8),
    );
    await until(tester, () => card.evaluate().isEmpty, what: 'the hover card closed');
    await mouse.removePointer();
    main.controller.text = original;
    await tester.pump(const Duration(milliseconds: 100));

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

    // 7. The symbol under the caret is highlighted where it occurs.
    final lsp = tester.state<LspEditorFeaturesState>(find.byType(LspEditorFeatures).first);
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 9);
    await tester.pump();
    await until(tester, () => main.highlights.length == 1, what: 'the highlight of total');
    expect(main.highlights.single.start, (line: 2, character: 8));

    // 8. Ctrl+Shift+Space: the signature, its first parameter active; Esc closes it.
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 22);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.space, ctrl: true, shift: true);
    await until(
      tester,
      () => lsp.signatureLabel == 'value(a: i32, b: i32) -> i32',
      what: 'the signature',
    );
    expect(named('Signature: value(a: i32, b: i32) -> i32'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump();
    expect(lsp.signatureLabel, isNull);

    // 9. Shift+F12 lists the references in both files; one opens there.
    other.controller.undo(); // the rename's other half (step 4)
    await tester.pump(const Duration(milliseconds: 300));
    expect(other.text, contains('fn value()'));
    main.controller.selection = const CodeLineSelection.collapsed(index: 2, offset: 18);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.f12, shift: true);
    await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'References');
    expect(find.textContaining('src/main.rs:3'), findsWidgets);
    expect(find.textContaining('src/other.rs:1'), findsWidgets);
    await typeFocused(tester, 'other');
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () =>
          app.documents.activePath == 'src/other.rs' && app.documents.cursorText.startsWith('Ln 1'),
      what: 'the reference opened',
    );

    // 10. Ctrl+Shift+O lists the document's symbols; one goes there.
    await app.documents.open('src/main.rs');
    await until(tester, () => app.documents.activePath == 'src/main.rs', what: 'main.rs');
    await tester.pump(const Duration(milliseconds: 300));
    main.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.keyO, ctrl: true, shift: true);
    await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'Go to Symbol');
    expect(find.text('helper'), findsOneWidget);
    expect(find.text('total'), findsOneWidget);
    await typeFocused(tester, 'total');
    await tester.pump(const Duration(milliseconds: 100));
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => app.documents.cursorText.startsWith('Ln 3, Col 9'),
      what: 'the symbol',
      detail: () => app.documents.cursorText,
    );

    // 11. Ctrl+. offers the server's quick fix; it edits the line.
    main.controller.selection = const CodeLineSelection.collapsed(index: 3, offset: 6);
    await tester.pump();
    await key(tester, LogicalKeyboardKey.period, ctrl: true);
    await until(tester, () => find.text('Add a semicolon').evaluate().isNotEmpty, what: 'actions');
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => main.controller.codeLines[3].text == '    ERROR;',
      what: 'the quick fix',
    );

    // 12. A tab dragged onto another takes its place.
    final order = app.documents.docs.map((d) => d.path).toList();
    expect(order, ['src/main.rs', 'src/other.rs']);
    Finder tab(String label) => find.bySemanticsLabel(RegExp('^${RegExp.escape(label)}(,|\$)'));
    final from = tester.getCenter(tab('other.rs').first);
    final to = tester.getCenter(tab('main.rs').first);
    await tester.timedDragFrom(from, to - from, const Duration(milliseconds: 400));
    await tester.pump(const Duration(milliseconds: 200));
    expect(app.documents.docs.map((d) => d.path).toList(), ['src/other.rs', 'src/main.rs']);

    // 13. Format on save (M5.5-EDIT-002): with the setting on, Mod+S formats, then saves.
    await app.settings.setValue('editor.formatOnSave', true);
    await app.documents.open('src/other.rs');
    final otherDoc = app.documents.doc('src/other.rs')!;
    otherDoc.controller.text = 'pub fn value() -> i32 {   \n    2\n}\n';
    await tester.pump(const Duration(milliseconds: 100));
    await app.documents.save('src/other.rs');
    expect(
      File('${ws.path}/src/other.rs').readAsStringSync(),
      startsWith('pub fn value() -> i32 {\n'),
    );

    // 6. The server received the sync and the requests.
    final got = File(log).readAsStringSync();
    for (final m in [
      'textDocument/didOpen',
      'textDocument/didChange',
      'textDocument/rename',
      'textDocument/formatting',
      'textDocument/definition',
      'textDocument/completion',
      'textDocument/references',
      'textDocument/documentSymbol',
      'textDocument/signatureHelp',
      'textDocument/codeAction',
      'textDocument/documentHighlight',
      'textDocument/semanticTokens/full',
      'workspace/symbol',
      'textDocument/prepareCallHierarchy',
      'callHierarchy/incomingCalls',
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
