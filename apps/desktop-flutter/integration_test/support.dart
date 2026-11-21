// Shared helpers for the end-to-end suites: boot the real app against the real daemon with a
// temporary data directory, wait for conditions, find by accessible name.

import 'dart:io';

import 'package:axiom_desktop/app/app_state.dart';
import 'package:axiom_desktop/app/args.dart';
import 'package:axiom_desktop/main.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

String daemonPath() {
  final p = Platform.environment['AXIOMD_UNDER_TEST'];
  if (p == null || p.isEmpty || !File(p).existsSync()) {
    throw StateError('AXIOMD_UNDER_TEST is not set to the axiomd binary (scripts/e2e.sh sets it)');
  }
  return p;
}

/// Boots the app (as `axiom --daemon … --data-dir <dataDir>`) and waits until it is connected.
Future<AppState> boot(WidgetTester tester, String dataDir) async {
  final app = await AppState.create(AppArgs(daemon: daemonPath(), dataDir: dataDir));
  await tester.pumpWidget(AxiomApp(app: app));
  await app.start();
  await until(tester, () => app.client.isReady && app.settings.loaded, what: 'the backend');
  return app;
}

/// Quits the app the way the window's close does (state saved, daemon stopped).
Future<void> quit(WidgetTester tester, AppState app) async {
  await app.shutdown();
  await tester.pumpWidget(const SizedBox.shrink());
  await tester.pump();
}

/// Pumps until [cond] holds; fails after [timeout].
Future<void> until(
  WidgetTester tester,
  bool Function() cond, {
  Duration timeout = const Duration(seconds: 20),
  String what = 'condition',
}) async {
  final end = DateTime.now().add(timeout);
  while (!cond()) {
    if (DateTime.now().isAfter(end)) fail('timed out waiting for $what');
    await tester.pump(const Duration(milliseconds: 50));
  }
}

/// Physical keys for the keys the suites press: profile builds (the frame benchmark) have no
/// logical-to-physical table in the key simulator.
final _physical = <LogicalKeyboardKey, PhysicalKeyboardKey>{
  LogicalKeyboardKey.controlLeft: PhysicalKeyboardKey.controlLeft,
  LogicalKeyboardKey.metaLeft: PhysicalKeyboardKey.metaLeft,
  LogicalKeyboardKey.shiftLeft: PhysicalKeyboardKey.shiftLeft,
  LogicalKeyboardKey.escape: PhysicalKeyboardKey.escape,
  LogicalKeyboardKey.enter: PhysicalKeyboardKey.enter,
  LogicalKeyboardKey.tab: PhysicalKeyboardKey.tab,
  LogicalKeyboardKey.home: PhysicalKeyboardKey.home,
  LogicalKeyboardKey.end: PhysicalKeyboardKey.end,
  LogicalKeyboardKey.arrowUp: PhysicalKeyboardKey.arrowUp,
  LogicalKeyboardKey.arrowDown: PhysicalKeyboardKey.arrowDown,
  LogicalKeyboardKey.arrowLeft: PhysicalKeyboardKey.arrowLeft,
  LogicalKeyboardKey.arrowRight: PhysicalKeyboardKey.arrowRight,
  LogicalKeyboardKey.comma: PhysicalKeyboardKey.comma,
  LogicalKeyboardKey.backslash: PhysicalKeyboardKey.backslash,
  LogicalKeyboardKey.keyB: PhysicalKeyboardKey.keyB,
  LogicalKeyboardKey.keyK: PhysicalKeyboardKey.keyK,
  LogicalKeyboardKey.keyO: PhysicalKeyboardKey.keyO,
  LogicalKeyboardKey.keyP: PhysicalKeyboardKey.keyP,
  LogicalKeyboardKey.keyS: PhysicalKeyboardKey.keyS,
  LogicalKeyboardKey.keyJ: PhysicalKeyboardKey.keyJ,
  LogicalKeyboardKey.keyL: PhysicalKeyboardKey.keyL,
  LogicalKeyboardKey.keyW: PhysicalKeyboardKey.keyW,
};

Future<void> _down(WidgetTester t, LogicalKeyboardKey k) =>
    t.sendKeyDownEvent(k, physicalKey: _physical[k]);
Future<void> _up(WidgetTester t, LogicalKeyboardKey k) =>
    t.sendKeyUpEvent(k, physicalKey: _physical[k]);

Future<void> key(
  WidgetTester tester,
  LogicalKeyboardKey k, {
  bool mod = false,
  bool shift = false,
}) async {
  final modKey = Platform.isMacOS ? LogicalKeyboardKey.metaLeft : LogicalKeyboardKey.controlLeft;
  if (mod) await _down(tester, modKey);
  if (shift) await _down(tester, LogicalKeyboardKey.shiftLeft);
  await _down(tester, k);
  await _up(tester, k);
  if (shift) await _up(tester, LogicalKeyboardKey.shiftLeft);
  if (mod) await _up(tester, modKey);
  await tester.pump(const Duration(milliseconds: 50));
}

/// A widget by its accessible name (exact).
Finder named(String label) => find.bySemanticsLabel(label);

Directory tempDir(String prefix) => Directory.systemTemp.createTempSync(prefix);

void writeFile(Directory root, String path, String text) {
  final f = File('${root.path}/$path');
  f.parent.createSync(recursive: true);
  f.writeAsStringSync(text);
}
