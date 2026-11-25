import 'dart:async';

import 'package:axiom/shell/outside.dart';
import 'package:axiom/shell/window.dart';
import 'package:flutter_test/flutter_test.dart';

/// A widget test that hangs is named within a minute. Flutter's own limit is ten, and three of
/// those outlast the job, which then leaves no log at all.
Future<void> testExecutable(FutureOr<void> Function() main) async {
  final binding = TestWidgetsFlutterBinding.ensureInitialized();
  if (binding is AutomatedTestWidgetsFlutterBinding) {
    binding.defaultTestTimeout = const Timeout(Duration(minutes: 1));
  }
  // No test opens anything.
  folders = (at) => throw StateError('a test would have opened a window');
  window = (what) => throw StateError('a test would have asked the window to $what');
  opens = (u) => throw StateError('a test would have opened $u');
  await main();
}
