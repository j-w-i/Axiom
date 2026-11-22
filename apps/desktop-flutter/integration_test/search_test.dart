// Search (FL-GIT-001), as the Tauri suite search.mjs, through the real app: Mod+Shift+F focuses
// the query; matches are grouped by file and ignored files are not searched; ↓ and Enter open the
// file at the match; Match case by keyboard drops the upper-case line; a regular expression finds
// its matches; Go to File finds a file the explorer has not loaded, ranked first for a fuzzy
// query, and opens it.
// Run: apps/desktop-flutter/scripts/e2e.sh search_test.dart

import 'dart:io';

import 'package:axiom_desktop/search/search_page.dart';
import 'package:axiom_desktop/shell/palette.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('search in the folder and Go to File', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-search-data-');
    final ws = tempDir('axiom-fl-search-ws-');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    writeFile(ws, 'src/main.rs', 'fn main() {\n    let needle = 1;\n    let NEEDLE = 2;\n}\n');
    writeFile(ws, 'src/deep/nested/needles.rs', '// needle in a haystack\n');
    writeFile(ws, 'docs/notes.md', 'No match here.\n');
    writeFile(ws, '.gitignore', 'target/\n');
    writeFile(ws, 'target/debug/needle.txt', 'needle (ignored)\n');
    await Process.run('git', ['init', '-q'], workingDirectory: ws.path);

    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.isOpen, what: 'the folder');

    // 1. Mod+Shift+F focuses the query; matches are grouped by file; ignored files are not.
    await key(tester, LogicalKeyboardKey.keyF, mod: true, shift: true);
    await until(tester, () => find.byType(SearchSide).evaluate().isNotEmpty, what: 'Search');
    final page = tester.state<SearchSideState>(find.byType(SearchSide));
    await until(tester, () => page.queryFocus.hasFocus, what: 'the query focused');
    await typeFocused(tester, 'needle');
    await until(tester, () => !app.search.running && app.search.total > 0, what: 'results');
    expect(app.search.groups.map((g) => g.path).toSet(), {
      'src/main.rs',
      'src/deep/nested/needles.rs',
    });
    expect(app.search.total, 3, reason: 'needle, NEEDLE, and the comment');
    expect(named('src/main.rs, 2 matches'), findsOneWidget);
    expect(find.textContaining('3 results in 2 files'), findsOneWidget);

    // 2. ↓ moves into the results; Enter opens the file at the match.
    final second = [for (final g in app.search.groups) ...g.matches][1];
    await key(tester, LogicalKeyboardKey.arrowDown);
    await key(tester, LogicalKeyboardKey.arrowDown);
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () =>
          app.documents.activePath == second.path &&
          app.documents.cursorText.startsWith('Ln ${second.line}, Col ${second.column}'),
      what: 'the file at the match',
      detail: () => '${app.documents.activePath} ${app.documents.cursorText}',
    );

    // 3. Tab reaches Match case; Space turns it on; the upper-case line drops out.
    page.queryFocus.requestFocus();
    await tester.pump();
    await key(tester, LogicalKeyboardKey.tab);
    if (Platform.isMacOS) {
      // A macOS text field leaves Tab to the system, which answers the real key with the
      // `insertTab:` selector (Flutter's DefaultTextEditingShortcuts); a synthesized key event
      // gets no answer, so the test sends the selector the system would.
      tester
          .state<EditableTextState>(
            find.byWidgetPredicate((w) => w is EditableText && w.focusNode == page.queryFocus),
          )
          .performSelector('insertTab:');
      await tester.pump();
    }
    final afterTab = FocusManager.instance.primaryFocus;
    await key(tester, LogicalKeyboardKey.space);
    expect(app.search.caseSensitive, isTrue, reason: 'focus after Tab: $afterTab');
    await until(
      tester,
      () => !app.search.running && app.search.total == 2,
      what: 'the case-sensitive results',
    );
    expect(named('src/main.rs, 1 match'), findsOneWidget);

    // 4. A regular expression finds its matches.
    await tester.tap(named('Regular expression'));
    page.queryFocus.requestFocus();
    await tester.pump();
    await typeFocused(tester, r'ne+dle\b');
    await until(
      tester,
      () => !app.search.running && app.search.regex && app.search.total == 2,
      what: 'the regex results',
      detail: () => '${app.search.total} ${app.search.error}',
    );

    // 6. Go to File finds a file the explorer has not loaded, first for a fuzzy query.
    expect(app.workspace.node('src/deep/nested')?.loaded ?? false, isFalse);
    await key(tester, LogicalKeyboardKey.keyP, mod: true);
    await until(tester, () => find.byType(Palette).evaluate().isNotEmpty, what: 'Go to File');
    await typeFocused(tester, 'dpnstd');
    await tester.pump(const Duration(milliseconds: 500));
    await key(tester, LogicalKeyboardKey.enter);
    await until(
      tester,
      () => app.documents.activePath == 'src/deep/nested/needles.rs',
      what: 'the deep file opened',
      detail: () => app.documents.activePath,
    );

    await quit(tester, app);
    semantics.dispose();
  });
}
