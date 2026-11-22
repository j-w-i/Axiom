// Git in the UI (FL-GIT-001), as the Tauri suite git.mjs, on a temporary repository through the
// real app: the branch in the status bar; an unsaved edit marks the gutter at once and, saved,
// the explorer and the tab show M; a new file is U; Source Control lists both and Enter opens the
// side-by-side diff; Esc closes it; a commit made outside the app clears every mark.
// Run: apps/desktop-flutter/scripts/e2e.sh git_test.dart

import 'dart:io';

import 'package:axiom_desktop/editor/code_view.dart';
import 'package:axiom_desktop/scm/scm_page.dart';
import 'package:axiom_desktop/state/git.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

Future<void> git(Directory ws, List<String> args) async {
  final r = await Process.run('git', args, workingDirectory: ws.path);
  if (r.exitCode != 0) throw StateError('git ${args.join(' ')}: ${r.stderr}');
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('git in the UI', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-git-data-');
    final ws = tempDir('axiom-fl-git-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(ws, 'README.md', '# repo\n');
    writeFile(ws, 'lib.rs', 'fn one() {}\nfn two() {}\nfn three() {}\n');
    await git(ws, ['init', '-q', '-b', 'main']);
    await git(ws, ['-c', 'user.name=t', '-c', 'user.email=t@e', 'add', '-A']);
    await git(ws, ['-c', 'user.name=t', '-c', 'user.email=t@e', 'commit', '-q', '-m', 'init']);

    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');

    // 1. The status bar shows the branch.
    await until(
      tester,
      () => named('Branch main, 0 changes').evaluate().isNotEmpty,
      what: 'the branch',
    );

    // 2. An unsaved edit marks the gutter.
    await app.documents.open('lib.rs');
    await tester.pump(const Duration(milliseconds: 300));
    final doc = app.documents.docs.firstWhere((d) => d.path == 'lib.rs');
    doc.controller.text = 'fn one() {}\nfn TWO() {}\nfn three() {}\n';
    Map<int, GutterMark> marks() =>
        tester.widget<GitMarkStrip>(find.byType(GitMarkStrip).first).marks.value;
    await until(
      tester,
      () => marks()[1] == GutterMark.modified,
      what: 'the gutter mark',
      detail: () => '${marks()}',
    );

    // Saved: M in the explorer and on the tab.
    await app.documents.save('lib.rs');
    await until(
      tester,
      () =>
          named('lib.rs, file, modified').evaluate().isNotEmpty &&
          find.bySemanticsLabel(RegExp(r'^lib\.rs, modified')).evaluate().isNotEmpty,
      what: 'M in the explorer and on the tab',
    );

    // 3. A new file is U.
    writeFile(ws, 'notes.txt', 'new\n');
    await until(
      tester,
      () => named('notes.txt, file, untracked').evaluate().isNotEmpty,
      what: 'U in the explorer',
    );

    // 4. Source Control lists both changes; Enter opens the diff.
    await key(tester, LogicalKeyboardKey.keyG, mod: true, shift: true);
    await until(tester, () => find.byType(ScmSide).evaluate().isNotEmpty, what: 'Source Control');
    await until(
      tester,
      () =>
          named('lib.rs, modified').evaluate().isNotEmpty &&
          named('notes.txt, untracked').evaluate().isNotEmpty,
      what: 'the changes listed',
    );
    expect(named('Branch main, 2 changes'), findsOneWidget);
    await tester.tap(
      find.descendant(of: find.byType(ScmSide), matching: named('lib.rs, modified')),
    );
    await until(tester, () => find.byType(DiffView).evaluate().isNotEmpty, what: 'the diff');
    await until(
      tester,
      () => named('Changes to lib.rs: 1 added, 1 removed').evaluate().isNotEmpty,
      what: 'the diff loaded',
    );
    expect(named('removed line 2: fn two() {}; added line 2: fn TWO() {}'), findsOneWidget);

    // 6. Esc closes the diff.
    tester.state<DiffViewState>(find.byType(DiffView)).focusNode.requestFocus();
    await tester.pump();
    await key(tester, LogicalKeyboardKey.escape);
    await until(tester, () => app.git.diff == null, what: 'the diff closed');

    // 7. A commit made outside the app clears the marks and the list.
    await git(ws, ['-c', 'user.name=t', '-c', 'user.email=t@e', 'add', '-A']);
    await git(ws, ['-c', 'user.name=t', '-c', 'user.email=t@e', 'commit', '-q', '-m', 'more']);
    await until(
      tester,
      () => app.git.marks.isEmpty && named('Branch main, 0 changes').evaluate().isNotEmpty,
      timeout: const Duration(seconds: 10),
      what: 'the marks cleared',
    );
    expect(named('No changes'), findsOneWidget);
    expect(named('lib.rs, file, modified'), findsNothing);
    await key(tester, LogicalKeyboardKey.keyE, mod: true, shift: true);
    await until(tester, () => marks().isEmpty, what: 'the gutter cleared');

    await quit(tester, app);
    semantics.dispose();
  });
}
