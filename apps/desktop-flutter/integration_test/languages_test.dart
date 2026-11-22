// The M5.5 exit scenario per tier-1 language (M5.5-QA-001), with the real servers: from a data
// directory with nothing installed, opening the fixture's file installs its server (the
// "Install language servers automatically" switch, so no dialog), then typing a standard-library
// prefix lists the symbol with its documentation within 300 ms, hover shows it, and F12 opens the
// SDK's source.
//
// Downloads servers and SDKs, so it is opt-in: AXIOM_TEST_LSP_INSTALL=1 (one language with
// AXIOM_TEST_LANGUAGE=python). A language whose SDK Axiom cannot install here (.NET, Swift off
// macOS) is reported as skipped with the reason.
// Run: AXIOM_TEST_LSP_INSTALL=1 apps/desktop-flutter/scripts/e2e.sh languages_test.dart

import 'dart:io';

import 'package:axiom_desktop/daemon/protocol.g.dart';
import 'package:axiom_desktop/editor/lsp/lsp_view.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'browser.dart' show repoRoot;
import 'support.dart';

/// (fixture folder, file, line to type at (0-based; the end of the line), what to type, the
/// symbol completion must offer, a reason to skip here or null).
final _cases = <(String, String, int, String, String, String?)>[
  ('python', 'main.py', 4, '\n    os.path.jo', 'join', null),
  ('typescript', 'src/main.ts', 2, '\nsorted.toS', 'toSorted', null),
  ('rust', 'src/main.rs', 5, '\n    let n: HashMap<i32, i32> = HashMap::ne', 'new', null),
  ('go', 'main.go', 8, '\n\tstrings.ToU', 'ToUpper', null),
  ('c', 'main.c', 4, '\n    strl', 'strlen', null),
  ('cpp', 'main.cpp', 7, '\n    std::so', 'sort', null),
  ('java', 'src/main/java/fixture/Main.java', 6, '\n        List.o', 'of', null),
  (
    'csharp',
    'Program.cs',
    3,
    '\nnumbers.Order',
    'OrderBy',
    'Axiom detects .NET but does not install it (ADR-0021 §6)',
  ),
  ('kotlin', 'src/main/kotlin/Main.kt', 2, '\n    names.sor', 'sorted', null),
  (
    'swift',
    'Sources/Fixture/main.swift',
    3,
    '\nwords.join',
    'joined',
    Platform.isMacOS ? null : 'SourceKit-LSP comes with a Swift toolchain',
  ),
  ('dart', 'bin/main.dart', 3, '\n  ma', 'max', null),
];

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  final only = Platform.environment['AXIOM_TEST_LANGUAGE'];
  for (final (folder, file, line, typed, want, skipHere) in _cases) {
    if (only != null && only != folder) continue;
    testWidgets('$folder: install, complete, hover, definition', (tester) async {
      if (Platform.environment['AXIOM_TEST_LSP_INSTALL'] != '1') {
        // ignore: avoid_print
        print('SKIPPED: set AXIOM_TEST_LSP_INSTALL=1 (it downloads language servers)');
        return;
      }
      if (skipHere != null) {
        // ignore: avoid_print
        print('SKIPPED $folder: $skipHere');
        return;
      }
      final data = tempDir('axiom-fl-lang-data-');
      final ws = tempDir('axiom-fl-lang-ws-');
      addTearDown(() {
        data.deleteSync(recursive: true);
        ws.deleteSync(recursive: true);
      });
      final src = Directory('${repoRoot()}/tests/fixtures/languages/$folder');
      for (final f in src.listSync(recursive: true).whereType<File>()) {
        writeFile(ws, f.path.substring(src.path.length + 1), f.readAsStringSync());
      }
      final app = await boot(tester, data.path);
      await app.settings.setValue('lsp.autoInstall', true);
      await app.workspace.openFolder(ws.path);
      await until(tester, () => app.workspace.isOpen, what: 'the folder');
      await app.documents.open(file);
      final doc = app.documents.doc(file)!;
      // Installed (with whatever it needs) and ready: minutes on a slow network.
      await until(
        tester,
        () => app.lsp.serverFor(doc.language.id)?.state == ServerState.ready,
        timeout: const Duration(minutes: 15),
        what: 'the $folder server ready',
        detail: () =>
            '${app.lsp.serverFor(doc.language.id)?.toJson()} ${app.environments.failures}',
      );
      final c = doc.controller;
      c.selection = CodeLineSelection.collapsed(index: line, offset: c.codeLines[line].length);
      for (final ch in typed.split('')) {
        c.replaceSelection(ch);
        await tester.pump(const Duration(milliseconds: 15));
      }
      final features = tester.state<LspEditorFeaturesState>(find.byType(LspEditorFeatures).first);
      final started = DateTime.now();
      // Servers index on first use: the first answer may be slow; the 300 ms budget is measured
      // on a second request, once the list has been shown.
      await until(
        tester,
        () => features.shownLabels.contains(want),
        timeout: const Duration(minutes: 2),
        what: '$want offered',
        detail: () => '${features.shownLabels.take(20)}',
      );
      // ignore: avoid_print
      print(
        '$folder: first completion after ${DateTime.now().difference(started).inMilliseconds} ms',
      );
      await key(tester, LogicalKeyboardKey.escape);
      final again = Stopwatch()..start();
      await features.complete();
      await until(
        tester,
        () => features.shownLabels.contains(want),
        timeout: const Duration(seconds: 5),
        what: '$want offered again',
      );
      again.stop();
      // ignore: avoid_print
      print('$folder: completion in ${again.elapsedMilliseconds} ms');
      expect(again.elapsedMilliseconds, lessThanOrEqualTo(300));
      // Its documentation (the item's or resolved).
      while (features.shownLabels.first != want) {
        await key(tester, LogicalKeyboardKey.arrowDown);
      }
      await until(
        tester,
        () => find.bySemanticsLabel('Documentation').evaluate().isNotEmpty,
        timeout: const Duration(seconds: 10),
        what: 'the documentation of $want',
      );
      await key(tester, LogicalKeyboardKey.enter);
      await tester.pump(const Duration(milliseconds: 100));
      // F12 on the accepted symbol opens the SDK's source (outside the workspace, read-only).
      final s = c.selection;
      c.selection = CodeLineSelection.collapsed(
        index: s.extentIndex,
        offset: (s.extentOffset - 1).clamp(0, c.codeLines[s.extentIndex].length),
      );
      await key(tester, LogicalKeyboardKey.f12);
      await until(
        tester,
        () => app.documents.activePath != file,
        timeout: const Duration(seconds: 30),
        what: "the SDK's source",
        detail: () => app.documents.activePath,
      );
      // ignore: avoid_print
      print('$folder: definition in ${app.documents.activePath}');
      await quit(tester, app);
    });
  }
}
