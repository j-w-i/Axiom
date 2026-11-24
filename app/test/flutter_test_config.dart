import 'dart:async';

import 'package:flutter_test/flutter_test.dart';

/// A widget test that hangs is named within a minute. Flutter's own limit is ten, and three of
/// those outlast the job, which then leaves no log at all.
Future<void> testExecutable(FutureOr<void> Function() main) async {
  final binding = TestWidgetsFlutterBinding.ensureInitialized();
  if (binding is AutomatedTestWidgetsFlutterBinding) {
    binding.defaultTestTimeout = const Timeout(Duration(minutes: 1));
  }
  await main();
}
