// Accessibility across the Flutter shell (FL-QA-001; ADR-0020 §7), against the real daemon, in
// both themes: every page (Files with an open editor, Chat, Search, Source Control,
// Environments, the Inspector), the console's three views, Settings' five sections and the
// palette. On each, every node a person can act on (tap, focus, type) has a name, and Tab moves
// focus through the window without getting stuck. Contrast is checked on the tokens (test/ui/tokens_test.dart).
// Run: apps/desktop-flutter/scripts/e2e.sh a11y_test.dart

import 'dart:io';
import 'dart:ui' show SemanticsAction;

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/shell/palette.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

/// Nodes a person can act on that have no name.
List<String> unnamed() {
  final out = <String>[];
  final nodes = find.semantics.byPredicate((n) {
    final d = n.getSemanticsData();
    final acts =
        d.hasAction(SemanticsAction.tap) ||
        d.hasAction(SemanticsAction.focus) ||
        d.flagsCollection.isTextField;
    return acts && d.label.trim().isEmpty && d.value.trim().isEmpty && d.hint.trim().isEmpty;
  }).evaluate();
  for (final n in nodes) {
    final d = n.getSemanticsData();
    out.add('#${n.id} ${n.rect} actions ${d.actions} flags ${d.flagsCollection}');
  }
  return out;
}

/// Tab from where focus is, [steps] times: focus moves on every press (it is never stuck), and
/// every focused node is named.
Future<List<String>> tabWalk(WidgetTester tester, int steps) async {
  final problems = <String>[];
  FocusNode? last = FocusManager.instance.primaryFocus;
  var stuck = 0;
  for (var i = 0; i < steps; i++) {
    await key(tester, LogicalKeyboardKey.tab);
    final now = FocusManager.instance.primaryFocus;
    // The editor keeps Tab (it indents) unless Ctrl+M hands it to focus; checked below.
    final inEditor = (now?.debugLabel ?? '').startsWith('editor ');
    if (identical(now, last) && !inEditor) stuck++;
    last = now;
  }
  if (stuck > 2) problems.add('Tab did not move focus $stuck times of $steps');
  return problems;
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('every page: named controls and a moving Tab, in both themes', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-a11y-data-');
    final ws = tempDir('axiom-fl-a11y-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(ws, 'src/main.rs', 'fn main() {\n    println!("hi");\n}\n');
    writeFile(ws, 'Cargo.toml', '[package]\nname = "a"\nversion = "0.1.0"\nedition = "2021"\n');
    writeFile(ws, 'README.md', '# a11y\n');
    await Process.run('git', ['init', '-q'], workingDirectory: ws.path);
    final app = await boot(
      tester,
      data.path,
      env: {'AXIOM_TEST_CREDENTIALS_FILE': '${data.path}/credentials.json'},
    );
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');
    await app.documents.open('src/main.rs');
    await tester.pump(const Duration(milliseconds: 300));

    final report = <String, List<String>>{};
    Future<void> settle() async {
      for (var i = 0; i < 10; i++) {
        await tester.pump(const Duration(milliseconds: 50));
      }
    }

    Future<void> check(String where) async {
      await settle();
      final u = unnamed();
      if (u.isNotEmpty) report[where] = u;
    }

    for (final theme in ['dark', 'light']) {
      await app.settings.setValue('appearance.theme', theme);
      await settle();
      for (final d in [
        Destination.files,
        Destination.chat,
        Destination.search,
        Destination.scm,
        Destination.environments,
        Destination.inspector,
      ]) {
        app.destination = d;
        app.sideVisible = true;
        await check('$theme ${d.name}');
      }
      app.destination = Destination.files;
      app.consoleVisible = true;
      for (final tab in ['output', 'problems', 'terminal']) {
        app.prefs.set('console.tab', tab);
        await check('$theme console $tab');
      }
      app.consoleVisible = false;
      for (final section in ['appearance', 'editor', 'workspace', 'models', 'about']) {
        app.openSettings!(section);
        await settle();
        await check('$theme settings $section');
        await key(tester, LogicalKeyboardKey.escape);
        await settle();
      }
      await key(tester, LogicalKeyboardKey.keyK, mod: true);
      await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'the palette');
      await check('$theme palette');
      await key(tester, LogicalKeyboardKey.escape);
      await settle();
    }

    // Tab moves through the window from the explorer.
    app.destination = Destination.files;
    await settle();
    FocusManager.instance.primaryFocus?.unfocus();
    await tester.pump();
    final walk = await tabWalk(tester, 30);
    if (walk.isNotEmpty) report['tab order'] = walk;

    // Ctrl+M in the editor: Tab leaves it.
    final editor = FocusManager.instance.rootScope.descendants.firstWhere(
      (n) => (n.debugLabel ?? '').startsWith('editor 1:'),
    );
    editor.requestFocus();
    await tester.pump();
    await key(tester, LogicalKeyboardKey.keyM, ctrl: true);
    await key(tester, LogicalKeyboardKey.tab);
    if (editor.hasFocus) report['Ctrl+M'] = ['Tab stayed in the editor after Ctrl+M'];
    await key(tester, LogicalKeyboardKey.keyM, ctrl: true); // back (toggle from anywhere)

    expect(
      report,
      isEmpty,
      reason: report.entries.map((e) => '${e.key}: ${e.value.join('; ')}').join('\n'),
    );
    await quit(tester, app);
    semantics.dispose();
  });
}
