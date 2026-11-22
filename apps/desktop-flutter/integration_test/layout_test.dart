// The layout holds at every window size the window allows and at larger text (audit batch 0,
// AUD-060; it guards AUD-003, AUD-004, AUD-014, AUD-016, AUD-017, AUD-020): no framework error
// (overflow, a throwing build) anywhere, and three geometry rules — the Git gutter strip spans
// the editor, the active tab is inside its strip, the side bar leaves the editor its minimum.
// Run: apps/desktop-flutter/scripts/e2e.sh layout_test.dart

import 'dart:io';

import 'package:axiom_desktop/editor/code_view.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'support.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the layout at every allowed size and at larger text', (tester) async {
    final semantics = tester.ensureSemantics();
    final errors = <String>[];
    final previous = FlutterError.onError;
    FlutterError.onError = (d) => errors.add(d.toString().split('\n').take(8).join('\n'));
    // Problems are collected and checked once the handler is restored (an expect() failing while
    // it is overridden hides the failure).
    final problems = <String>[];
    void want(bool ok, String what) {
      if (!ok) {
        problems.add(what);
        // ignore: avoid_print
        print('LAYOUT PROBLEM: $what');
      }
    }

    try {
      final data = tempDir('axiom-layout-data-');
      final ws = tempDir('axiom-layout-ws-');
      addTearDown(() {
        data.deleteSync(recursive: true);
        ws.deleteSync(recursive: true);
      });
      final deep = List.generate(18, (i) => 'level$i').join('/');
      writeFile(ws, '$deep/leaf_with_a_long_name.txt', 'leaf\n');
      writeFile(ws, 'src/lib.rs', '/// One.\npub fn one() -> i32 {\n    1\n}\n');
      for (var i = 0; i < 20; i++) {
        writeFile(ws, 'tabs/file_with_a_name_$i.txt', '$i\n');
      }
      Process.runSync('git', ['init', '-q'], workingDirectory: ws.path);
      Process.runSync('git', ['add', '-A'], workingDirectory: ws.path);
      Process.runSync('git', [
        '-c',
        'user.email=a@b',
        '-c',
        'user.name=t',
        'commit',
        '-qm',
        'init',
      ], workingDirectory: ws.path);
      // The last lines change: their marks are far down the editor.
      writeFile(ws, 'src/lib.rs', '/// One.\npub fn one() -> i32 {\n    1\n}\n\npub fn two() {}\n');

      tester.view.devicePixelRatio = 1;
      tester.view.physicalSize = const Size(1180, 760);

      final app = await boot(tester, data.path);
      await app.workspace.openFolder(ws.path);
      await until(tester, () => app.workspace.rootLoaded, what: 'the folder');
      // Every level of the deep tree expanded.
      var path = '';
      for (final part in deep.split('/')) {
        path = path.isEmpty ? part : '$path/$part';
        app.workspace.setExpanded(path, true);
        final p = path;
        await until(tester, () => app.workspace.node(p)?.loaded ?? false, what: p);
      }
      for (var i = 0; i < 20; i++) {
        await app.documents.open('tabs/file_with_a_name_$i.txt');
      }
      await app.documents.open('src/lib.rs');
      await until(tester, () => app.documents.activePath == 'src/lib.rs', what: 'lib.rs');
      await until(tester, () => app.git.marks.isNotEmpty, what: 'git marks');
      await tester.pump(const Duration(milliseconds: 800));

      Future<void> check(String where) async {
        for (var i = 0; i < 15; i++) {
          await tester.pump(const Duration(milliseconds: 40));
        }
        if (errors.isNotEmpty) {
          problems.add('$where: ${errors.length} framework errors, first: ${errors.first}');
          errors.clear();
        }
        // The gutter strip spans the editor (AUD-003).
        // The ones laid out on screen (views kept mounted off stage are not).
        Finder shown(Finder f) => find.byElementPredicate(
          (el) =>
              f.evaluate().contains(el) &&
              el.renderObject is RenderBox &&
              (el.renderObject! as RenderBox).hasSize &&
              (el.renderObject! as RenderBox).attached,
        );
        final strip = shown(find.byType(GitMarkStrip));
        final editor = shown(find.byType(CodeEditor));
        if (strip.evaluate().isNotEmpty && editor.evaluate().isNotEmpty) {
          final s = tester.getRect(strip.first), e = tester.getRect(editor.first);
          want((s.top - e.top).abs() < 2, '$where: gutter strip top ${s.top} vs editor ${e.top}');
          want(s.height > e.height * 0.9, '$where: gutter strip height ${s.height} of ${e.height}');
        }
        // The active tab is inside the window (AUD-017).
        final active = find.bySemanticsLabel(RegExp(r'^lib\.rs')).hitTestable();
        if (active.evaluate().isNotEmpty) {
          final r = tester.getRect(active.first);
          want(r.right <= tester.view.physicalSize.width + 1 && r.left >= 0, '$where: tab at $r');
        }
      }

      for (final scale in [1.0, 1.3]) {
        tester.platformDispatcher.textScaleFactorTestValue = scale;
        for (final size in [const Size(900, 560), const Size(1180, 760), const Size(1920, 1080)]) {
          tester.view.physicalSize = size;
          await check('${size.width.toInt()}x${size.height.toInt()} at $scale');
        }
      }
      tester.platformDispatcher.clearTextScaleFactorTestValue();

      // The dock, a split and the console at the minimum window (AUD-004, AUD-014).
      tester.view.physicalSize = const Size(900, 560);
      await key(tester, LogicalKeyboardKey.keyL, mod: true);
      await key(tester, LogicalKeyboardKey.backslash, mod: true);
      await check('900x560, dock and split');
      await key(tester, LogicalKeyboardKey.keyJ, mod: true);
      app.consoleHeight = 640;
      await check('900x560, console at its largest');
      // Side bar and dock at their largest on the default window (AUD-014).
      tester.view.physicalSize = const Size(1180, 760);
      app.consoleHeight = 220;
      app.documents.unsplit();
      app.sideWidth = 520;
      app.dockWidth = 720;
      await check('1180x760, side bar and dock at their largest');
      final editor = find.byType(CodeEditor);
      want(editor.evaluate().isNotEmpty, 'the editor is still shown');
      if (editor.evaluate().isNotEmpty) {
        want(
          tester.getSize(editor.first).width >= 240,
          'editor width ${tester.getSize(editor.first)}',
        );
      }

      app.sideWidth = 260;
      app.dockWidth = 380;
      await quit(tester, app);
      semantics.dispose();
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
      tester.platformDispatcher.clearTextScaleFactorTestValue();
    } finally {
      // Restored before any expect or failure reaches the test binding.
      FlutterError.onError = previous;
    }
    expect(problems, isEmpty);
  });
}
