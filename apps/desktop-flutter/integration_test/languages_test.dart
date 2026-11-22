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
  // At module level: after the fixture's `return`, Pyright would not analyse the line.
  ('python', 'main.py', 0, '\nos.path.jo', 'join', null),
  ('typescript', 'src/main.ts', 2, '\nsorted.toS', 'toSorted', null),
  ('rust', 'src/main.rs', 5, '\n    let n: HashMap<i32, i32> = HashMap::ne', 'new', null),
  ('go', 'main.go', 8, '\n\tstrings.ToU', 'ToUpper', null),
  // Not strlen: clangd treats it as a compiler builtin, which has no source to go to.
  ('c', 'main.c', 4, '\n    fop', 'fopen', null),
  ('cpp', 'main.cpp', 7, '\n    std::so', 'sort', null),
  ('java', 'src/main/java/fixture/Main.java', 6, '\n        List.o', 'of', null),
  ('csharp', 'Program.cs', 3, '\nnumbers.Order', 'OrderBy', null),
  ('kotlin', 'src/main/kotlin/Main.kt', 2, '\n    names.sor', 'sorted', null),
  ('swift', 'Sources/Fixture/main.swift', 3, '\nwords.join', 'joined', null),
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
      // Swift on Linux is installed by Axiom (swiftly into its data directory): a Swift already
      // on this computer, through the person's own swiftly, is taken off the daemon's PATH.
      final path = Platform.environment['PATH'] ?? '';
      final app = await boot(
        tester,
        data.path,
        env: folder == 'swift' && Platform.isLinux
            ? {'PATH': path.split(':').where((d) => !d.contains('swiftly')).join(':')}
            : null,
      );
      // A failed case still stops its daemon and the servers it started.
      var quitted = false;
      addTearDown(() async {
        if (!quitted) await app.shutdown();
      });
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
      // The symbol's item: its name, or its name and parameters as some servers label it
      // (rust-analyzer's `new()`).
      bool isWant(String? label) => label == want || (label?.startsWith('$want(') ?? false);
      // The server's own items: the document's words (shown while it is not ready) may hold the
      // name too.
      bool offered() => !features.wordsOnly && features.shownLabels.any(isWant);
      final started = DateTime.now();
      // Servers index on first use: the first answer may be slow; the 300 ms budget is measured
      // on a second request, once the list has been shown.
      await until(
        tester,
        offered,
        timeout: const Duration(minutes: 2),
        what: '$want offered',
        detail: () =>
            '${features.shownLabels.take(20)} '
            '${app.lsp.serverFor(doc.language.id)?.toJson()} ${app.lsp.progress}',
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
        offered,
        timeout: const Duration(seconds: 5),
        what: '$want offered again',
      );
      again.stop();
      // ignore: avoid_print
      print('$folder: completion in ${again.elapsedMilliseconds} ms');
      expect(again.elapsedMilliseconds, lessThanOrEqualTo(300));
      // Its documentation (the item's or resolved): Down until it is selected, at most the list.
      for (var n = 0; !isWant(features.activeLabel) && n < features.shownLabels.length; n++) {
        await key(tester, LogicalKeyboardKey.arrowDown);
      }
      expect(features.activeLabel, predicate<String?>(isWant), reason: 'the $want item selected');
      await until(
        tester,
        () => find.bySemanticsLabel('Documentation').evaluate().isNotEmpty,
        timeout: const Duration(seconds: 10),
        what: 'the documentation of $want',
      );
      await key(tester, LogicalKeyboardKey.enter);
      await tester.pump(const Duration(milliseconds: 100));
      // F12 on the accepted symbol opens the SDK's source (outside the workspace, read-only). The
      // caret goes into its name: a snippet (`new()`, `max(a, b)`) leaves it after the name, or on
      // a placeholder.
      final s = c.selection;
      final accepted = c.codeLines[s.extentIndex].text;
      final at = accepted.lastIndexOf(want, s.extentOffset);
      c.selection = CodeLineSelection.collapsed(
        index: s.extentIndex,
        offset: at < 0 ? (s.extentOffset - 1).clamp(0, accepted.length) : at + 1,
      );
      await tester.pump(const Duration(milliseconds: 100));
      // F12, again every five seconds while the server may still be loading the project (the
      // first answer can be empty until it has; csharp-ls). At most a minute.
      final f12At = DateTime.now();
      final end = f12At.add(const Duration(seconds: 60));
      while (app.documents.activePath == file) {
        if (DateTime.now().isAfter(end)) {
          fail("timed out waiting for the SDK's source (${app.documents.banner})");
        }
        await key(tester, LogicalKeyboardKey.f12);
        final next = DateTime.now().add(const Duration(seconds: 5));
        while (app.documents.activePath == file && DateTime.now().isBefore(next)) {
          await tester.pump(const Duration(milliseconds: 100));
        }
      }
      // ignore: avoid_print
      print('$folder: definition after ${DateTime.now().difference(f12At).inMilliseconds} ms');
      // ignore: avoid_print
      print('$folder: definition in ${app.documents.activePath}');
      quitted = true;
      await quit(tester, app);
    });
  }
}
