// AUDIT ONLY (disposable copy, never committed): an exploratory pass over the real app against the
// real daemon, with screenshots, timings and every framework error recorded instead of failing.

import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:axiom_desktop/app/app_state.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'support.dart';

final out = Platform.environment['AUDIT_OUT'] ?? '/tmp/audit-out';
final results = <Map<String, Object?>>[];
final errors = <String>[];
final timings = <String, num>{};

Future<void> shot(WidgetTester tester, String name) async {
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pump();
  final rv = tester.binding.renderViews.first;
  final layer = rv.debugLayer! as OffsetLayer;
  final img = await layer.toImage(rv.paintBounds);
  final bytes = await img.toByteData(format: ui.ImageByteFormat.png);
  File('$out/screenshots/$name.png')
    ..parent.createSync(recursive: true)
    ..writeAsBytesSync(bytes!.buffer.asUint8List());
}

Future<void> step(String name, Future<void> Function() fn) async {
  final before = errors.length;
  final sw = Stopwatch()..start();
  String status = 'pass';
  String? detail;
  try {
    await fn();
  } on Object catch (e, st) {
    status = 'fail';
    detail = '$e\n${st.toString().split('\n').take(6).join('\n')}';
  }
  final newErrors = errors.sublist(before);
  if (status == 'pass' && newErrors.isNotEmpty) status = 'framework-errors';
  results.add({
    'step': name,
    'status': status,
    'ms': sw.elapsedMilliseconds,
    'detail': detail,
    'frameworkErrors': newErrors.map((e) => e.split('\n').take(12).join('\n')).toList(),
  });
  // ignore: avoid_print
  print('[audit] $status $name (${sw.elapsedMilliseconds} ms) ${detail ?? ''}');
}

void type(dynamic doc, String text) {
  for (final ch in text.split('')) {
    (doc.controller as CodeLineEditingController).replaceSelection(ch);
  }
}

Future<void> settle(WidgetTester t, [int ms = 300]) async {
  final end = DateTime.now().add(Duration(milliseconds: ms));
  while (DateTime.now().isBefore(end)) {
    await t.pump(const Duration(milliseconds: 30));
  }
}

Future<void> tapLabel(WidgetTester t, String label) async {
  await t.tap(named(label).first);
  await settle(t, 250);
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('audit exploration', timeout: const Timeout(Duration(minutes: 20)), (tester) async {
    final semantics = tester.ensureSemantics();
    final oldOnError = FlutterError.onError;
    FlutterError.onError = (d) => errors.add(d.toString());
    tester.view.physicalSize = const Size(1440, 900);
    tester.view.devicePixelRatio = 1.0;

    final data = tempDir('axiom-audit-data-');
    final ws = tempDir('axiom-audit-ws-');
    // A realistic small project, plus stress cases.
    writeFile(ws, 'Cargo.toml', '[package]\nname = "demo"\nversion = "0.1.0"\nedition = "2021"\n');
    writeFile(ws, 'src/main.rs', 'use std::collections::HashMap;\n\nfn main() {\n    let mut m = HashMap::new();\n    m.insert("a", 1);\n    println!("{:?}", m);\n}\n');
    writeFile(ws, 'src/lib.rs', '/// Adds.\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n');
    writeFile(ws, 'web/app.ts', 'export function greet(name: string): string {\n  return `hi \${name}`;\n}\n');
    writeFile(ws, 'tools/script.py', 'import os\n\ndef main():\n    print(os.getcwd())\n');
    writeFile(ws, 'README.md', '# Demo\n\nA demo project.\n');
    writeFile(ws, 'crlf.txt', 'one\r\ntwo\r\nthree\r\n');
    writeFile(ws, 'unicode-ñame-日本.txt', 'unicode name\n');
    writeFile(ws, 'a-very-long-file-name-that-goes-on-and-on-and-on-and-on-for-a-while-longer-still.txt', 'long\n');
    writeFile(ws, 'deep/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/leaf.txt', 'deep\n');
    final big = StringBuffer();
    for (var i = 0; i < 50000; i++) {
      big.writeln('fn f$i(x: i32) -> i32 { let y = x * $i; y + ${i % 7} } // line $i of the big file');
    }
    writeFile(ws, 'big/big.rs', big.toString());
    final longLine = 'x' * 20000;
    writeFile(ws, 'big/longline.txt', '$longLine\n');
    for (var i = 0; i < 3000; i++) {
      writeFile(ws, 'many/file_$i.txt', 'needle $i\n');
    }
    File('${ws.path}/binary.bin').writeAsBytesSync(List.generate(4096, (i) => i % 256));
    // A git repository with a change.
    Process.runSync('git', ['init', '-q'], workingDirectory: ws.path);
    Process.runSync('git', ['add', '-A'], workingDirectory: ws.path);
    Process.runSync('git', ['-c', 'user.email=a@b', '-c', 'user.name=audit', 'commit', '-qm', 'init'], workingDirectory: ws.path);
    writeFile(ws, 'src/lib.rs', '/// Adds two numbers.\npub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n\npub fn sub(a: i32, b: i32) -> i32 { a - b }\n');
    writeFile(ws, 'new_untracked.txt', 'new\n');

    late AppState app;
    await step('boot (connect to daemon)', () async {
      final sw = Stopwatch()..start();
      app = await boot(tester, data.path);
      timings['boot_to_ready_ms'] = sw.elapsedMilliseconds;
      await settle(tester, 500);
      await shot(tester, '01-welcome-system');
    });

    for (final theme in ['dark', 'light']) {
      await step('welcome in $theme', () async {
        await app.settings.setValue('appearance.theme', theme);
        await settle(tester, 500);
        await shot(tester, '02-welcome-$theme');
      });
    }
    await app.settings.setValue('appearance.theme', 'dark');

    await step('File menu opens', () async {
      await tapLabel(tester, 'File menu');
      await shot(tester, '03-file-menu');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester);
    });
    await step('View menu opens', () async {
      await tapLabel(tester, 'View menu');
      await shot(tester, '03-view-menu');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester);
    });
    await step('Help menu + About', () async {
      await tapLabel(tester, 'Help menu');
      await shot(tester, '03-help-menu');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester);
    });

    await step('rail with no folder: each destination', () async {
      for (final d in ['Chat', 'Search', 'Source Control', 'Environments', 'Inspector', 'Files']) {
        await tapLabel(tester, d);
        await settle(tester, 600);
        await shot(tester, '04-nofolder-${d.replaceAll(' ', '_').toLowerCase()}');
      }
    });

    await step('open folder (3000+ files, git)', () async {
      final sw = Stopwatch()..start();
      await app.workspace.openFolder(ws.path);
      await until(tester, () => app.workspace.rootLoaded, what: 'root');
      timings['open_folder_to_root_listed_ms'] = sw.elapsedMilliseconds;
      await settle(tester, 1500);
      await shot(tester, '05-folder-open');
    });

    await step('expand many/ (3000 entries) in explorer', () async {
      final sw = Stopwatch()..start();
      await tester.tap(named('many, folder').first);
      await tester.pump();
      await key(tester, LogicalKeyboardKey.arrowRight);
      await until(tester, () => app.workspace.node('many')?.loaded ?? false, what: 'many listed');
      timings['expand_3000_dir_ms'] = sw.elapsedMilliseconds;
      await settle(tester, 500);
      await shot(tester, '06-explorer-many');
      await key(tester, LogicalKeyboardKey.arrowLeft);
      await settle(tester);
    });

    await step('expand deep path', () async {
      await tester.tap(named('deep, folder').first);
      await tester.pump();
      for (var i = 0; i < 18; i++) {
        await key(tester, LogicalKeyboardKey.arrowRight);
        await settle(tester, 150);
        await key(tester, LogicalKeyboardKey.arrowRight);
        await settle(tester, 150);
      }
      await shot(tester, '07-explorer-deep');
    });

    await step('open main.rs from explorer', () async {
      await app.documents.open('src/main.rs');
      await until(tester, () => app.documents.activePath == 'src/main.rs', what: 'main.rs');
      await settle(tester, 1500);
      await shot(tester, '08-editor-main-rs');
    });

    await step('open several files (tabs)', () async {
      for (final f in ['src/lib.rs', 'web/app.ts', 'tools/script.py', 'README.md', 'crlf.txt',
        'unicode-ñame-日本.txt', 'a-very-long-file-name-that-goes-on-and-on-and-on-and-on-for-a-while-longer-still.txt']) {
        await app.documents.open(f);
        await settle(tester, 200);
      }
      await settle(tester, 800);
      await shot(tester, '09-many-tabs');
    });

    await step('CRLF file not dirty after open+click', () async {
      await app.documents.open('crlf.txt');
      await settle(tester, 300);
      final d = app.documents.doc('crlf.txt')!;
      if (d.dirty) throw StateError('crlf.txt dirty right after opening');
    });

    await step('open 50k-line file and scroll', () async {
      final sw = Stopwatch()..start();
      await app.documents.open('big/big.rs');
      await until(tester, () => app.documents.activePath == 'big/big.rs', what: 'big');
      timings['open_50k_lines_ms'] = sw.elapsedMilliseconds;
      await settle(tester, 1500);
      await shot(tester, '10-big-file');
      final ed = find.byType(CodeEditor).last;
      final sw2 = Stopwatch()..start();
      for (var i = 0; i < 20; i++) {
        await tester.fling(ed, const Offset(0, -2000), 8000);
        await tester.pump(const Duration(milliseconds: 16));
      }
      await settle(tester, 500);
      timings['big_file_20_flings_ms'] = sw2.elapsedMilliseconds;
      await shot(tester, '11-big-file-scrolled');
    });

    await step('open 20k-char single line', () async {
      final sw = Stopwatch()..start();
      await app.documents.open('big/longline.txt');
      await until(tester, () => app.documents.activePath == 'big/longline.txt', what: 'longline');
      await settle(tester, 800);
      timings['open_longline_ms'] = sw.elapsedMilliseconds;
      await shot(tester, '12-long-line');
    });

    await step('open binary file', () async {
      await app.documents.open('binary.bin');
      await settle(tester, 800);
      await shot(tester, '13-binary');
    });

    await step('edit + dirty indicator + close prompt', () async {
      await app.documents.open('src/lib.rs');
      await settle(tester, 300);
      final d = app.documents.doc('src/lib.rs')!;
      d.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
      type(d, '// audit edit\n');
      await settle(tester, 300);
      await shot(tester, '14-dirty');
      await key(tester, LogicalKeyboardKey.keyW, mod: true);
      await settle(tester, 500);
      await shot(tester, '15-close-dirty-prompt');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester, 300);
      if (app.documents.doc('src/lib.rs') == null) throw StateError('Escape on the prompt closed the dirty doc');
    });

    await step('undo/redo', () async {
      final d = app.documents.doc('src/lib.rs')!;
      await key(tester, LogicalKeyboardKey.keyZ, mod: true);
      await settle(tester, 200);
      await key(tester, LogicalKeyboardKey.keyZ, mod: true, shift: true);
      await settle(tester, 200);
      results.last['note'] = 'dirty after undo/redo: ${d.dirty}';
    });

    await step('save via Mod+S', () async {
      await key(tester, LogicalKeyboardKey.keyS, mod: true);
      await until(tester, () => !(app.documents.doc('src/lib.rs')?.dirty ?? true), what: 'saved');
    });

    await step('external change conflict dialog', () async {
      final d = app.documents.doc('src/lib.rs')!;
      d.controller.selection = const CodeLineSelection.collapsed(index: 0, offset: 0);
      type(d, '// ours\n');
      await settle(tester, 200);
      File('${ws.path}/src/lib.rs').writeAsStringSync('// theirs\n');
      await until(tester, () => d.changedOnDisk, what: 'changed on disk');
      await settle(tester, 300);
      await shot(tester, '16-changed-on-disk-banner');
      await key(tester, LogicalKeyboardKey.keyS, mod: true);
      await settle(tester, 600);
      await shot(tester, '17-conflict-dialog');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester, 300);
    });

    await step('split editor right', () async {
      await key(tester, LogicalKeyboardKey.backslash, mod: true);
      await settle(tester, 800);
      await shot(tester, '18-split');
    });

    await step('command palette', () async {
      final sw = Stopwatch()..start();
      await key(tester, LogicalKeyboardKey.keyK, mod: true);
      await settle(tester, 300);
      timings['palette_open_ms'] = sw.elapsedMilliseconds;
      await shot(tester, '19-palette');
      await typeFocused(tester, 'theme');
      await settle(tester, 300);
      await shot(tester, '20-palette-query');
      await typeFocused(tester, 'zzzzqqq-no-match');
      await settle(tester, 300);
      await shot(tester, '21-palette-empty');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester);
    });

    await step('go to file among 3000 files', () async {
      final sw = Stopwatch()..start();
      await key(tester, LogicalKeyboardKey.keyP, mod: true);
      await settle(tester, 200);
      await typeFocused(tester, 'file_2999');
      await until(tester, () => find.textContaining('file_2999').evaluate().isNotEmpty, what: 'file listed');
      timings['go_to_file_query_ms'] = sw.elapsedMilliseconds;
      await shot(tester, '22-go-to-file');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester);
    });

    await step('search: needle across 3000 files', () async {
      await key(tester, LogicalKeyboardKey.keyF, mod: true, shift: true);
      await settle(tester, 300);
      final sw = Stopwatch()..start();
      await typeFocused(tester, 'needle');
      await key(tester, LogicalKeyboardKey.enter);
      await until(tester, () => find.textContaining('file_1').evaluate().isNotEmpty, what: 'results', timeout: const Duration(seconds: 30));
      timings['search_3000_files_ms'] = sw.elapsedMilliseconds;
      await settle(tester, 800);
      await shot(tester, '23-search-results');
      await typeFocused(tester, 'zzzz-nothing-matches');
      await key(tester, LogicalKeyboardKey.enter);
      await settle(tester, 1200);
      await shot(tester, '24-search-empty');
      await typeFocused(tester, '([invalid');
      await key(tester, LogicalKeyboardKey.enter);
      await settle(tester, 800);
      await shot(tester, '24b-search-weird-query');
    });

    await step('source control', () async {
      await key(tester, LogicalKeyboardKey.keyG, mod: true, shift: true);
      await settle(tester, 1500);
      await shot(tester, '25-scm');
      final f = named('src/lib.rs, modified');
      if (f.evaluate().isNotEmpty) {
        await tester.tap(f.first);
        await settle(tester, 1200);
        await shot(tester, '26-scm-diff');
      } else {
        results.last['note'] = 'no "src/lib.rs, modified" row found';
      }
    });

    await step('environments', () async {
      await tapLabel(tester, 'Environments');
      await settle(tester, 4000);
      await shot(tester, '27-environments');
    });

    await step('inspector', () async {
      await tapLabel(tester, 'Inspector');
      await settle(tester, 800);
      await shot(tester, '28-inspector');
    });

    await step('chat with no model configured', () async {
      await tapLabel(tester, 'Chat');
      await settle(tester, 800);
      await shot(tester, '29-chat');
    });

    await step('AI dock', () async {
      await tapLabel(tester, 'Files');
      await key(tester, LogicalKeyboardKey.keyL, mod: true);
      await settle(tester, 800);
      await shot(tester, '30-dock');
      await key(tester, LogicalKeyboardKey.keyL, mod: true);
      await settle(tester, 300);
    });

    await step('console: run a command', () async {
      await key(tester, LogicalKeyboardKey.keyJ, mod: true);
      await settle(tester, 800);
      await shot(tester, '31-console');
      await key(tester, LogicalKeyboardKey.keyR, mod: true, shift: true);
      await settle(tester, 400);
      await shot(tester, '32-run-command-prompt');
      await typeFocused(tester, 'echo hello-audit; seq 1 20000 | tail -3; exit 3');
      await key(tester, LogicalKeyboardKey.enter);
      await settle(tester, 2500);
      await shot(tester, '33-command-output');
    });

    await step('terminal', () async {
      await key(tester, LogicalKeyboardKey.backquote, mod: true, shift: true);
      await settle(tester, 2000);
      await shot(tester, '34-terminal');
    });

    await step('settings', () async {
      await key(tester, LogicalKeyboardKey.comma, mod: true);
      await settle(tester, 600);
      await shot(tester, '35-settings');
      await key(tester, LogicalKeyboardKey.escape);
      await settle(tester, 300);
    });

    await step('light theme workbench', () async {
      await app.settings.setValue('appearance.theme', 'light');
      await tapLabel(tester, 'Files');
      await app.documents.open('src/main.rs');
      await settle(tester, 1000);
      await shot(tester, '36-workbench-light');
      await key(tester, LogicalKeyboardKey.keyK, mod: true);
      await settle(tester, 300);
      await shot(tester, '37-palette-light');
      await key(tester, LogicalKeyboardKey.escape);
      await tapLabel(tester, 'Source Control');
      await settle(tester, 800);
      await shot(tester, '38-scm-light');
      await key(tester, LogicalKeyboardKey.comma, mod: true);
      await settle(tester, 600);
      await shot(tester, '39-settings-light');
      await key(tester, LogicalKeyboardKey.escape);
      await tapLabel(tester, 'Files');
      await app.settings.setValue('appearance.theme', 'dark');
      await settle(tester, 300);
    });

    for (final s in [const Size(1024, 700), const Size(800, 600), const Size(640, 480)]) {
      await step('window ${s.width.toInt()}x${s.height.toInt()}', () async {
        tester.view.physicalSize = s;
        await settle(tester, 800);
        await shot(tester, '40-size-${s.width.toInt()}x${s.height.toInt()}');
      });
    }
    tester.view.physicalSize = const Size(1440, 900);
    await settle(tester, 300);

    await step('high DPI 2x at 1440x900 logical 720x450', () async {
      tester.view.devicePixelRatio = 2.0;
      tester.view.physicalSize = const Size(2880, 1800);
      await settle(tester, 800);
      await shot(tester, '41-dpr2');
      tester.view.devicePixelRatio = 1.0;
      tester.view.physicalSize = const Size(1440, 900);
      await settle(tester, 300);
    });

    await step('text scale 1.5', () async {
      tester.platformDispatcher.textScaleFactorTestValue = 1.5;
      await settle(tester, 800);
      await shot(tester, '42-textscale-1.5');
      await key(tester, LogicalKeyboardKey.comma, mod: true);
      await settle(tester, 600);
      await shot(tester, '43-settings-textscale-1.5');
      await key(tester, LogicalKeyboardKey.escape);
      tester.platformDispatcher.clearTextScaleFactorTestValue();
      await settle(tester, 300);
    });

    await step('rapid toggles (side bar, console) x30', () async {
      final sw = Stopwatch()..start();
      for (var i = 0; i < 30; i++) {
        await key(tester, LogicalKeyboardKey.keyB, mod: true);
        await key(tester, LogicalKeyboardKey.keyJ, mod: true);
      }
      await settle(tester, 800);
      timings['60_toggles_ms'] = sw.elapsedMilliseconds;
      await shot(tester, '44-after-rapid-toggles');
    });

    await step('rapid palette open/close x30', () async {
      for (var i = 0; i < 30; i++) {
        await key(tester, LogicalKeyboardKey.keyK, mod: true);
        await key(tester, LogicalKeyboardKey.escape);
      }
      await settle(tester, 500);
      await shot(tester, '45-after-rapid-palette');
    });

    await step('keyboard focus traversal (Tab x25)', () async {
      final seen = <String>[];
      for (var i = 0; i < 25; i++) {
        await key(tester, LogicalKeyboardKey.tab);
        final f = FocusManager.instance.primaryFocus;
        seen.add(f?.debugLabel ?? f?.context?.widget.runtimeType.toString() ?? 'null');
      }
      results.last['note'] = seen.join(' | ');
      await shot(tester, '46-focus-after-tabs');
    });

    await step('workspace folder deleted while open', () async {
      final ws2 = tempDir('axiom-audit-ws2-');
      writeFile(ws2, 'a.txt', 'a\n');
      // switch folders: dirty lib.rs should prompt
      await app.workspace.openFolder(ws2.path);
      await settle(tester, 1000);
      await shot(tester, '47-switch-folder-dirty-prompt');
      // Don't Save if prompt is up
      final dont = find.text("Don't Save");
      if (dont.evaluate().isNotEmpty) {
        await tester.tap(dont.first);
        await settle(tester, 1500);
      }
      await until(tester, () => app.workspace.rootLoaded && app.workspace.root == ws2.path, what: 'ws2');
      await app.documents.open('a.txt');
      await settle(tester, 500);
      ws2.deleteSync(recursive: true);
      await settle(tester, 2500);
      await shot(tester, '48-folder-deleted');
    });

    await step('daemon killed: backend failed + restart', () async {
      final pid = app.client.pid;
      if (pid == null) throw StateError('no daemon pid exposed');
      Process.killPid(pid, ProcessSignal.sigkill);
      await settle(tester, 2500);
      await shot(tester, '49-backend-died');
      final r = named('Restart the Backend');
      if (r.evaluate().isNotEmpty) {
        final sw = Stopwatch()..start();
        await tester.tap(r.first);
        await until(tester, () => app.client.isReady, what: 'restarted');
        timings['restart_backend_ms'] = sw.elapsedMilliseconds;
        await settle(tester, 1500);
        await shot(tester, '50-after-restart');
      } else {
        results.last['note'] = 'no Restart the Backend control visible';
      }
    });

    await step('reduced motion setting', () async {
      await app.settings.setValue('appearance.motion', 'off');
      await settle(tester, 300);
      await key(tester, LogicalKeyboardKey.keyK, mod: true);
      await tester.pump(const Duration(milliseconds: 16));
      await shot(tester, '51-palette-motion-off');
      await key(tester, LogicalKeyboardKey.escape);
      await app.settings.setValue('appearance.motion', 'standard');
    });

    // Semantics: interactive nodes without a label.
    await step('semantics: unlabeled actionable nodes', () async {
      final unlabeled = <String>[];
      void walk(SemanticsNode n) {
        final d = n.getSemanticsData();
        final actionable = d.hasAction(SemanticsAction.tap) || d.flagsCollection.isButton || d.flagsCollection.isTextField;
        if (actionable && d.label.trim().isEmpty && d.value.trim().isEmpty && d.tooltip.trim().isEmpty) {
          unlabeled.add('${n.id} ${d.flagsCollection} rect=${n.rect}');
        }
        n.visitChildren((c) { walk(c); return true; });
      }
      final root = tester.binding.rootPipelineOwner.semanticsOwner?.rootSemanticsNode
          ?? tester.binding.renderViews.first.owner?.semanticsOwner?.rootSemanticsNode;
      if (root != null) walk(root);
      results.last['note'] = 'unlabeled actionable: ${unlabeled.length}\n${unlabeled.take(30).join('\n')}';
    });

    await step('quit (shutdown) timing', () async {
      final sw = Stopwatch()..start();
      await quit(tester, app);
      timings['shutdown_ms'] = sw.elapsedMilliseconds;
    });

    FlutterError.onError = oldOnError;
    semantics.dispose();
    tester.view.resetPhysicalSize();
    tester.view.resetDevicePixelRatio();
    File('$out/logs/explore-results.json')
      ..parent.createSync(recursive: true)
      ..writeAsStringSync(const JsonEncoder.withIndent('  ').convert({
        'timings': timings,
        'results': results,
        'frameworkErrorCount': errors.length,
        'frameworkErrors': errors.map((e) => e.split('\n').take(25).join('\n')).toList(),
      }));
    try {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    } on Object catch (_) {}
  });
}
