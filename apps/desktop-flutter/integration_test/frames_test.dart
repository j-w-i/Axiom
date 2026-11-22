// Frame timings of the shell's interactions (ADR-0020 §5), from the engine's FrameTiming (build
// and raster per frame). Run in a profile build, in a private compositor at the owner's scale:
//   apps/desktop-flutter/scripts/frames.sh
// Every frame of every interaction must build and raster within the display's frame (16.7 ms).

import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/ui/splitter.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:re_editor/re_editor.dart';

import 'frame_report.dart';
import 'editor_frames_test.dart' show bigRust;
import 'support.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  binding.framePolicy = LiveTestWidgetsFlutterBindingFramePolicy.fullyLive;

  testWidgets('frame timings', (tester) async {
    final data = tempDir('axiom-fl-frames-data-');
    final ws = tempDir('axiom-fl-frames-ws-');
    // A folder long enough to scroll: 3000 files.
    for (final name in ['a', 'b', 'c']) {
      writeFile(ws, 'big/$name.rs', bigRust(5000));
    }
    for (var i = 0; i < 3000; i++) {
      writeFile(ws, 'many/file_${i.toString().padLeft(4, '0')}.txt', '$i\n');
    }
    addTearDown(() {
      data.deleteSync(recursive: true);
      ws.deleteSync(recursive: true);
    });
    final app = await boot(tester, data.path);
    await app.workspace.openFolder(ws.path);
    await until(tester, () => app.workspace.rootLoaded, what: 'the root');
    app.workspace.setExpanded('many', true);
    await until(tester, () => app.workspace.node('many')?.loaded ?? false, what: 'the long folder');
    await Future<void>.delayed(const Duration(seconds: 1));

    final reports = <Report>[];
    Future<void> measure(String name, Future<void> Function() body) async =>
        reports.add(await measureFrames(name, body));

    try {
      await _interactions(tester, app, measure);
    } catch (e, st) {
      // ignore: avoid_print
      print('FRAMES ERROR $e\n$st');
      rethrow;
    }

    final size = tester.view.physicalSize;
    final line =
        'window ${size.width.round()}x${size.height.round()} physical, dpr ${tester.view.devicePixelRatio}';
    // ignore: avoid_print
    print(line);
    for (final r in reports) {
      // ignore: avoid_print
      print(r);
    }
    binding.reportData = {'window': line, for (final r in reports) r.name: r.toJson()};
    await app.shutdown();
    if (Platform.environment['AXIOM_FRAMES_ASSERT'] != '0') {
      expect(
        [
          for (final r in reports)
            if (r.over > 0) '${r.name}: ${r.over} of ${r.frames.length}',
        ],
        isEmpty,
        reason: 'frames over $budgetMs ms',
      );
    }
  });
}

Future<void> _interactions(
  WidgetTester tester,
  AppState app,
  Future<void> Function(String name, Future<void> Function() body) measure,
) async {
  final list = find.byType(Scrollable).first;
  await measure('explorer fling', () async {
    for (var i = 0; i < 4; i++) {
      await tester.fling(list, const Offset(0, -1600), 4000);
      await Future<void>.delayed(const Duration(milliseconds: 900));
    }
    for (var i = 0; i < 2; i++) {
      await tester.fling(list, const Offset(0, 1600), 4000);
      await Future<void>.delayed(const Duration(milliseconds: 900));
    }
  });
  await measure('explorer wheel', () async {
    final g = await tester.createGesture(kind: PointerDeviceKind.mouse);
    await g.addPointer(location: tester.getCenter(list));
    for (var i = 0; i < 60; i++) {
      await tester.sendEventToBinding(
        PointerScrollEvent(position: tester.getCenter(list), scrollDelta: const Offset(0, 120)),
      );
      await Future<void>.delayed(const Duration(milliseconds: 16));
    }
    await g.removePointer();
  });
  await measure('side bar toggle', () async {
    for (var i = 0; i < 8; i++) {
      await key(tester, LogicalKeyboardKey.keyB, mod: true);
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  });
  await measure('palette open/close', () async {
    for (var i = 0; i < 6; i++) {
      await key(tester, LogicalKeyboardKey.keyK, mod: true);
      await Future<void>.delayed(const Duration(milliseconds: 250));
      await key(tester, LogicalKeyboardKey.escape);
      await Future<void>.delayed(const Duration(milliseconds: 250));
    }
  });
  await measure('splitter drag', () async {
    final at = tester.getCenter(find.byType(AxSplitter));
    final g = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
    for (var i = 0; i < 60; i++) {
      await g.moveBy(Offset(i < 30 ? 6 : -6, 0));
      await Future<void>.delayed(const Duration(milliseconds: 16));
    }
    await g.up();
  });
  await measure('theme switch', () async {
    for (final th in ['dark', 'light', 'dark']) {
      await app.settings.setValue('appearance.theme', th);
      await Future<void>.delayed(const Duration(milliseconds: 400));
    }
  });

  // The editor inside the shell: three 5000-line files, Ctrl+Tab between them, typing, scrolling.
  for (final n in ['a', 'b', 'c']) {
    await app.documents.open('big/$n.rs');
  }
  await Future<void>.delayed(const Duration(seconds: 1));
  await measure('tab switching', () async {
    for (var i = 0; i < 12; i++) {
      await key(tester, LogicalKeyboardKey.tab, ctrl: true);
      await Future<void>.delayed(const Duration(milliseconds: 150));
    }
  });
  final doc = app.documents.active!;
  doc.controller.selection = const CodeLineSelection.collapsed(index: 40, offset: 4);
  await measure('typing in the shell', () async {
    for (final ch in 'let value = compute(items, 42); // typed\n'.split('')) {
      doc.controller.replaceSelection(ch);
      await Future<void>.delayed(const Duration(milliseconds: 40));
    }
  });
  final view = find.byType(CodeEditor).first;
  // Selecting text: a mouse drag down and back over the lines, and Shift+Down held.
  await measure('drag selection in the shell', () async {
    final box = tester.getRect(view);
    final from = Offset(box.left + 120, box.top + 60);
    final g = await tester.startGesture(from, kind: PointerDeviceKind.mouse);
    for (var i = 1; i <= 60; i++) {
      await g.moveTo(from + Offset(i * 6.0, i * 7.0));
      await Future<void>.delayed(const Duration(milliseconds: 16));
    }
    for (var i = 60; i >= 0; i--) {
      await g.moveTo(from + Offset(i * 6.0, i * 7.0));
      await Future<void>.delayed(const Duration(milliseconds: 16));
    }
    await g.up();
  });
  doc.controller.selection = const CodeLineSelection.collapsed(index: 10, offset: 0);
  await measure('shift+down selection in the shell', () async {
    for (var i = 0; i < 40; i++) {
      await key(tester, LogicalKeyboardKey.arrowDown, shift: true);
      await Future<void>.delayed(const Duration(milliseconds: 33));
    }
  });
  await measure('editor fling in the shell', () async {
    for (var i = 0; i < 3; i++) {
      await tester.fling(view, const Offset(0, -1500), 5000);
      await Future<void>.delayed(const Duration(milliseconds: 900));
    }
  });
}
