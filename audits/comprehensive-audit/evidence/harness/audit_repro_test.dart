// AUDIT ONLY (disposable copy): focused reproductions of suspected defects, each in a fresh state.

import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:axiom_desktop/app/app_state.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'support.dart';

final out = Platform.environment['AUDIT_OUT'] ?? '/tmp/audit-out';
final results = <String, Object?>{};
final errors = <String>[];

Future<void> shot(WidgetTester tester, String name) async {
  for (var i = 0; i < 10; i++) {
    await tester.pump(const Duration(milliseconds: 40));
  }
  final rv = tester.binding.renderViews.first;
  final layer = rv.debugLayer! as OffsetLayer;
  final img = await layer.toImage(rv.paintBounds);
  final bytes = await img.toByteData(format: ui.ImageByteFormat.png);
  File('$out/screenshots/repro-$name.png')
    ..parent.createSync(recursive: true)
    ..writeAsBytesSync(bytes!.buffer.asUint8List());
}

Future<void> settle(WidgetTester t, [int ms = 300]) async {
  final end = DateTime.now().add(Duration(milliseconds: ms));
  while (DateTime.now().isBefore(end)) {
    await t.pump(const Duration(milliseconds: 30));
  }
}

Future<void> rec(String name, Future<Object?> Function() fn) async {
  final before = errors.length;
  Object? v;
  try {
    v = await fn();
  } on Object catch (e, st) {
    v = 'EXCEPTION $e ${st.toString().split('\n').take(4).join(' / ')}';
  }
  results[name] = {
    'value': v,
    'frameworkErrors': errors.sublist(before).map((e) => e.split('\n').take(14).join('\n')).toList(),
  };
  // ignore: avoid_print
  print('[repro] $name => $v (${errors.length - before} framework errors)');
}

void type(CodeLineEditingController c, String text) {
  for (final ch in text.split('')) {
    c.replaceSelection(ch);
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('audit repros', timeout: const Timeout(Duration(minutes: 15)), (tester) async {
    final semantics = tester.ensureSemantics();
    final oldOnError = FlutterError.onError;
    FlutterError.onError = (d) => errors.add(d.toString());
    tester.view.physicalSize = const Size(1180, 760);
    tester.view.devicePixelRatio = 1.0;
    final data = tempDir('axiom-audit-repro-data-');
    final ws = tempDir('axiom-audit-repro-ws-');
    writeFile(ws, 'src/lib.rs', '/// Adds.\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n');
    writeFile(ws, 'notes.txt', 'alpha\nbeta\n');
    final big = StringBuffer();
    for (var i = 0; i < 3000; i++) {
      big.writeln('line $i');
    }
    writeFile(ws, 'big.txt', big.toString());
    for (var i = 0; i < 16; i++) {
      writeFile(ws, 'tabs/file_number_$i.txt', 'tab $i\n');
    }
    Process.runSync('git', ['init', '-q'], workingDirectory: ws.path);
    Process.runSync('git', ['add', '-A'], workingDirectory: ws.path);
    Process.runSync('git', ['-c', 'user.email=a@b', '-c', 'user.name=audit', 'commit', '-qm', 'init'], workingDirectory: ws.path);
    writeFile(ws, 'src/lib.rs', '/// Adds two numbers.\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 { a - b }\n');

    var app = await boot(tester, data.path);
    await app.settings.setValue('appearance.theme', 'dark');
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'root');
    await settle(tester, 800);

    // R1: git gutter marks in a fresh session, a modified file opened first.
    await rec('R1 gutter fresh', () async {
      await app.documents.open('src/lib.rs');
      await settle(tester, 2500);
      await shot(tester, 'R1-gutter-fresh');
      return 'see screenshot';
    });
    // R1b: after scrolling another document and coming back.
    await rec('R1b gutter after scrolling big.txt', () async {
      await app.documents.open('big.txt');
      await settle(tester, 800);
      await tester.fling(find.byType(CodeEditor).last, const Offset(0, -3000), 6000);
      await settle(tester, 1500);
      await app.documents.open('src/lib.rs');
      await settle(tester, 1500);
      await shot(tester, 'R1b-gutter-after-scroll');
      return 'see screenshot';
    });
    // R1c: scroll lib.rs itself? (short file). Type a line at the top: marks should move.
    await rec('R1c gutter after typing a line', () async {
      final d = app.documents.doc('src/lib.rs')!;
      d.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
      type(d.controller, '// new first line\n');
      await settle(tester, 2500);
      await shot(tester, 'R1c-gutter-after-typing');
      return 'dirty=${d.dirty}';
    });

    // R2: daemon crash with an unsaved buffer (requirement N3).
    await rec('R2 unsaved buffer survives daemon crash', () async {
      final d = app.documents.doc('src/lib.rs')!;
      final textBefore = d.text;
      final dirtyBefore = d.dirty;
      Process.killPid(app.client.pid!, ProcessSignal.sigkill);
      await settle(tester, 2000);
      await shot(tester, 'R2-after-daemon-kill');
      final docsAfterKill = app.documents.docs.map((x) => '${x.path}:${x.dirty}').toList();
      await tester.tap(named('Restart the Backend').first);
      await until(tester, () => app.client.isReady, what: 'restart');
      await settle(tester, 2500);
      await shot(tester, 'R2-after-restart');
      final again = app.documents.doc('src/lib.rs');
      return {
        'dirtyBefore': dirtyBefore,
        'docsAfterKill': docsAfterKill,
        'workspaceOpenAfterRestart': app.workspace.isOpen,
        'libOpenAfterRestart': again != null,
        'textPreserved': again?.text == textBefore,
        'onDisk': File('${ws.path}/src/lib.rs').readAsStringSync().split('\n').first,
      };
    });

    if (!app.workspace.isOpen) {
      await app.workspace.openFolder(ws.path);
      await until(tester, () => app.workspace.rootLoaded, what: 'root again');
    }

    // R3: tab overflow: 16 tabs; is the active (last opened) tab visible?
    await rec('R3 tab overflow', () async {
      for (var i = 0; i < 16; i++) {
        await app.documents.open('tabs/file_number_$i.txt');
        await settle(tester, 120);
      }
      await settle(tester, 800);
      await shot(tester, 'R3-tabs-16');
      final active = find.bySemanticsLabel(RegExp('file_number_15.txt'));
      final visible = active.evaluate().map((e) {
        final box = e.renderObject as RenderBox?;
        if (box == null || !box.hasSize) return 'nobox';
        final r = box.localToGlobal(Offset.zero) & box.size;
        return r.toString();
      }).toList();
      return {'activeTabRects': visible, 'viewWidth': tester.view.physicalSize.width};
    });

    // R4: side bar at its max and the dock at its max on the default window (1180x760).
    await rec('R4 side 520 + dock 720 at 1180x760', () async {
      app.sideWidth = 520;
      app.dockWidth = 720;
      if (!app.dockVisible) await key(tester, LogicalKeyboardKey.keyL, mod: true);
      await settle(tester, 1000);
      await shot(tester, 'R4-side-dock-max');
      return 'dockVisible=${app.dockVisible}';
    });
    // R5: split at the minimum window (900x560) with the dock at its default width.
    await rec('R5 split at 900x560 with dock 380 and side 260', () async {
      app.sideWidth = 260;
      app.dockWidth = 380;
      tester.view.physicalSize = const Size(900, 560);
      await app.documents.open('notes.txt');
      await settle(tester, 300);
      await key(tester, LogicalKeyboardKey.backslash, mod: true);
      await settle(tester, 1000);
      await shot(tester, 'R5-split-min-window');
      return 'split=${app.documents.split != null}';
    });
    // R6: console at its max height on the minimum window.
    await rec('R6 console 640 at 900x560', () async {
      if (app.dockVisible) await key(tester, LogicalKeyboardKey.keyL, mod: true);
      app.consoleHeight = 640;
      if (!app.consoleVisible) await key(tester, LogicalKeyboardKey.keyJ, mod: true);
      await settle(tester, 1000);
      await shot(tester, 'R6-console-max-min-window');
      return 'consoleVisible=${app.consoleVisible}';
    });
    tester.view.physicalSize = const Size(1180, 760);
    app.consoleHeight = 220;
    await settle(tester, 500);

    // R7: binary error banner persists on other documents?
    await rec('R7 error banner after opening a text file', () async {
      File('${ws.path}/blob.bin').writeAsBytesSync(List.generate(512, (i) => i % 256));
      await app.documents.open('blob.bin');
      await settle(tester, 600);
      await app.documents.open('notes.txt');
      await settle(tester, 600);
      await shot(tester, 'R7-banner-after-switch');
      return {'banner': app.documents.banner};
    });

    // R8: a file deleted on disk while open (clean) and while dirty.
    await rec('R8 open file deleted on disk', () async {
      await app.documents.open('tabs/file_number_3.txt');
      await settle(tester, 300);
      final d = app.documents.doc('tabs/file_number_3.txt')!;
      d.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
      type(d.controller, 'dirty ');
      await settle(tester, 200);
      File('${ws.path}/tabs/file_number_3.txt').deleteSync();
      await settle(tester, 2000);
      await shot(tester, 'R8-dirty-file-deleted');
      final stillOpen = app.documents.doc('tabs/file_number_3.txt') != null;
      await key(tester, LogicalKeyboardKey.keyS, mod: true);
      await settle(tester, 1500);
      await shot(tester, 'R8-save-after-delete');
      return {
        'stillOpen': stillOpen,
        'dirtyAfterSave': app.documents.doc('tabs/file_number_3.txt')?.dirty,
        'existsAfterSave': File('${ws.path}/tabs/file_number_3.txt').existsSync(),
      };
    });

    // R9: quitting with dirty files asks; Cancel keeps the app.
    await rec('R9 tab state after restart', () async {
      await app.documents.saveAll();
      await settle(tester, 500);
      await quit(tester, app);
      app = await boot(tester, data.path);
      await settle(tester, 2500);
      await shot(tester, 'R9-after-restart');
      return {
        'workspaceReopened': app.workspace.isOpen,
        'docs': app.documents.docs.map((d) => d.path).toList(),
        'active': app.documents.activePath,
      };
    });

    await quit(tester, app);
    FlutterError.onError = oldOnError;
    semantics.dispose();
    tester.view.resetPhysicalSize();
    tester.view.resetDevicePixelRatio();
    File('$out/logs/repro-results.json')
      ..parent.createSync(recursive: true)
      ..writeAsStringSync(const JsonEncoder.withIndent('  ').convert(results));
    try {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    } on Object catch (_) {}
  });
}
