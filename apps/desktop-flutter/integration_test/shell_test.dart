// The shell end to end (FL-SHELL-001, FL-FILES-001), against the real daemon: connecting, opening
// a folder, the explorer by keyboard, the hint line, the palette, the side bar and its splitter,
// settings, and a restart that brings the folder, the layout and the theme back.
// Run: apps/desktop-flutter/scripts/e2e.sh shell_test.dart

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/files/explorer.dart';
import 'package:axiom_desktop/state/settings.dart';
import 'package:axiom_desktop/ui/splitter.dart';
import 'package:axiom_desktop/ui/theme.dart';
import 'package:axiom_desktop/ui/tokens.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the shell end to end', (tester) async {
    final semantics = tester.ensureSemantics();
    final data = tempDir('axiom-fl-e2e-data-');
    final ws = tempDir('axiom-fl-e2e-ws-');
    writeFile(ws, 'src/main.rs', 'fn main() {}\n');
    writeFile(ws, 'src/deep/x.txt', 'x\n');
    writeFile(ws, 'README.md', '# hi\n');
    writeFile(ws, '.gitignore', 'target/\n');
    writeFile(ws, 'target/debug/out', '');
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });

    // 1. Connects; the welcome page answers first.
    var app = await boot(tester, data.path);
    await until(
      tester,
      () => find.textContaining('Connected — axiomd').evaluate().isNotEmpty,
      what: 'the status',
    );
    expect(find.text('No folder open'), findsWidgets);

    // 2. A folder opens; the explorer lists it lazily, ignored entries marked.
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'the root listing');
    await tester.pump(const Duration(milliseconds: 100));
    expect(named('src, folder'), findsOneWidget);
    expect(named('target, folder, ignored'), findsOneWidget);
    expect(named('README.md, file'), findsOneWidget);

    // 3. The explorer by keyboard: click a row, ↓ moves, → expands, ← collapses.
    await tester.tap(named('README.md, file'));
    await tester.pump();
    await key(tester, LogicalKeyboardKey.home);
    await tester.pump(const Duration(milliseconds: 100));
    // The first row is ".gitignore" (dot files first in the daemon's order) or "src".
    final first = app.workspace.visibleRows().first.node.path;
    expect(first, isNotEmpty);
    final srcIndex = app.workspace.visibleRows().indexWhere((r) => r.node.path == 'src');
    for (var i = 0; i < srcIndex; i++) {
      await key(tester, LogicalKeyboardKey.arrowDown);
    }
    await key(tester, LogicalKeyboardKey.arrowRight);
    await until(tester, () => app.workspace.node('src')?.loaded ?? false, what: 'src listed');
    await tester.pump(const Duration(milliseconds: 100));
    expect(named('main.rs, file'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.arrowLeft);
    await tester.pump();
    expect(named('main.rs, file'), findsNothing);
    await key(tester, LogicalKeyboardKey.arrowRight); // expanded again, for the restart check

    // 4. The hint line follows hover.
    final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await gesture.addPointer(location: tester.getCenter(named('Settings')));
    await tester.pump();
    expect(app.hint.value, 'Settings — Mod+,');
    await gesture.moveTo(Offset.zero);
    await gesture.removePointer();

    // 5. The palette: Mod+K, type, Enter runs; the theme switches.
    await key(tester, LogicalKeyboardKey.keyK, mod: true);
    await tester.pump(const Duration(milliseconds: 100));
    expect(named('Command palette'), findsOneWidget);
    await typeFocused(tester, 'theme dark');
    await tester.pump();
    await key(tester, LogicalKeyboardKey.enter);
    await until(tester, () => app.settings.theme == ThemeSetting.dark, what: 'the dark theme');
    await tester.pump();
    expect(named('Command palette'), findsNothing);
    expect(AxiomTheme.of(tester.element(find.byType(Explorer))).colors.brightness, Brightness.dark);

    // 6. The side bar: Mod+B hides and shows it; the splitter answers → (+16) and Home.
    await key(tester, LogicalKeyboardKey.keyB, mod: true);
    expect(app.sideVisible, isFalse);
    expect(find.byType(AxSplitter), findsNothing);
    await key(tester, LogicalKeyboardKey.keyB, mod: true);
    expect(app.sideVisible, isTrue);
    final w0 = app.sideWidth;
    tester.widget<AxSplitter>(find.byType(AxSplitter)).focusNode!.requestFocus();
    await tester.pump();
    await key(tester, LogicalKeyboardKey.arrowRight);
    expect(app.sideWidth, w0 + 16);
    await key(tester, LogicalKeyboardKey.home);
    expect(app.sideWidth, AppState.sideDefault);
    await key(tester, LogicalKeyboardKey.arrowRight);
    await key(tester, LogicalKeyboardKey.arrowRight);
    final wide = app.sideWidth;

    // 7. Settings: Mod+, opens it; Escape closes it.
    await key(tester, LogicalKeyboardKey.comma, mod: true);
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Appearance'), findsOneWidget);
    await key(tester, LogicalKeyboardKey.escape);
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.text('Appearance'), findsNothing);

    // 8. Restart: the folder, its expanded folders, the side bar width and the theme come back.
    await quit(tester, app);
    app = await boot(tester, data.path);
    await until(
      tester,
      () => app.workspace.isOpen && app.workspace.rootLoaded,
      what: 'the folder reopened',
    );
    await until(tester, () => app.workspace.node('src')?.loaded ?? false, what: 'src restored');
    await tester.pump(const Duration(milliseconds: 100));
    expect(named('main.rs, file'), findsOneWidget);
    expect(app.sideWidth, wide);
    expect(app.settings.theme, ThemeSetting.dark);
    expect(app.settings.motion, MotionMode.standard);
    await quit(tester, app);
    semantics.dispose();
  });
}
